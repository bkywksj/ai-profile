//! `spec/` 的生成器：与语言无关的预置数据 + 一致性测试用例。
//!
//! # 为什么要有
//!
//! 本库的价值大半与语言无关：预置数据是一份 JSON，地址拼接 / 错误判定 / 模型清洗 / ai.profile 解析
//! 是一组规则。别的语言要接，不该重抄一份数据、凭文字去猜规则 —— 那正是本库当初要消灭的
//! 「同一套逻辑写 N 遍、各自漂移」。
//!
//! 所以这里把两样东西导出成 JSON：
//! - `spec/presets.json`：全部预置与服务商目录，与 crate 同源
//! - `spec/conformance/*.json`：一组「输入 → 期望输出」用例，期望值由 Rust 参考实现**现场算出**。
//!   别的语言的实现跑通全部用例，就等于与 Rust 版行为一致
//!
//! 守卫测试 `spec_files_in_sync` 比对已提交的文件与本生成器输出，不一致 CI 直接红 ——
//! 用例永远与代码同步，不会变成又一份会过时的文档。

use std::collections::BTreeMap;

use ai_profile::client::{diagnose, parse_model_ids, parse_model_limits, suggest_url};
use ai_profile::endpoint::{anthropic_base_url, join_api_path, join_chat_endpoint};
use ai_profile::model_filter::{clean_fetched_models, is_chat_model_id};
use ai_profile::preset::{presets, vendors_all};
use ai_profile::{parse_profiles, to_profile, Protocol, TokenLimits};
use serde_json::{json, Value};

/// 用例格式版本。**只在格式本身变了**（字段改名 / 结构调整）时加 1；加用例、改期望值都不算。
pub const SPEC_VERSION: u32 = 1;

fn header(title: &str, what: &str) -> Value {
    json!({
        "specVersion": SPEC_VERSION,
        "crateVersion": env!("CARGO_PKG_VERSION"),
        "title": title,
        "description": what,
        "generatedBy": "cargo xtask gen-spec（期望值由 Rust 参考实现算出，请勿手改）",
    })
}

fn with_cases(mut head: Value, cases: Vec<Value>) -> Value {
    head["cases"] = Value::Array(cases);
    head
}

// ── 预置数据 ─────────────────────────────────────────────────────

fn presets_json() -> Value {
    let mut v = header(
        "ai-profile 预置数据",
        "全部服务商预置（对话 / 生图 / 视频 / 配音）与按厂商聚合的目录。字段含义见 https://ai-profile.ruoyi.plus/api/preset",
    );
    v["presets"] = serde_json::to_value(presets()).expect("预置可序列化");
    v["vendors"] = serde_json::to_value(vendors_all()).expect("目录可序列化");
    v
}

// ── 端点拼接 ─────────────────────────────────────────────────────

fn endpoint_json() -> Value {
    let api_bases = [
        "https://api.deepseek.com/v1",
        "https://api.deepseek.com/v1/",
        "  https://api.deepseek.com/v1  ",
        "https://api.deepseek.com",
        "https://open.bigmodel.cn/api/paas/v4",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "https://generativelanguage.googleapis.com/v1beta/openai#",
        "https://proxy.example.com/v1/chat/completions",
        "https://proxy.example.com/v1/messages",
    ];
    let chat_bases = [
        "https://api.deepseek.com/v1",
        "https://api.deepseek.com",
        "https://relay.example.com/v1/chat/completions",
        "https://api.anthropic.com",
        "https://api.anthropic.com/v1",
        "https://cc.example.top:8443",
        "https://api.deepseek.com/anthropic",
        "https://proxy.example.com/v1/messages",
        "https://odd.gateway.com/raw#",
    ];
    let mut cases = Vec::new();
    for base in api_bases {
        for path in ["models", "chat/completions", "embeddings"] {
            cases.push(json!({
                "fn": "join_api_path",
                "input": { "base": base, "path": path },
                "expected": join_api_path(base, path),
            }));
        }
    }
    for base in chat_bases {
        for path in ["chat/completions", "messages"] {
            cases.push(json!({
                "fn": "join_chat_endpoint",
                "input": { "base": base, "path": path },
                "expected": join_chat_endpoint(base, path),
            }));
        }
    }
    for base in chat_bases {
        cases.push(json!({
            "fn": "anthropic_base_url",
            "input": { "base": base },
            "expected": anthropic_base_url(base),
        }));
    }
    with_cases(
        header(
            "端点拼接",
            "join_api_path：base_url 原样使用，只剥掉误填的对话端点后缀与末尾 #。\
             join_chat_endpoint：对话端点允许整条粘进来；path 为 messages（Anthropic）时按 anthropic_base_url 补 /v1。\
             anthropic_base_url：末段不是 v<数字> 就补 /v1，末尾 # 表示别补。",
        ),
        cases,
    )
}

