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

use serde::Serialize;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Protocol {
    /// Anthropic 原生：`/v1/messages` + `x-api-key`
    Anthropic,
    /// OpenAI 兼容：`/v1/chat/completions` + `Authorization: Bearer`
    OpenAiCompatible,
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
}
