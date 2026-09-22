//! `ai.profile` —— 跨应用的模型服务配置交换格式。
//!
//! # 信封
//!
//! ```json
//! {
//!   "kind": "ai.profile",
//!   "v": 1,
//!   "data": {
//!     "name": "我的 DeepSeek",
//!     "provider": "deepseek",
//!     "baseURL": "https://api.deepseek.com/v1",
//!     "apiKey": "sk-...",
//!     "model": "deepseek-flash",
//!     "hints": { "toolId": "claude-code" }
//!   }
//! }
//! ```
//!
//! # 🔴 字段命名：规范一种、接受三种
//!
//! 规范写法是 `baseURL` / `apiKey`（URL 全大写是历史既成事实，不是笔误）。
//! 但现实中各家生成的 JSON 并不统一，所以解析时**同时接受**：
//!
//! | 规范 | 也接受 |
//! |---|---|
//! | `baseURL` | `baseUrl`、`base_url` |
//! | `apiKey` | `api_key` |
//!
//! 这不是洁癖问题：调研四份既有实现时发现，其中一份的 Rust 解析器只认
//! `baseURL` / `base_url`，**漏了 `baseUrl`** —— 另一个应用生成的配置它解析不了，
//! 而两边都自认为"实现了 ai.profile"。统一到本 crate 正是为了消掉这类静默不兼容。
//!
//! # 为什么 `model` 不是必填
//!
//! 不少来源软件分享的是"中转站的 key + 端点"，model 留空让接收方自己选。
//! 解析时用 [`ParsedProfile::model_fallback`] 标记这种情况，调用方可据此提示
//! 用户确认模型名。

use serde::{Deserialize, Serialize};

use crate::kind::Protocol;

/// 信封的 `kind` 常量。
pub const AI_PROFILE_KIND: &str = "ai.profile";
/// 当前协议版本。
pub const AI_PROFILE_VERSION: u32 = 1;

/// 解析成功的结果。
///
/// 带 `Serialize` 是为了能直接从 Tauri Command 返回给「粘贴导入」表单。
///
/// 🔴 **它含 `api_key` 明文** —— 序列化后会经 IPC 到达前端。这是粘贴导入这个
/// 功能本身的要求（表单要把密钥填进去），但因此：**不要把整个结构体写进日志**，
/// 也不要把它存进任何缓存。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ParsedProfile {
    /// 配置名；来源没给时为空串，调用方可用 provider 名兜底
    pub name: String,
    /// 推断出的协议 —— 决定走 `/v1/messages` 还是 `/v1/chat/completions`
    pub protocol: Protocol,
    /// 来源软件给的原始 provider 字符串（原样保留，便于 UI 让用户确认）
    pub raw_provider: String,
    /// 端点地址；空串 = 来源没给，走接收方的默认端点
    pub base_url: String,
    /// 明文密钥；空串 = 来源没给
    pub api_key: String,
    /// 模型 id
    pub model: String,
    /// 🔴 为真表示**来源没给 model**，当前值是接收方补的默认值。
    ///
    /// 调用方应当提示用户确认 —— 中转站暴露的模型名往往与官方不同，
    /// 猜错要等第一次对话才报错。
    pub model_fallback: bool,
}

/// 解析失败的原因。
///
/// 与 [`crate::VerifyError`] 分开：那个是"配置对不对"，这个是"这段文本是不是配置"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ParseError {
    /// 输入为空
    #[error("内容为空")]
    Empty,
    /// 不是合法 JSON
    #[error("不是合法的 JSON：{detail}")]
    InvalidJson {
        /// serde_json 的报错摘要
        detail: String,
    },
    /// `kind` 字段不是 `ai.profile`
    #[error("这段内容不是 ai.profile（kind = {found}）")]
    NotAiProfile {
        /// 实际读到的 kind；缺失时为空串
        found: String,
    },
    /// 协议版本高于本实现所能理解的
    #[error("协议版本 v{found} 高于当前支持的 v{supported}")]
    UnsupportedVersion {
        /// 来源声明的版本
        found: u32,
        /// 本实现支持的最高版本
        supported: u32,
    },
    /// `data` 缺失或不是对象
    #[error("缺少 data 对象")]
    MissingData,
}

