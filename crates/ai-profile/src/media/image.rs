//! 漫剧化 V1-B · 文生图供应商抽象（ImageProvider）。
//!
//! 与 chat 的 `OpenAiCompatProvider` 区分：文生图是 OpenAI images generations 风格
//! （`POST {endpoint}/images/generations`），响应给图 URL（非 chat 的流式文本）。
//! 首个实现 `OpenAiImageProvider` 适配硅基流动（SiliconFlow）—— Kolors/FLUX 等模型。
//!
//! 关键：硅基流动返回的图 URL **仅 1 小时有效**，故 generate 内即时下载为二进制返回，
//! 由调用方（StoryboardService）落地到本地，不存 URL。

use serde::{Deserialize, Serialize};

use super::truncate;
use super::MediaError;

/// 文生图调用配置（含解密后的 Key，仅 Rust 内部传递）
#[derive(Debug, Clone)]
pub struct ImageGenConfig {
    /// 端点（base_url；火山语音为完整接口路径）
    pub endpoint: String,
    /// 模型 id
    pub model: String,
    /// 明文密钥（仅在内存中传递，不落盘、不写日志）
    pub api_key: String,
}

/// 文生图入参
#[derive(Debug, Clone)]
pub struct ImageGenParams {
    /// 出图提示词（分镜 image_prompt，已含画面+角色锚点+画风）
    pub prompt: String,
    /// 反向提示词（可空）
    pub negative_prompt: String,
    /// 随机种子（同角色复用同 seed 求一致性；None 由服务端随机）
    pub seed: Option<i64>,
    /// 尺寸「宽x高」（漫剧竖屏默认 720x1280 ≈ 9:16）
    pub size: String,
    /// 参考图（漫剧化 V3·M6 → V5 多图）：base64 data URL 或 URL 列表，作多参考图锁形象/场景。
    /// 空=纯文生图；1 张=单图 img2img（Kolors 等）；≥2 张=多图融合（Seedream 4.0 支持最多 10 张，
    /// 如「角色参考图 + 同场景定场图」一起喂，锁角色又锁背景）。
    pub images: Vec<String>,
}

impl Default for ImageGenParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative_prompt: String::new(),
            seed: None,
            size: "720x1280".to_string(), // 9:16 竖屏（Kolors 支持）
            images: Vec::new(),
        }
    }
}

/// 出图结果：图二进制 + 实际使用的 seed（供复现/一致性）
#[derive(Debug, Clone)]
pub struct ImageResult {
    /// 图片二进制（已下载，不外泄临时 URL）
    pub bytes: Vec<u8>,
    /// 图片格式扩展名（png/jpg/webp，据 content-type 或 URL 推断，缺省 png）
    pub ext: String,
    /// 实际使用的随机种子（供复现 / 一致性）
    pub seed: Option<i64>,
}

/// 文生图供应商抽象。首个实现走 OpenAI images 兼容；trait 留扩展点（可灵/即梦各自 REST 后续加）。
#[allow(async_fn_in_trait)]
pub trait ImageProvider {
    /// 文生图，返回图二进制（内部已下载，URL 不外泄）。
    async fn generate(&self, params: &ImageGenParams) -> Result<ImageResult, MediaError>;
}

/// OpenAI images 兼容供应商（硅基流动 `/v1/images/generations` 等）
pub struct OpenAiImageProvider {
    client: super::http::MediaClient,
    config: ImageGenConfig,
}

impl OpenAiImageProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: ImageGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: ImageGenConfig, http: &super::MediaHttp) -> Self {
        // 出图专用客户端：connect + read 双超时（非整体超时）。整体超时会在算图逼近上限时
        // 误杀「上游已算完并已计费」的请求，造成「有消费记录却没图」，见 shared::http::image_client。
        Self {
            client: super::http::image_client(http),
            config,
        }
    }

    /// 拼出 images/generations 完整地址（容忍端点是否已含该路径/尾斜杠）
    fn images_url(&self) -> String {
        let e = self.config.endpoint.trim_end_matches('/');
        if e.ends_with("/images/generations") {
            e.to_string()
        } else {
            format!("{e}/images/generations")
        }
    }
}