// ── 模型清单清洗 ─────────────────────────────────────────────────

fn model_filter_json() -> Value {
    let ids = [
        "deepseek-flash",
        "gpt-4o",
        "gpt-6-sol",
        "claude-opus-5",
        "qwen-plus",
        "glm-4.6",
        "Qwen/Qwen3-VL-32B-Instruct",
        "Qwen3-Omni-30B-A3B-Instruct",
        "Qwen3-Omni-30B-A3B-Captioner",
        "Qwen/Qwen-Image-Edit-2509",
        "BAAI/bge-large-zh-v1.5",
        "bge-m3",
        "BAAI/bge-reranker-v2-m3",
        "text-embedding-3-large",
        "FunAudioLLM/CosyVoice2-0.5B",
        "Kwai-Kolors/Kolors",
        "whisper-1",
        "tts-1",
        "omni-moderation-latest",
        "Llama-Guard-4-12B",
        "LoRA/Qwen/Qwen2.5-7B-Instruct",
        // 大小写不敏感
        "BGE-M3",
        "",
        "   ",
    ];
    let mut cases: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({
                "fn": "is_chat_model_id",
                "input": { "id": id },
                "expected": is_chat_model_id(id),
            })
        })
        .collect();
    let lists: [&[&str]; 3] = [
        &[
            "deepseek-flash",
            "BAAI/bge-large-zh-v1.5",
            "Qwen/Qwen3-VL-32B-Instruct",
            "FunAudioLLM/CosyVoice2-0.5B",
            "Kwai-Kolors/Kolors",
        ],
        // 去重 + 去空白
        &[" gpt-4o ", "gpt-4o", "gpt-4o"],
        // 全被滤光：宁可原样摆出，也不给空下拉
        &["bge-m3", "text-embedding-3-large"],
    ];
    for ids in lists {
        let c = clean_fetched_models(ids.iter().copied());
        cases.push(json!({
            "fn": "clean_fetched_models",
            "input": { "ids": ids },
            "expected": { "models": c.models, "dropped": c.dropped, "droppedModels": c.dropped_models },
        }));
    }
    with_cases(
        header(
            "模型清单清洗",
            "排除法：按特征词滤掉向量 / 重排 / 语音 / 生图 / 审核等非对话模型；去重去空白；\
             全被滤光时返回原始清单（dropped 为 0）。",
        ),
        cases,
    )
}

// ── /models 响应解析 ─────────────────────────────────────────────

