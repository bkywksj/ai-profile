//! 生图（image）能力的 provider 预置。
//!
//! 数据来源：StoryLoom 2026-09-24 已发布的预置（生产在用），外加 onestop 的 OpenAI 生图一档。
//! 协议实现见 [`crate::media::image`]：通义万相走 DashScope 异步任务，其余走 OpenAI
//! `/images/generations` 同步接口（火山方舟 Seedream、硅基流动、聚合站都兼容）。
//!
//! 🔴 同组必须连续（按 kind 分别判）；`vendor_id` 与同 host 的 chat 预置共用 ——
//! 用户配一次密钥，对话和生图一起可用。

use super::{
    ExtraField, ModelOption, ProviderPreset, GROUP_CHINA, GROUP_INTERNATIONAL, GROUP_LOCAL,
};
use crate::kind::{Kind, Protocol};

const NO_EXTRA: &[ExtraField] = &[];

pub(super) const IMAGE_PRESETS: &[ProviderPreset] = &[
    // ── 国内 ─────────────────────────────────────────────────────
    ProviderPreset {
        key: "seedream_image",
        // 火山方舟：chat / image / video 同一个 base_url、同一个密钥
        vendor_id: "volcengine",
        kind: Kind::Image,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.seedreamImage.label",
        label: "即梦 Seedream（火山方舟）",
        hint_key: Some("providerTemplate.seedreamImage.hint"),
        // 两家下游的实测经验并存：StoryLoom 用 4.0 多图融合；onestop 遇到过 4.0 报
        // 「does not support this api」，换 t2i 模型解决 —— 与账号开通的模型有关
        hint: Some("中文 / 人像质量最好；模型 ID 须与控制台「开通」后给的完全一致（带日期后缀）。报「does not support this api」就换 *-t2i-* 模型"),
        base_url: Some("https://ark.cn-beijing.volces.com/api/v3"),
        model: "doubao-seedream-4-0-250828",
        models: &[
            ModelOption::plain("doubao-seedream-4-0-250828"),
            ModelOption::plain("doubao-seedream-3-0-t2i-250415"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["ark.cn-beijing.volces.com"],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: Some("https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "wan_image",
        vendor_id: "aliyun",
        kind: Kind::Image,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.wanImage.label",
        label: "通义万相（阿里百炼）",
        hint_key: Some("providerTemplate.wanImage.hint"),
        hint: Some("异步任务（提交 → 轮询 → 下载）；地址是原生 api/v1，不是对话用的 compatible-mode"),
        // 🔴 与通义千问对话档同 host 不同路径：DashScope 原生接口在 /api/v1
        base_url: Some("https://dashscope.aliyuncs.com/api/v1"),
        model: "wan2.2-t2i-flash",
        models: &[
            ModelOption::plain("wan2.2-t2i-flash"),
            ModelOption::plain("wan2.2-t2i-plus"),
            ModelOption::plain("wanx2.1-t2i-turbo"),
            ModelOption::plain("wanx2.1-t2i-plus"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["dashscope.aliyuncs.com"],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: Some("https://bailian.console.aliyun.com/?tab=model#/api-key"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "siliconflow_image",
        vendor_id: "siliconflow",
        kind: Kind::Image,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.siliconflowImage.label",
        label: "硅基流动 生图",
        hint_key: Some("providerTemplate.siliconflowImage.hint"),
        hint: Some("Kolors 国风 / 人像，FLUX 质感；结果图链接 1 小时失效，调用方应立即下载"),
        base_url: Some("https://api.siliconflow.cn/v1"),
        model: "Kwai-Kolors/Kolors",
        models: &[
            ModelOption::plain("Kwai-Kolors/Kolors"),
            ModelOption::plain("black-forest-labs/FLUX.1-dev"),
            ModelOption::plain("black-forest-labs/FLUX.1-schnell"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.siliconflow.cn"],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: Some("https://cloud.siliconflow.cn/account/ak"),
        is_local: false,
        verified_at: None,
    },
    // ── 国际 ─────────────────────────────────────────────────────
    ProviderPreset {
        key: "openai_image",
        vendor_id: "openai",
        kind: Kind::Image,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.openaiImage.label",
        label: "OpenAI 生图",
        hint_key: Some("providerTemplate.openaiImage.hint"),
        hint: Some("国内访问需要代理"),
        base_url: Some("https://api.openai.com/v1"),
        model: "gpt-image-1",
        models: &[ModelOption::plain("gpt-image-1"), ModelOption::plain("dall-e-3")],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.openai.com"],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: Some("https://platform.openai.com/api-keys"),
        is_local: false,
        verified_at: None,
    },
    // ── 自定义 ───────────────────────────────────────────────────
    ProviderPreset {
        key: "custom_image",
        vendor_id: "custom_image",
        kind: Kind::Image,
        group_key: GROUP_LOCAL.0,
        group_label: GROUP_LOCAL.1,
        label_key: "providerTemplate.customImage.label",
        label: "自定义生图端点",
        hint_key: Some("providerTemplate.customImage.hint"),
        hint: Some("OpenAI /images/generations 兼容的任意端点"),
        base_url: None,
        model: "",
        models: &[],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &[],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: None,
        is_local: false,
        verified_at: None,
    },
];
