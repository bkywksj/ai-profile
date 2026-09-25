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
//!
//! # 多条打包
//!
//! ```json
//! { "kind": "ai.profile.bundle", "v": 1, "data": { "profiles": [ { …单条的 data… } ] } }
//! ```
//!
//! 用独立的 `kind`：只认单条的旧实现遇到它会报「不是 ai.profile」，而不是误读成一条。
//! 🔴 兼容**已经发出去**的写法：智码（tauri-cc）的打包是 `data.api_profiles`，每条是它
//! 同步载荷里的 snake_case 档案（`base_url` / `api_key` / 顶层 `tool_id` / `auth_type`），
//! 还带一个 `manifest`。这些都照收；`auth_type = "oauth"` 的条目跳过 —— OAuth 凭据与
//! 签发它的设备绑定，换一台机器不可用。
//!
//! 入口是 [`parse_profiles`]：单条、多条都接受，调用方不必先判断是哪种。

use serde::{Deserialize, Serialize};

use crate::kind::Protocol;

/// 信封的 `kind` 常量。
pub const AI_PROFILE_KIND: &str = "ai.profile";
/// 多条打包的 `kind` 常量。
pub const AI_PROFILE_BUNDLE_KIND: &str = "ai.profile.bundle";
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
    /// 多条打包里没有一条可导入的配置
    #[error("打包里没有可导入的配置（跳过 {skipped} 条设备绑定的 OAuth 档案）")]
    EmptyBundle {
        /// 被跳过的条数（OAuth 档案）
        skipped: usize,
    },
}

/// [`parse_profiles`] 的结果：单条与多条统一成列表。
///
/// 🔴 同 [`ParsedProfile`]，**含明文密钥**，别写日志、别缓存。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ParsedProfiles {
    /// 解析出的配置，顺序与来源一致；单条信封时恰好一条
    pub profiles: Vec<ParsedProfile>,
    /// 跳过的条数（设备绑定的 OAuth 档案）。界面应当告诉用户，否则会以为漏导了
    pub skipped: usize,
    /// 来源是否为多条打包
    pub bundle: bool,
}

/// 线格式：只用于 serde，公开 API 是 [`ParsedProfile`]。
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    kind: String,
    #[serde(default = "default_version")]
    v: u32,
    /// 先收成 Value：单条与多条的 data 形状不同，按 kind 再解
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// 多条打包的 data。
#[derive(Deserialize)]
struct BundleData {
    /// 规范写法 `profiles`；智码已发出去的写法是 `api_profiles`
    #[serde(default, alias = "api_profiles")]
    profiles: Vec<serde_json::Value>,
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
    /// 智码打包条目把 tool_id 放在顶层（不在 hints 里），作用相同
    #[serde(default, rename = "toolId", alias = "tool_id")]
    tool_id: String,
    /// 智码档案的认证类型；`oauth` 与设备绑定，打包导入时跳过
    #[serde(default, rename = "authType", alias = "auth_type")]
    auth_type: String,
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
    let env = read_envelope(text)?;
    if env.kind != AI_PROFILE_KIND {
        return Err(ParseError::NotAiProfile { found: env.kind });
    }
    let data = env.data.ok_or(ParseError::MissingData)?;
    Ok(build_profile(parse_data(data)?, default_model))
}

/// 解析单条或多条打包，统一返回列表。
///
/// - `ai.profile` → 一条
/// - `ai.profile.bundle` → 多条；OAuth 档案跳过并计入 [`ParsedProfiles::skipped`]，
///   不是对象的条目静默丢弃
/// - 打包里一条可导入的都没有 → [`ParseError::EmptyBundle`]
///
/// `default_model` 的含义同 [`parse_profile`]，对每一条生效。
pub fn parse_profiles(text: &str, default_model: &str) -> Result<ParsedProfiles, ParseError> {
    let env = read_envelope(text)?;
    // 🔴 先认 kind 再要 data：粘了一段别的 JSON（没有 data）时该说「不是 ai.profile」，
    //    而不是「缺少 data」—— 后者会让用户以为自己粘的是一条残缺的配置
    if env.kind != AI_PROFILE_KIND && env.kind != AI_PROFILE_BUNDLE_KIND {
        return Err(ParseError::NotAiProfile { found: env.kind });
    }
    let data = env.data.ok_or(ParseError::MissingData)?;
    match env.kind.as_str() {
        AI_PROFILE_KIND => Ok(ParsedProfiles {
            profiles: vec![build_profile(parse_data(data)?, default_model)],
            skipped: 0,
            bundle: false,
        }),
        AI_PROFILE_BUNDLE_KIND => {
            let bundle: BundleData = serde_json::from_value(data).map_err(invalid_json)?;
            let mut profiles = Vec::with_capacity(bundle.profiles.len());
            let mut skipped = 0;
            for item in bundle.profiles {
                if !item.is_object() {
                    continue;
                }
                let d = parse_data(item)?;
                if d.auth_type.trim().eq_ignore_ascii_case("oauth") {
                    skipped += 1;
                    continue;
                }
                profiles.push(build_profile(d, default_model));
            }
            if profiles.is_empty() {
                return Err(ParseError::EmptyBundle { skipped });
            }
            Ok(ParsedProfiles {
                profiles,
                skipped,
                bundle: true,
            })
        }
        _ => Err(ParseError::NotAiProfile { found: env.kind }),
    }
}

