//! 冒烟测试：从**外部**调用公开 API，确认 crate 对下游真的可用。
//!
//! 单元测试在模块内部，能访问私有项；这里只用 `pub` 接口 —— 等价于下游应用的视角。
//! 忘了 `pub use` 某个类型、或把某项设成 `pub(crate)`，单元测试照样绿，这里会红。

use ai_profile::{preset, Kind, Protocol};

#[test]
fn public_api_is_usable_from_outside() {
    // 1. 拿全部预置
    let all = preset::presets();
    assert!(all.len() >= 16, "chat 预置应有 16 家，实际 {}", all.len());

    // 2. 按 kind 过滤
    let chat: Vec<_> = preset::presets_for(Kind::Chat).collect();
    assert_eq!(chat.len(), all.len(), "当前只有 chat 一种 kind");

    // 3. 按 key 查
    let ds = preset::preset_by_key("deepseek").expect("deepseek 应存在");
    assert_eq!(ds.base_url, Some("https://api.deepseek.com/v1"));
    assert_eq!(ds.protocol, Protocol::OpenAiCompatible);

    // 4. 反推：存量配置（不带版本段的旧写法）要认得回来
    assert_eq!(
        preset::infer_preset_key(Protocol::OpenAiCompatible, Some("https://api.deepseek.com")),
        "deepseek"
    );

    // 5. 厂商聚合
    let vs = preset::vendors(&[Kind::Chat]);
    assert!(!vs.is_empty());
    assert!(vs.iter().any(|v| v.is_local), "应当有本地服务（Ollama 等）");
    assert!(
        vs.iter().any(|v| v.apply_url.is_some()),
        "应当有可申请密钥的云服务"
    );

    // 6. 端点拼接：原样使用，不补版本段
    let url = ai_profile::endpoint::join_chat_endpoint(ds.base_url.unwrap(), "chat/completions");
    assert_eq!(url, "https://api.deepseek.com/v1/chat/completions");

    // 7. 序列化给前端 —— 字段名必须是 camelCase
    let json = serde_json::to_string(ds).expect("预置应可序列化");
    assert!(json.contains("\"vendorId\""), "字段应是 camelCase：{json}");
    assert!(json.contains("\"isLocal\""), "字段应是 camelCase：{json}");
}

/// 🔴 每条预置的默认 model 必须在它自己的 models 清单里（除非清单为空）。
///
/// 不一致会让「默认值」在下拉里选不中 —— 用户打开表单就看到一个选不中的值，
/// 以为是自己配错了。
#[test]
fn default_model_is_in_its_own_list() {
    for p in preset::presets() {
        if p.models.is_empty() || p.model.is_empty() {
            continue;
        }
        assert!(
            p.models.iter().any(|m| m.value == p.model),
            "{}: 默认 model `{}` 不在自己的 models 清单里",
            p.key,
            p.model
        );
    }
}

/// 没有 base_url 的预置必须能被用户自己填地址 —— 即它不该同时没有 base_url
/// 又没有任何 match_hosts 和提示，那样用户不知道该填什么。
#[test]
fn presets_without_base_url_have_guidance() {
    for p in preset::presets() {
        if p.base_url.is_some() {
            continue;
        }
        assert!(
            p.hint.is_some() || !p.match_hosts.is_empty(),
            "{}: 既没预填地址也没提示，用户不知道该填什么",
            p.key
        );
    }
}