/// 请求体（OpenAI images 兼容 + 硅基流动扩展字段；空值不序列化）
#[derive(Serialize)]
struct ImageRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    negative_prompt: &'a str,
    image_size: &'a str,
    batch_size: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    /// 参考图（None=纯文生图）：单张序列化为字符串（兼容 Kolors 等单图 img2img），
    /// 多张序列化为数组（Seedream 4.0 多图融合）。生命周期无关，故用 owned Value。
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ImageResponse {
    #[serde(default)]
    images: Vec<ImageItem>,
    #[serde(default)]
    seed: Option<i64>,
    /// OpenAI 官方风格用 data[].url/b64_json；硅基流动用 images[].url。两者都兜底。
    #[serde(default)]
    data: Vec<ImageItem>,
}

#[derive(Deserialize, Default)]
struct ImageItem {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    b64_json: Option<String>,
}

impl ImageProvider for OpenAiImageProvider {
    async fn generate(&self, params: &ImageGenParams) -> Result<ImageResult, MediaError> {
        // 参考图：0 张→不带；1 张→字符串（Kolors 单图兼容）；≥2 张→数组（Seedream 多图融合）
        let image = match params.images.as_slice() {
            [] => None,
            [one] => Some(serde_json::Value::String(one.clone())),
            many => Some(serde_json::Value::Array(
                many.iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            )),
        };
        let body = ImageRequest {
            model: &self.config.model,
            prompt: &params.prompt,
            negative_prompt: &params.negative_prompt,
            image_size: &params.size,
            batch_size: 1,
            seed: params.seed,
            image,
        };
        let resp = self
            .client
            .get()?
            .post(self.images_url())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            // reqwest 0.12 的 {e} 只到 URL 为止，底层真因（超时/连接重置/DNS/TLS）在 source 链里，
            // 用 describe_reqwest_error 提取分类 + 底层原因，避免日志「很秃」定位不到根因。
            .map_err(|e| {
                MediaError::Failed(format!(
                    "出图请求失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            // 模型不存在/未开通（火山方舟等常见 404 InvalidEndpointOrModel.NotFound）→ 给可操作指引：
            // 模型 ID 是账号 + 控制台开通的（带日期后缀），默认值未必在你账号下，需到设置改成控制台一致的 ID。
            let model_not_found = status.as_u16() == 404
                || text.contains("InvalidEndpointOrModel")
                || text.contains("does not exist")
                || (text.contains("model") && text.contains("Not Found"));
            if model_not_found {
                return Err(MediaError::Failed(format!(
                    "出图失败：模型「{}」不存在或未开通。请到 设置 → 生图 把模型改成你在服务商控制台已开通的模型 ID（火山方舟 Seedream 带日期后缀，需与控制台「开通模型」一致）。原始响应 HTTP {}: {}",
                    self.config.model,
                    status.as_u16(),
                    truncate(&text, 200)
                )));
            }
            // new-api 中转站把上游真因吞成固定 fail_to_fetch_task/4xx，
            // 这里按状态码+关键词翻译成可操作提示（内容审核 / 余额 / 限流），避免误判成程序 bug。
            return Err(MediaError::Failed(explain_image_http_error(
                status.as_u16(),
                &text,
            )));
        }

        let parsed: ImageResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析出图响应失败: {e}")))?;

        // 取第一张（images 优先，其次 OpenAI 风格 data）
        // 返回 2xx 但 data 为空：new-api 中转站在**内容审核拦截**或额度异常时常如此，
        // 给出可操作提示而非笼统"未返回图片"，避免用户误以为程序坏了。
        let item = parsed
            .images
            .into_iter()
            .next()
            .or_else(|| parsed.data.into_iter().next())
            .ok_or_else(|| {
                MediaError::Failed(
                    "出图失败：供应商返回成功但未给出图片（通常是内容安全审核拦截，或额度异常）。\
                     建议：① 调整提示词 / 参考图，避开露肤、暴力、政治等敏感元素重试；\
                     ② 或改用其他生图供应商（如火山方舟）"
                        .into(),
                )
            })?;

        // b64_json（OpenAI 风格）或 url（硅基流动）二选一
        if let Some(b64) = item.b64_json.filter(|s| !s.is_empty()) {
            let bytes = base64_decode(&b64)?;
            // 校验确实是图：base64 字段可能是空串解码出的 0 字节，或一段被误塞的错误文本
            validate_image_bytes(&bytes, "供应商内联返回(b64_json)")?;
            return Ok(ImageResult {
                bytes,
                ext: "png".into(),
                seed: parsed.seed,
            });
        }
        let url = item
            .url
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("出图响应缺少图片 URL".into()))?;

        // URL 仅 1 小时有效 → 即时下载为二进制（含校验 + 失败重试一次，不重复计费）
        let (bytes, ext) = download_image_checked(self.client.get()?, &url, "下载出图结果").await?;
        Ok(ImageResult {
            bytes,
            ext,
            seed: parsed.seed,
        })
    }
}

// ───────────────────────── 通义万相（DashScope 异步）文生图 ─────────────────────────

/// 通义万相（阿里百炼 DashScope）文生图。**异步两段式**（与 OpenAI 同步出图不同）：
///   - 提交 POST {base}/services/aigc/text2image/image-synthesis
///     头 X-DashScope-Async: enable；body {model, input:{prompt, negative_prompt?},
///     parameters:{size:"宽*高", n:1, seed?}} → {output:{task_id, task_status}}
///   - 轮询 GET {base}/tasks/{task_id} → {output:{task_status, results:[{url}]}}
///     task_status: PENDING / RUNNING / SUCCEEDED / FAILED / CANCELED / UNKNOWN
///
/// 端点形如 `https://dashscope.aliyuncs.com/api/v1`（注意非 compatible-mode）。
/// 结果 URL 有时效 → 即时下载为二进制返回（与 OpenAiImageProvider 一致）。
/// 注：当前仅纯文生图；params.image（img2img 锁角色）暂不下发（Wan t2i 不支持，后续可加 imageedit 模型）。
pub struct DashScopeImageProvider {
    client: super::http::MediaClient,
    config: ImageGenConfig,
}

impl DashScopeImageProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: ImageGenConfig) -> Self {
        Self::with_http(config, &super::MediaHttp::default())
    }