/// 读信封并做与 kind 无关的检查（空、JSON、版本）。
fn read_envelope(text: &str) -> Result<Envelope, ParseError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).map_err(invalid_json)?;
    // 🔴 信封必须是对象。直接反序列化成结构体的话，serde 会「按字段顺序」接受数组，
    //    `["ai.profile",1,{…}]` 也能解析成功 —— 协议里没有这种写法
    if !value.is_object() {
        return Err(ParseError::InvalidJson {
            detail: "顶层必须是 JSON 对象".to_string(),
        });
    }
    let env: Envelope = serde_json::from_value(value).map_err(invalid_json)?;
    // 🔴 只拒绝**更高**的版本：低版本能被高版本实现读懂（字段只增不改），
    //    拒绝低版本会把老软件分享的配置挡在外面。单条与打包共用同一个版本号。
    if env.v > AI_PROFILE_VERSION {
        return Err(ParseError::UnsupportedVersion {
            found: env.v,
            supported: AI_PROFILE_VERSION,
        });
    }
    Ok(env)
}

fn invalid_json(e: serde_json::Error) -> ParseError {
    ParseError::InvalidJson {
        detail: e.to_string(),
    }
}

/// data 必须是对象；字段类型不对（如 name 是数字）按 JSON 错误处理。
fn parse_data(v: serde_json::Value) -> Result<Data, ParseError> {
    if !v.is_object() {
        return Err(ParseError::MissingData);
    }
    serde_json::from_value(v).map_err(invalid_json)
}

