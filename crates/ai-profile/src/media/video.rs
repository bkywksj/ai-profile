//! 漫剧化 V2 · 图生视频供应商抽象（VideoProvider）。
//!
//! 与同步出图（ImageProvider）不同，图生视频是**异步两段式**：
//! submit（提交任务拿云端 task_id）→ poll（轮询直到 succeeded/failed，取视频 URL）。
//! 首个实现 `ArkVideoProvider` 适配火山方舟（Seedance）：
//!   - 创建任务 POST {endpoint}/contents/generations/tasks
//!     body content 数组：text 运镜提示 + image_url(role=first_frame) 用分镜图作首帧
//!   - 查询任务 GET {endpoint}/contents/generations/tasks/{id}
//!     status: queued/running/succeeded/failed/expired；succeeded 时 content.video_url 为结果

use serde::Deserialize;

use super::truncate;
use super::MediaError;

/// 图生视频调用配置（含解密后的 Key，仅 Rust 内部传递）
#[derive(Debug, Clone)]
pub struct VideoGenConfig {
    /// 端点（火山方舟 base url，如 `https://ark.cn-beijing.volces.com/api/v3`）
    pub endpoint: String,
    /// 模型 id
    pub model: String,
    /// 明文密钥（仅在内存中传递，不落盘、不写日志）
    pub api_key: String,
}

/// 图生视频入参
#[derive(Debug, Clone)]
pub struct VideoGenParams {
    /// 运镜 / 动态提示（如「镜头缓慢推进，人物自然眨眼」）
    pub prompt: String,
    /// 首帧图（分镜出图）的 URL 或 base64 data URL（作 first_frame）
    pub image: String,
    /// 尾帧图（可选）：给了就走「首尾帧」模式 —— AI 在首帧与尾帧之间补出过程，视频走向可控。
    ///
    /// 与仅给首帧的区别：只给首帧时 AI 自由推演，「无法控制视频最终走向到哪里」；
    /// 给了尾帧则终点被钉死，适合「主角从 A 走到 B」这类需要精确控制的镜头。
    /// 本项目的主用法是**镜间衔接**：拿下一镜的出图当尾帧，让镜 N 的画面收束到镜 N+1 的起始画面，
    /// 成片 concat 时切点自然顺滑 —— 尾帧图是现成的，零额外出图成本。
    ///
    /// 空串 = 退回原有的纯首帧模式（各 provider 的行为逐字不变）。
    /// ⚠️ 并非所有供应商都支持，见 `supports_last_frame`。
    pub last_image: String,
    /// 画幅比例（竖屏短剧默认 9:16）
    pub ratio: String,
    /// 分辨率（480p/720p/1080p）
    pub resolution: String,
    /// 时长秒数
    pub duration: u32,
}

impl Default for VideoGenParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            image: String::new(),
            last_image: String::new(),
            ratio: "9:16".to_string(),
            resolution: "720p".to_string(),
            duration: 5,
        }
    }
}

/// 轮询返回的任务状态
#[derive(Debug, Clone)]
pub enum VideoTaskStatus {
    /// 排队 / 运行中（queued / running）
    Pending,
    /// 完成，携带视频 URL
    Succeeded(String),
    /// 失败，携带错误摘要
    Failed(String),
}

/// 图生视频供应商抽象（异步两段式）。trait 留扩展点（可灵视频等后续加）。
/// future 显式约束 `Send`（工作流完善 V4·T6）：单镜 fire-and-forget 要把泛型 provider
/// move 进 `tokio::spawn` 后台轮询，要求其方法返回的 future 跨线程安全。
pub trait VideoProvider: Send + Sync {
    /// 提交图生视频任务，返回云端 task_id
    fn submit(
        &self,
        params: &VideoGenParams,
    ) -> impl std::future::Future<Output = Result<String, MediaError>> + Send;
    /// 轮询任务状态
    fn poll(
        &self,
        task_id: &str,
    ) -> impl std::future::Future<Output = Result<VideoTaskStatus, MediaError>> + Send;
}

/// 火山方舟（Seedance）图生视频实现
pub struct ArkVideoProvider {
    client: super::http::MediaClient,
    config: VideoGenConfig,
}

impl ArkVideoProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VideoGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: VideoGenConfig, http: &super::MediaHttp) -> Self {
        // 带超时客户端：submit/poll/下载各请求兜底 180s，杜绝供应商挂起永久阻塞（审查 P0）
        Self {
            client: super::http::default_client(http),
            config,
        }
    }

    /// 拼任务创建/列表地址（容忍端点尾斜杠 / 是否已含路径）
    fn tasks_url(&self) -> String {
        let e = self.config.endpoint.trim_end_matches('/');
        if e.ends_with("/contents/generations/tasks") {
            e.to_string()
        } else {
            format!("{e}/contents/generations/tasks")
        }
    }
}

#[derive(Deserialize)]
struct SubmitResponse {
    /// 任务号（火山方舟返回 id）
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
struct PollResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    content: Option<PollContent>,
    #[serde(default)]
    error: Option<PollError>,
}

#[derive(Deserialize, Default)]
struct PollContent {
    #[serde(default)]
    video_url: Option<String>,
}

#[derive(Deserialize, Default)]
struct PollError {
    #[serde(default)]
    message: Option<String>,
}