/// 线格式：只用于 serde，公开 API 是 [`ParsedProfile`]。
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    kind: String,
    #[serde(default = "default_version")]
    v: u32,
    #[serde(default)]
    data: Option<Data>,
}

fn default_version() -> u32 {
    AI_PROFILE_VERSION
}

#[derive(Deserialize)]
struct Data {
    #[serde(default)]
    name: String,
    #[serde(default)]
    provider: String,
    /// 🔴 三种命名都接受 —— 见模块文档
    #[serde(default, rename = "baseURL", alias = "baseUrl", alias = "base_url")]
    base_url: String,
    #[serde(default, rename = "apiKey", alias = "api_key")]
    api_key: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    hints: Option<Hints>,
}

#[derive(Deserialize)]
struct Hints {
    #[serde(default, rename = "toolId", alias = "tool_id")]
    tool_id: String,
}

/// 把来源的 provider 字符串映射回协议枚举。
///
/// 常见来源写法：
/// - `anthropic` / `claude` / `claude_code` / `claude-code` → Anthropic 原生
/// - `deepseek` / `openai` / `openrouter` / `google` / `custom` / … → OpenAI 兼容
///
/// **两条兜底**（都来自真实场景，不是防御性编程）：
///
/// 1. provider 不明显但 `model` 以 `claude-` 开头 → Anthropic。
///    密钥限定 `/v1/messages` 的中转卖家，model 通常就是 `claude-opus` / `claude-sonnet`。
/// 2. `hints.toolId` 提到 claude → Anthropic。
///    某些软件分享中转配置时是 `provider: "custom"` + 空 model + `toolId: "claude-code"`，
///    不看 toolId 会误判成 OpenAI 兼容而请求错端点。
fn map_protocol(raw_provider: &str, model: &str, tool_id: &str) -> Protocol {
    let p = raw_provider.trim().to_ascii_lowercase();
    if p.contains("anthropic") || p.contains("claude") {
        return Protocol::Anthropic;
    }
    if model.trim().to_ascii_lowercase().starts_with("claude-") {
        return Protocol::Anthropic;
    }
    let t = tool_id.trim().to_ascii_lowercase();
    if t.contains("claude") || t.contains("anthropic") {
        return Protocol::Anthropic;
    }
    Protocol::OpenAiCompatible
}

/// 解析一段文本为 `ai.profile`。
///
/// `default_model`：来源没给 model 时用它兜底（调用方按 provider 预置传入）。
/// 用了兜底值时 [`ParsedProfile::model_fallback`] 为 `true`。
pub fn parse_profile(text: &str, default_model: &str) -> Result<ParsedProfile, ParseError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let env: Envelope = serde_json::from_str(trimmed).map_err(|e| ParseError::InvalidJson {
        detail: e.to_string(),
    })?;

    if env.kind != AI_PROFILE_KIND {
        return Err(ParseError::NotAiProfile { found: env.kind });
    }
    // 🔴 只拒绝**更高**的版本：低版本能被高版本实现读懂（字段只增不改），
    //    拒绝低版本会把老软件分享的配置挡在外面。
    if env.v > AI_PROFILE_VERSION {
        return Err(ParseError::UnsupportedVersion {
            found: env.v,
            supported: AI_PROFILE_VERSION,
        });
    }
    let d = env.data.ok_or(ParseError::MissingData)?;

    let tool_id = d.hints.map(|h| h.tool_id).unwrap_or_default();
    let protocol = map_protocol(&d.provider, &d.model, &tool_id);

    let model_given = !d.model.trim().is_empty();
    let model = if model_given {
        d.model.trim().to_string()
    } else {
        default_model.trim().to_string()
    };

    Ok(ParsedProfile {
        name: d.name.trim().to_string(),
        protocol,
        raw_provider: d.provider.trim().to_string(),
        base_url: d.base_url.trim().to_string(),
        api_key: d.api_key.trim().to_string(),
        model,
        model_fallback: !model_given,
    })
}

