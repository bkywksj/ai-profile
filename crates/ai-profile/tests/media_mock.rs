//! 多模态调用的模拟服务端测试：本地起 HTTP 服务，按各家协议回放响应，跑完整的「请求 → 解析」链路。
//!
//! 为什么要有：`media` 占全库三分之一代码，此前只有分发与解析的单测，
//! 没有一条测试真正发过请求。协议细节（路径、鉴权头写法、轮询用哪个 id、三段式多一步取址）
//! 一旦被改错，单测全绿、下游上线才炸。
//!
//! 协议识别靠域名，模拟服务端是 127.0.0.1 识别不出 —— 所以这里**直接构造具体 provider**；
//! 识别规则由 `media::video` / `media::image` 里的单测单独守。
//!
//! 🔴 只断言「协议契约」（路径、方法、鉴权、关键字段、结果提取、错误翻译），
//! 不断言无关字段 —— 否则每调一次请求体都要改测试，测试会被当成噪音删掉。
#![cfg(all(
    feature = "client",
    feature = "image",
    feature = "video",
    feature = "tts"
))]

use ai_profile::media::image::{
    DashScopeImageProvider, ImageGenConfig, ImageGenParams, ImageProvider, OpenAiImageProvider,
    MIN_IMAGE_BYTES,
};
use ai_profile::media::tts::{
    SiliconFlowTtsProvider, TtsConfig, TtsParams, TtsProvider, VolcTtsConfig, VolcTtsProvider,
};
use ai_profile::media::video::{
    ArkVideoProvider, MinimaxVideoProvider, NewApiVideoProvider, SiliconFlowVideoProvider,
    VideoGenConfig, VideoGenParams, VideoProvider, VideoTaskStatus, ViduVideoProvider,
    ZhipuVideoProvider,
};
use ai_profile::media::{MediaError, MediaHttp};
use base64::Engine;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-test";

/// 🔴 关掉代理：reqwest 默认读 HTTP_PROXY，且**不会**为 127.0.0.1 自动绕开 ——
/// 开着 Clash 之类的机器上，请求会先绕到代理再转回本机，代理掐断复用连接时随机报 os error 10053。
/// 顺带也就测到了 `MediaHttp` 的底座注入这条路径。
fn http() -> MediaHttp {
    MediaHttp::from_fn(|| ai_profile::reqwest::Client::builder().no_proxy())
}

/// 够长、魔数正确的假 PNG（过得了 `validate_image_bytes`）。
fn fake_png() -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.resize(MIN_IMAGE_BYTES + 64, 0);
    v
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn image_cfg(endpoint: String) -> ImageGenConfig {
    ImageGenConfig {
        endpoint,
        model: "img-model".into(),
        api_key: KEY.into(),
    }
}

fn image_params() -> ImageGenParams {
    ImageGenParams {
        prompt: "一只猫".into(),
        seed: Some(42),
        ..Default::default()
    }
}

fn video_cfg(endpoint: String) -> VideoGenConfig {
    VideoGenConfig {
        endpoint,
        model: "vid-model".into(),
        api_key: KEY.into(),
    }
}

fn video_params() -> VideoGenParams {
    VideoGenParams {
        prompt: "镜头推进".into(),
        image: "https://img.example/first.png".into(),
        ..Default::default()
    }
}

fn failed_msg<T: std::fmt::Debug>(r: Result<T, MediaError>) -> String {
    match r {
        Err(MediaError::Failed(m)) => m,
        other => panic!("期望 MediaError::Failed，实际 {other:?}"),
    }
}

// ───────────────────────────── 生图 ─────────────────────────────