fn build_profile(d: Data, default_model: &str) -> ParsedProfile {
    // hints.toolId 优先；智码打包条目把它放在顶层
    let tool_id = d
        .hints
        .map(|h| h.tool_id)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(d.tool_id);
    let protocol = map_protocol(&d.provider, &d.model, &tool_id);

    let model_given = !d.model.trim().is_empty();
    let model = if model_given {
        d.model.trim().to_string()
    } else {
        default_model.trim().to_string()
    };

    ParsedProfile {
        name: d.name.trim().to_string(),
        protocol,
        raw_provider: d.provider.trim().to_string(),
        base_url: d.base_url.trim().to_string(),
        api_key: d.api_key.trim().to_string(),
        model,
        model_fallback: !model_given,
    }
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

    /// 信封必须是 JSON 对象。serde 默认允许「按字段顺序」从数组反序列化结构体，
    /// 不拦的话 `["ai.profile",1,{…}]` 会被当成合法配置 —— 协议里没有这种写法，
    /// 写进多语言规范就等于逼别的语言去模仿一个库的副作用。
    #[test]
    fn envelope_must_be_an_object() {
        let arr =
            r#"["ai.profile",1,{"name":"x","baseURL":"https://a/v1","apiKey":"k","model":"m"}]"#;
        assert!(matches!(
            parse_profiles(arr, "m"),
            Err(ParseError::InvalidJson { .. })
        ));
        assert!(matches!(
            parse_profile(arr, "m"),
            Err(ParseError::InvalidJson { .. })
        ));
    }

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

    /// 🔴 智码（tauri-cc）已经发出去的打包写法：`data.api_profiles` + snake_case 同步档案 +
    /// 顶层 `tool_id` + `auth_type` + `manifest`。形状取自它的 `ApiProfile` 结构体。
    const TAURI_CC_BUNDLE: &str = r#"{
      "kind":"ai.profile.bundle","v":1,
      "manifest":{"app":"tauri-cc","count":3},
      "data":{"api_profiles":[
        {"id":"a","name":"中转 Claude","provider":"custom","api_key":"sk-cc","base_url":"https://relay.example.cn",
         "is_active":true,"tool_id":"claude-code","model":"","use_proxy":false,"key_auth_type":"auto",
         "auth_type":"api_key","workspace_id":"local","created_at":"t","updated_at":"t"},
        {"id":"b","name":"Codex 登录","provider":"openai","api_key":"","base_url":"","tool_id":"codex",
         "model":"gpt-6","auth_type":"oauth","oauth_payload":"enc:v1:xx"},
        {"id":"c","name":"DeepSeek","provider":"deepseek","api_key":"sk-ds","base_url":"https://api.deepseek.com/v1",
         "tool_id":"codex","model":"deepseek-flash","auth_type":"api_key"}
      ]}
    }"#;

    #[test]
    fn parses_tauri_cc_bundle() {
        let r = parse_profiles(TAURI_CC_BUNDLE, "claude-opus-5").unwrap();
        assert!(r.bundle);
        assert_eq!(r.skipped, 1, "OAuth 档案与设备绑定，应跳过并计数");
        assert_eq!(r.profiles.len(), 2);

        let relay = &r.profiles[0];
        assert_eq!(relay.name, "中转 Claude");
        assert_eq!(
            relay.protocol,
            Protocol::Anthropic,
            "顶层 tool_id = claude-code 应触发 Anthropic 兜底"
        );
        assert_eq!(relay.base_url, "https://relay.example.cn");
        assert_eq!(relay.api_key, "sk-cc");
        assert!(relay.model_fallback);
        assert_eq!(relay.model, "claude-opus-5");

        let ds = &r.profiles[1];
        assert_eq!(ds.protocol, Protocol::OpenAiCompatible);
        assert_eq!(ds.model, "deepseek-flash");
        assert!(!ds.model_fallback);
    }

    /// 规范写法 `data.profiles`，条目用单条的 camelCase 字段。
    #[test]
    fn parses_canonical_bundle() {
        let json = r#"{"kind":"ai.profile.bundle","v":1,"data":{"profiles":[
          {"name":"a","provider":"deepseek","baseURL":"https://api.deepseek.com/v1","apiKey":"k1","model":"deepseek-flash"},
          {"name":"b","provider":"anthropic","baseUrl":"https://api.anthropic.com/v1","apiKey":"k2","model":"claude-opus-5"}
        ]}}"#;
        let r = parse_profiles(json, "m").unwrap();
        assert_eq!(r.profiles.len(), 2);
        assert_eq!(r.skipped, 0);
        assert_eq!(r.profiles[1].protocol, Protocol::Anthropic);
        assert_eq!(r.profiles[1].base_url, "https://api.anthropic.com/v1");
    }

    /// 单条信封走 `parse_profiles` 得到一条，结果与 `parse_profile` 一致。
    #[test]
    fn parse_profiles_accepts_single() {
        let r = parse_profiles(CANONICAL, "fallback").unwrap();
        assert!(!r.bundle);
        assert_eq!(
            r.profiles,
            vec![parse_profile(CANONICAL, "fallback").unwrap()]
        );
    }

    #[test]
    fn bundle_edge_cases() {
        // 全是 OAuth → 空打包，带跳过条数
        let only_oauth = r#"{"kind":"ai.profile.bundle","v":1,"data":{"api_profiles":[
          {"name":"x","auth_type":"oauth"}]}}"#;
        assert_eq!(
            parse_profiles(only_oauth, "m"),
            Err(ParseError::EmptyBundle { skipped: 1 })
        );
        // 非对象条目丢弃，不连累其它条
        let mixed = r#"{"kind":"ai.profile.bundle","v":1,"data":{"profiles":[1,"x",{"name":"ok","model":"m1"}]}}"#;
        let r = parse_profiles(mixed, "m").unwrap();
        assert_eq!(r.profiles.len(), 1);
        assert_eq!(r.profiles[0].name, "ok");
        // 更高版本的打包照样拒绝
        let future = r#"{"kind":"ai.profile.bundle","v":99,"data":{"profiles":[]}}"#;
        assert!(matches!(
            parse_profiles(future, "m"),
            Err(ParseError::UnsupportedVersion { found: 99, .. })
        ));
        // 🔴 不是 ai.profile 的 JSON（没有 data）要报 not_ai_profile，不能报 missing_data ——
        //    reeve 的测试抓到过：先查 data 再认 kind，粘错东西时提示成了「缺少 data」
        assert!(matches!(
            parse_profiles(r#"{"kind":"other"}"#, "m"),
            Err(ParseError::NotAiProfile { .. })
        ));
        assert!(matches!(
            parse_profiles(r#"{"kind":"ai.profile.bundle","v":1}"#, "m"),
            Err(ParseError::MissingData)
        ));
        // 🔴 只认单条的入口遇到打包要明确拒绝，而不是误读成一条
        assert!(matches!(
            parse_profile(TAURI_CC_BUNDLE, "m"),
            Err(ParseError::NotAiProfile { .. })
        ));
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
