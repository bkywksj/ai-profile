//! 冒烟测试：从**外部**调用公开 API，确认 crate 对下游真的可用。
//!
//! 单元测试在模块内部，能访问私有项；这里只用 `pub` 接口 —— 等价于下游应用的视角。
//! 忘了 `pub use` 某个类型、或把某项设成 `pub(crate)`，单元测试照样绿，这里会红。

use ai_profile::{preset, protocol, Kind, Protocol};

#[test]
fn public_api_is_usable_from_outside() {
    // 1. 拿全部预置
    let all = preset::presets();
    assert!(all.len() >= 25, "chat 预置应有 25 家，实际 {}", all.len());

    // 2. 按 kind 过滤
    let chat: Vec<_> = preset::presets_for(Kind::Chat).collect();
    assert!(chat.len() >= 25, "chat 预置应有 25 家，实际 {}", chat.len());
    #[cfg(not(any(feature = "image", feature = "video", feature = "tts")))]
    assert_eq!(chat.len(), all.len(), "只开 chat 时全部都是对话预置");
    // 开了多模态：每种都有预置，且下游不用关心数组怎么拼的
    #[cfg(feature = "image")]
    assert!(preset::presets_for(Kind::Image).count() >= 4);
    #[cfg(feature = "video")]
    assert!(preset::presets_for(Kind::Video).count() >= 7);
    #[cfg(feature = "tts")]
    assert!(preset::preset_by_key("volc_tts").is_some_and(|p| p
        .extra_fields
        .iter()
        .any(|f| f.key == "appid" && f.required)));

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

/// 🔴 协议的真正用途：A 应用生成 → B 应用解析，字段一个不丢。
///
/// 这是 `ai.profile` 存在的全部理由。调研时发现四份既有实现里有一份漏了
/// `baseUrl` 别名 —— 两边都自认为"实现了本协议"，实际互导会静默丢字段。
#[test]
fn cross_app_interop() {
    // A 应用：用户配好一条，分享出去
    let shared = protocol::to_profile(
        "公司 DeepSeek",
        Protocol::OpenAiCompatible,
        "https://api.deepseek.com/v1",
        "sk-shared-secret",
        "deepseek-flash",
    );

    // B 应用：粘贴导入。default_model 传自己的预置默认值
    let ds = preset::preset_by_key("deepseek").unwrap();
    let got = protocol::parse_profile(&shared, ds.model).expect("应能解析 A 生成的配置");

    assert_eq!(got.name, "公司 DeepSeek");
    assert_eq!(got.base_url, "https://api.deepseek.com/v1");
    assert_eq!(got.api_key, "sk-shared-secret");
    assert_eq!(got.model, "deepseek-flash");
    assert!(!got.model_fallback, "来源给了 model，不该标记为兜底");

    // B 应用还能把它认回对应的预置模板
    assert_eq!(
        preset::infer_preset_key(got.protocol, Some(&got.base_url)),
        "deepseek"
    );
}

/// 来源软件用了非规范拼写时，接收方仍要认得 —— 这是宽进的意义。
#[test]
fn tolerates_foreign_spellings() {
    let foreign = r#"{"kind":"ai.profile","v":1,"data":{
        "name":"来自某软件","provider":"custom",
        "baseUrl":"https://cc.example.cn/v1","api_key":"sk-x","model":"claude-opus-5"}}"#;
    let got = protocol::parse_profile(foreign, "fallback").expect("非规范拼写也应接受");
    assert_eq!(got.base_url, "https://cc.example.cn/v1");
    assert_eq!(got.api_key, "sk-x");
    // provider 写的是 custom，但 model 名暴露了它其实要走 Anthropic 端点
    assert_eq!(got.protocol, Protocol::Anthropic);
}

/// 🔴 `ServiceConfig` 带 `#[non_exhaustive]`，外部 crate 不能用字面量构造它 ——
/// 必须能走完整的 builder 链。
///
/// 这条是实测踩出来的：最初只有 `new()` 没有 `with_*()`，xtask 作为外部 crate
/// 编译直接报 E0639。**单元测试抓不到** —— 它们在 crate 内部，不受该限制。
/// 集成测试是这类问题的唯一防线。
#[cfg(feature = "client")]
#[test]
fn service_config_builder_is_usable_from_outside() {
    use ai_profile::client::ServiceConfig;

    let extra = [("appid", "123"), ("cluster", "volcano_tts")];
    let cfg = ServiceConfig::new(Protocol::OpenAiCompatible, "https://api.deepseek.com/v1")
        .with_preset("deepseek")
        .with_api_key("sk-test")
        .with_model("deepseek-flash")
        .with_extra(&extra);

    assert_eq!(cfg.preset_key, Some("deepseek"));
    assert_eq!(cfg.api_key, "sk-test");
    assert_eq!(cfg.model, "deepseek-flash");
    assert_eq!(cfg.extra.len(), 2);
}

/// 缺必填专有字段时，调用方能在**发请求之前**就判出来 —— 用于禁用按钮。
#[cfg(feature = "client")]
#[test]
fn required_fields_check_is_public() {
    use ai_profile::client::check_required_fields;
    assert!(check_required_fields(Some("deepseek"), &[]).is_ok());
    assert!(check_required_fields(None, &[]).is_ok());
}