impl VideoProvider for ArkVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        // 🔴 火山方舟 Seedance：时长/分辨率/画幅是**文本命令参数**（拼进 text 尾缀的 --dur/--rs/--rt），
        // 不是请求体顶层字段。此前放顶层 → Ark 直接忽略、落回默认 5 秒，导致前端无论设几秒成片永远 5 秒。
        // 修复：把三项拼进 text 命令尾缀（官方唯一生效的写法）；顶层同名字段保留作 Ark 兼容中转站的
        // 冗余兜底（值一致不冲突，官方 Ark 忽略之）。
        let text = format!(
            "{} --rs {} --rt {} --dur {}",
            params.prompt.trim(),
            params.resolution,
            params.ratio,
            params.duration
        );
        // content 数组：文本命令（含运镜提示 + 参数尾缀，始终带）+ 首帧图
        let mut content = vec![
            serde_json::json!({ "type": "text", "text": text.trim() }),
            serde_json::json!({
                "type": "image_url",
                "image_url": { "url": params.image },
                "role": "first_frame",
            }),
        ];
        // 首尾帧模式：再追加一个 role=last_frame 的图元素，AI 在首尾之间补出过程（走向可控）。
        // Seedance 官方协议就是靠 role 区分首/尾帧，故此处只需多一个元素、不动其它字段。
        // 空则不追加 → 逐字退回原有的纯首帧行为。
        if !params.last_image.trim().is_empty() {
            content.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": params.last_image },
                "role": "last_frame",
            }));
        }
        let body = serde_json::json!({
            "model": self.config.model,
            "content": content,
            // 顶层字段仅为 Ark 兼容中转站兜底；官方 Ark 忽略这三项、只认 text 里的 --dur/--rs/--rt
            "ratio": params.ratio,
            "resolution": params.resolution,
            "duration": params.duration,
        });

        let resp = self
            .client
            .get()?
            .post(self.tasks_url())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频提交 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: SubmitResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务创建响应失败: {e}")))?;
        parsed
            .id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("任务创建响应未返回 task id".into()))
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        let url = format!("{}/{}", self.tasks_url(), task_id);
        let resp = self
            .client
            .get()?
            .get(&url)
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "查询图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频查询 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: PollResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务状态响应失败: {e}")))?;

        match parsed.status.as_deref() {
            Some("succeeded") => {
                let url = parsed
                    .content
                    .and_then(|c| c.video_url)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| MediaError::Failed("任务成功但未返回视频 URL".into()))?;
                Ok(VideoTaskStatus::Succeeded(url))
            }
            Some("failed") | Some("expired") => {
                let msg = parsed
                    .error
                    .and_then(|e| e.message)
                    .unwrap_or_else(|| "任务失败".into());
                Ok(VideoTaskStatus::Failed(msg))
            }
            // queued / running / 其它 → 继续等
            _ => Ok(VideoTaskStatus::Pending),
        }
    }
}

/// 硅基流动 视频生成（Wan2.x 图生视频）实现 —— 与 Ark 不同端点/协议：
///   - 提交 POST {endpoint}/video/submit  body {model, prompt, image_size, image(首帧)}
///   - 查询 POST {endpoint}/video/status  body {requestId}
///     status: Succeed / InProgress / Failed；Succeed 时 `results.videos[0].url` 为结果
pub struct SiliconFlowVideoProvider {
    client: super::http::MediaClient,
    config: VideoGenConfig,
}

impl SiliconFlowVideoProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VideoGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: VideoGenConfig, http: &super::MediaHttp) -> Self {
        // 带超时客户端：submit/poll/下载各请求兜底 180s，杜绝供应商挂起永久阻塞（审查 P0）
        Self {
            client: super::http::default_client(http),
            config,
        }
    }

    fn base(&self) -> String {
        self.config.endpoint.trim_end_matches('/').to_string()
    }

    /// 画幅比例 → 硅基流动 image_size 尺寸串（竖屏短剧默认 9:16）
    fn image_size(ratio: &str) -> &'static str {
        match ratio {
            "16:9" => "1280x720",
            "1:1" => "960x960",
            _ => "720x1280",
        }
    }
}

#[derive(Deserialize)]
struct SfSubmitResponse {
    #[serde(rename = "requestId", default)]
    request_id: Option<String>,
}

#[derive(Deserialize)]
struct SfStatusResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    results: Option<SfResults>,
}

#[derive(Deserialize, Default)]
struct SfResults {
    #[serde(default)]
    videos: Vec<SfVideo>,
}

#[derive(Deserialize, Default)]
struct SfVideo {
    #[serde(default)]
    url: Option<String>,
}

impl VideoProvider for SiliconFlowVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        let body = serde_json::json!({
            "model": self.config.model,
            "prompt": params.prompt,
            "image_size": Self::image_size(&params.ratio),
            "image": params.image, // 分镜图（base64 data URL 或 URL）作首帧（图生视频）
        });
        let resp = self
            .client
            .get()?
            .post(format!("{}/video/submit", self.base()))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频提交 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: SfSubmitResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务创建响应失败: {e}")))?;
        parsed
            .request_id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("任务创建响应未返回 requestId".into()))
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        let body = serde_json::json!({ "requestId": task_id });
        let resp = self
            .client
            .get()?
            .post(format!("{}/video/status", self.base()))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "查询图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频查询 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: SfStatusResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务状态响应失败: {e}")))?;
        match parsed.status.as_deref() {
            Some("Succeed") => {
                let url = parsed
                    .results
                    .and_then(|r| r.videos.into_iter().next())
                    .and_then(|v| v.url)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| MediaError::Failed("任务成功但未返回视频 URL".into()))?;
                Ok(VideoTaskStatus::Succeeded(url))
            }
            Some("Failed") => {
                let msg = parsed.reason.unwrap_or_else(|| "任务失败".into());
                Ok(VideoTaskStatus::Failed(msg))
            }
            // InProgress / 其它 → 继续等
            _ => Ok(VideoTaskStatus::Pending),
        }
    }
}

