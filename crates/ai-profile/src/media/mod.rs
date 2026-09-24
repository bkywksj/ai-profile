//! 多模态调用：生图 / 视频 / 配音的各家协议实现。
//!
//! # 来源
//!
//! 整体搬自 StoryLoom（2026-09-24，v0.9.0 生产在用）的 `services/ai/{image,video,tts}_provider.rs`
//! 与分发逻辑（`services/media.rs`、`services/tts.rs`）。**只做了机械替换**（错误类型、HTTP 客户端路径），
//! 各家协议细节、错误翻译、超时策略原样保留 —— 它们都是实测踩出来的，重写一遍只会把坑再踩一遍。
//!
//! # 边界
//!
//! 本模块只管「一次 HTTP 调用怎么发、怎么解析」：生图 `generate`、视频 `submit` / `poll`、配音 `synthesize`。
//! **任务编排归调用方**：视频轮询循环、取消、落盘、进度事件、数据库记录都与应用的存储和界面绑定，不进 crate。
//!
//! # 协议按地址识别
//!
//! 各家协议不同却都只给一个 base_url，所以协议从地址（和 `extra`）里认：
//! [`image::AnyImageProvider::from_config`]、[`video::VideoProtocol::detect`]、[`tts::TtsProtocol::detect`]。
//! 预置里的地址就是按这套规则写的 —— **改预置地址前先看识别规则**。

use std::time::Duration;

#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "tts")]
pub mod tts;
#[cfg(feature = "video")]
pub mod video;

/// 多模态调用的错误。
///
/// 只分两类，对应调用方要不要让用户改输入：
/// - [`MediaError::InvalidInput`]：入参或配置不对（文本为空、缺 App ID、端点填错产品线……），重试没用
/// - [`MediaError::Failed`]：其余一切（网络、上游拒绝、解析失败），消息已是可读中文
///
/// 🔴 `Display` 与 StoryLoom 的 `AppError::{InvalidInput, Custom}` 逐字一致（`参数无效: …` / 原文），
/// 调用方 `From` 转回自己的错误类型后，用户看到的文字与搬迁前相同。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MediaError {
    /// 入参或配置不对
    #[error("参数无效: {0}")]
    InvalidInput(String),
    /// 调用失败（消息为可读中文，含上游摘要）
    #[error("{0}")]
    Failed(String),
}

impl MediaError {
    /// 不带前缀的原始消息。
    pub fn message(&self) -> &str {
        match self {
            MediaError::InvalidInput(m) | MediaError::Failed(m) => m,
        }
    }
}

/// 调用方提供的 HTTP 客户端**底座**（代理 / 自签证书 / 自定义 DNS 从这里进）。
///
/// 🔴 与 [`crate::client::Verifier::from_builder`] 同一个思路：底座由调用方配，**超时策略由本 crate 在其后施加**，
/// 调用方覆盖不掉 —— 各家超时是实测踩出来的（出图不能用整体超时，否则算图慢时被误杀却照样计费）。
///
/// 每次构造 provider 都会调一次工厂拿新的 builder（`reqwest::ClientBuilder` 不能克隆）。
/// 用本 crate 重导出的 [`crate::reqwest`] 建 builder，版本必然匹配：
///
/// ```no_run
/// use ai_profile::media::MediaHttp;
/// # fn proxy_url() -> String { String::new() }
/// let url = proxy_url();
/// let http = MediaHttp::from_fn(move || {
///     let b = ai_profile::reqwest::Client::builder();
///     match ai_profile::reqwest::Proxy::all(&url) {
///         Ok(p) => b.proxy(p),
///         Err(_) => b,
///     }
/// });
/// ```
#[derive(Clone, Default)]
pub struct MediaHttp {
    base: Option<std::sync::Arc<dyn Fn() -> reqwest::ClientBuilder + Send + Sync>>,
}

impl MediaHttp {
    /// 用调用方的工厂构造（每次建客户端时调用它拿一个新 builder）。
    pub fn from_fn(f: impl Fn() -> reqwest::ClientBuilder + Send + Sync + 'static) -> Self {
        Self {
            base: Some(std::sync::Arc::new(f)),
        }
    }

    /// 拿一个底座 builder：有工厂用工厂，否则用 reqwest 默认（会读系统代理环境变量）。
    pub(crate) fn builder(&self) -> reqwest::ClientBuilder {
        match &self.base {
            Some(f) => f(),
            None => reqwest::Client::builder(),
        }
    }
}

impl std::fmt::Debug for MediaHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaHttp")
            .field("custom_base", &self.base.is_some())
            .finish()
    }
}

/// 按字符截断（不切断中文），超出加省略号。
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

/// HTTP 客户端工厂与错误描述（搬自 StoryLoom `shared::http`）。
///
/// 🔴 各家客户端的超时策略不同，**不要合并成一个**：出图若用整体超时，高峰期算图逼近上限时
/// 会被误杀，而中转站上游此时已经算完并照常计费 —— 用户得到「有消费记录却没图」。
pub(crate) mod http {
    use super::Duration;

