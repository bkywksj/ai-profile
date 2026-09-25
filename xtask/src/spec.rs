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

use ai_profile::client::{
    check_required_fields, diagnose, parse_model_ids, parse_model_limits, suggest_url,
};
use ai_profile::endpoint::{
    anthropic_base_url, ends_with_version_segment, join_api_path, join_chat_endpoint,
};
use ai_profile::history::is_context_overflow;
use ai_profile::model_filter::{clean_fetched_models, is_chat_model_id};
use ai_profile::preset::{infer_preset_key, model_limits, presets, vendors_all};
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

/// 附上规则用到的数据表。
///
/// 🔴 只给用例不给表，其他语言的实现只能对着用例凑特征词 —— 第一次外部试写（Python）就是这样：
/// 用例全过，但凑出来的词表把 `FLUX.1-dev` 判成了对话模型。表直接取自 crate 的公开常量，不另抄一份。
fn with_rules(mut v: Value, rules: Value) -> Value {
    v["rules"] = rules;
    v
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
        // 只剥 chat/completions 与 messages 两种；其它端点名原样当 base
        "https://proxy.example.com/v1/completions",
        "https://proxy.example.com/v1/responses",
        // 先去末尾 #、再去末尾 /、再剥后缀
        "https://proxy.example.com/v1/chat/completions/#",
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
    // 版本段判定：只认「v + 纯数字」，主机名以 v4 开头的不算
    for base in [
        "https://api.deepseek.com/v1",
        "https://api.deepseek.com/v1/",
        "https://open.bigmodel.cn/api/paas/v4",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "https://x.com/v1beta",
        "https://api.deepseek.com",
        "https://v4.example.com",
        "https://x.com/v",
        "",
    ] {
        cases.push(json!({
            "fn": "ends_with_version_segment",
            "input": { "base": base },
            "expected": ends_with_version_segment(base),
        }));
    }
    with_cases(
        header(
            "端点拼接",
            "join_api_path：base_url 原样使用。处理顺序：去两端空白 → 去末尾所有 # → 去末尾所有 / → \
             若以 chat/completions 或 messages 结尾（只认这两种，按此顺序、只剥一次）则剥掉并再去末尾 / → 拼上 /path。\
             join_chat_endpoint：对话端点允许整条粘进来；path 为 messages（Anthropic）时按 anthropic_base_url 补 /v1。\
             anthropic_base_url：末段不是 v<数字> 就补 /v1，末尾 # 表示别补。\
             ends_with_version_segment：末段（去掉末尾 /）是否形如 v + 纯数字；v1beta、主机名 v4.example.com 都不算。",
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
        // 外部试写时凑出的词表恰好漏了它 —— 特征词见 rules.nonChatMarkers 的 flux
        "black-forest-labs/FLUX.1-dev",
        "whisper-1",
        "tts-1",
        // 生图 / 视频 / 配音（来自本库非对话预置）
        "dall-e-3",
        "doubao-seedream-4-0-250828",
        "doubao-seedance-1-0-pro-250528",
        "wan2.2-t2i-flash",
        "Wan-AI/Wan2.2-I2V-A14B",
        "vidu/viduq3-pro_img2video",
        "cogvideox-3",
        "MiniMax-Hailuo-02",
        "fishaudio/fish-speech-1.5",
        // 名字相近但能对话，不能误伤
        "doubao-seed-1-6-250615",
        "gpt-4o-audio-preview",
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
    with_rules(
        with_cases(
            header(
                "模型清单清洗",
                "is_chat_model_id：id 去两端空白、ASCII 小写后，为空 → false；以 rules.nonChatPrefixes 任一开头 → false；\
                 包含 rules.nonChatMarkers 任一子串 → false；其余一律 true（排除法，未知名称放行）。\
                 clean_fetched_models：逐个去两端空白、丢空串、按首次出现去重（保持顺序），再按上面的规则分成 models 与 droppedModels；\
                 models 为空（全被滤光）时改为返回去重后的原始清单，dropped 为 0、droppedModels 为空。",
            ),
            cases,
        ),
        json!({
            "nonChatMarkers": ai_profile::model_filter::NON_CHAT_MARKERS,
            "nonChatPrefixes": ai_profile::model_filter::NON_CHAT_PREFIXES,
        }),
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
        (
            "字符串两端空白去掉；带小数点的字符串不认",
            r#"{"data":[{"id":"a","context_length":" 65536 "},{"id":"b","context_length":"65536.0"}]}"#,
        ),
        (
            "0 视为没有；负数不认；浮点取整",
            r#"{"data":[{"id":"a","context_length":0,"max_tokens":4096},{"id":"b","context_length":-1},{"id":"c","context_length":131072.9}]}"#,
        ),
        (
            "top_provider 只报了窗口：输出上限回落到顶层",
            r#"{"data":[{"id":"m","context_length":1000000,"max_completion_tokens":8192,"top_provider":{"context_length":200000}}]}"#,
        ),
        (
            "同一层多个字段：按表的顺序取第一个有效的",
            r#"{"data":[{"id":"m","max_input_tokens":100,"context_window":200,"max_tokens":10,"max_output_tokens":20}]}"#,
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
    with_rules(
        with_cases(
            header(
                "/models 响应解析",
                "parse_models_response 是两个函数的合并：ids = parse_model_ids，limits = parse_model_limits。\
                 ids：{data:[…]} 或裸数组都接受，元素是对象取 id、是字符串取本身。\
                 limits：每条模型的上下文窗口按 rules.contextWindowFields、输出上限按 rules.maxOutputFields 取值 —— \
                 先按表的顺序查 top_provider 对象里的字段，都没有再按同样顺序查顶层；两个值各自独立。\
                 有效值：正整数；非负浮点取整；字符串去两端空白后按整数解析（带小数点的不认）；0、负数、超出 u32 视为没有。\
                 两个值都没有的模型不收录。用例里每项写成 {id, limits}；注意 verify 成功结果里的 modelLimits 线格式是 [id, limits] 二元数组。",
            ),
            cases,
        ),
        json!({
            "contextWindowFields": ai_profile::limits::CONTEXT_WINDOW_FIELDS,
            "maxOutputFields": ai_profile::limits::MAX_OUTPUT_FIELDS,
            "nestedFirst": "top_provider",
        }),
    )
}

// ── 验证错误判定 ─────────────────────────────────────────────────

fn diagnose_json() -> Value {
    let long_message = format!(r#"{{"error":{{"message":"{}"}}}}"#, "很长的报错".repeat(80));
    let inputs: [(u16, &str, &str, &str); 13] = [
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
        // error 直接是字符串的写法
        (400, r#"{"error":"model not supported"}"#, "u", "b"),
        // error.message 优先于顶层 message；两端空白去掉
        (
            400,
            r#"{"error":{"message":"  from error  "},"message":"from top"}"#,
            "u",
            "b",
        ),
        // 只有空白的 message 等于没有 → 退回 HTTP 状态码
        (400, r#"{"message":"   "}"#, "u", "b"),
        // 超过 300 个字符（按字符数，不按字节）截断
        (400, &long_message, "u", "b"),
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
        // 路径里只认字面的 /v1beta/ 与 /v1/；其它非版本路径照样建议补 /v1
        "https://x.com/api",
        "https://x.com/v2beta/openai",
        "https://x.com/v1/extra",
        "",
    ] {
        cases.push(json!({
            "fn": "suggest_url",
            "input": { "baseUrl": base },
            "expected": suggest_url(base),
        }));
    }
    // 发请求前的必填字段检查：结果只取决于预置数据，其他语言用 presets.json 就能复现
    type ExtraCase<'a> = (Option<&'a str>, &'a [(&'a str, &'a str)]);
    let extras: [ExtraCase; 6] = [
        (Some("volc_tts"), &[("appid", "123"), ("cluster", "")]),
        (Some("volc_tts"), &[("cluster", "volcano_tts")]),
        // 只有空白等于没填
        (Some("volc_tts"), &[("appid", "   ")]),
        (Some("deepseek"), &[]),
        // 未知预置 / 自定义服务商：没有可查的必填项，放行
        (None, &[]),
        (Some("no_such_preset"), &[]),
    ];
    for (preset_key, extra) in extras {
        let expected = match check_required_fields(preset_key, extra) {
            Ok(()) => json!({ "ok": null }),
            Err(e) => json!({ "error": e }),
        };
        cases.push(json!({
            "fn": "check_required_fields",
            "input": {
                "presetKey": preset_key,
                "extra": extra.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect::<Vec<_>>(),
            },
            "expected": expected,
        }));
    }
    with_cases(
        header(
            "验证错误判定",
            "diagnose：HTTP 状态 + 响应体 → 结构化错误（code 判别字段）。401/403 → auth_failed；\
             404 → not_found（requested_url 原样，suggested_url = suggest_url(baseUrl)）；其余 → malformed。\
             detail：响应体是 JSON 时依次取 error.message、error（本身是字符串时）、顶层 message，去两端空白后非空即用，\
             超过 300 个字符（按字符数）截断；都取不到时为「HTTP <状态码>」。响应体的其余内容不进 detail。\
             suggest_url：去两端空白与末尾 / 后，为空、或末段是版本段（见 endpoint.json 的 ends_with_version_segment）、\
             或包含字面的 /v1beta/ 或 /v1/ → null；否则建议「<去掉末尾 / 的地址>/v1」。\
             check_required_fields：按 presetKey 在 presets.json 里找预置（找不到、或为 null → 放行，返回 ok）；\
             对它 extraFields 中 required 为 true 的每一项，extra 里要有同 key 且值去空白后非空的条目，\
             否则返回第一个缺的 missing_extra_field。defaultExtra 不算已填。extra 在用例里写成 [{key, value}]。\
             返回值为空的 Result 在用例里写成 {\"ok\": null}。unreachable 由网络层产生，不在本文件范围。",
        ),
        cases,
    )
}

// ── ai.profile 解析 / 生成 ───────────────────────────────────────

fn ai_profile_json() -> Value {
    let inputs: [(&str, &str); 20] = [
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
        ("只有空白", "  \n\t "),
        ("不是 JSON", "kind: ai.profile"),
        ("JSON 但不是对象", "[1, 2]"),
        // 检查顺序：空 → JSON → 版本 → kind → data
        ("版本先于 kind 检查", r#"{"kind":"something.else","v":99}"#),
        ("data 不是对象", r#"{"kind":"ai.profile","v":1,"data":"x"}"#),
        (
            "model 为空串：等同没给",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","baseURL":"https://a/v1","apiKey":"k","model":""}}"#,
        ),
        (
            "字段两端空白",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"  x  ","baseURL":"  https://a/v1  ","apiKey":"  k  ","model":"  m  "}}"#,
        ),
        (
            "打包缺 profiles 字段",
            r#"{"kind":"ai.profile.bundle","v":1,"data":{}}"#,
        ),
    ];
    let mut cases: Vec<Value> = inputs
        .iter()
        .map(|(name, text)| {
            let expected = match parse_profiles(text, "deepseek-flash") {
                Ok(r) => json!({ "ok": r }),
                Err(e) => json!({ "error": e }),
            };
            let mut case = json!({
                "fn": "parse_profiles",
                "name": name,
                "input": { "text": text, "defaultModel": "deepseek-flash" },
                "expected": expected,
            });
            // invalid_json 的 detail 是 Rust JSON 库的报错原文，其他语言不可能逐字复现 —— 标明比较时跳过
            if case["expected"]["error"]["code"] == "invalid_json" {
                case["ignore"] = json!(["error.detail"]);
            }
            case
        })
        .collect();
    // 协议推断看的是来源里**原始的** model，不是兜底后的 —— 兜底值是 claude-* 也不能据此判成 Anthropic
    for (name, text, default_model) in [
        (
            "推断协议只看原始 model，不看兜底值",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","baseURL":"https://a/v1","apiKey":"k"}}"#,
            "claude-opus-5",
        ),
        (
            "toolId 提示也参与推断",
            r#"{"kind":"ai.profile","v":1,"data":{"name":"x","baseURL":"https://a","apiKey":"k","model":"m","hints":{"toolId":"claude-code"}}}"#,
            "deepseek-flash",
        ),
    ] {
        cases.push(json!({
            "fn": "parse_profiles",
            "name": name,
            "input": { "text": text, "defaultModel": default_model },
            "expected": { "ok": parse_profiles(text, default_model).expect("合法输入") },
        }));
    }
    for (protocol, base, model) in [
        (
            Protocol::OpenAiCompatible,
            "https://api.deepseek.com/v1",
            "some-model",
        ),
        (
            Protocol::Anthropic,
            "https://api.anthropic.com/v1",
            "some-model",
        ),
        // model 为空时照样输出空串字段，不省略
        (
            Protocol::OpenAiCompatible,
            "https://api.deepseek.com/v1",
            "",
        ),
    ] {
        let text = to_profile("示例", protocol, base, "sk-示例", model);
        cases.push(json!({
            "fn": "to_profile",
            "input": { "name": "示例", "protocol": protocol, "baseUrl": base, "apiKey": "sk-示例", "model": model },
            // 比对解析后的 JSON 值，不比对字符串（键序、缩进各语言可以不同）
            "expected": serde_json::from_str::<Value>(&text).expect("生成的是合法 JSON"),
        }));
    }
    with_cases(
        header(
            "ai.profile 解析与生成",
            "parse_profiles 检查顺序：去两端空白后为空 → empty；不是 JSON 对象 → invalid_json（detail 是自由文本，用例里标了 ignore）；\
             v 大于 1 → unsupported_version（先于 kind 检查）；kind 既不是 ai.profile 也不是 ai.profile.bundle → not_ai_profile（found 为读到的 kind，缺失为空串）；\
             data 缺失或不是对象 → missing_data。\
             单条：data 里 baseURL / baseUrl / base_url、apiKey / api_key、toolId / tool_id 都认；各字段去两端空白；\
             model 去空白后为空 → 用 defaultModel 并标 modelFallback: true。\
             协议推断（看原始 model，不看兜底值）：provider 小写后含 anthropic 或 claude → anthropic；否则 model 小写后以 claude- 开头 → anthropic；\
             否则 toolId（hints.toolId 优先，其次顶层 toolId）小写后含 claude 或 anthropic → anthropic；其余 → openai_compatible。rawProvider 为去空白的原始 provider。\
             打包：条目在 data.profiles（或 data.api_profiles），authType / auth_type 为 oauth（不区分大小写）的跳过并计入 skipped；\
             一条都没剩 → empty_bundle（skipped 为跳过数）。\
             to_profile：只产出规范写法（baseURL / apiKey），provider 为 anthropic 或 openai，model 为空也照样输出空串字段。\
             格式规范见 https://ai-profile.ruoyi.plus/api/protocol",
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
             否则逐字段取 hi 的值、为空才用 lo 的，source 保留 hi 的。\
             推论：所有层都为空时，结果是最后一层（两个 null + 最后一层的 source）—— 调用方看两个值都是 null 即视为「未知」，不看 source。",
        ),
        cases,
    )
}

// ── 从已存配置反推预置 ───────────────────────────────────────────

fn preset_lookup_json() -> Value {
    let inputs: [(Protocol, Option<&str>); 13] = [
        (Protocol::Anthropic, None),
        (Protocol::Anthropic, Some("https://api.anthropic.com")),
        // 官方地址带不带 /v1 都是官方档
        (Protocol::Anthropic, Some("https://api.anthropic.com/v1")),
        (Protocol::Anthropic, Some("https://relay.example.com")),
        (Protocol::OpenAiCompatible, None),
        (
            Protocol::OpenAiCompatible,
            Some("https://api.deepseek.com/v1"),
        ),
        // 存量配置常是不带版本段的旧写法：按主机名匹配，不按全串
        (Protocol::OpenAiCompatible, Some("https://api.deepseek.com")),
        // 大小写不敏感
        (
            Protocol::OpenAiCompatible,
            Some("HTTPS://API.DEEPSEEK.COM/v1"),
        ),
        (
            Protocol::OpenAiCompatible,
            Some("https://open.bigmodel.cn/api/paas/v4"),
        ),
        (
            Protocol::OpenAiCompatible,
            Some("https://unknown.example.com/v1"),
        ),
        // matchHosts 可以带端口（本地服务），同样是子串匹配
        (
            Protocol::OpenAiCompatible,
            Some("http://localhost:11434/v1"),
        ),
        // OpenAI 兼容协议只在「OpenAI 兼容的对话预置」里找：填了 Anthropic 官方地址也认不出
        (
            Protocol::OpenAiCompatible,
            Some("https://api.anthropic.com/v1"),
        ),
        (Protocol::OpenAiCompatible, Some("   ")),
    ];
    let mut cases: Vec<Value> = inputs
        .iter()
        .map(|(protocol, base)| {
            json!({
                "fn": "infer_preset_key",
                "input": { "protocol": protocol, "baseUrl": base },
                "expected": infer_preset_key(*protocol, *base),
            })
        })
        .collect();
    let limits_inputs: [(Protocol, Option<&str>, &str); 6] = [
        (
            Protocol::OpenAiCompatible,
            Some("https://api.deepseek.com/v1"),
            "deepseek-flash",
        ),
        // model 两端空白要去掉
        (
            Protocol::OpenAiCompatible,
            Some("https://api.deepseek.com"),
            "  deepseek-flash ",
        ),
        // 预置里有这家、但没有这个模型：不猜
        (
            Protocol::OpenAiCompatible,
            Some("https://api.deepseek.com/v1"),
            "not-a-real-model",
        ),
        // 自定义端点：没有预置可查
        (
            Protocol::OpenAiCompatible,
            Some("https://unknown.example.com/v1"),
            "deepseek-flash",
        ),
        (Protocol::OpenAiCompatible, None, "whatever"),
        // 找到了模型，但预置没登记限额（两个值都空）→ null，不返回全空的限额对象
        (Protocol::Anthropic, None, "claude-opus-5-5"),
    ];
    for (protocol, base, model) in limits_inputs {
        cases.push(json!({
            "fn": "model_limits",
            "input": { "protocol": protocol, "baseUrl": base, "model": model },
            "expected": model_limits(protocol, base, model),
        }));
    }
    with_cases(
        header(
            "从已存配置反推预置",
            "infer_preset_key：baseUrl（null 当空串）去两端空白、ASCII 小写后记为 url。\
             Anthropic 协议：url 为空或包含 \"://api.anthropic.com\" → anthropic_official，否则 → claude_code。\
             其它协议：url 为空 → openai_compatible_custom；否则按 presets.json 的顺序，只看 kind 为 chat 且 protocol 为 \
             openai_compatible 的预置，url 包含它 matchHosts 里任一项（子串，可带端口）即返回它的 key；都不中 → openai_compatible_custom。\
             子串匹配让不带版本段的老地址也能认出来。\
             model_limits：先 infer_preset_key 找到预置，再用 model（去两端空白）与它 models 的 value 精确匹配；\
             匹配到且 contextWindow、maxOutput 至少一个非空 → 返回这两个值加 source: preset；其余情况一律 null，不猜。",
        ),
        cases,
    )
}

// ── 超长报错识别 ─────────────────────────────────────────────────

fn history_json() -> Value {
    let inputs: [(&str, u16, &str); 16] = [
        (
            "OpenAI / DeepSeek / vLLM",
            400,
            r#"{"error":{"message":"This model's maximum context length is 131072 tokens. However, you requested 140000 tokens (135904 in the messages, 4096 in the completion).","type":"invalid_request_error","code":"context_length_exceeded"}}"#,
        ),
        (
            "Anthropic",
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 215000 tokens > 200000 maximum"}}"#,
        ),
        (
            "Gemini（OpenAI 兼容层）",
            400,
            r#"[{"error":{"code":400,"message":"The input token count (1200000) exceeds the maximum number of tokens allowed (1048576).","status":"INVALID_ARGUMENT"}}]"#,
        ),
        (
            "Kimi",
            400,
            r#"{"error":{"message":"Invalid request: Your request exceeded model token limit: 131072","type":"invalid_request_error"}}"#,
        ),
        (
            "通义",
            400,
            r#"{"error":{"message":"Range of input length should be [1, 129024]","type":"invalid_request_error","code":"invalid_parameter_error"}}"#,
        ),
        (
            "OpenRouter",
            400,
            r#"{"error":{"message":"This endpoint's maximum context length is 200000 tokens. However, you requested about 230000 tokens","code":400}}"#,
        ),
        (
            "智谱（中文片段原文匹配）",
            400,
            r#"{"error":{"code":"1261","message":"Prompt 超长"}}"#,
        ),
        (
            "422 同样检查",
            422,
            r#"{"detail":"Input is too long for requested model."}"#,
        ),
        (
            "大小写不敏感",
            400,
            r#"{"error":{"message":"CONTEXT WINDOW EXCEEDED"}}"#,
        ),
        (
            "提到 max_tokens 但也提到 context：仍算超长",
            400,
            r#"{"error":{"message":"max_tokens + messages exceed the context length"}}"#,
        ),
        ("请求体过大", 413, "Request Entity Too Large"),
        (
            "限流：提到 tokens 但与窗口无关",
            429,
            r#"{"error":{"message":"Rate limit reached for gpt-x on tokens per min (TPM): Limit 30000, Used 29000","type":"tokens"}}"#,
        ),
        (
            "输出上限太大：裁历史解决不了",
            400,
            r#"{"error":{"message":"max_tokens is too large: 50000. This model supports at most 8192 completion tokens.","type":"invalid_request_error"}}"#,
        ),
        (
            "Anthropic 输出上限",
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 64000 > 32000, which is the maximum allowed number of output tokens"}}"#,
        ),
        (
            "鉴权失败",
            401,
            r#"{"error":{"message":"maximum context length"}}"#,
        ),
        ("服务端错误", 500, "context length exceeded"),
    ];
    let cases = inputs
        .iter()
        .map(|(name, status, body)| {
            json!({
                "fn": "is_context_overflow",
                "name": name,
                "input": { "status": status, "body": body },
                "expected": is_context_overflow(*status, body),
            })
        })
        .collect();
    with_rules(
        with_cases(
            header(
                "超长报错识别",
                "is_context_overflow：宁可漏判不可误判（误判会把用户历史无谓地裁掉一半）。按顺序：\
                 ① 状态码 413 → true；② 状态码不是 400 或 422 → false；③ 响应体 ASCII 小写后记为 b（非 ASCII 字符不变）；\
                 ④ b 包含 max_tokens 或 max_completion_tokens，且不含 context、prompt、input 中任何一个 → false（输出上限问题，裁历史没用）；\
                 ⑤ b 包含 rules.patterns 任一子串 → true，否则 false。",
            ),
            cases,
        ),
        json!({ "patterns": ai_profile::history::CONTEXT_OVERFLOW_PATTERNS }),
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
        ("conformance/preset_lookup.json", preset_lookup_json()),
        ("conformance/history.json", history_json()),
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
