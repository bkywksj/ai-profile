//! 真实 HTTP 调用 —— `feature = "client"` 才编译。
//!
//! # 分层：纯函数与 IO 分开
//!
//! 本模块刻意把「判断逻辑」与「发请求」拆开：
//!
//! - [`diagnose`]、[`suggest_url`]、[`check_required_fields`] 是**纯函数**，可完整单测
//! - [`verify`] 只负责发请求，拿到结果后交给上面那些函数判断
//!
//! 这样错误映射（本模块最容易出错的地方）不需要网络就能测。
//!
//! # 🔴 安全约定
//!
//! - **禁重定向**：reqwest 默认跟随最多 10 跳，跨 host 时只剥 `Authorization` 等
//!   标准头，**不剥自定义头** —— Anthropic 的 `x-api-key` 会被原样发往跳转目标。
//!   LLM 端点正常返回 200，不需要重定向。这同时堵死了二段跳 SSRF。
//! - **密钥绝不进错误信息**：[`crate::VerifyError`] 的所有 `detail` 字段只放
//!   端点返回的文本摘要，调用方可以安全地写日志。
//! - 本模块**不持久化任何东西**，密钥用完即弃。

use std::time::Duration;

use crate::endpoint::{ends_with_version_segment, join_api_path};
use crate::error::VerifyError;
use crate::kind::Protocol;
use crate::model_filter::clean_fetched_models;
use crate::preset::preset_by_key;

/// 发起验证所需的配置 —— 全部来自调用方表单里**正在编辑**的值。
///
/// 刻意用借用而非 `String`：调用方通常直接从表单字段取，不该为验证一次而 clone。
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ServiceConfig<'a> {
    /// 预置 key。给了就会校验该预置要求的 `extra_fields`，并在 base_url 为空时用它兜底
    pub preset_key: Option<&'a str>,
    /// 协议：决定鉴权头的形式
    pub protocol: Protocol,
    /// 端点地址；空串 = 用预置的 base_url
    pub base_url: &'a str,
    /// 明文密钥；空串 = 不带鉴权（本地服务常见）
    pub api_key: &'a str,
    /// 当前填的模型 id；空串 = 不校验模型是否存在
    pub model: &'a str,
    /// 专有字段的实际值，`(key, value)` 对
    pub extra: &'a [(&'a str, &'a str)],
}

/// 🔴 本类型带 `#[non_exhaustive]`，**外部 crate 不能用字面量构造它** ——
/// 必须走下面这组链式方法。
///
/// 这不是绕弯路：`non_exhaustive` 让本 crate 以后加字段只算 minor 版本，
/// 代价就是调用方不能写 `ServiceConfig { .. }`。入参类型上用它，
/// **必须同时提供完整的 builder**，否则下游根本没法用（实测踩过 E0639）。
///
/// ```no_run
/// # use ai_profile::{client::ServiceConfig, Protocol};
/// let cfg = ServiceConfig::new(Protocol::OpenAiCompatible, "https://api.deepseek.com/v1")
///     .with_preset("deepseek")
///     .with_api_key("sk-…")
///     .with_model("deepseek-flash");
/// ```
impl<'a> ServiceConfig<'a> {
    /// 最小构造：只给协议与端点。
    pub fn new(protocol: Protocol, base_url: &'a str) -> Self {
        Self {
            preset_key: None,
            protocol,
            base_url,
            api_key: "",
            model: "",
            extra: &[],
        }
    }

    /// 指定预置 key —— 会校验该预置要求的 `extra_fields`，
    /// 并在 `base_url` 为空时用预置的地址兜底。
    pub fn with_preset(mut self, key: &'a str) -> Self {
        self.preset_key = Some(key);
        self
    }

    /// 明文密钥。空串 = 不带鉴权（本地服务常见）。
    pub fn with_api_key(mut self, key: &'a str) -> Self {
        self.api_key = key;
        self
    }

    /// 当前填的模型 id；给了就会校验它在不在端点返回的清单里。
    pub fn with_model(mut self, model: &'a str) -> Self {
        self.model = model;
        self
    }

    /// 服务商专有字段的实际值。
    pub fn with_extra(mut self, extra: &'a [(&'a str, &'a str)]) -> Self {
        self.extra = extra;
        self
    }
}

/// 验证成功的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VerifyOk {
    /// 往返耗时 —— UI 显示「正常 · 320ms」，中转站慢不慢一眼看出
    pub latency_ms: u32,
    /// 端点返回的可对话模型清单（已去重 + 清洗）
    pub models: Vec<String>,
    /// 当前填的 `model` 是否在清单里；`model` 为空或端点没返回清单时为 `true`
    pub model_in_list: bool,
}

/// 交互式动作的超时 —— 用户正看着转圈，比对话请求短得多。
const VERIFY_TIMEOUT_SECS: u64 = 20;