/// 生成 `ai.profile` JSON 文本。
///
/// 🔴 输出**一律用规范命名**（`baseURL` / `apiKey`）—— 宽进严出：
/// 解析时接受各种别名，生成时只产出一种，别再给生态添新的变体。
///
/// # 安全
///
/// 产物含**明文密钥**。调用方必须自己决定它去哪（剪贴板 / 文件），
/// 并且不要把它写进日志。本 crate 不做持久化。
pub fn to_profile(
    name: &str,
    protocol: Protocol,
    base_url: &str,
    api_key: &str,
    model: &str,
) -> String {
    // provider 用通用标签而非内部枚举名：接收方按这个字符串做映射，
    // "openai" 比 "openai_compatible" 更容易被别家认出来。
    let provider = match protocol {
        Protocol::Anthropic => "anthropic",
        _ => "openai",
    };
    let v = serde_json::json!({
        "kind": AI_PROFILE_KIND,
        "v": AI_PROFILE_VERSION,
        "data": {
            "name": name,
            "provider": provider,
            "baseURL": base_url,
            "apiKey": api_key,
            "model": model,
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 `ParsedProfile` 的线格式是「粘贴导入」表单的契约。
    #[test]
    fn parsed_profile_serializes_camel_case() {
        let p = parse_profile(
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","baseURL":"https://a/v1","apiKey":"sk-1"}}"#,
            "fallback-model",
        )
        .unwrap();
        let j = serde_json::to_string(&p).unwrap();
        assert!(j.contains(r#""baseUrl":"https://a/v1""#), "{j}");
        assert!(j.contains(r#""apiKey":"sk-1""#), "{j}");
        assert!(j.contains(r#""modelFallback":true"#), "来源没给 model：{j}");
        assert!(j.contains(r#""rawProvider""#), "{j}");
    }

    const CANONICAL: &str = r#"{
      "kind":"ai.profile","v":1,
      "data":{"name":"我的 DeepSeek","provider":"deepseek",
              "baseURL":"https://api.deepseek.com/v1","apiKey":"sk-x","model":"deepseek-flash"}
    }"#;

    #[test]
    fn parses_canonical() {
        let p = parse_profile(CANONICAL, "fallback").unwrap();
        assert_eq!(p.name, "我的 DeepSeek");
        assert_eq!(p.protocol, Protocol::OpenAiCompatible);
        assert_eq!(p.base_url, "https://api.deepseek.com/v1");
        assert_eq!(p.model, "deepseek-flash");
        assert!(!p.model_fallback);
    }

    /// 🔴 三种 base_url 命名都要认 —— 调研中发现某实现漏了 `baseUrl`，
    /// 导致另一个应用生成的配置它解析不了，而两边都自认为实现了本协议。
    #[test]
    fn accepts_all_three_base_url_spellings() {
        for key in ["baseURL", "baseUrl", "base_url"] {
            let json = format!(
                r#"{{"kind":"ai.profile","v":1,"data":{{"{key}":"https://x.com/v1","apiKey":"k"}}}}"#
            );
            let p = parse_profile(&json, "m").unwrap_or_else(|e| panic!("{key} 应被接受：{e}"));
            assert_eq!(p.base_url, "https://x.com/v1", "{key} 没解析出来");
        }
        for key in ["apiKey", "api_key"] {
            let json = format!(r#"{{"kind":"ai.profile","v":1,"data":{{"{key}":"sk-secret"}}}}"#);
            let p = parse_profile(&json, "m").unwrap();
            assert_eq!(p.api_key, "sk-secret", "{key} 没解析出来");
        }
    }

    /// 兜底 1：provider 不明显，但 model 以 claude- 开头。
    #[test]
    fn infers_anthropic_from_model_name() {
        let json = r#"{"kind":"ai.profile","v":1,
          "data":{"provider":"custom","model":"claude-opus-5","baseURL":"https://cc.x.cn/v1"}}"#;
        let p = parse_profile(json, "m").unwrap();
        assert_eq!(
            p.protocol,
            Protocol::Anthropic,
            "model 名应触发 Anthropic 兜底"
        );
    }

    /// 兜底 2：provider=custom + 空 model + hints.toolId 指向 claude-code。
    /// 不看 toolId 会误判成 OpenAI 兼容而请求错端点。
    #[test]
    fn infers_anthropic_from_tool_id() {
        let json = r#"{"kind":"ai.profile","v":1,
          "data":{"provider":"custom","model":"","hints":{"toolId":"claude-code"}}}"#;
        let p = parse_profile(json, "claude-opus-5").unwrap();
        assert_eq!(p.protocol, Protocol::Anthropic);
        assert!(p.model_fallback, "来源没给 model，应标记为兜底值");
        assert_eq!(p.model, "claude-opus-5");
    }

    #[test]
    fn rejects_non_profile_and_bad_json() {
        assert!(matches!(parse_profile("", "m"), Err(ParseError::Empty)));
        assert!(matches!(
            parse_profile("{not json", "m"),
            Err(ParseError::InvalidJson { .. })
        ));
        assert!(matches!(
            parse_profile(r#"{"kind":"something.else"}"#, "m"),
            Err(ParseError::NotAiProfile { .. })
        ));
        assert!(matches!(
            parse_profile(r#"{"kind":"ai.profile","v":1}"#, "m"),
            Err(ParseError::MissingData)
        ));
    }

    /// 🔴 只拒绝更高版本 —— 拒绝低版本会把老软件分享的配置挡在外面。
    #[test]
    fn accepts_older_version_rejects_newer() {
        let old = r#"{"kind":"ai.profile","v":1,"data":{"model":"m"}}"#;
        assert!(parse_profile(old, "m").is_ok());

        let future = r#"{"kind":"ai.profile","v":99,"data":{"model":"m"}}"#;
        assert!(matches!(
            parse_profile(future, "m"),
            Err(ParseError::UnsupportedVersion { found: 99, .. })
        ));
    }

    /// 生成 → 解析 → 再生成，字段不丢不变。
    #[test]
    fn protocol_roundtrip() {
        let out = to_profile(
            "我的 Claude",
            Protocol::Anthropic,
            "https://cc.example.cn/v1",
            "sk-secret",
            "claude-opus-5",
        );
        let p = parse_profile(&out, "fallback").unwrap();
        assert_eq!(p.name, "我的 Claude");
        assert_eq!(p.protocol, Protocol::Anthropic);
        assert_eq!(p.base_url, "https://cc.example.cn/v1");
        assert_eq!(p.api_key, "sk-secret");
        assert_eq!(p.model, "claude-opus-5");
        assert!(!p.model_fallback);

        let again = to_profile("我的 Claude", p.protocol, &p.base_url, &p.api_key, &p.model);
        assert_eq!(out, again, "两次生成应当一致");
    }

    /// 生成时只产出规范命名 —— 宽进严出，别给生态添新变体。
    #[test]
    fn output_uses_canonical_spelling_only() {
        let out = to_profile("n", Protocol::OpenAiCompatible, "https://x/v1", "k", "m");
        assert!(out.contains("\"baseURL\""), "应输出规范的 baseURL");
        assert!(!out.contains("\"base_url\""), "不该输出 snake_case 变体");
        assert!(out.contains("\"apiKey\""));
        assert!(!out.contains("\"api_key\""));
    }
}