/// 海螺 MiniMax 图生视频实现（工作流完善 V4·T10）。**三段式**（与 Ark/SiliconFlow 两段不同）：
///   - 提交 POST {base}/video_generation  body {model, prompt, first_frame_image(首帧 base64/url)} → {task_id}
///   - 查询 GET  {base}/query/video_generation?task_id=xxx → {status, file_id(Success 时)}
///     status: Queueing / Preparing / Processing / Success / Fail
///   - 取址 GET  {base}/files/retrieve?file_id=xxx → {file.download_url}（Success 后多一步取下载地址）
///
/// 端点形如 `https://api.minimaxi.com/v1`。
pub struct MinimaxVideoProvider {
    client: super::http::MediaClient,
    config: VideoGenConfig,
}

impl MinimaxVideoProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VideoGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: VideoGenConfig, http: &super::MediaHttp) -> Self {
        // 带超时客户端：submit/poll/下载各请求兜底 180s，杜绝供应商挂起永久阻塞（审查 P0）
        Self {
            client: super::http::default_client(http),
            config,
        }
    }
    fn base(&self) -> String {
        self.config.endpoint.trim_end_matches('/').to_string()
    }
}

#[derive(Deserialize)]
struct MmSubmitResponse {
    #[serde(default)]
    task_id: Option<String>,
}

#[derive(Deserialize)]
struct MmQueryResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    file_id: Option<String>,
}

#[derive(Deserialize)]
struct MmFileResponse {
    #[serde(default)]
    file: Option<MmFile>,
}

#[derive(Deserialize, Default)]
struct MmFile {
    #[serde(default)]
    download_url: Option<String>,
}

impl VideoProvider for MinimaxVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "prompt": params.prompt,
            // 分镜出图作首帧（base64 data URL 或公网 URL）—— 图生视频
            "first_frame_image": params.image,
        });
        // 首尾帧：官方顶层字段 last_frame_image（仅 Hailuo-02 及以上；不支持 512P；
        // 视频分辨率以首帧为准，尾帧尺寸不同会被裁到与首帧一致）。空则不下发，行为不变。
        if !params.last_image.trim().is_empty() {
            body["last_frame_image"] = serde_json::Value::String(params.last_image.clone());
        }
        let resp = self
            .client
            .get()?
            .post(format!("{}/video_generation", self.base()))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频提交 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: MmSubmitResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务创建响应失败: {e}")))?;
        parsed
            .task_id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("任务创建响应未返回 task_id".into()))
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        let url = format!("{}/query/video_generation?task_id={}", self.base(), task_id);
        let resp = self
            .client
            .get()?
            .get(&url)
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "查询图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频查询 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: MmQueryResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务状态响应失败: {e}")))?;
        match parsed.status.as_deref() {
            Some("Success") => {
                let file_id = parsed
                    .file_id
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| MediaError::Failed("任务成功但未返回 file_id".into()))?;
                // 三段式多一步：用 file_id 取下载地址
                let furl = format!("{}/files/retrieve?file_id={}", self.base(), file_id);
                let fresp = self
                    .client
                    .get()?
                    .get(&furl)
                    .bearer_auth(&self.config.api_key)
                    .send()
                    .await
                    .map_err(|e| {
                        MediaError::Failed(format!(
                            "取视频文件地址失败: {}",
                            super::http::describe_reqwest_error(&e)
                        ))
                    })?;
                if !fresp.status().is_success() {
                    let code = fresp.status().as_u16();
                    let text = fresp.text().await.unwrap_or_default();
                    return Err(MediaError::Failed(format!(
                        "取文件 HTTP {}: {}",
                        code,
                        truncate(&text, 300)
                    )));
                }
                let fparsed: MmFileResponse = fresp
                    .json()
                    .await
                    .map_err(|e| MediaError::Failed(format!("解析文件响应失败: {e}")))?;
                let dl = fparsed
                    .file
                    .and_then(|f| f.download_url)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| MediaError::Failed("文件响应未返回 download_url".into()))?;
                Ok(VideoTaskStatus::Succeeded(dl))
            }
            Some("Fail") => Ok(VideoTaskStatus::Failed("任务失败".into())),
            // Queueing / Preparing / Processing / 其它 → 继续等
            _ => Ok(VideoTaskStatus::Pending),
        }
    }
}

/// Vidu 图生视频（阿里百炼 DashScope，工作流完善 V4·T11）。异步两段式：
///   - 提交 POST {base}/services/aigc/video-generation/video-synthesis
///     头 X-DashScope-Async: enable；body {model, input:{media:[{type:image,url}], prompt},
///     parameters:{duration,resolution}} → {output:{task_id, task_status}}
///   - 查询 GET {base}/tasks/{task_id} → {output:{task_status, video_url}}
///     task_status: PENDING / RUNNING / SUCCEEDED / FAILED / CANCELED / UNKNOWN
///
/// 端点形如 `https://dashscope.aliyuncs.com/api/v1`（百炼亦可经 302 透传 /dashscope）。
pub struct ViduVideoProvider {
    client: super::http::MediaClient,
    config: VideoGenConfig,
}

impl ViduVideoProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VideoGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: VideoGenConfig, http: &super::MediaHttp) -> Self {
        // 带超时客户端：submit/poll/下载各请求兜底 180s，杜绝供应商挂起永久阻塞（审查 P0）
        Self {
            client: super::http::default_client(http),
            config,
        }
    }
    fn base(&self) -> String {
        self.config.endpoint.trim_end_matches('/').to_string()
    }
}