/// 校验预置要求的必填 `extra_fields` 是否都给了。
///
/// 🔴 调用方应当在**发起验证之前**就调它，用于禁用按钮 ——
/// 禁用态优于「点了才报错」。
pub fn check_required_fields(
    preset_key: Option<&str>,
    extra: &[(&str, &str)],
) -> Result<(), VerifyError> {
    let Some(p) = preset_key.and_then(preset_by_key) else {
        return Ok(());
    };
    for f in p.extra_fields {
        if !f.required {
            continue;
        }
        let given = extra
            .iter()
            .find(|(k, _)| *k == f.key)
            .is_some_and(|(_, v)| !v.trim().is_empty());
        if !given {
            return Err(VerifyError::MissingExtraField {
                key: f.key.to_string(),
            });
        }
    }
    Ok(())
}

/// 404 时推断「正确的地址应该长什么样」—— 「一键修正」的数据来源。
///
/// 只在**确实看不到版本段**时给建议；已经有版本段却 404 说明是别的问题，
/// 乱给建议会把用户引向另一个错误答案。
pub fn suggest_url(base_url: &str) -> Option<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    // 版本段可能在末尾（/v1、/v4），也可能在中间（Gemini 的 /v1beta/openai）
    if ends_with_version_segment(trimmed)
        || trimmed.contains("/v1beta/")
        || trimmed.contains("/v1/")
    {
        return None;
    }
    Some(format!("{trimmed}/v1"))
}

/// HTTP 状态码 + 响应体 → 结构化错误。
///
/// 抽成纯函数是为了能脱离网络单测 —— 这是本模块最容易写错的地方。
pub fn diagnose(status: u16, body: &str, requested_url: &str, base_url: &str) -> VerifyError {
    // 端点通常把原因放在 error.message 里，比裸状态码有用得多
    let detail = extract_error_message(body).unwrap_or_else(|| format!("HTTP {status}"));
    match status {
        401 | 403 => VerifyError::AuthFailed { detail },
        404 => VerifyError::NotFound {
            requested_url: requested_url.to_string(),
            suggested_url: suggest_url(base_url),
        },
        _ => VerifyError::Malformed { detail },
    }
}

/// 从端点的错误响应里抽出人话。各家格式不一，按常见顺序试。
fn extract_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let msg = v
        .get("error")
        .and_then(|e| e.get("message"))
        .or_else(|| v.get("error").and_then(|e| e.as_str().map(|_| e)))
        .and_then(|m| m.as_str())
        .or_else(|| v.get("message").and_then(|m| m.as_str()))?;
    let msg = msg.trim();
    if msg.is_empty() {
        return None;
    }
    // 截断：错误信息进 UI 也进日志，过长的原样回灌没意义
    Some(msg.chars().take(300).collect())
}

/// 从 `/models` 响应里抽出模型 id 列表。
///
/// 主流端点是 `{ "data": [{ "id": "…" }] }`；少数自建网关直接返回裸数组
/// `["gpt-4o", …]` 或 `[{"id": …}]`，一并容错。
pub fn parse_model_ids(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array());
    let Some(arr) = arr else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(|i| i.as_str())
                .or_else(|| item.as_str())
                .map(str::to_string)
        })
        .collect()
}

/// 这个 host 在国内通常需要代理 —— 用于给 `Unreachable` 带上提示。
fn needs_proxy_hint(url: &str) -> bool {
    const BLOCKED: &[&str] = &[
        "api.openai.com",
        "api.anthropic.com",
        "generativelanguage.googleapis.com",
        "openrouter.ai",
        "api.groq.com",
    ];
    let u = url.to_ascii_lowercase();
    BLOCKED.iter().any(|h| u.contains(h))
}

