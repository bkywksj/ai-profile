//! 配音（tts）能力的 provider 预置。
//!
//! 数据来源：StoryLoom 2026-09-24 已发布的预置（生产在用），外加 onestop 的 OpenAI 配音一档。
//! 实现见 [`crate::media::tts`]：火山语音（openspeech）是专有协议，其余走 OpenAI `/audio/speech`。

use super::{
    ExtraField, ModelOption, ProviderPreset, GROUP_CHINA, GROUP_INTERNATIONAL, GROUP_LOCAL,
};
use crate::kind::{Kind, Protocol};

const NO_EXTRA: &[ExtraField] = &[];

/// 火山语音专有协议：除密钥（access_token）外还要 `appid`，`cluster` 缺省 `volcano_tts`。
const VOLC_TTS_EXTRA: &[ExtraField] = &[
    ExtraField {
        key: "appid",
        label_key: "providerTemplate.volcTts.appid",
        label: "App ID",
        placeholder: Some("火山语音控制台应用 appid"),
        required: true,
    },
    ExtraField {
        key: "cluster",
        label_key: "providerTemplate.volcTts.cluster",
        label: "Cluster（集群）",
        placeholder: Some("默认 volcano_tts"),
        required: false,
    },
];

pub(super) const TTS_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        key: "volc_tts",
        // 🔴 与火山方舟（volcengine）不是同一个产品线，也不是同一个密钥：
        //    方舟没有 TTS 接口，语音合成在「火山语音 openspeech」
        vendor_id: "volc_speech",
        kind: Kind::Tts,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.volcTts.label",
        label: "字节豆包 配音（火山语音）",
        hint_key: Some("providerTemplate.volcTts.hint"),
        hint: Some("中文有声书音色库大、自然稳定；需要 App ID，密钥填 access_token"),
        // 🔴 专有协议：地址是**完整接口路径**，直接 POST，不再拼任何后缀
        base_url: Some("https://openspeech.bytedance.com/api/v1/tts"),
        model: "volcano_tts",
        models: &[ModelOption::plain("volcano_tts")],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["openspeech.bytedance.com"],
        extra_fields: VOLC_TTS_EXTRA,
        default_extra: &[],
        apply_url: Some("https://console.volcengine.com/speech/app"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "siliconflow_tts",
        vendor_id: "siliconflow",
        kind: Kind::Tts,
        group_key: GROUP_CHINA.0,
        group_label: GROUP_CHINA.1,
        label_key: "providerTemplate.siliconflowTts.label",
        label: "硅基流动 配音 CosyVoice",
        hint_key: Some("providerTemplate.siliconflowTts.hint"),
        hint: Some("CosyVoice2 系统音色，支持情绪指令；fish-speech 可克隆"),
        base_url: Some("https://api.siliconflow.cn/v1"),
        model: "FunAudioLLM/CosyVoice2-0.5B",
        models: &[
            ModelOption::plain("FunAudioLLM/CosyVoice2-0.5B"),
            ModelOption::plain("fishaudio/fish-speech-1.5"),
        ],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.siliconflow.cn"],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: Some("https://cloud.siliconflow.cn/account/ak"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "openai_tts",
        vendor_id: "openai",
        kind: Kind::Tts,
        group_key: GROUP_INTERNATIONAL.0,
        group_label: GROUP_INTERNATIONAL.1,
        label_key: "providerTemplate.openaiTts.label",
        label: "OpenAI 配音",
        hint_key: Some("providerTemplate.openaiTts.hint"),
        hint: Some("国内访问需要代理"),
        base_url: Some("https://api.openai.com/v1"),
        model: "tts-1",
        models: &[ModelOption::plain("tts-1"), ModelOption::plain("tts-1-hd")],
        protocol: Protocol::OpenAiCompatible,
        match_hosts: &["api.openai.com"],
        extra_fields: NO_EXTRA,
        default_extra: &[],
        apply_url: Some("https://platform.openai.com/api-keys"),
        is_local: false,
        verified_at: None,
    },
    ProviderPreset {
        key: "custom_tts",
        vendor_id: "custom_tts",
        kind: Kind::Tts,
        group_key: GROUP_LOCAL.0,
        group_label: GROUP_LOCAL.1,
        label_key: "providerTemplate.customTts.label",
        label: "自定义配音端点",
        hint_key: Some("providerTemplate.customTts.hint"),
        hint: Some("OpenAI /audio/speech 兼容的任意端点"),
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
