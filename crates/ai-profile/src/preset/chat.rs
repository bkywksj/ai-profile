//! 对话（chat）能力的 provider 预置 —— 19 家。
//!
//! 🔴 **数组顺序即下拉呈现顺序，同组必须连续。**
//!
//! 数据来源：
//!
//! - 前 16 家来自 sigil 2026-09-22 重写并验证过的那一份（1363 个测试全绿）
//! - 火山方舟 / 腾讯 TokenHub / xAI 三家于 2026-09-22 按各家官方文档补入，**未实际调通**
//!
//! `verified_at` 只填**实际调通过**的，照文档抄来的一律留 `None` ——
//! 开源后别人提 PR 加 provider，没有这个字段就没法判断该不该信。
//! `cargo xtask probe` 能验地址可达性，但验不了 model id 对不对（那要真实密钥）。

use super::{
    ExtraField, ModelOption, ProviderPreset, GROUP_ANTHROPIC, GROUP_CHINA, GROUP_INTERNATIONAL,
    GROUP_LOCAL,
};
use crate::kind::{Kind, Protocol};

const NO_EXTRA: &[ExtraField] = &[];

/// Anthropic 模型候选（新→旧）。
///
/// 提成常量而不是在 `anthropic_official` / `claude_code` 两处各抄一份：sigil 此前就是
/// 因为抄了两份，Claude 5 系列上线后只补了一处，另一处漏了。共享一份至少让漏更新时是
/// **全漏**，而不是「有的下拉有、有的没有」。
///
/// 保留 4.x：中转站常常只带旧型号，用户也可能刻意锁版本。
const CLAUDE_MODELS: &[ModelOption] = &[
    ModelOption::plain("claude-opus-5"),
    ModelOption::plain("claude-sonnet-5"),
    ModelOption::plain("claude-fable-5-1"),
    ModelOption::plain("claude-fable-5"),
    ModelOption::plain("claude-haiku-4-5"),
    ModelOption::plain("claude-opus-4-8"),
];