/// 🔴 `Verifier` 必须能从外部建、能跨线程共享、能并发调用。
///
/// 这条守的是本类型存在的全部意义：**复用连接池**。桌面应用会把它存进全局状态
/// （Tauri 的 `AppState`、`OnceLock`…），那就要求 `Send + Sync + 'static`；
/// 「全部验证」那种批量按钮要 `join_all`，那就要求 `verify` 只借 `&self`。
///
/// 任何一条被破坏（比如给结构体加了 `RefCell`、把 `verify` 改成 `&mut self`），
/// 这个测试会直接编译失败 —— 而不是等下游应用升级时才炸。
#[cfg(feature = "client")]
#[test]
fn verifier_is_shareable_and_concurrent() {
    use ai_profile::client::{ServiceConfig, Verifier};
    use ai_profile::Protocol;

    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<Verifier>();

    let verifier = Verifier::new().expect("默认 Verifier 应当建得起来");

    // 只借 &self —— 同一个实例能同时持有多个未完成的验证 future。
    // 这里不真的发请求（不 await），要的就是「这段能编译」：
    // 两个 future 同时持有 &verifier，改成 &mut self 就会在这里报错。
    let a = verifier.verify(ServiceConfig::new(
        Protocol::OpenAiCompatible,
        "https://a.example/v1",
    ));
    let b = verifier.verify(ServiceConfig::new(
        Protocol::OpenAiCompatible,
        "https://b.example/v1",
    ));
    drop((a, b));

    // Clone 是浅拷贝（内部 Arc），克隆出来的共用同一个连接池
    let cloned = verifier.clone();
    std::thread::spawn(move || drop(cloned)).join().unwrap();
}

/// 🔴 三层回退：端点上报 > 预置静态 > None，且三者可分辨。
///
/// 这是 `limits` 模块存在的全部理由。调研真实故障时几乎每一条都源于
/// 「把猜的数字当成真的」——某网关丢掉元数据，客户端回落硬编码表，
/// 512K 的模型被当成 131K 提前触发压缩，而**没有任何迹象**表明数字是猜的。
///
/// 所以这里不只验「能不能拿到数字」，更验「拿到的数字知不知道自己是哪来的」。
#[cfg(feature = "client")]
#[test]
fn token_limits_three_layer_fallback() {
    use ai_profile::client::parse_model_limits;
    use ai_profile::{preset, LimitSource};

    // ── 第 1 层：端点报了 → 用它，标记 Endpoint
    let body = r#"{"data":[{"id":"x/y","context_length":262144,
                  "top_provider":{"context_length":131072,"max_completion_tokens":32768}}]}"#;
    let from_endpoint = parse_model_limits(body);
    assert_eq!(from_endpoint.len(), 1);
    let (id, l) = &from_endpoint[0];
    assert_eq!(id, "x/y");
    assert_eq!(l.source, LimitSource::Endpoint);
    assert_eq!(
        l.context_window,
        Some(131_072),
        "top_provider 是这家实际给的额度，优先级高于模型标称值"
    );

    // ── 第 2 层：端点是 OpenAI 裸规范（最常见）→ 空表，回落预置
    let bare = r#"{"object":"list","data":[{"id":"deepseek-flash","object":"model","owned_by":"deepseek"}]}"#;
    assert!(
        parse_model_limits(bare).is_empty(),
        "OpenAI 规范里没有 context 字段，这是常态不是异常"
    );

    let ds = preset::preset_by_key("deepseek").unwrap();
    let flash = ds
        .models
        .iter()
        .find(|m| m.value == "deepseek-flash")
        .unwrap();
    let preset_limits = flash.preset_limits().expect("DeepSeek 该有静态兜底");
    assert_eq!(
        preset_limits.source,
        LimitSource::Preset,
        "🔴 静态值必须标记成 Preset —— 界面据此允许用户修改"
    );
    assert!(preset_limits.context_window.is_some());

    // ── 第 3 层：两层都没有 → None，让用户自己填，别猜
    let anth = preset::preset_by_key("anthropic_official").unwrap();
    let opus = anth
        .models
        .iter()
        .find(|m| m.value == "claude-opus-5")
        .unwrap();
    assert!(
        opus.preset_limits().is_none(),
        "没查到官方数字的就该留空，而不是编一个"
    );
}

/// 裁历史用的预算 = 窗口 − 本次输出预留，且不下溢。
#[test]
fn input_budget_is_usable_for_truncation() {
    use ai_profile::TokenLimits;

    let l = TokenLimits::from_preset(Some(200_000), Some(8192));
    assert_eq!(l.input_budget(4096), Some(195_904));

    // 预留比窗口还大 → 0，不是回绕成天文数字
    assert_eq!(
        TokenLimits::from_preset(Some(1024), None).input_budget(4096),
        Some(0)
    );

    // 不知道窗口 → 不猜
    assert_eq!(
        TokenLimits::from_preset(None, None).input_budget(4096),
        None
    );
}