#[tokio::test]
async fn openai_image_inline_b64() {
    let s = MockServer::start().await;
    let png = fake_png();
    Mock::given(method("POST"))
        .and(path("/v1/images/generations"))
        .and(header("authorization", "Bearer sk-test"))
        .and(body_partial_json(
            json!({"model": "img-model", "prompt": "一只猫", "seed": 42}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": [{"b64_json": b64(&png)}], "seed": 42})),
        )
        .expect(1)
        .mount(&s)
        .await;

    let p = OpenAiImageProvider::with_http(image_cfg(format!("{}/v1", s.uri())), &http());
    let r = p.generate(&image_params()).await.expect("出图成功");
    assert_eq!(r.bytes, png);
    assert_eq!(r.ext, "png");
    assert_eq!(r.seed, Some(42));
}

/// 硅基流动风格：`images[].url` → 即时下载；下载第一次 5xx 要自动重试一次（图已计费，不能判整单失败）。
#[tokio::test]
async fn openai_image_url_is_downloaded_with_one_retry() {
    let s = MockServer::start().await;
    let png = fake_png();
    Mock::given(method("POST"))
        .and(path("/v1/images/generations"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"images": [{"url": format!("{}/out/a.webp", s.uri())}]})),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/out/a.webp"))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/out/a.webp"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.clone()))
        .expect(1)
        .mount(&s)
        .await;

    let p = OpenAiImageProvider::with_http(image_cfg(format!("{}/v1", s.uri())), &http());
    let r = p.generate(&image_params()).await.expect("重试后成功");
    assert_eq!(r.bytes, png);
    assert_eq!(r.ext, "webp", "扩展名按 URL 后缀推断");
}

#[tokio::test]
async fn openai_image_error_translations() {
    let s = MockServer::start().await;
    let p = OpenAiImageProvider::with_http(image_cfg(format!("{}/v1", s.uri())), &http());

    // 404 → 模型未开通的可操作提示
    let m = Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(404).set_body_string("InvalidEndpointOrModel.NotFound"))
        .mount_as_scoped(&s)
        .await;
    let msg = failed_msg(p.generate(&image_params()).await);
    assert!(
        msg.contains("不存在或未开通") && msg.contains("img-model"),
        "{msg}"
    );
    drop(m);

    // 2xx 但没给图 → 内容审核提示，而不是笼统的「未返回图片」
    let m = Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .mount_as_scoped(&s)
        .await;
    let msg = failed_msg(p.generate(&image_params()).await);
    assert!(msg.contains("内容安全审核"), "{msg}");
    drop(m);

    // 下载到的是一段 JSON 错误体 → 明确说「不是有效图片」并带上内容开头
    let url = format!("{}/out/bad.png", s.uri());
    let _a = Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"images": [{"url": url}]})))
        .mount_as_scoped(&s)
        .await;
    let body = format!(
        "{{\"error\":\"expired\",\"pad\":\"{}\"}}",
        "x".repeat(MIN_IMAGE_BYTES)
    );
    let _b = Mock::given(method("GET"))
        .and(path("/out/bad.png"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount_as_scoped(&s)
        .await;
    let msg = failed_msg(p.generate(&image_params()).await);
    assert!(
        msg.contains("不是有效图片") && msg.contains("expired"),
        "{msg}"
    );
}

/// 通义万相：异步两段式，提交必须带 `X-DashScope-Async: enable`，尺寸用 `*` 而不是 `x`。
#[tokio::test]
async fn dashscope_image_submit_poll_download() {
    let s = MockServer::start().await;
    let png = fake_png();
    Mock::given(method("POST"))
        .and(path("/api/v1/services/aigc/text2image/image-synthesis"))
        .and(header("x-dashscope-async", "enable"))
        .and(header("authorization", "Bearer sk-test"))
        .and(body_partial_json(
            json!({"model": "img-model", "input": {"prompt": "一只猫"},
                   "parameters": {"size": "720*1280", "seed": 42}}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"output": {"task_id": "t-1", "task_status": "PENDING"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tasks/t-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "output": {"task_status": "SUCCEEDED", "results": [{"url": format!("{}/r.png", s.uri())}]}
        })))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/r.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.clone()))
        .expect(1)
        .mount(&s)
        .await;

    let p = DashScopeImageProvider::with_http(image_cfg(format!("{}/api/v1", s.uri())), &http());
    let r = p.generate(&image_params()).await.expect("出图成功");
    assert_eq!(r.bytes, png);
}

#[tokio::test]
async fn dashscope_image_task_failure_is_reported() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"output": {"task_id": "t-2"}})),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tasks/t-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"output": {"task_status": "FAILED", "message": "DataInspectionFailed"}}),
        ))
        .mount(&s)
        .await;

    let p = DashScopeImageProvider::with_http(image_cfg(format!("{}/api/v1", s.uri())), &http());
    let msg = failed_msg(p.generate(&image_params()).await);
    assert!(msg.contains("DataInspectionFailed"), "{msg}");
}