    /// 连接建立超时：端点不可达 / DNS 卡住时快速失败。
    const CONNECT_TIMEOUT_SECS: u64 = 15;
    /// 非流式一次性请求的整体超时（视频提交 / 轮询、配音）。
    #[cfg(any(feature = "video", feature = "tts"))]
    const DEFAULT_TIMEOUT_SECS: u64 = 240;
    /// 出图「读」超时：等价于首字节等待上限，不设整体超时（见模块说明）。
    #[cfg(feature = "image")]
    const IMAGE_READ_TIMEOUT_SECS: u64 = 360;

    /// 非流式客户端：整体超时兜底，避免任一请求永久阻塞。超时施加在调用方底座之后（覆盖不掉）。
    #[cfg(any(feature = "video", feature = "tts"))]
    pub(crate) fn default_client(base: &super::MediaHttp) -> reqwest::Client {
        base.builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            // builder 仅在 TLS / 系统配置异常时才失败，退回无超时客户端保证可用性
            .unwrap_or_else(|_| reqwest::Client::new())
    }

    /// 出图客户端（文生图请求 + 结果图下载共用）：connect + read 双超时。超时施加在调用方底座之后。
    #[cfg(feature = "image")]
    pub(crate) fn image_client(base: &super::MediaHttp) -> reqwest::Client {
        base.builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .read_timeout(Duration::from_secs(IMAGE_READ_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }

    /// 沿 reqwest 错误的 `source()` 链提取底层真因（reqwest 的 `Display` 只到 URL 为止）。
    fn reqwest_cause_chain(e: &reqwest::Error) -> String {
        use std::error::Error as _;
        let mut causes: Vec<String> = Vec::new();
        let mut cur = e.source();
        let mut depth = 0;
        while let Some(src) = cur {
            if depth >= 8 {
                break;
            }
            let s = src.to_string();
            if causes.last() != Some(&s) {
                causes.push(s);
            }
            cur = src.source();
            depth += 1;
        }
        causes.join(" → ")
    }

    /// 把 reqwest 错误格式化成「分类 + 底层真因链」的可读字符串。
    pub(crate) fn describe_reqwest_error(e: &reqwest::Error) -> String {
        let kind = if e.is_timeout() {
            "超时"
        } else if e.is_connect() {
            "连接失败(端点不可达/DNS/TLS)"
        } else if e.is_request() {
            "请求发送失败"
        } else if e.is_body() {
            "请求/响应体错误"
        } else if e.is_decode() {
            "响应解码失败"
        } else if e.is_redirect() {
            "重定向过多"
        } else {
            "网络错误"
        };
        let chain = reqwest_cause_chain(e);
        if chain.is_empty() {
            format!("{kind}: {e}")
        } else {
            format!("{kind}: {e} —— 底层原因: {chain}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 Display 必须与 StoryLoom 的 AppError 逐字一致，否则搬迁后用户看到的报错变了。
    #[test]
    fn display_matches_storyloom_app_error() {
        assert_eq!(
            MediaError::InvalidInput("配音文本为空".into()).to_string(),
            "参数无效: 配音文本为空"
        );
        assert_eq!(
            MediaError::Failed("上游 500".into()).to_string(),
            "上游 500"
        );
        assert_eq!(MediaError::InvalidInput("x".into()).message(), "x");
    }

    /// 🔴 调用方的底座工厂必须真的被用上 —— 否则代理配了等于没配，且不报任何错。
    #[test]
    #[cfg(any(feature = "image", feature = "video"))]
    fn media_http_factory_is_used_by_every_entry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let http = MediaHttp::from_fn(move || {
            c.fetch_add(1, Ordering::SeqCst);
            reqwest::Client::builder()
        });
        let mut expected = 0;
        #[cfg(feature = "image")]
        {
            let _ = image::AnyImageProvider::from_config_with(
                image::ImageGenConfig {
                    endpoint: "https://a/v1".into(),
                    model: "m".into(),
                    api_key: "k".into(),
                },
                &http,
            );
            expected += 1;
        }
        #[cfg(feature = "video")]
        {
            let _ = video::AnyVideoProvider::from_config_with(
                video::VideoGenConfig {
                    endpoint: "https://a/v1".into(),
                    model: "m".into(),
                    api_key: "k".into(),
                },
                "",
                &http,
            );
            expected += 1;
        }
        assert_eq!(calls.load(Ordering::SeqCst), expected);
        assert!(format!("{http:?}").contains("custom_base: true"));
        assert!(format!("{:?}", MediaHttp::default()).contains("custom_base: false"));
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("你好世界", 2), "你好…");
        assert_eq!(truncate("abc", 3), "abc");
    }
}