/// 零成本验证：只打端点的模型列表接口，**不产生任何生成费用**。
///
/// 四种 kind 都可调 —— 与 `dry_run`（真实调用，产生费用）分开正是因为
/// 四者的调用成本差几个数量级。
///
/// # 错误
///
/// 每个 [`VerifyError`] 变体都对应一个调用方应当给出的动作，见该类型文档。
pub async fn verify(cfg: ServiceConfig<'_>) -> Result<VerifyOk, VerifyError> {
    // 1. 必填的专有字段 —— 不发请求就能判
    check_required_fields(cfg.preset_key, cfg.extra)?;

    // 2. 定端点：表单填的优先，空则用预置的
    let base = if cfg.base_url.trim().is_empty() {
        cfg.preset_key
            .and_then(preset_by_key)
            .and_then(|p| p.base_url)
            .unwrap_or("")
    } else {
        cfg.base_url.trim()
    };
    if base.is_empty() {
        return Err(VerifyError::MissingExtraField {
            key: "base_url".to_string(),
        });
    }
    let url = join_api_path(base, "models");

    // 3. 发请求
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(VERIFY_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(10))
        // 🔴 禁重定向：跨 host 跳转时 reqwest 不剥自定义头，
        //    Anthropic 的 x-api-key 会被原样发往跳转目标
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| VerifyError::Malformed {
            detail: format!("构造 HTTP 客户端失败: {e}"),
        })?;

    let key = cfg.api_key.trim();
    let mut req = client.get(&url);
    req = match cfg.protocol {
        Protocol::Anthropic => {
            let r = req.header("anthropic-version", "2023-06-01");
            if key.is_empty() {
                r
            } else {
                r.header("x-api-key", key)
            }
        }
        // 本地服务（Ollama / vLLM）常常不校验密钥，空密钥也要允许发出去
        _ if key.is_empty() => req,
        _ => req.bearer_auth(key),
    };

    let started = std::time::Instant::now();
    let resp = req.send().await.map_err(|_| VerifyError::Unreachable {
        proxy_hint: needs_proxy_hint(&url),
    })?;
    let latency_ms = started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    if !(200..300).contains(&status) {
        return Err(diagnose(status, &body, &url, base));
    }

    // 4. 成功：清洗清单 + 校验当前 model 是否在里面
    let ids = parse_model_ids(&body);
    let cleaned = clean_fetched_models(ids);
    let model = cfg.model.trim();
    let model_in_list =
        model.is_empty() || cleaned.models.is_empty() || cleaned.models.iter().any(|m| m == model);

    Ok(VerifyOk {
        latency_ms,
        models: cleaned.models,
        model_in_list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 只在看不到版本段时给建议 —— 已有版本段却 404 是别的问题，
    /// 乱给建议会把用户引向另一个错误答案。
    #[test]
    fn suggest_url_only_when_version_missing() {
        assert_eq!(
            suggest_url("https://api.deepseek.com").as_deref(),
            Some("https://api.deepseek.com/v1")
        );
        assert_eq!(
            suggest_url("https://api.deepseek.com/").as_deref(),
            Some("https://api.deepseek.com/v1")
        );
        // 已有版本段 → 不建议
        assert_eq!(suggest_url("https://api.deepseek.com/v1"), None);
        assert_eq!(suggest_url("https://open.bigmodel.cn/api/paas/v4"), None);
        // Gemini：版本段不在末尾，也不该建议
        assert_eq!(
            suggest_url("https://generativelanguage.googleapis.com/v1beta/openai"),
            None
        );
        assert_eq!(suggest_url(""), None);
    }

    /// 状态码要映射到能驱动 UI 动作的变体。
    #[test]
    fn diagnose_maps_status_to_actionable_errors() {
        let e = diagnose(401, r#"{"error":{"message":"invalid key"}}"#, "u", "b");
        assert!(matches!(e, VerifyError::AuthFailed { ref detail } if detail == "invalid key"));

        let e = diagnose(404, "{}", "https://x.com/models", "https://x.com");
        match e {
            VerifyError::NotFound {
                requested_url,
                suggested_url,
            } => {
                assert_eq!(requested_url, "https://x.com/models");
                assert_eq!(suggested_url.as_deref(), Some("https://x.com/v1"));
            }
            other => panic!("404 应映射为 NotFound，实际 {other:?}"),
        }

        // 403 与 401 同类
        assert!(matches!(
            diagnose(403, "{}", "u", "b"),
            VerifyError::AuthFailed { .. }
        ));
        // 其余归兜底
        assert!(matches!(
            diagnose(500, "{}", "u", "b"),
            VerifyError::Malformed { .. }
        ));
    }

    /// 错误信息要从端点响应里抽人话，而不是只给「HTTP 500」。
    #[test]
    fn extracts_error_message_from_various_shapes() {
        assert_eq!(
            extract_error_message(r#"{"error":{"message":"no credit"}}"#).as_deref(),
            Some("no credit")
        );
        assert_eq!(
            extract_error_message(r#"{"message":"bad request"}"#).as_deref(),
            Some("bad request")
        );
        assert_eq!(extract_error_message("not json"), None);
        assert_eq!(extract_error_message(r#"{"error":{"message":"  "}}"#), None);
    }

    /// 三种响应形态都要能抽出模型 id。
    #[test]
    fn parses_model_ids_from_common_shapes() {
        assert_eq!(
            parse_model_ids(r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#),
            vec!["gpt-4o", "gpt-4o-mini"]
        );
        assert_eq!(
            parse_model_ids(r#"["llama3.1:8b","qwen3:8b"]"#),
            vec!["llama3.1:8b", "qwen3:8b"]
        );
        assert_eq!(parse_model_ids(r#"[{"id":"a"}]"#), vec!["a"]);
        assert!(parse_model_ids("garbage").is_empty());
    }

    /// 必填的专有字段缺失时，调用方能在发请求前就拦住。
    #[test]
    fn check_required_fields_catches_missing() {
        // 当前预置都没有 required 的 extra_fields，给不存在的 key 也应放行
        assert!(check_required_fields(Some("deepseek"), &[]).is_ok());
        assert!(check_required_fields(None, &[]).is_ok());
        assert!(check_required_fields(Some("不存在的预置"), &[]).is_ok());
    }

    /// 需要代理的 host 要带上提示，国内可直连的不带 —— 避免误导。
    #[test]
    fn proxy_hint_only_for_blocked_hosts() {
        assert!(needs_proxy_hint("https://api.openai.com/v1/models"));
        assert!(needs_proxy_hint(
            "https://generativelanguage.googleapis.com/x"
        ));
        assert!(!needs_proxy_hint("https://api.deepseek.com/v1/models"));
        assert!(!needs_proxy_hint("http://localhost:11434/v1/models"));
    }
}