    /// 用调用方的 HTTP 底座（代理 / 证书）构造；超时策略仍由本 crate 施加。见 [`super::MediaHttp`]。
    pub fn with_http(config: ImageGenConfig, http: &super::MediaHttp) -> Self {
        // 出图专用客户端（同 OpenAiImageProvider）：connect + read 双超时，不用整体超时。
        // 本家是异步两段式（submit/poll 都是快请求），read 超时对它只是更宽松的兜底，无副作用。
        Self {
            client: super::http::image_client(http),
            config,
        }
    }

    fn base(&self) -> String {
        self.config.endpoint.trim_end_matches('/').to_string()
    }

    /// 提交异步出图任务，返回 task_id。
    async fn submit(&self, params: &ImageGenParams) -> Result<String, MediaError> {
        // 百炼尺寸用「宽*高」（ImageGenParams.size 是「宽x高」）；负向提示为空则不带。
        let size = params.size.replace(['x', 'X'], "*");
        let mut input = serde_json::json!({ "prompt": params.prompt });
        if !params.negative_prompt.is_empty() {
            input["negative_prompt"] = serde_json::Value::String(params.negative_prompt.clone());
        }
        let mut parameters = serde_json::json!({ "size": size, "n": 1 });
        if let Some(seed) = params.seed {
            parameters["seed"] = serde_json::json!(seed);
        }
        let body = serde_json::json!({
            "model": self.config.model,
            "input": input,
            "parameters": parameters,
        });
        let resp = self
            .client
            .get()?
            .post(format!(
                "{}/services/aigc/text2image/image-synthesis",
                self.base()
            ))
            .bearer_auth(&self.config.api_key)
            .header("X-DashScope-Async", "enable")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交文生图任务失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "文生图提交 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: DashImageResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析文生图任务创建响应失败: {e}")))?;
        parsed
            .output
            .and_then(|o| o.task_id)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                MediaError::Failed(format!(
                    "文生图任务创建未返回 task_id{}",
                    parsed.message.map(|m| format!(": {m}")).unwrap_or_default()
                ))
            })
    }

    /// 轮询任务直至成功（返回图 URL）或失败；兜底约 300s 防供应商挂起永久阻塞。
    async fn poll_until_url(&self, task_id: &str) -> Result<String, MediaError> {
        // 最多 100 次 × 3s ≈ 300s（万相 plus / 高分辨率高峰期可能超 3 分钟）
        for _ in 0..100 {
            let resp = self
                .client
                .get()?
                .get(format!("{}/tasks/{}", self.base(), task_id))
                .bearer_auth(&self.config.api_key)
                .send()
                .await
                .map_err(|e| {
                    MediaError::Failed(format!(
                        "查询文生图任务失败: {}",
                        super::http::describe_reqwest_error(&e)
                    ))
                })?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(MediaError::Failed(format!(
                    "文生图查询 HTTP {}: {}",
                    status.as_u16(),
                    truncate(&text, 300)
                )));
            }
            let parsed: DashImageResponse = resp
                .json()
                .await
                .map_err(|e| MediaError::Failed(format!("解析文生图任务状态失败: {e}")))?;
            let out = parsed.output.unwrap_or_default();
            match out.task_status.as_deref() {
                Some("SUCCEEDED") => {
                    return out
                        .results
                        .into_iter()
                        .find_map(|r| r.url.filter(|s| !s.is_empty()))
                        .ok_or_else(|| MediaError::Failed("文生图成功但未返回图片 URL".into()));
                }
                Some("FAILED") | Some("CANCELED") | Some("UNKNOWN") => {
                    return Err(MediaError::Failed(format!(
                        "文生图任务失败: {}",
                        out.message.unwrap_or_else(|| "未知原因".into())
                    )));
                }
                // PENDING / RUNNING / 其它 → 继续等
                _ => tokio::time::sleep(std::time::Duration::from_secs(3)).await,
            }
        }
        Err(MediaError::Failed("文生图任务超时（>300s 未完成）".into()))
    }
}