fn models_response_json() -> Value {
    let bodies = [
        (
            "OpenAI 规范",
            r#"{"object":"list","data":[{"id":"gpt-4o","object":"model","owned_by":"openai"},{"id":"gpt-4o-mini","object":"model"}]}"#,
        ),
        ("裸数组（字符串）", r#"["gpt-4o","deepseek-flash"]"#),
        ("裸数组（对象）", r#"[{"id":"a"},{"id":"b"}]"#),
        (
            "DeepSeek 报限额",
            r#"{"data":[{"id":"deepseek-flash","context_window":1048576,"max_output_tokens":393216}]}"#,
        ),
        (
            "OpenRouter 取当前服务商",
            r#"{"data":[{"id":"anthropic/claude-opus-5","context_length":1000000,"top_provider":{"context_length":200000,"max_completion_tokens":32000}}]}"#,
        ),
        (
            "数字写成字符串",
            r#"{"data":[{"id":"m","context_length":"65536"}]}"#,
        ),
        ("不是 JSON", "<!doctype html><html></html>"),
    ];
    let cases = bodies
        .iter()
        .map(|(name, body)| {
            json!({
                "fn": "parse_models_response",
                "name": name,
                "input": { "body": body },
                "expected": {
                    "ids": parse_model_ids(body),
                    "limits": parse_model_limits(body)
                        .into_iter()
                        .map(|(id, l)| json!({ "id": id, "limits": l }))
                        .collect::<Vec<_>>(),
                },
            })
        })
        .collect();
    with_cases(
        header(
            "/models 响应解析",
            "ids：{data:[{id}]} 或裸数组都接受。limits：只收录报了限额的模型，字段名按固定顺序尝试 \
             （context_length / context_window / max_input_tokens…），OpenRouter 优先取 top_provider。",
        ),
        cases,
    )
}

// ── 验证错误判定 ─────────────────────────────────────────────────

fn diagnose_json() -> Value {
    let inputs: [(u16, &str, &str, &str); 9] = [
        (
            401,
            r#"{"error":{"message":"invalid key"}}"#,
            "https://api.deepseek.com/v1/models",
            "https://api.deepseek.com/v1",
        ),
        (
            403,
            "{}",
            "https://api.deepseek.com/v1/models",
            "https://api.deepseek.com/v1",
        ),
        (
            404,
            "{}",
            "https://api.deepseek.com/models",
            "https://api.deepseek.com",
        ),
        (
            404,
            "{}",
            "https://api.deepseek.com/v1/models",
            "https://api.deepseek.com/v1",
        ),
        (
            404,
            "{}",
            "https://x.com/v1beta/openai/models",
            "https://x.com/v1beta/openai",
        ),
        (400, r#"{"message":"bad request"}"#, "u", "b"),
        (429, "Too Many Requests", "u", "b"),
        (500, "{}", "u", "b"),
        (502, "<html>Bad Gateway</html>", "u", "b"),
    ];
    let mut cases: Vec<Value> = inputs
        .iter()
        .map(|(status, body, url, base)| {
            json!({
                "fn": "diagnose",
                "input": { "status": status, "body": body, "requestedUrl": url, "baseUrl": base },
                "expected": serde_json::to_value(diagnose(*status, body, url, base)).expect("可序列化"),
            })
        })
        .collect();
    for base in [
        "https://api.deepseek.com",
        "https://api.deepseek.com/",
        "https://api.deepseek.com/v1",
        "https://open.bigmodel.cn/api/paas/v4",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "",
    ] {
        cases.push(json!({
            "fn": "suggest_url",
            "input": { "baseUrl": base },
            "expected": suggest_url(base),
        }));
    }
    with_cases(
        header(
            "验证错误判定",
            "HTTP 状态 + 响应体 → 结构化错误（code 判别字段）。401/403 → auth_failed；404 → not_found，\
             看不到版本段时带 suggested_url；其余 → malformed。detail 优先取 error.message / message。\
             unreachable / model_not_found / protocol_mismatch / missing_extra_field 不由本函数产生。",
        ),
        cases,
    )
}

// ── ai.profile 解析 / 生成 ───────────────────────────────────────

fn ai_profile_json() -> Value {
    let inputs: [(&str, &str); 12] = [
        (
            "规范单条",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"DeepSeek","provider":"openai","baseURL":"https://api.deepseek.com/v1","apiKey":"sk-1","model":"deepseek-flash"}}"#,
        ),
        (
            "字段别名 baseUrl",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","baseUrl":"https://a/v1","apiKey":"k","model":"m"}}"#,
        ),
        (
            "字段别名 base_url / api_key",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","base_url":"https://a/v1","api_key":"k","model":"m"}}"#,
        ),
        (
            "没给 model：用兜底值",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","baseURL":"https://a/v1","apiKey":"sk-1"}}"#,
        ),
        (
            "Anthropic 只填主机名",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"中转","provider":"anthropic","baseURL":"https://relay.example.com","apiKey":"sk-x","model":"claude-opus-5"}}"#,
        ),
        (
            "model 名推断 Anthropic",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","provider":"custom","baseURL":"https://a","apiKey":"k","model":"claude-sonnet-5"}}"#,
        ),
        (
            "规范打包",
            r#"{"kind":"ai.profile.bundle","v":1,"data":{"profiles":[{"name":"a","provider":"deepseek","baseURL":"https://api.deepseek.com/v1","apiKey":"k1","model":"deepseek-flash"},{"name":"b","provider":"anthropic","baseUrl":"https://api.anthropic.com/v1","apiKey":"k2","model":"claude-opus-5"}]}}"#,
        ),
        (
            "打包：OAuth 条目跳过",
            r#"{"kind":"ai.profile.bundle","v":1,"data":{"api_profiles":[{"name":"登录","auth_type":"oauth","model":"gpt-6"},{"name":"DeepSeek","provider":"deepseek","api_key":"sk-ds","base_url":"https://api.deepseek.com/v1","model":"deepseek-flash","auth_type":"api_key"}]}}"#,
        ),
        (
            "打包：全是 OAuth",
            r#"{"kind":"ai.profile.bundle","v":1,"data":{"api_profiles":[{"name":"x","auth_type":"oauth"}]}}"#,
        ),
        (
            "更高版本",
            r#"{"kind":"ai.profile","v":99,"data":{"model":"m"}}"#,
        ),
        ("不是 ai.profile", r#"{"hello":"world"}"#),
        ("空内容", ""),
    ];
    let mut cases: Vec<Value> = inputs
        .iter()
        .map(|(name, text)| {
            let expected = match parse_profiles(text, "deepseek-flash") {
                Ok(r) => json!({ "ok": r }),
                Err(e) => json!({ "error": e }),
            };
            json!({
                "fn": "parse_profiles",
                "name": name,
                "input": { "text": text, "defaultModel": "deepseek-flash" },
                "expected": expected,
            })
        })
        .collect();
    for (protocol, base) in [
        (Protocol::OpenAiCompatible, "https://api.deepseek.com/v1"),
        (Protocol::Anthropic, "https://api.anthropic.com/v1"),
    ] {
        let text = to_profile("示例", protocol, base, "sk-示例", "some-model");
        cases.push(json!({
            "fn": "to_profile",
            "input": { "name": "示例", "protocol": protocol, "baseUrl": base, "apiKey": "sk-示例", "model": "some-model" },
            // 比对解析后的 JSON 值，不比对字符串（键序、缩进各语言可以不同）
            "expected": serde_json::from_str::<Value>(&text).expect("生成的是合法 JSON"),
        }));
    }
    with_cases(
        header(
            "ai.profile 解析与生成",
            "解析宽进：接受 baseURL / baseUrl / base_url 等多种拼写，单条与打包统一返回列表，OAuth 条目跳过并计数。\
             生成严出：只产出规范写法。格式规范见 https://ai-profile.ruoyi.plus/api/protocol",
        ),
        cases,
    )
}