// ───────────────────────────── 视频 ─────────────────────────────

/// 先回一次「进行中」再回「成功」：验证 Pending 能正确识别，不会把进行中误判成失败。
async fn mount_poll_pending_then(
    s: &MockServer,
    m: &str,
    p: &str,
    pending: serde_json::Value,
    done: serde_json::Value,
) {
    Mock::given(method(m))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(pending))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(s)
        .await;
    Mock::given(method(m))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(done))
        .expect(1)
        .mount(s)
        .await;
}

async fn run_to_done(p: &impl VideoProvider) -> VideoTaskStatus {
    let id = p.submit(&video_params()).await.expect("提交成功");
    assert!(matches!(
        p.poll(&id).await.expect("轮询 1"),
        VideoTaskStatus::Pending
    ));
    p.poll(&id).await.expect("轮询 2")
}

fn assert_succeeded(st: VideoTaskStatus, url: &str) {
    match st {
        VideoTaskStatus::Succeeded(u) => assert_eq!(u, url),
        other => panic!("期望成功，实际 {other:?}"),
    }
}

/// 火山方舟：时长 / 分辨率 / 画幅必须拼进 text 尾缀（放顶层会被官方忽略、永远 5 秒）。
#[tokio::test]
async fn ark_video() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/contents/generations/tasks"))
        .and(header("authorization", "Bearer sk-test"))
        .and(body_partial_json(json!({
            "model": "vid-model",
            "content": [{"type": "text", "text": "镜头推进 --rs 720p --rt 9:16 --dur 5"},
                        {"type": "image_url", "role": "first_frame"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "cgt-1"})))
        .expect(1)
        .mount(&s)
        .await;
    mount_poll_pending_then(
        &s,
        "GET",
        "/api/v3/contents/generations/tasks/cgt-1",
        json!({"status": "running"}),
        json!({"status": "succeeded", "content": {"video_url": "https://v/1.mp4"}}),
    )
    .await;

    let p = ArkVideoProvider::with_http(video_cfg(format!("{}/api/v3", s.uri())), &http());
    assert_succeeded(run_to_done(&p).await, "https://v/1.mp4");
}

#[tokio::test]
async fn ark_video_failed_and_http_error() {
    let s = MockServer::start().await;
    let p = ArkVideoProvider::with_http(video_cfg(format!("{}/api/v3", s.uri())), &http());

    let _m = Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"status": "failed", "error": {"message": "OutputVideoSensitiveContentDetected"}}),
        ))
        .mount_as_scoped(&s)
        .await;
    match p.poll("cgt-x").await.expect("轮询本身成功") {
        VideoTaskStatus::Failed(m) => assert!(m.contains("SensitiveContent"), "{m}"),
        other => panic!("期望失败态，实际 {other:?}"),
    }

    let _m = Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("AuthenticationError"))
        .mount_as_scoped(&s)
        .await;
    let msg = failed_msg(p.submit(&video_params()).await);
    assert!(
        msg.contains("HTTP 401") && msg.contains("AuthenticationError"),
        "{msg}"
    );
}

/// 硅基流动：查询也是 POST，id 放在 body 的 `requestId`（驼峰）里。
#[tokio::test]
async fn siliconflow_video() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/video/submit"))
        .and(body_partial_json(
            json!({"model": "vid-model", "image": "https://img.example/first.png"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"requestId": "rq-1"})))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/video/status"))
        .and(body_partial_json(json!({"requestId": "rq-1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "InProgress"})))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/video/status"))
        .and(body_partial_json(json!({"requestId": "rq-1"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"status": "Succeed", "results": {"videos": [{"url": "https://v/sf.mp4"}]}}),
        ))
        .expect(1)
        .mount(&s)
        .await;

    let p = SiliconFlowVideoProvider::with_http(video_cfg(format!("{}/v1", s.uri())), &http());
    assert_succeeded(run_to_done(&p).await, "https://v/sf.mp4");
}

/// 海螺：三段式 —— 成功后还要用 file_id 再取一次下载地址。
#[tokio::test]
async fn minimax_video_three_steps() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/video_generation"))
        .and(body_partial_json(
            json!({"first_frame_image": "https://img.example/first.png"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"task_id": "mm-1"})))
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/query/video_generation"))
        .and(query_param("task_id", "mm-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "Processing"})))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/query/video_generation"))
        .and(query_param("task_id", "mm-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "Success", "file_id": "f-9"})),
        )
        .expect(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/files/retrieve"))
        .and(query_param("file_id", "f-9"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"file": {"download_url": "https://v/mm.mp4"}})),
        )
        .expect(1)
        .mount(&s)
        .await;

    let p = MinimaxVideoProvider::with_http(video_cfg(format!("{}/v1", s.uri())), &http());
    assert_succeeded(run_to_done(&p).await, "https://v/mm.mp4");
}