#[derive(Deserialize, Default)]
struct DashOutput {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    task_status: Option<String>,
    #[serde(default)]
    video_url: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct DashResponse {
    #[serde(default)]
    output: Option<DashOutput>,
    #[serde(default)]
    message: Option<String>,
}

impl VideoProvider for ViduVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        let body = serde_json::json!({
            "model": self.config.model,
            "input": {
                // 分镜出图作首帧（base64 data URL 或公网 URL）
                "media": [ { "type": "image", "url": params.image } ],
                "prompt": params.prompt,
            },
            "parameters": {
                "duration": params.duration,
                // 百炼分辨率枚举为大写（540P/720P/1080P）
                "resolution": params.resolution.to_uppercase(),
            },
        });
        let resp = self
            .client
            .get()?
            .post(format!(
                "{}/services/aigc/video-generation/video-synthesis",
                self.base()
            ))
            .bearer_auth(&self.config.api_key)
            .header("X-DashScope-Async", "enable")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频提交 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: DashResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务创建响应失败: {e}")))?;
        parsed
            .output
            .and_then(|o| o.task_id)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                MediaError::Failed(format!(
                    "任务创建响应未返回 task_id{}",
                    parsed.message.map(|m| format!(": {m}")).unwrap_or_default()
                ))
            })
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        let resp = self
            .client
            .get()?
            .get(format!("{}/tasks/{}", self.base(), task_id))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "查询图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频查询 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: DashResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务状态响应失败: {e}")))?;
        let out = parsed.output.unwrap_or_default();
        match out.task_status.as_deref() {
            Some("SUCCEEDED") => out
                .video_url
                .filter(|s| !s.is_empty())
                .map(VideoTaskStatus::Succeeded)
                .ok_or_else(|| MediaError::Failed("任务成功但未返回 video_url".into())),
            Some("FAILED") | Some("CANCELED") | Some("UNKNOWN") => Ok(VideoTaskStatus::Failed(
                out.message.unwrap_or_else(|| "任务失败".into()),
            )),
            // PENDING / RUNNING / 其它 → 继续等
            _ => Ok(VideoTaskStatus::Pending),
        }
    }
}

/// 智谱 CogVideoX 图生视频（工作流完善 V4·T12）。异步两段式：
///   - 提交 POST {base}/videos/generations  body {model, prompt, image_url(首帧 base64/url)}
///     → {id, task_status}
///   - 查询 GET {base}/async-result/{id} → {task_status, video_result:[{url}]}
///     task_status: PROCESSING / SUCCESS / FAIL
///
/// 端点形如 `https://open.bigmodel.cn/api/paas/v4`（亦可经 302 透传 /zhipu/api/paas/v4）。
pub struct ZhipuVideoProvider {
    client: super::http::MediaClient,
    config: VideoGenConfig,
}

impl ZhipuVideoProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VideoGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: VideoGenConfig, http: &super::MediaHttp) -> Self {
        // 带超时客户端：submit/poll/下载各请求兜底 180s，杜绝供应商挂起永久阻塞（审查 P0）
        Self {
            client: super::http::default_client(http),
            config,
        }
    }
    fn base(&self) -> String {
        self.config.endpoint.trim_end_matches('/').to_string()
    }
}

#[derive(Deserialize)]
struct ZhipuSubmit {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
struct ZhipuVideoItem {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Deserialize)]
struct ZhipuResult {
    #[serde(default)]
    task_status: Option<String>,
    #[serde(default)]
    video_result: Option<Vec<ZhipuVideoItem>>,
}

impl VideoProvider for ZhipuVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        // 智谱 CogVideoX：纯首帧时 image_url 是**单个字符串**；首尾帧时改传 **[首帧, 尾帧] 数组**
        // （官方以数组长度区分两种模式，不是另加字段）。故这里按有无尾帧切换 image_url 的类型。
        let image_url = if params.last_image.trim().is_empty() {
            serde_json::Value::String(params.image.clone())
        } else {
            serde_json::json!([params.image, params.last_image])
        };
        let body = serde_json::json!({
            "model": self.config.model,
            "prompt": params.prompt,
            // 分镜出图作首帧（base64 data URL 或公网 URL）；首尾帧模式下为 [首帧, 尾帧]
            "image_url": image_url,
        });
        let resp = self
            .client
            .get()?
            .post(format!("{}/videos/generations", self.base()))
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频提交 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: ZhipuSubmit = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务创建响应失败: {e}")))?;
        parsed
            .id
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("任务创建响应未返回 id".into()))
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        let resp = self
            .client
            .get()?
            .get(format!("{}/async-result/{}", self.base(), task_id))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "查询图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频查询 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: ZhipuResult = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务状态响应失败: {e}")))?;
        match parsed.task_status.as_deref() {
            Some("SUCCESS") => parsed
                .video_result
                .and_then(|v| v.into_iter().next())
                .and_then(|item| item.url)
                .filter(|s| !s.is_empty())
                .map(VideoTaskStatus::Succeeded)
                .ok_or_else(|| MediaError::Failed("任务成功但未返回 video_result url".into())),
            Some("FAIL") => Ok(VideoTaskStatus::Failed("任务失败".into())),
            // PROCESSING / 其它 → 继续等
            _ => Ok(VideoTaskStatus::Pending),
        }
    }
}

/// New API 中转站统一视频协议（new-api / one-api 系聚合站）。异步两段式：
///   - 提交 POST {base}/video/generations  body {model, prompt, image(首帧 base64/url，图生视频), duration}
///     → {id, task_id, status:"queued", ...}。**轮询要用返回的 `id`（task_xxx），不是 uuid 的 `task_id`**。
///   - 查询 GET {base}/video/generations/{id} → {status, progress, metadata:{url}}
///     status: queued / in_progress / completed / failed
///
/// 端点形如 `https://relay.example.com/v1`（与该站 chat/image 共用同一 base + Bearer Key）。
/// 注：纯文生视频时 image 为空不下发；图生视频时 image 作首帧，输出画幅随首帧比例（无需额外传 size）。
pub struct NewApiVideoProvider {
    client: super::http::MediaClient,
    config: VideoGenConfig,
}