#[derive(Deserialize, Default)]
struct DashImageOutput {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    task_status: Option<String>,
    #[serde(default)]
    results: Vec<DashImageItem>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize, Default)]
struct DashImageItem {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Deserialize)]
struct DashImageResponse {
    #[serde(default)]
    output: Option<DashImageOutput>,
    #[serde(default)]
    message: Option<String>,
}

impl ImageProvider for DashScopeImageProvider {
    async fn generate(&self, params: &ImageGenParams) -> Result<ImageResult, MediaError> {
        let task_id = self.submit(params).await?;
        let url = self.poll_until_url(&task_id).await?;
        // 结果 URL 有时效 → 即时下载为二进制（与 OpenAiImageProvider 一致，含校验 + 重试一次）
        let (bytes, ext) =
            download_image_checked(self.client.get()?, &url, "下载文生图结果").await?;
        Ok(ImageResult {
            bytes,
            ext,
            seed: params.seed,
        })
    }
}

// ───────────────────────── 文生图分发包装 ─────────────────────────

/// 生图协议。与视频的 [`super::video::VideoProtocol`]、配音的 [`super::tts::TtsProtocol`] 对称：
/// 调用方只想知道「这个地址走哪套协议」时用它（如只实现了 OpenAI images 的下游据此过滤预置），
/// 不必为此构造 provider。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ImageProtocol {
    /// OpenAI `/images/generations` 兼容（同步）—— 识别不出时的兜底
    OpenAi,
    /// 通义万相（DashScope 异步任务：提交 → 轮询 → 下载）
    DashScope,
}