/// 🔴 默认 model 一律选「够用档」而非「最强档」。
///
/// 用户点开预置是奔着"能用"来的，不是奔着"最贵"—— 把旗舰塞进默认值，
/// 等于替他做了一个没同意的花钱决定。最强档留在 `models` 里随时可选。
pub(super) const CHAT_PRESETS: &[ProviderPreset] = &[
    // ── 组 1 · Anthropic / 协议档 ────────────────────────────────
    ProviderPreset {
        key: "anthropic_official",
        vendor_id: "anthropic",
        kind: Kind::Chat,
        group_key: GROUP_ANTHROPIC.0,
        group_label: GROUP_ANTHROPIC.1,
        label_key: "providerTemplate.anthropicOfficial.label",
        label: "Anthropic 官方",
        hint_key: Some("providerTemplate.anthropicOfficial.hint"),
        hint: Some("密钥以 sk-ant- 开头；固定走 https://api.anthropic.com"),
        // 官方档不填 base_url —— 调用方据此隐藏输入框
        base_url: None,
        model: "claude-opus-5",
        models: CLAUDE_MODELS,
        protocol: Protocol::Anthropic,
        match_hosts: &["api.anthropic.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://console.anthropic.com/settings/keys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "claude_code",
        vendor_id: "claude_code",
        kind: Kind::Chat,
        group_key: GROUP_ANTHROPIC.0,
        group_label: GROUP_ANTHROPIC.1,
        label_key: "providerTemplate.claudeCode.label",
        label: "Claude Code 客户端（自定义接口地址）",
        hint_key: Some("providerTemplate.claudeCode.hint"),
        hint: Some("走 /v1/messages 端点；适合密钥限定此端点的中转 / 代理服务"),
        base_url: None,
        model: "claude-opus-5",
        models: CLAUDE_MODELS,
        protocol: Protocol::Anthropic,
        // 自定义端点档：靠 protocol 而非 host 反推
        match_hosts: &[],
        extra_fields: NO_EXTRA,
        apply_url: None,
        is_local: false,
        verified_at: Some("2026-09-22"),
    },
    ProviderPreset {
        key: "codex",
        vendor_id: "codex",
        kind: Kind::Chat,
        group_key: GROUP_ANTHROPIC.0,
        group_label: GROUP_ANTHROPIC.1,
        label_key: "providerTemplate.codex.label",
        label: "Codex 客户端（自定义接口地址）",
        hint_key: Some("providerTemplate.codex.hint"),
        hint: Some("走 /v1/chat/completions；适合密钥限定 Codex 端点的中转服务"),
        base_url: None,
        model: "gpt-5.6-terra",
        models: &[
            ModelOption::plain("gpt-5.6-terra"),
            ModelOption::plain("gpt-5.6-sol"),
            ModelOption::plain("gpt-5.6-luna"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &[],
        extra_fields: NO_EXTRA,
        apply_url: None,
        is_local: false,
        verified_at: None,
    },
    // ── 组 2 · 国内 ──────────────────────────────────────────────
    ProviderPreset {
        key: "deepseek",
        vendor_id: "deepseek",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.deepseek.label",
        label: "DeepSeek",
        hint_key: None,
        hint: None,
        base_url: Some("https://api.deepseek.com/v1"),
        // 🔴 老别名 deepseek-chat / deepseek-reasoner 官方已于 2026-07-24 下线
        //    （V4 发布时公告的三个月过渡期到期）—— 它们曾留在预置里，表现为「点开即报错」。
        model: "deepseek-flash",
        models: &[
            ModelOption::plain("deepseek-flash"),
            ModelOption::plain("deepseek-v4-pro"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.deepseek.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://platform.deepseek.com/api_keys"),
        is_local: false,
        verified_at: Some("2026-09-17"),
    },
    ProviderPreset {
        key: "zhipu",
        vendor_id: "zhipu",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.zhipu.label",
        label: "智谱 GLM",
        hint_key: None,
        hint: None,
        // 🔴 智谱的版本段是 v4 不是 v1 —— 端点拼接一旦"好心"补 /v1 就是 404
        base_url: Some("https://open.bigmodel.cn/api/paas/v4"),
        model: "glm-5.3",
        models: &[
            ModelOption::plain("glm-5.3"),
            ModelOption::plain("glm-5.3-flash"),
            ModelOption::plain("glm-5"),
            ModelOption::plain("glm-4.7"),
            ModelOption::plain("glm-4.6"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["open.bigmodel.cn"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://open.bigmodel.cn/usercenter/apikeys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "qwen",
        vendor_id: "aliyun",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.qwen.label",
        label: "通义千问（阿里云百炼）",
        hint_key: Some("providerTemplate.qwen.hint"),
        hint: Some("默认北京地域；新加坡站把 dashscope 换成 dashscope-intl"),
        base_url: Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        model: "qwen-plus",
        // qwen-plus / max / turbo 是**滚动别名**，阿里会把它们指到当代快照，
        // 不会因为出新版而失效；qwen3-max 是需要显式点名的当代旗舰。
        models: &[
            ModelOption::plain("qwen3-max"),
            ModelOption::plain("qwen-plus"),
            ModelOption::plain("qwen-max"),
            ModelOption::plain("qwen-turbo"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["dashscope.aliyuncs.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://bailian.console.aliyun.com/"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "moonshot",
        vendor_id: "moonshot",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.moonshot.label",
        label: "月之暗面 Kimi",
        hint_key: None,
        hint: None,
        // 国内站；国际站是 api.moonshot.ai
        base_url: Some("https://api.moonshot.cn/v1"),
        // legacy 的 moonshot-v1-8k/32k/128k 按上下文长度分档，已被 K 系的统一上下文
        // 取代，从建议清单里去掉 —— 真要用老模型，让用户点「获取」拉端点全量清单即可。
        model: "kimi-k3",
        models: &[
            ModelOption::plain("kimi-k3"),
            ModelOption::plain("kimi-k2-0711-preview"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.moonshot.cn", "api.moonshot.ai"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://platform.moonshot.cn/console/api-keys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "siliconflow",
        vendor_id: "siliconflow",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.siliconflow.label",
        label: "硅基流动 SiliconFlow",
        hint_key: Some("providerTemplate.siliconflow.hint"),
        hint: Some("聚合平台，在架清单变动快 —— 建议点「获取」拉当天真实清单"),
        base_url: Some("https://api.siliconflow.cn/v1"),
        // ⚠️ 聚合平台的在架清单变动最快（上下架都不发公告），这里刻意只留两条保守兜底，
        //    不追新：写死一个已下架的 id 比写一个旧的更糟。这一档正是「获取」按钮的主场。
        model: "deepseek-ai/DeepSeek-V3",
        models: &[
            ModelOption::plain("deepseek-ai/DeepSeek-V3"),
            ModelOption::plain("Qwen/Qwen2.5-72B-Instruct"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.siliconflow.cn"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://cloud.siliconflow.cn/account/ak"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "volcengine_ark",
        // 🔴 这个 vendor_id 是本 crate「同厂商共用一个密钥」设计的最佳例证：
        //    火山方舟的 chat / image / video 走的是**同一个 base_url**
        //    （https://ark.cn-beijing.volces.com/api/v3）。后续补 image / video 预置时
        //    复用这个 vendor_id，用户配一次密钥就能三种能力全开。
        vendor_id: "volcengine",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.volcengineArk.label",
        label: "火山方舟（豆包）",
        hint_key: Some("providerTemplate.volcengineArk.hint"),
        hint: Some("模型 id 带发布日期后缀，换代就变 —— 建议点「获取」拉当天真实清单"),
        base_url: Some("https://ark.cn-beijing.volces.com/api/v3"),
        // ⚠️ 豆包的 id 形如 doubao-seed-2-1-pro-260628，日期后缀是模型版本的一部分，
        //    不是可省略的装饰。写死的这两条迟早过期，「获取」按钮才是主路径。
        model: "doubao-seed-1-6-251015",
        models: &[
            ModelOption::plain("doubao-seed-2-1-pro-260628"),
            ModelOption::plain("doubao-seed-1-6-251015"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["ark.cn-beijing.volces.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "tencent_tokenhub",
        vendor_id: "tencent",
        kind: Kind::Chat,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.tencentTokenhub.label",
        label: "腾讯 TokenHub",
        hint_key: Some("providerTemplate.tencentTokenhub.hint"),
        // 🔴 这条 hint 是有具体用处的：老用户手里多半是 api.hunyuan.cloud.tencent.com/v1 的
        //    地址和密钥，直接填进来不会报错、但拿不到新模型。必须明说要换。
        hint: Some(
            "混元老地址 api.hunyuan.cloud.tencent.com 已停止新增模型，密钥需在 TokenHub 重新申请",
        ),
        base_url: Some("https://tokenhub.tencentmaas.com/v1"),
        // TokenHub 是**聚合平台**而不是纯混元入口：同一个密钥下既有腾讯自家的 hy3，
        // 也代理 deepseek / glm / kimi / minimax。注意同一个模型在不同平台 id 不同 ——
        // DeepSeek 自家叫 deepseek-flash，这里叫 deepseek-v4-flash，所以不能跨预置共用常量。
        model: "hy3-preview",
        models: &[
            ModelOption::plain("hy3-preview"),
            ModelOption::plain("deepseek-v4-flash"),
            ModelOption::plain("deepseek-v4-pro"),
            ModelOption::plain("glm-5.1"),
            ModelOption::plain("kimi-k2.6"),
            ModelOption::plain("minimax-m2.7"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["tokenhub.tencentmaas.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://console.cloud.tencent.com/tokenhub/apikey"),
        is_local: false,
        verified_at: None,
    },
    // ── 组 3 · 国际 ──────────────────────────────────────────────
    ProviderPreset {
        key: "openai_official",
        vendor_id: "openai",
        kind: Kind::Chat,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.openaiOfficial.label",
        label: "OpenAI 官方",
        hint_key: None,
        hint: None,
        base_url: Some("https://api.openai.com/v1"),
        // 默认取 5.6-terra（日常主力）而非 gpt-6-astra（四倍价）
        model: "gpt-5.6-terra",
        models: &[
            ModelOption::plain("gpt-6-astra"),
            ModelOption::plain("gpt-5.6-sol"),
            ModelOption::plain("gpt-5.6-terra"),
            ModelOption::plain("gpt-5.6-luna"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.openai.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://platform.openai.com/api-keys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "openrouter",
        vendor_id: "openrouter",
        kind: Kind::Chat,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.openrouter.label",
        label: "OpenRouter",
        hint_key: Some("providerTemplate.openrouter.hint"),
        hint: Some("模型 id 形如「厂商/模型」，与各家原生 id 不同名"),
        base_url: Some("https://openrouter.ai/api/v1"),
        // OpenRouter 的 id 形如 `厂商/模型`，与各家原生 id 不同名，不能直接抄上面几档
        model: "anthropic/claude-sonnet-5",
        models: &[
            ModelOption::plain("anthropic/claude-sonnet-5"),
            ModelOption::plain("openai/gpt-6-astra"),
            ModelOption::plain("moonshotai/kimi-k3"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["openrouter.ai"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://openrouter.ai/keys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "gemini",
        vendor_id: "google",
        kind: Kind::Chat,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.gemini.label",
        label: "Google Gemini（OpenAI 兼容层）",
        hint_key: Some("providerTemplate.gemini.hint"),
        hint: Some("版本段 v1beta 不在末尾，照抄整串即可"),
        // 🔴 版本段 v1beta **不在末尾**（末段是 openai）。旧实现的"末段是 v<数字>才算
        //    有版本段"规则认不出它，只能靠末尾加 # 开后门 —— 而"需要转义符才能表达的
        //    规则"本身就说明规则不对，这是端点拼接改成「原样使用」的直接动因。
        base_url: Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        // Pro 线目前只有 preview 档（会不打招呼地换代），故默认取 Flash
        model: "gemini-3.8-flash",
        models: &[
            ModelOption::plain("gemini-3.8-flash"),
            ModelOption::plain("gemini-3.1-pro-preview"),
            ModelOption::plain("gemini-3.5-flash-lite"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["generativelanguage.googleapis.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://aistudio.google.com/apikey"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "groq",
        vendor_id: "groq",
        kind: Kind::Chat,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.groq.label",
        label: "Groq",
        hint_key: None,
        hint: None,
        base_url: Some("https://api.groq.com/openai/v1"),
        model: "llama-3.3-70b-versatile",
        models: &[
            ModelOption::plain("llama-3.3-70b-versatile"),
            ModelOption::plain("llama-3.1-8b-instant"),
            ModelOption::plain("openai/gpt-oss-120b"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.groq.com"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://console.groq.com/keys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "xai",
        vendor_id: "xai",
        kind: Kind::Chat,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.xai.label",
        label: "xAI Grok",
        hint_key: Some("providerTemplate.xai.hint"),
        // 🔴 值得写进 hint：xAI 曾提供 Anthropic 协议兼容层，已于 2026-08-01 完全废弃。
        //    照着老教程按 Anthropic 协议配会失败，且失败信息不会提这件事。
        hint: Some("只支持 OpenAI 协议 —— Anthropic 兼容层已于 2026-08-01 下线"),
        base_url: Some("https://api.x.ai/v1"),
        // ⚠️ id 里的是**点号**不是连字符：grok-4.7，不是 grok-4-7。
        //    这一家的命名与其它家相反，抄错了表现为「模型不存在」。
        model: "grok-4.5",
        models: &[
            ModelOption::plain("grok-4.7"),
            ModelOption::plain("grok-4.6"),
            ModelOption::plain("grok-4.5"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.x.ai"],
        extra_fields: NO_EXTRA,
        apply_url: Some("https://console.x.ai/"),
        is_local: false,
        verified_at: None,
    },
    // ── 组 4 · 本地 / 自建 ───────────────────────────────────────
    ProviderPreset {
        key: "ollama",
        vendor_id: "ollama",
        kind: Kind::Chat,
        group_key: GROUP_LOCAL.0,
        group_label: GROUP_LOCAL.1,
        label_key: "providerTemplate.ollama.label",
        label: "Ollama（本地）",
        hint_key: Some("providerTemplate.ollama.hint"),
        hint: Some("需先本机跑起 ollama serve；默认端口 11434，通常不需要密钥"),
        // Ollama 自 v0.1.24 起原生支持 OpenAI 兼容端点，不需要为它单开原生协议分支
        base_url: Some("http://localhost:11434/v1"),
        model: "qwen3:8b",
        models: &[
            ModelOption::plain("qwen3:8b"),
            ModelOption::plain("llama3.1:8b"),
            ModelOption::plain("deepseek-r1:7b"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["localhost:11434", "127.0.0.1:11434"],
        extra_fields: NO_EXTRA,
        apply_url: None,
        is_local: true,
        verified_at: None,
    },
    ProviderPreset {
        key: "lmstudio",
        vendor_id: "lmstudio",
        kind: Kind::Chat,
        group_key: GROUP_LOCAL.0,
        group_label: GROUP_LOCAL.1,
        label_key: "providerTemplate.lmstudio.label",
        label: "LM Studio（本地）",
        hint_key: Some("providerTemplate.lmstudio.hint"),
        hint: Some("在 LM Studio 里开启本地服务器；模型名取决于你加载了哪个"),
        base_url: Some("http://localhost:1234/v1"),
        // 本地服务的 model id 取决于用户加载了哪个模型，给不出有意义的默认值
        model: "",
        models: &[],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["localhost:1234", "127.0.0.1:1234"],
        extra_fields: NO_EXTRA,
        apply_url: None,
        is_local: true,
        verified_at: None,
    },
    ProviderPreset {
        key: "vllm",
        vendor_id: "vllm",
        kind: Kind::Chat,
        group_key: GROUP_LOCAL.0,
        group_label: GROUP_LOCAL.1,
        label_key: "providerTemplate.vllm.label",
        label: "vLLM / 自建推理服务",
        hint_key: Some("providerTemplate.vllm.hint"),
        hint: Some("任何暴露 OpenAI 兼容端点的自建服务都可用这档"),
        base_url: Some("http://localhost:8000/v1"),
        model: "",
        models: &[],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["localhost:8000", "127.0.0.1:8000"],
        extra_fields: NO_EXTRA,
        apply_url: None,
        is_local: true,
        verified_at: None,
    },
    ProviderPreset {
        key: super::CUSTOM_PRESET_KEY,
        vendor_id: "custom",
        kind: Kind::Chat,
        group_key: GROUP_LOCAL.0,
        group_label: GROUP_LOCAL.1,
        label_key: "providerTemplate.openaiCompatibleCustom.label",
        label: "其它 OpenAI 兼容（自定义接口地址）",
        hint_key: Some("providerTemplate.openaiCompatibleCustom.hint"),
        hint: Some("上面都不匹配时用这档，自己填接口地址与模型名"),
        base_url: None,
        model: "",
        models: &[],
        protocol: Protocol::OpenAiCompatible,
        // 兜底档：反推不到任何 host 时落到这里
        match_hosts: &[],
        extra_fields: NO_EXTRA,
        apply_url: None,
        is_local: false,
        verified_at: None,
    },
];