impl NewApiVideoProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VideoGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: VideoGenConfig, http: &super::MediaHttp) -> Self {
        // 带超时客户端：submit/poll 各请求兜底，杜绝供应商挂起永久阻塞（与同文件其它 provider 一致）
        Self {
            client: super::http::default_client(http),
            config,
        }
    }

    /// 拼视频任务创建/查询基址 `{endpoint}/video/generations`（容忍端点尾斜杠 / 是否已含该路径）
    fn videos_url(&self) -> String {
        let e = self.config.endpoint.trim_end_matches('/');
        if e.ends_with("/video/generations") {
            e.to_string()
        } else {
            format!("{e}/video/generations")
        }
    }
}

#[derive(Deserialize)]
struct NewApiSubmit {
    /// 轮询句柄（new-api 返回的 `id`，形如 task_xxx）；缺失时回退 `task_id`。
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct NewApiMetadata {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Deserialize)]
struct NewApiPoll {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    metadata: Option<NewApiMetadata>,
    /// 部分实现把结果 URL 放顶层 video_url / url（非 metadata），一并兜底。
    #[serde(default)]
    video_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
    /// 失败原因（new-api 失败态字段不统一，挨个兜底）
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    fail_reason: Option<String>,
}

impl VideoProvider for NewApiVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        // 按画幅比例 + 分辨率档锁定输出宽高（官方 createVideoGeneration 支持 width/height）。
        // 不下发时上游靠默认（跟随首帧比例），易导致各镜画幅漂移；显式锁定保证成片画幅统一。
        let (width, height) = newapi_video_dimensions(&params.ratio, &params.resolution);
        let mut body = serde_json::json!({
            "model": self.config.model,
            "prompt": params.prompt,
            "duration": params.duration,
            "width": width,
            "height": height,
        });
        // 图生视频：分镜出图作首帧（base64 data URL 或公网 URL）；纯文生视频则不下发 image。
        // 首帧大图先压缩到请求体限额内，避免完整 PNG 的 base64（数 MB）触发 new-api 上游 HTTP 413。
        if !params.image.trim().is_empty() {
            body["image"] = serde_json::Value::String(shrink_first_frame(&params.image));
        }
        let resp = self
            .client
            .get()?
            .post(self.videos_url())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            // new-api 网关把上游真实原因吞成固定的 fail_to_fetch_task/upstream 4xx，
            // 这里按状态码 + 关键词翻译成可操作的中文提示（内容审核 / 余额 / 限流）。
            return Err(MediaError::Failed(explain_newapi_submit_error(
                status.as_u16(),
                &text,
            )));
        }
        let parsed: NewApiSubmit = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务创建响应失败: {e}")))?;
        // 优先用 `id`（new-api 轮询句柄）；个别实现 id 缺失才回退 task_id。
        parsed
            .id
            .or(parsed.task_id)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("任务创建响应未返回 id".into()))
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        let url = format!("{}/{}", self.videos_url(), task_id);
        let resp = self
            .client
            .get()?
            .get(&url)
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "查询图生视频任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "图生视频查询 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: NewApiPoll = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析任务状态响应失败: {e}")))?;
        match parsed.status.as_deref() {
            Some("completed") | Some("succeeded") | Some("success") => parsed
                .metadata
                .and_then(|m| m.url)
                .or(parsed.video_url)
                .or(parsed.url)
                .filter(|s| !s.is_empty())
                .map(VideoTaskStatus::Succeeded)
                .ok_or_else(|| MediaError::Failed("任务成功但未返回视频 URL".into())),
            Some("failed") | Some("error") => {
                let msg = parsed
                    .error
                    .or(parsed.message)
                    .or(parsed.fail_reason)
                    .unwrap_or_else(|| "任务失败".into());
                Ok(VideoTaskStatus::Failed(msg))
            }
            // queued / in_progress / 其它 → 继续等
            _ => Ok(VideoTaskStatus::Pending),
        }
    }
}

/// 按画幅比例 + 分辨率档推导视频输出宽高（供 New API createVideoGeneration 的 width/height）。
/// 漫剧默认竖屏 9:16 / 720p → 720×1280；支持 1080p（1080×1920）与横屏 16:9、方形 1:1。
/// 短边取分辨率档（720/1080），长边按比例换算；未知比例回退竖屏。
fn newapi_video_dimensions(ratio: &str, resolution: &str) -> (u32, u32) {
    // 分辨率档 → 短边像素（默认 720p）
    let short = if resolution.contains("1080") {
        1080
    } else if resolution.contains("480") {
        480
    } else {
        720
    };
    // 长边 = 短边 × 长宽比；竖屏短边是宽、横屏短边是高
    let long = short * 16 / 9;
    match ratio.trim() {
        "16:9" => (long, short), // 横屏
        "1:1" => (short, short), // 方形
        _ => (short, long),      // 默认竖屏 9:16
    }
}

/// 把 new-api 中转站的提交失败响应翻译成可操作的中文提示。
///
/// 背景：new-api 的视频中继**只透传上游 HTTP 状态码、丢弃上游错误正文**，
/// 失败时永远回固定的 `{"code":"fail_to_fetch_task","message":"...upstream returned status N..."}`，
/// 用户看不出真实原因。这里按「状态码 + body 关键词」把它翻译成人话，避免误判成程序 bug。
/// 实测结论（doubao-seedance via 某 new-api 中转站）：图生视频 400 绝大多数是**首帧图触发上游内容安全审核**
/// （躺卧/露肤/暗光等易误判）；403 多为网关余额不足；413 为请求体过大；429 为限流。
fn explain_newapi_submit_error(status: u16, body: &str) -> String {
    let low = body.to_ascii_lowercase();
    let is_fail_fetch =
        low.contains("fail_to_fetch_task") || low.contains("upstream returned status");
    let hint = match status {
        // 内容审核误判是图生视频 400 的主因；先匹配显式余额/限流关键词，再落到审核兜底。
        400 if low.contains("insufficient") || low.contains("quota") || body.contains("余额") => {
            "供应商余额不足，请充值后重试"
        }
        400 if is_fail_fetch => {
            "该图被视频供应商拒绝（通常是内容安全审核误判：首帧含躺卧/露肤/暗光等易触发元素）。\
             建议：① 换一张构图规整、光线明亮的分镜图重试；② 或对该镜头改用其他视频供应商（如火山方舟，会明确返回审核原因）"
        }
        400 => "提交被供应商拒绝（参数或内容不被接受）。建议换图或换供应商重试",
        401 | 403 if low.contains("insufficient") || low.contains("quota") || body.contains("余额") => {
            "供应商余额不足或无权调用该模型，请充值 / 确认该 Key 已开通此视频模型"
        }
        403 => "无权调用该视频模型（Key 未开通或越权），请在供应商后台确认权限",
        413 => "请求体过大（首帧图过大）——按理已自动压缩，若仍出现请反馈",
        429 => "供应商限流，请稍后重试",
        _ => "",
    };
    if hint.is_empty() {
        format!("图生视频提交 HTTP {status}: {}", truncate(body, 300))
    } else {
        // 同时保留原始 body 摘要，便于进一步排查（但把"人话"放最前面）
        format!(
            "图生视频提交失败（HTTP {status}）：{hint}。原始响应：{}",
            truncate(body, 200)
        )
    }
}