#[tokio::test]
async fn vidu_video() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v1/services/aigc/video-generation/video-synthesis",
        ))
        .and(header("x-dashscope-async", "enable"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"output": {"task_id": "vd-1", "task_status": "PENDING"}})),
        )
        .expect(1)
        .mount(&s)
        .await;
    mount_poll_pending_then(
        &s,
        "GET",
        "/api/v1/tasks/vd-1",
        json!({"output": {"task_status": "RUNNING"}}),
        json!({"output": {"task_status": "SUCCEEDED", "video_url": "https://v/vd.mp4"}}),
    )
    .await;

    let p = ViduVideoProvider::with_http(video_cfg(format!("{}/api/v1", s.uri())), &http());
    assert_succeeded(run_to_done(&p).await, "https://v/vd.mp4");
}

#[tokio::test]
async fn zhipu_video() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/paas/v4/videos/generations"))
        .and(body_partial_json(
            json!({"model": "vid-model", "image_url": "https://img.example/first.png"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "zp-1", "task_status": "PROCESSING"})),
        )
        .expect(1)
        .mount(&s)
        .await;
    mount_poll_pending_then(
        &s,
        "GET",
        "/api/paas/v4/async-result/zp-1",
        json!({"task_status": "PROCESSING"}),
        json!({"task_status": "SUCCESS", "video_result": [{"url": "https://v/zp.mp4"}]}),
    )
    .await;

    let p = ZhipuVideoProvider::with_http(video_cfg(format!("{}/api/paas/v4", s.uri())), &http());
    assert_succeeded(run_to_done(&p).await, "https://v/zp.mp4");
}

/// New API：🔴 轮询必须用返回的 `id`（task_xxx），不是 uuid 的 `task_id` —— 用错就永远 404。
#[tokio::test]
async fn newapi_video_polls_with_id_not_task_id() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/video/generations"))
        .and(body_partial_json(
            json!({"model": "vid-model", "width": 720, "height": 1280}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"id": "task_abc", "task_id": "0b9e-uuid", "status": "queued"}),
            ),
        )
        .expect(1)
        .mount(&s)
        .await;
    mount_poll_pending_then(
        &s,
        "GET",
        "/v1/video/generations/task_abc",
        json!({"status": "in_progress", "progress": 40}),
        json!({"status": "completed", "metadata": {"url": "https://v/na.mp4"}}),
    )
    .await;

    let p = NewApiVideoProvider::with_http(video_cfg(format!("{}/v1", s.uri())), &http());
    assert_succeeded(run_to_done(&p).await, "https://v/na.mp4");
}

/// New API 网关把上游真因吞成 fail_to_fetch_task：400 要翻译成「首帧触发内容审核」的可操作提示。
#[tokio::test]
async fn newapi_video_submit_error_is_translated() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": "fail_to_fetch_task", "message": "upstream returned status 400"
        })))
        .mount(&s)
        .await;

    let p = NewApiVideoProvider::with_http(video_cfg(format!("{}/v1", s.uri())), &http());
    let msg = failed_msg(p.submit(&video_params()).await);
    assert!(
        msg.contains("内容安全审核") && msg.contains("HTTP 400"),
        "{msg}"
    );
}

// ───────────────────────────── 配音 ─────────────────────────────

fn tts_cfg(endpoint: String, model: &str) -> TtsConfig {
    TtsConfig {
        endpoint,
        model: model.into(),
        api_key: KEY.into(),
    }
}