impl ImageProtocol {
    /// 按端点识别：含 `dashscope` → 通义万相，其余 → OpenAI images 兼容。
    pub fn detect(endpoint: &str) -> Self {
        if endpoint.to_ascii_lowercase().contains("dashscope") {
            Self::DashScope
        } else {
            Self::OpenAi
        }
    }
}

/// 文生图供应商分发：按 [`ImageProtocol::detect`] 选具体实现。
/// 加新家（可灵图等）= 实现一个 ImageProvider + 在 `ImageProtocol` 加一个变体。
pub enum AnyImageProvider {
    /// OpenAI `/images/generations` 兼容（火山方舟 Seedream、硅基流动、聚合站）
    OpenAi(OpenAiImageProvider),
    /// 通义万相（DashScope 异步任务）
    DashScope(DashScopeImageProvider),
}

impl AnyImageProvider {
    /// 按端点选实现：含 `dashscope` → 通义万相，其余 → OpenAI images 兼容。
    pub fn from_config(config: ImageGenConfig) -> Self {
        Self::from_config_with(config, &super::MediaHttp::default())
    }

    /// 同 [`Self::from_config`]，带调用方的 HTTP 底座（代理 / 证书）。
    pub fn from_config_with(config: ImageGenConfig, http: &super::MediaHttp) -> Self {
        match ImageProtocol::detect(&config.endpoint) {
            ImageProtocol::DashScope => {
                Self::DashScope(DashScopeImageProvider::with_http(config, http))
            }
            ImageProtocol::OpenAi => Self::OpenAi(OpenAiImageProvider::with_http(config, http)),
        }
    }

    /// 文生图（内部已下载，URL 不外泄）。
    pub async fn generate(&self, params: &ImageGenParams) -> Result<ImageResult, MediaError> {
        match self {
            Self::OpenAi(p) => p.generate(params).await,
            Self::DashScope(p) => p.generate(params).await,
        }
    }
}

/// 把生图供应商（尤其 new-api 中转站）的 HTTP 错误翻译成可操作中文提示。
///
/// new-api 把上游真因吞成固定的 `fail_to_fetch_task / upstream returned status N`，
/// 用户看不出原因。按「状态码 + body 关键词」分类：
/// - 400 + 余额/quota → 余额不足；400 其余（含 fail_to_fetch_task / 审核关键词）→ 内容审核误判；
/// - 401/403 + 余额/quota → 余额或权限；403 → 模型未开通；413 → 图过大；429 → 限流。
fn explain_image_http_error(status: u16, body: &str) -> String {
    let low = body.to_ascii_lowercase();
    let is_balance = low.contains("insufficient") || low.contains("quota") || body.contains("余额");
    let is_moderation = low.contains("sensitive")
        || low.contains("moderation")
        || low.contains("risk")
        || low.contains("content_policy")
        || body.contains("审核")
        || body.contains("违规");
    let hint = match status {
        400 if is_balance => "供应商余额不足，请充值后重试",
        400 if is_moderation || low.contains("fail_to_fetch_task") || low.contains("upstream returned status") => {
            "该出图请求被供应商拒绝（通常是内容安全审核：提示词/参考图含露肤、暴力、政治等敏感元素）。\
             建议：① 调整提示词与参考图重试；② 或改用其他生图供应商（如火山方舟）"
        }
        400 => "出图被供应商拒绝（参数或内容不被接受）。建议调整提示词/尺寸或换供应商重试",
        401 | 403 if is_balance => "供应商余额不足或无权调用该模型，请充值 / 确认该 Key 已开通此生图模型",
        403 => "无权调用该生图模型（Key 未开通或越权），请在供应商后台确认权限",
        413 => "请求体过大（参考图过大），请压缩参考图后重试",
        429 => "供应商限流，请稍后重试",
        _ => "",
    };
    if hint.is_empty() {
        format!("出图 HTTP {status}: {}", truncate(body, 300))
    } else {
        format!(
            "出图失败（HTTP {status}）：{hint}。原始响应：{}",
            truncate(body, 200)
        )
    }
}