/// 把首帧图压到中转站（new-api）请求体限额内，避免大图 base64 触发 HTTP 413。
/// 逻辑：data URL 解码 → 等比缩到长边逐档（1280/1024/768/512）→ 重编码 JPEG(q80) →
/// 取首个体积达标的结果重新包成 data URL。等比缩放保持首帧比例（视频画幅随首帧）。
/// 原图已足够小 / 非 base64(公网 URL) / 解码失败 → 原样返回，绝不阻断提交。
fn shrink_first_frame(data_url: &str) -> String {
    use base64::Engine;
    // 原图字节小于此值（base64 后约 470KB）直接用，省 CPU 也不劣化画质
    const SKIP_BELOW: usize = 350 * 1024;
    // 压缩后 JPEG 目标上限（base64 后约 530KB，留足余量避开常见 1MB / 512KB body 限额）
    const TARGET_BYTES: usize = 400 * 1024;

    let Some(comma) = data_url.find(',') else {
        return data_url.to_string();
    };
    // 非 base64 内联（如公网 URL 直传）→ 原样
    if !data_url[..comma].contains("base64") {
        return data_url.to_string();
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data_url[comma + 1..].trim())
    else {
        return data_url.to_string();
    };
    if bytes.len() <= SKIP_BELOW {
        return data_url.to_string();
    }
    let Ok(img) = img::load_from_memory(&bytes) else {
        return data_url.to_string();
    };

    use img::ImageEncoder;
    for max_edge in [1280u32, 1024, 768, 512] {
        let scaled = img.thumbnail(max_edge, max_edge); // 等比缩放（不放大），保持首帧比例
        let rgb = scaled.to_rgb8(); // JPEG 不支持 alpha，丢通道
        let (w, h) = (rgb.width(), rgb.height());
        let mut out: Vec<u8> = Vec::new();
        let enc = img::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
        if enc
            .write_image(rgb.as_raw(), w, h, img::ExtendedColorType::Rgb8)
            .is_err()
        {
            return data_url.to_string();
        }
        // 达标即用；最小档(512)即便略超也用它兜底（已尽力压缩）
        if out.len() <= TARGET_BYTES || max_edge == 512 {
            let nb64 = base64::engine::general_purpose::STANDARD.encode(&out);
            return format!("data:image/jpeg;base64,{nb64}");
        }
    }
    data_url.to_string()
}

// ─────────────────────────── 协议识别与统一入口 ───────────────────────────
//
// 搬自 StoryLoom `services/media.rs`：此前「提交」「长镜头续写」「重启后续跑」「首尾帧能力」
// 四处各写一串 contains 判断，续跑那处只认 3 家（海螺 / Vidu / 智谱的任务续跑时被当成
// 火山方舟轮询）。收成一个 `detect`，四处同源。

/// 视频协议。每家的提交与轮询接口都不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VideoProtocol {
    /// 火山方舟 Seedance（`/contents/generations/tasks`）—— 识别不出时的兜底
    Ark,
    /// 海螺 MiniMax（提交 → 轮询 → `files/retrieve` 取下载地址）
    Minimax,
    /// Vidu（阿里百炼 DashScope 异步任务）
    Vidu,
    /// 智谱 CogVideoX（`generations` → `async-result`）
    Zhipu,
    /// 硅基流动 Wan（`video/submit` + `video/status`）
    SiliconFlow,
    /// New API 中转站统一视频协议（`/video/generations` 提交 → `/{id}` 轮询）
    NewApi,
}

impl VideoProtocol {
    /// 按端点与供应商 `extra`（JSON 字符串，可为空）识别协议。
    ///
    /// 顺序有讲究：302.AI 这类聚合站走透传地址（`api.302.ai/minimaxi/…`），靠路径子串命中对应厂商协议；
    /// New API 中转站排在厂商子串**之后**判，与 StoryLoom 原分发顺序一致。
    /// New API 中转站**只能**靠 `extra` 里 `{"video_api":"newapi"}` 显式指定（不按域名猜，见 `is_newapi`）。
    pub fn detect(endpoint: &str, extra: &str) -> Self {
        let ep = endpoint.to_ascii_lowercase();
        if ep.contains("minimaxi") {
            Self::Minimax
        } else if ep.contains("dashscope") {
            Self::Vidu
        } else if ep.contains("bigmodel") || ep.contains("zhipu") {
            Self::Zhipu
        } else if ep.contains("siliconflow") {
            Self::SiliconFlow
        } else if is_newapi(&ep, extra) {
            Self::NewApi
        } else {
            Self::Ark
        }
    }
}