fn tts_params(text: &str) -> TtsParams {
    TtsParams {
        text: text.into(),
        ..Default::default()
    }
}

/// CosyVoice：音色为空时补 `<model>:alex`（不给 voice 硅基流动直接 400）。
#[tokio::test]
async fn siliconflow_tts_default_voice() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/speech"))
        .and(header("authorization", "Bearer sk-test"))
        .and(body_partial_json(json!({
            "model": "FunAudioLLM/CosyVoice2-0.5B",
            "input": "你好",
            "voice": "FunAudioLLM/CosyVoice2-0.5B:alex",
            "response_format": "mp3"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ID3-mp3-bytes".to_vec()))
        .expect(1)
        .mount(&s)
        .await;

    let p = SiliconFlowTtsProvider::with_http(
        tts_cfg(format!("{}/v1", s.uri()), "FunAudioLLM/CosyVoice2-0.5B"),
        &http(),
    );
    assert_eq!(
        p.synthesize(&tts_params("你好")).await.unwrap(),
        b"ID3-mp3-bytes"
    );
}

/// glm-tts 走同一接口但只收 wav，且音色是固定枚举名（发 mp3 直接 400）。
#[tokio::test]
async fn glm_tts_forces_wav_and_enum_voice() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/speech"))
        .and(body_partial_json(
            json!({"voice": "tongtong", "response_format": "wav"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"RIFF-wav".to_vec()))
        .expect(1)
        .mount(&s)
        .await;

    let p =
        SiliconFlowTtsProvider::with_http(tts_cfg(format!("{}/v1", s.uri()), "glm-tts"), &http());
    let params = TtsParams {
        voice: "alex".into(), // 切换供应商后残留的 CosyVoice 音色 → 应回退 tongtong
        ..tts_params("你好")
    };
    assert_eq!(p.synthesize(&params).await.unwrap(), b"RIFF-wav");
}

#[tokio::test]
async fn tts_empty_audio_and_empty_text() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&s)
        .await;
    let p = SiliconFlowTtsProvider::with_http(tts_cfg(format!("{}/v1", s.uri()), "m"), &http());

    let msg = failed_msg(p.synthesize(&tts_params("你好")).await);
    assert!(msg.contains("空音频"), "{msg}");
    // 空文本在发请求前就拦下，且归为「入参不对」而不是调用失败
    assert!(matches!(
        p.synthesize(&tts_params("  ")).await,
        Err(MediaError::InvalidInput(_))
    ));
}

/// 火山语音：鉴权头是 `Bearer;{token}`（分号），成功码 3000，音频以 base64 放在 data 里。
#[tokio::test]
async fn volc_tts() {
    let s = MockServer::start().await;
    let audio = b"volc-mp3".to_vec();
    Mock::given(method("POST"))
        .and(path("/api/v1/tts"))
        .and(header("authorization", "Bearer;tok-1"))
        .and(body_partial_json(json!({
            "app": {"appid": "app-1", "token": "tok-1", "cluster": "volcano_tts"},
            "request": {"text": "你好", "operation": "query"}
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code": 3000, "data": b64(&audio)})),
        )
        .expect(1)
        .mount(&s)
        .await;

    let p = VolcTtsProvider::with_http(
        VolcTtsConfig {
            endpoint: format!("{}/api/v1/tts", s.uri()),
            appid: "app-1".into(),
            cluster: String::new(),
            access_token: "tok-1".into(),
        },
        &http(),
    );
    assert_eq!(p.synthesize(&tts_params("你好")).await.unwrap(), audio);
}

#[tokio::test]
async fn volc_tts_business_error_code() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code": 3001, "message": "requested resource not granted"})),
        )
        .mount(&s)
        .await;

    let p = VolcTtsProvider::with_http(
        VolcTtsConfig {
            endpoint: format!("{}/api/v1/tts", s.uri()),
            appid: "app-1".into(),
            cluster: "volcano_tts".into(),
            access_token: "tok-1".into(),
        },
        &http(),
    );
    let msg = failed_msg(p.synthesize(&tts_params("你好")).await);
    assert!(
        msg.contains("code 3001") && msg.contains("not granted"),
        "{msg}"
    );
}