/// 合法出图的最小字节数。真实出图（720×1280）至少几十 KB；小于 1KB 必然不是图，
/// 而是中转站/CDN 在异常时返回的空 body、错误页或占位内容。
/// `pub`：调用方落盘后的对账自愈（如 StoryLoom 的 `reconcile_chapter_images`）用同一阈值判定坏图，
/// 保持「下载时拒收」与「事后体检」口径一致，避免两处各写一份数字漂移。
pub const MIN_IMAGE_BYTES: usize = 1024;

/// 校验拿到的字节确实是一张图，**杜绝「空图落盘却标成功」**。
///
/// 为什么必须校验（真实事故）：中转站（new-api 系）在上游异常时会返回
/// `200 + 空 body`，图 URL 回源失败时 CDN 也可能给 `200 + 0 字节`。原先代码不校验直接
/// `fs::write`，于是 0 字节文件落盘 → 分镜标 `done` → 任务记录绿色「完成」，但前端 `<img>`
/// 读到的是空 data URL，表现为「显示成功却没有图」，且重进也不自愈。
///
/// 校验两层：① 长度下限；② 图片格式魔数头（PNG/JPEG/WEBP/GIF/BMP）—— 后者能识破
/// 「返回一段 JSON 错误文本但 HTTP 200」这类伪装。
fn validate_image_bytes(bytes: &[u8], source: &str) -> Result<(), MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Failed(format!(
            "出图失败：{source}返回了空内容（0 字节）。通常是供应商内容审核拦截、额度异常或中转站回源失败。\
             建议：① 调整提示词/参考图重试；② 或改用其他生图供应商"
        )));
    }
    if bytes.len() < MIN_IMAGE_BYTES {
        return Err(MediaError::Failed(format!(
            "出图失败：{source}返回的内容只有 {} 字节，不是有效图片（通常是错误页或占位内容）。请重试或更换生图供应商",
            bytes.len()
        )));
    }
    let is_png = bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
    let is_jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
    // WEBP：RIFF....WEBP（第 8..12 字节是 "WEBP"）
    let is_webp = bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP";
    let is_gif = bytes.starts_with(b"GIF8");
    let is_bmp = bytes.starts_with(b"BM");
    if !(is_png || is_jpeg || is_webp || is_gif || is_bmp) {
        // 头部往往是 JSON 错误体，截一小段进错误信息便于定位真因
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(120)]).to_string();
        return Err(MediaError::Failed(format!(
            "出图失败：{source}返回的内容不是有效图片格式（非 PNG/JPEG/WEBP）。内容开头：{}",
            truncate(&head, 120)
        )));
    }
    Ok(())
}

/// 下载结果图并校验，失败自动重试一次（短退避 1s）。
///
/// 为什么重试：图已经算完并**已经计费**，此时若因网络抖动/CDN 瞬时 5xx 下载失败就判整单失败，
/// 用户得重出一张、再花一次钱。重试一次即可覆盖绝大多数瞬时抖动，成本为零（不重复调生图 API）。
/// 注意只重试「下载」这一步，不重试生图本身（那会重复计费）。
async fn download_image_checked(
    client: &reqwest::Client,
    url: &str,
    label: &str,
) -> Result<(Vec<u8>, String), MediaError> {
    let mut last_err: Option<MediaError> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            log::warn!("{label}下载失败，重试第 {attempt} 次（图已生成，重试不重复计费）");
        }
        match download_image_once(client, url, label).await {
            Ok(v) => return Ok(v),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| MediaError::Failed(format!("{label}下载失败"))))
}