/// 是否走 New API 中转站协议：**只认 `extra` 显式标记** `{"video_api":"newapi"}`。
///
/// 🔴 不按域名猜：New API / one-api 是通用的中转架构，站点千千万，写死某家域名等于让 crate 认识品牌。
/// 需要它的预置用 [`crate::preset::ProviderPreset::with_default_extra`] 带上这个标记（新建配置时写进 extra），
/// 存量配置由应用自己迁移补标记。
fn is_newapi(_endpoint: &str, extra: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(extra)
        .ok()
        .and_then(|v| {
            v.get("video_api")
                .and_then(|x| x.as_str())
                .map(|s| s == "newapi")
        })
        .unwrap_or(false)
}

/// 当前视频供应商是否支持「首尾帧」（给起点图 + 终点图，AI 补中间过程）。
///
/// 已核对官方文档的三家：火山方舟 Seedance（`content` 里第二个 `image_url` 带 `role: "last_frame"`）、
/// 海螺（顶层 `last_frame_image`，仅 Hailuo-02 及以上）、智谱（`image_url` 传 `[首帧, 尾帧]`）。
///
/// 🔴 与 [`VideoProtocol::detect`] 的判断顺序**不同**：这里先排除中转站，
/// 否则 `relay.example/minimaxi` 这类中转地址会被误判成支持，下发一个上游可能不认的字段。
/// 未列入的一律返回 false —— 宁可少给功能，也不发会被忽略或报错的字段。
pub fn supports_last_frame(endpoint: &str, extra: &str) -> bool {
    let ep = endpoint.to_ascii_lowercase();
    if is_newapi(&ep, extra) || ep.contains("siliconflow") || ep.contains("dashscope") {
        return false;
    }
    // 海螺 / 智谱支持；其余走火山方舟 Seedance，同样支持
    true
}

/// 把视频供应商的原始错误翻译成可操作的中文（审核拦截最常见），其余原样截断。
pub fn explain_error(raw: &str) -> String {
    let low = raw.to_ascii_lowercase();
    // 首帧被判真人 / 隐私信息（图生视频最常见的审核拦截）
    if low.contains("privacyinformation")
        || low.contains("sensitivecontent")
        || low.contains("inputimagesensitive")
        || low.contains("真人")
        || low.contains("人脸")
    {
        return format!(
            "首帧图被视频供应商判为「真人/敏感内容」而拒绝（多为写实脸误判）。\
建议：① 把作品画风换成「韩漫·写实身材+动漫脸」或「赛璐璐·保底」再重出该镜首帧；\
② 或改用审核更宽松的视频供应商（如 Vidu）重试。原始错误：{}",
            truncate(raw, 300)
        );
    }
    // 其它审核类拦截
    if low.contains("sensitive")
        || low.contains("审核")
        || low.contains("违规")
        || low.contains("risk")
    {
        return format!(
            "首帧或提示词被内容安全审核拦截。建议换构图规整、画风更风格化的首帧图，或改用其他视频供应商重试。原始错误：{}",
            truncate(raw, 300)
        );
    }
    truncate(raw, 300)
}

/// 按协议分发的统一入口 —— 调用方不必再写一串 `if contains`。
///
/// 实现了 [`VideoProvider`]，可以直接 move 进 `tokio::spawn` 后台轮询。
#[non_exhaustive]
pub enum AnyVideoProvider {
    /// 火山方舟 Seedance
    Ark(ArkVideoProvider),
    /// 海螺 MiniMax
    Minimax(MinimaxVideoProvider),
    /// Vidu（阿里百炼）
    Vidu(ViduVideoProvider),
    /// 智谱 CogVideoX
    Zhipu(ZhipuVideoProvider),
    /// 硅基流动 Wan
    SiliconFlow(SiliconFlowVideoProvider),
    /// New API 中转站
    NewApi(NewApiVideoProvider),
}

impl AnyVideoProvider {
    /// 按 [`VideoProtocol::detect`] 选实现。`extra` 为供应商的额外配置 JSON（可为空串）。
    pub fn from_config(config: VideoGenConfig, extra: &str) -> Self {
        Self::from_config_with(config, extra, &super::MediaHttp::default())
    }

    /// 同 [`Self::from_config`]，带调用方的 HTTP 底座（代理 / 证书）。
    pub fn from_config_with(config: VideoGenConfig, extra: &str, http: &super::MediaHttp) -> Self {
        match VideoProtocol::detect(&config.endpoint, extra) {
            VideoProtocol::Minimax => Self::Minimax(MinimaxVideoProvider::with_http(config, http)),
            VideoProtocol::Vidu => Self::Vidu(ViduVideoProvider::with_http(config, http)),
            VideoProtocol::Zhipu => Self::Zhipu(ZhipuVideoProvider::with_http(config, http)),
            VideoProtocol::SiliconFlow => {
                Self::SiliconFlow(SiliconFlowVideoProvider::with_http(config, http))
            }
            VideoProtocol::NewApi => Self::NewApi(NewApiVideoProvider::with_http(config, http)),
            VideoProtocol::Ark => Self::Ark(ArkVideoProvider::with_http(config, http)),
        }
    }

    /// 实际选中的协议。
    pub fn protocol(&self) -> VideoProtocol {
        match self {
            Self::Ark(_) => VideoProtocol::Ark,
            Self::Minimax(_) => VideoProtocol::Minimax,
            Self::Vidu(_) => VideoProtocol::Vidu,
            Self::Zhipu(_) => VideoProtocol::Zhipu,
            Self::SiliconFlow(_) => VideoProtocol::SiliconFlow,
            Self::NewApi(_) => VideoProtocol::NewApi,
        }
    }
}