// ── 限额取值 ─────────────────────────────────────────────────────

fn limits_json() -> Value {
    let layer = |src: &str, cw: Option<u32>, mo: Option<u32>| match src {
        "user" => TokenLimits::from_user(cw, mo),
        "endpoint" => TokenLimits::from_endpoint(cw, mo),
        _ => TokenLimits::from_preset(cw, mo),
    };
    type Layer = (&'static str, Option<u32>, Option<u32>);
    let combos: [&[Layer]; 5] = [
        &[
            ("user", Some(64_000), None),
            ("endpoint", Some(128_000), Some(8_192)),
            ("preset", Some(1_000_000), Some(384_000)),
        ],
        &[
            ("endpoint", Some(128_000), None),
            ("preset", Some(1_000_000), Some(384_000)),
        ],
        &[("user", None, Some(4_096)), ("preset", Some(200_000), None)],
        &[("preset", Some(1_000_000), Some(384_000))],
        &[("user", None, None), ("endpoint", None, None)],
    ];
    let cases = combos
        .iter()
        .map(|layers| {
            let merged = layers
                .iter()
                .map(|(s, cw, mo)| layer(s, *cw, *mo))
                .reduce(|hi, lo| hi.or(lo))
                .expect("非空");
            json!({
                "fn": "merge_limits",
                "input": {
                    "layers": layers.iter().map(|(s, cw, mo)| json!({ "source": s, "contextWindow": cw, "maxOutput": mo })).collect::<Vec<_>>()
                },
                "expected": merged,
            })
        })
        .collect();
    with_cases(
        header(
            "限额逐字段取值",
            "layers 按优先级从高到低（user > endpoint > preset），从左往右两两合并 merge(merge(l0, l1), l2)。\
             merge(hi, lo)：hi 两个字段都为空时整条换成 lo（连 source 一起，空的用户设置不能冒充来源）；\
             否则逐字段取 hi 的值、为空才用 lo 的，source 保留 hi 的。",
        ),
        cases,
    )
}

/// 全部生成物：`spec/` 下的相对路径 → 文件内容（带末尾换行的格式化 JSON）。
pub fn render_all() -> BTreeMap<&'static str, String> {
    let files = [
        ("presets.json", presets_json()),
        ("conformance/endpoint.json", endpoint_json()),
        ("conformance/model_filter.json", model_filter_json()),
        ("conformance/models_response.json", models_response_json()),
        ("conformance/diagnose.json", diagnose_json()),
        ("conformance/ai_profile.json", ai_profile_json()),
        ("conformance/limits.json", limits_json()),
    ];
    files
        .into_iter()
        .map(|(p, v)| {
            (
                p,
                serde_json::to_string_pretty(&v).expect("可序列化") + "\n",
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 已提交的 spec/ 必须与代码一致。不一致的修法：`cargo xtask gen-spec`。
    ///
    /// 这是「用例永远跟着代码走」的唯一保证 —— 改了拼接规则却忘了重新生成，
    /// 别的语言照着旧用例写出来的实现就会与 Rust 版悄悄分叉。
    #[test]
    fn spec_files_in_sync() {
        let root = crate::repo_root().join("spec");
        for (rel, expected) in render_all() {
            let path = root.join(rel);
            let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "读不到 {}：{e}\n跑 `cargo xtask gen-spec` 生成",
                    path.display()
                )
            });
            assert_eq!(
                on_disk.replace("\r\n", "\n"),
                expected,
                "spec/{rel} 与代码不一致 —— 跑 `cargo xtask gen-spec` 重新生成"
            );
        }
    }

    /// 每个用例文件都要有用例，且带格式版本 —— 防止生成器改坏后产出空文件还照样「同步」
    #[test]
    fn every_conformance_file_has_cases() {
        for (rel, text) in render_all() {
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["specVersion"], SPEC_VERSION, "{rel}");
            if rel.starts_with("conformance/") {
                assert!(
                    v["cases"].as_array().is_some_and(|c| !c.is_empty()),
                    "{rel} 没有用例"
                );
            }
        }
    }
}
