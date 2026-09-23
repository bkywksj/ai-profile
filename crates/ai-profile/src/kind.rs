//! 能力类型（kind）—— 决定一条预置属于对话 / 生图 / 视频 / 配音。
//!
//! # 🔴 kind 是数据，不是分支
//!
//! 调用方应当**遍历** `kinds` 渲染界面，而不是写 `if kind == Video`。
//! 验收判据：新增一种 kind 时，下游前端应零改动。
//!
//! # 四种 kind 的执行模型根本不同
//!
//! | kind  | 执行模型                    | 每家的差异           |
//! |-------|-----------------------------|----------------------|
//! | chat  | 同步 / 流式                 | 协议二分（openai / anthropic） |
//! | image | 同步 **或** submit+poll     | 因家而异             |
//! | video | **必然 submit+poll**        | 轮询协议各家都不同   |
//! | tts   | 同步，返回字节流            | 有的专有协议         |
//!
//! 所以 client 层**按 kind 分 trait**，不强求一个统一接口。

use serde::{Deserialize, Serialize};

/// 模型服务提供的能力类型。
///
/// `#[non_exhaustive]`：加枚举值是 minor 而非 major —— 下游 `match` 必须带 `_` 分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Kind {
    /// 对话（大纲 / 续写 / 问答…）
    Chat,
    /// 文生图 / 图生图
    #[cfg(feature = "image")]
    Image,
    /// 图生视频 / 文生视频（异步任务）
    #[cfg(feature = "video")]
    Video,
    /// 语音合成
    #[cfg(feature = "tts")]
    Tts,
}

impl Kind {
    /// i18n key（调用方用 `t()` 解析）。
    pub const fn label_key(self) -> &'static str {
        match self {
            Kind::Chat => "kind.chat",
            #[cfg(feature = "image")]
            Kind::Image => "kind.image",
            #[cfg(feature = "video")]
            Kind::Video => "kind.video",
            #[cfg(feature = "tts")]
            Kind::Tts => "kind.tts",
        }
    }

    /// 纯文本名 —— 给没接 i18n 的消费方（如 knowledge_base）。
    pub const fn label(self) -> &'static str {
        match self {
            Kind::Chat => "对话",
            #[cfg(feature = "image")]
            Kind::Image => "生图",
            #[cfg(feature = "video")]
            Kind::Video => "视频",
            #[cfg(feature = "tts")]
            Kind::Tts => "配音",
        }
    }

    /// 本 kind 是否提供「试运行」。
    ///
    /// 🔴 video 返回 false：单次生成约 90 秒且费用较高（约 ¥1–5），
    /// 不该做成随手一点的按钮 —— 让用户在真实业务流程里验证。
    pub const fn supports_dry_run(self) -> bool {
        match self {
            Kind::Chat => true,
            #[cfg(feature = "image")]
            Kind::Image => true,
            #[cfg(feature = "video")]
            Kind::Video => false,
            #[cfg(feature = "tts")]
            Kind::Tts => true,
        }
    }
}

/// 对话接口协议：决定走 `/v1/messages` 还是 `/v1/chat/completions`。
///
/// 🔴 线格式是 `"anthropic"` / `"openai_compatible"`，与 [`Protocol::as_str`] 同一套拼写。
/// 此前靠 `rename_all = "snake_case"` 自动推出 `"open_ai_compatible"`，
/// 下游存的是 `openai_compatible`，前端不得不写一层转换 —— 两种拼写并存，
/// 迟早有一处直接透传而静默错配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Protocol {
    /// Anthropic 原生：`/v1/messages` + `x-api-key`
    #[serde(rename = "anthropic")]
    Anthropic,
    /// OpenAI 兼容：`/v1/chat/completions` + `Authorization: Bearer`
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

impl Protocol {
    /// 规范字符串，与 serde 线格式一致。调用方持久化协议时用它。
    pub const fn as_str(self) -> &'static str {
        match self {
            Protocol::Anthropic => "anthropic",
            Protocol::OpenAiCompatible => "openai_compatible",
        }
    }

    /// 从字符串解析；认不出返回 `None`。
    ///
    /// 只收规范拼写 —— 这是给「读自己存的值」用的，存进去的一定是 [`Self::as_str`]。
    /// 解析别人家分享来的配置（`"openai"` / `"custom"` 之类）请用
    /// [`crate::protocol::parse_profile`]，那边有宽松的别名映射。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "anthropic" => Some(Protocol::Anthropic),
            "openai_compatible" => Some(Protocol::OpenAiCompatible),
            _ => None,
        }
    }

    /// 用户没填 base_url 时的官方端点。
    ///
    /// 🔴 带版本段：[`crate::endpoint`] 原样拼接、不做推断，少一段就是 404。
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Protocol::Anthropic => "https://api.anthropic.com/v1",
            Protocol::OpenAiCompatible => "https://api.openai.com/v1",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 video 不给试运行 —— 这条是成本约束，不是实现遗漏。
    #[test]
    fn video_has_no_dry_run() {
        assert!(Kind::Chat.supports_dry_run());
        #[cfg(feature = "video")]
        assert!(
            !Kind::Video.supports_dry_run(),
            "视频单次约 90 秒且费用高，不提供试运行"
        );
    }

    /// 🔴 serde 线格式、`as_str`、`parse` 三者必须是同一套拼写。
    ///
    /// 不一致的后果是静默的：前端拿 serde 的值去比对后端存的 `as_str`，永远不相等。
    #[test]
    fn protocol_spellings_agree() {
        for p in [Protocol::Anthropic, Protocol::OpenAiCompatible] {
            let wire = serde_json::to_value(p).unwrap();
            assert_eq!(wire, p.as_str(), "serde 线格式与 as_str 不一致");
            assert_eq!(Protocol::parse(p.as_str()), Some(p));
            let back: Protocol = serde_json::from_value(wire).unwrap();
            assert_eq!(back, p);
        }
        assert_eq!(
            Protocol::parse("open_ai_compatible"),
            None,
            "旧拼写不再接受"
        );
    }

    /// 默认端点必须带版本段 —— endpoint 模块不再替调用方补。
    #[test]
    fn default_base_url_has_version_segment() {
        for p in [Protocol::Anthropic, Protocol::OpenAiCompatible] {
            assert!(
                crate::endpoint::ends_with_version_segment(p.default_base_url()),
                "{} 缺版本段",
                p.default_base_url()
            );
        }
    }
}