impl VideoProvider for AnyVideoProvider {
    async fn submit(&self, params: &VideoGenParams) -> Result<String, MediaError> {
        match self {
            Self::Ark(p) => p.submit(params).await,
            Self::Minimax(p) => p.submit(params).await,
            Self::Vidu(p) => p.submit(params).await,
            Self::Zhipu(p) => p.submit(params).await,
            Self::SiliconFlow(p) => p.submit(params).await,
            Self::NewApi(p) => p.submit(params).await,
        }
    }

    async fn poll(&self, task_id: &str) -> Result<VideoTaskStatus, MediaError> {
        match self {
            Self::Ark(p) => p.poll(task_id).await,
            Self::Minimax(p) => p.poll(task_id).await,
            Self::Vidu(p) => p.poll(task_id).await,
            Self::Zhipu(p) => p.poll(task_id).await,
            Self::SiliconFlow(p) => p.poll(task_id).await,
            Self::NewApi(p) => p.poll(task_id).await,
        }
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    /// 各预置地址识别到的协议（302.AI 靠透传路径命中厂商协议）。
    /// 🔴 crate 不按域名认 New API：没有 extra 标记的中转站地址按兜底（火山方舟）处理。
    #[test]
    fn newapi_requires_explicit_extra() {
        assert_eq!(
            VideoProtocol::detect("https://relay.example.com/v1", ""),
            VideoProtocol::Ark
        );
        assert_eq!(
            VideoProtocol::detect("https://relay.example.com/v1", r#"{"video_api":"newapi"}"#),
            VideoProtocol::NewApi
        );
    }

    #[test]
    fn detect_by_endpoint() {
        let cases = [
            (
                "https://ark.cn-beijing.volces.com/api/v3",
                VideoProtocol::Ark,
            ),
            ("https://api.minimaxi.com/v1", VideoProtocol::Minimax),
            ("https://api.302.ai/minimaxi/v1", VideoProtocol::Minimax),
            ("https://dashscope.aliyuncs.com/api/v1", VideoProtocol::Vidu),
            ("https://open.bigmodel.cn/api/paas/v4", VideoProtocol::Zhipu),
            ("https://api.302.ai/zhipu/api/paas/v4", VideoProtocol::Zhipu),
            ("https://api.siliconflow.cn/v1", VideoProtocol::SiliconFlow),
            ("https://unknown.example/v1", VideoProtocol::Ark),
        ];
        for (ep, want) in cases {
            assert_eq!(VideoProtocol::detect(ep, ""), want, "{ep}");
        }
        assert_eq!(
            VideoProtocol::detect("https://relay.example/v1", r#"{"video_api":"newapi"}"#),
            VideoProtocol::NewApi,
            "extra 显式标记"
        );
    }

    /// 🔴 每条视频预置的地址都要识别到预期协议 —— 改预置地址时这条会红。
    #[test]
    fn every_video_preset_detects_a_real_protocol() {
        for p in crate::preset::presets_for(crate::Kind::Video) {
            let Some(url) = p.base_url else { continue };
            let got = VideoProtocol::detect(url, "");
            let want = match p.key {
                "seedance" => VideoProtocol::Ark,
                "minimax_video" | "ai302_minimax_video" => VideoProtocol::Minimax,
                "vidu_video" => VideoProtocol::Vidu,
                "zhipu_video" | "ai302_zhipu_video" => VideoProtocol::Zhipu,
                "siliconflow_video" => VideoProtocol::SiliconFlow,
                other => panic!("新增视频预置 {other} 要在这里登记预期协议"),
            };
            assert_eq!(got, want, "{}", p.key);
        }
    }

    // ── 以下搬自 StoryLoom media.rs 的首尾帧测试 ──

    #[test]
    fn supports_last_frame_for_verified_providers() {
        assert!(
            supports_last_frame("https://ark.cn-beijing.volces.com/api/v3", ""),
            "火山方舟"
        );
        assert!(
            supports_last_frame("https://api.minimaxi.com/v1", ""),
            "MiniMax 海螺"
        );
        assert!(
            supports_last_frame("https://open.bigmodel.cn/api/paas/v4", ""),
            "智谱 bigmodel"
        );
        assert!(
            supports_last_frame("https://example.com/zhipu/v1", ""),
            "智谱 zhipu 别名"
        );
    }

    #[test]
    fn rejects_unverified_providers() {
        assert!(
            !supports_last_frame("https://dashscope.aliyuncs.com/api/v1", ""),
            "Vidu"
        );
        assert!(
            !supports_last_frame("https://api.siliconflow.cn/v1", ""),
            "硅基流动"
        );
        assert!(
            !supports_last_frame("https://relay.example.com/v1", r#"{"video_api":"newapi"}"#),
            "new-api 中转站"
        );
        assert!(
            !supports_last_frame("https://any-relay.example/v1", r#"{"video_api":"newapi"}"#),
            "extra 标记的 newapi 中转站"
        );
    }

    #[test]
    fn relay_check_precedes_vendor_substring() {
        assert!(!supports_last_frame(
            "https://relay.example/minimaxi/v1",
            r#"{"video_api":"newapi"}"#
        ));
        assert!(!supports_last_frame(
            "https://relay.example/zhipu/v1",
            r#"{"video_api":"newapi"}"#
        ));
    }

    #[test]
    fn endpoint_matching_is_case_insensitive() {
        assert!(supports_last_frame("https://API.MINIMAXI.COM/v1", ""));
        assert!(!supports_last_frame(
            "https://DASHSCOPE.aliyuncs.com/api/v1",
            ""
        ));
        assert_eq!(
            VideoProtocol::detect("https://API.MINIMAXI.COM/v1", ""),
            VideoProtocol::Minimax
        );
    }

    #[test]
    fn explain_error_translates_moderation() {
        assert!(explain_error("InputImageSensitiveContentDetected")
            .starts_with("首帧图被视频供应商判为"));
        assert!(explain_error("risk control").starts_with("首帧或提示词被内容安全审核拦截"));
        assert_eq!(explain_error("timeout"), "timeout");
    }
}