/// 单次下载 + 校验（供 `download_image_checked` 重试包装）
async fn download_image_once(
    client: &reqwest::Client,
    url: &str,
    label: &str,
) -> Result<(Vec<u8>, String), MediaError> {
    let resp = client.get(url).send().await.map_err(|e| {
        MediaError::Failed(format!(
            "{label}失败: {}",
            super::http::describe_reqwest_error(&e)
        ))
    })?;
    if !resp.status().is_success() {
        return Err(MediaError::Failed(format!(
            "{label} HTTP {}",
            resp.status().as_u16()
        )));
    }
    let ext = ext_from_url_or_ct(url, resp.headers());
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| MediaError::Failed(format!("读取出图字节失败: {e}")))?
        .to_vec();
    validate_image_bytes(&bytes, "结果图下载")?;
    Ok((bytes, ext))
}

/// 据 URL 后缀 / content-type 推断图片扩展名（缺省 png）
fn ext_from_url_or_ct(url: &str, headers: &reqwest::header::HeaderMap) -> String {
    let lower = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    for e in ["png", "jpeg", "jpg", "webp"] {
        if lower.ends_with(&format!(".{e}")) {
            return if e == "jpeg" { "jpg".into() } else { e.into() };
        }
    }
    if let Some(ct) = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        if ct.contains("png") {
            return "png".into();
        } else if ct.contains("webp") {
            return "webp".into();
        } else if ct.contains("jpeg") || ct.contains("jpg") {
            return "jpg".into();
        }
    }
    "png".into()
}

/// base64 解码（复用 base64 crate，与 license/cache 同源依赖）
fn base64_decode(s: &str) -> Result<Vec<u8>, MediaError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .map_err(|e| MediaError::Failed(format!("图片 base64 解码失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一段够长的假图数据：前缀魔数 + 填充到 MIN_IMAGE_BYTES 以上
    fn padded(magic: &[u8]) -> Vec<u8> {
        let mut v = magic.to_vec();
        v.resize(MIN_IMAGE_BYTES + 64, 0x00);
        v
    }

    /// 核心回归防线：曾因不校验字节，中转站返回的空 body 被当成图落盘，
    /// 导致「任务记录显示完成、界面却没有图」且重进不自愈。这里锁死判定口径。
    #[test]
    fn rejects_empty_and_undersized_payloads() {
        assert!(
            validate_image_bytes(&[], "测试").is_err(),
            "0 字节必须判失败"
        );
        assert!(
            validate_image_bytes(&[0x89, 0x50, 0x4E, 0x47], "测试").is_err(),
            "带正确魔数但长度不足 1KB 仍应判失败"
        );
    }

    #[test]
    fn rejects_non_image_payload() {
        // 典型伪装：HTTP 200 但 body 是一段 JSON 错误体
        let mut json_err = br#"{"error":"upstream returned status 500"}"#.to_vec();
        json_err.resize(MIN_IMAGE_BYTES + 64, b' ');
        assert!(
            validate_image_bytes(&json_err, "测试").is_err(),
            "长度达标但非图片格式必须判失败"
        );
    }

    #[test]
    fn accepts_common_image_formats() {
        assert!(
            validate_image_bytes(&padded(&[0x89, 0x50, 0x4E, 0x47]), "测试").is_ok(),
            "PNG"
        );
        assert!(
            validate_image_bytes(&padded(&[0xFF, 0xD8, 0xFF]), "测试").is_ok(),
            "JPEG"
        );
        assert!(
            validate_image_bytes(&padded(b"GIF8"), "测试").is_ok(),
            "GIF"
        );
        // WEBP：RIFF + 4 字节长度 + WEBP
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]);
        webp.extend_from_slice(b"WEBP");
        webp.resize(MIN_IMAGE_BYTES + 64, 0x00);
        assert!(validate_image_bytes(&webp, "测试").is_ok(), "WEBP");
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    /// 🔴 每条生图预置都要识别到预期协议 —— 改预置地址时这条会红。
    #[test]
    fn every_image_preset_detects_expected_protocol() {
        for p in crate::preset::presets_for(crate::Kind::Image) {
            let Some(url) = p.base_url else { continue };
            let want = if p.key == "wan_image" {
                ImageProtocol::DashScope
            } else {
                ImageProtocol::OpenAi
            };
            assert_eq!(ImageProtocol::detect(url), want, "{}", p.key);
        }
    }
}
