//! Provider 预置清单 —— 本 crate 存在的首要理由。
//!
//! # 为什么这份数据值得抽出来
//!
//! 它是**变动最频繁、跨应用差异为零**的那部分。模型 id 月月换代，而五个应用各存一份
//! 就意味着漏改是必然的：DeepSeek 2026-07-24 下线 `deepseek-chat` 别名后，某个应用
//! 直到两个月后才发现「点开即报错」。
//!
//! # 🔴 加一家 provider 的规矩
//!
//! 1. `base_url` **照抄服务商文档原文** —— 含版本段、不含端点后缀。
//!    后端 [`crate::endpoint`] 原样使用、不做任何推断，所以这里少写一段就是 404。
//! 2. `model` 选**够用档**而非最强档 —— 把旗舰塞进默认值等于替用户做了一个
//!    他没同意的花钱决定。最强档留在 `models` 里随时可选。
//! 3. `models` 只放**核对过**的 id，并填 `verified_at`。没实际调通过就留 `None`。
//! 4. `vendor_id` 已有厂商要复用同一个（硅基流动的 chat/image 共用 `siliconflow`）。
//! 5. 跑 `cargo test`，守卫测试必须全绿。
//!
//! # 数组顺序即呈现顺序
//!
//! 下拉按 `group_key` 分组，**同组必须连续排列** —— 不连续会切出重复的分组标题。
//! 有 `preset_groups_are_contiguous` 守着。

use serde::Serialize;

use crate::kind::{Kind, Protocol};

pub mod vendor;

mod chat;
mod docgen;

pub use docgen::render_providers_markdown;
pub use vendor::{vendors, vendors_all, Vendor};

/// 一个模型候选项。
///
/// 下拉只是**建议清单** —— 调用方应当允许用户手填任意 model id，也可以调
/// [`crate::model_filter`] 清洗端点返回的真实清单。所以这里的值过期不会卡死用户。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ModelOption {
    /// 模型 id，原样传给服务商
    pub value: &'static str,
    /// 展示用标签；与 `value` 相同则调用方直接显示 `value`
    pub label: &'static str,
}

impl ModelOption {
    /// 标签与 id 相同的快捷构造（绝大多数情况）。
    pub const fn plain(value: &'static str) -> Self {
        Self {
            value,
            label: value,
        }
    }
}

/// 服务商专有的额外配置字段（如豆包 TTS 的 `appid` / `cluster`）。
///
/// # 🔴 为什么放在数据里而不是写死在界面里
///
/// 这类字段**按服务商而非按 kind 变化** —— 同是 tts，硅基流动就不需要 `appid`。
/// 写死在组件里意味着**每接一家新服务商就要改前端**；放进预置表，
/// 加一家 provider 只改本 crate 的一个数组字面量，所有下游同时生效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ExtraField {
    /// 字段 key，收集后存进调用方的 `extra` JSON
    pub key: &'static str,
    /// i18n key
    pub label_key: &'static str,
    /// 纯文本标签 —— 给没接 i18n 的调用方
    pub label: &'static str,
    /// 输入框占位符
    pub placeholder: Option<&'static str>,
    /// 为真时缺失会让验证直接失败（[`crate::VerifyError::MissingExtraField`]），
    /// 调用方应在**发起验证之前**就禁用按钮
    pub required: bool,
}

/// 一条 provider 预置。
///
/// `#[non_exhaustive]`：加字段是 minor 而非 major。调用方不能用字面量构造它，
/// 只能从 [`presets`] / [`preset_by_key`] 取。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ProviderPreset {
    /// 模板 key = 下拉 value，也是**存量配置回填的依据**。
    ///
    /// 🔴 改名 = major 版本：用户已存的配置靠它找回对应模板，改了就全掉进「自定义」。
    pub key: &'static str,

    /// 🔴 跨 kind 聚合的依据。
    ///
    /// 硅基流动的 chat / image / video / tts 共用一个 `vendor_id`，服务商目录靠它
    /// 把四条预置并成一张卡。同 `vendor_id` 的 `base_url` host 必须一致 ——
    /// 有 `vendor_ids_consistent` 守着。
    pub vendor_id: &'static str,

    /// 这条预置属于哪种能力
    pub kind: Kind,

    /// 分组 key（i18n）。数组顺序即呈现顺序，**同组必须连续**
    pub group_key: &'static str,
    /// 分组纯文本名 —— 给没接 i18n 的调用方
    pub group_label: &'static str,

    /// provider 名的 i18n key
    pub label_key: &'static str,
    /// provider 纯文本名。同 `group_label` 的理由
    pub label: &'static str,

    /// 下拉项副文本的 i18n key
    pub hint_key: Option<&'static str>,
    /// 副文本纯文本
    pub hint: Option<&'static str>,

    /// 预填的 base_url。
    ///
    /// 🔴 含版本段、不含端点后缀。`None` = 走官方端点或让用户自填
    /// （调用方据此隐藏/显示输入框）。
    pub base_url: Option<&'static str>,

    /// 默认 model；空串 = 让用户自己填（本地推理服务的 id 因人而异）
    pub model: &'static str,
    /// 建议模型清单
    pub models: &'static [ModelOption],

    /// 实际协议：走 `/v1/messages` 还是 `/v1/chat/completions`
    pub protocol: Protocol,

    /// 从已存配置的 base_url 反推模板 key 用的主机名片段。
    ///
    /// 空数组 = 该档靠 `protocol` 而非 host 反推（自定义端点类）。
    pub match_hosts: &'static [&'static str],

    /// 专有协议的额外配置字段
    pub extra_fields: &'static [ExtraField],

    /// API Key 申请页；`None` = 本地服务，不需要申请
    pub apply_url: Option<&'static str>,

    /// 🔴 是否需要用户**先在本机把服务跑起来**（Ollama / LM Studio / vLLM）。
    ///
    /// 服务商目录据此区分「云端服务，去申请 key」与「本地服务，先启动它」——
    /// 不区分的话，用户会按云服务的思路去配，配好却连不上。
    pub is_local: bool,

    /// 最后一次实际调通的日期（`YYYY-MM-DD`）。
    ///
    /// `None` = 未核实，仅供参考。调用方可对久未核实的预置给一个淡色提示。
    pub verified_at: Option<&'static str>,
}

/// 分组 key —— 声明顺序与预置数组里的出现顺序一致。
///
/// 排序按「用户找到它的概率」从高到低：
/// - **Anthropic / 协议档**在最前：按端点约定分档的那几条，放一起免得用户找两次
/// - **国内**次之：主要用户群在国内
/// - **国际**再次
/// - **本地/自建**最后：这几档要求用户先把推理服务跑起来，能用的人本就知道自己在找什么
pub const GROUP_ANTHROPIC: (&str, &str) = ("providerGroup.anthropic", "Anthropic / 协议档");
/// 国内服务商
pub const GROUP_CHINA: (&str, &str) = ("providerGroup.china", "国内");
/// 国际服务商
pub const GROUP_INTERNATIONAL: (&str, &str) = ("providerGroup.international", "国际");
/// 本地 / 自建推理服务
pub const GROUP_LOCAL: (&str, &str) = ("providerGroup.local", "本地 / 自建");

/// 兜底模板 key —— 反推不出任何 host 时落到这里。
pub const CUSTOM_PRESET_KEY: &str = "openai_compatible_custom";

/// 全部预置。数组顺序即呈现顺序。
pub fn presets() -> &'static [ProviderPreset] {
    chat::CHAT_PRESETS
}

/// 按 kind 过滤 —— 调用方渲染下拉时用。
pub fn presets_for(kind: Kind) -> impl Iterator<Item = &'static ProviderPreset> {
    presets().iter().filter(move |p| p.kind == kind)
}

/// 按 key 查找。
pub fn preset_by_key(key: &str) -> Option<&'static ProviderPreset> {
    presets().iter().find(|p| p.key == key)
}

/// 从已存配置反推模板 key。
///
/// - `Anthropic` 协议 + 空 base_url 或官方 URL → `anthropic_official`
/// - `Anthropic` 协议 + 自定义 base_url → `claude_code`
/// - 其余按 `match_hosts` 命中主机名；都不中 → [`CUSTOM_PRESET_KEY`]
///
/// 🔴 按 host 匹配而非全串相等：预置的 base_url 带版本段，而存量配置可能存的是
/// 不带版本段的旧写法，全串比较会让老配置统统掉进「自定义端点」。
pub fn infer_preset_key(protocol: Protocol, base_url: Option<&str>) -> &'static str {
    let url = base_url.unwrap_or("").trim().to_ascii_lowercase();

    if protocol == Protocol::Anthropic {
        if url.is_empty() || url.trim_end_matches('/') == "https://api.anthropic.com" {
            return "anthropic_official";
        }
        return "claude_code";
    }
    if url.is_empty() {
        return CUSTOM_PRESET_KEY;
    }
    for p in presets() {
        if p.protocol != Protocol::OpenAiCompatible {
            continue;
        }
        if p.match_hosts.iter().any(|h| url.contains(h)) {
            return p.key;
        }
    }
    CUSTOM_PRESET_KEY
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 同组必须连续 —— 不连续会让下拉切出两个同名分组标题。
    #[test]
    fn preset_groups_are_contiguous() {
        let mut seen: Vec<&str> = Vec::new();
        let mut prev = "";
        for p in presets() {
            if p.group_key == prev {
                continue;
            }
            assert!(
                !seen.contains(&p.group_key),
                "分组 {} 被拆成了不连续的多段",
                p.group_key
            );
            seen.push(p.group_key);
            prev = p.group_key;
        }
    }

    /// 🔴 base_url 必须自带版本段（或显式为 None）——
    /// 端点拼接不做推断，少一段就是下游 404。
    #[test]
    fn preset_base_urls_are_well_formed() {
        for p in presets() {
            let Some(url) = p.base_url else { continue };
            assert!(!url.ends_with('/'), "{}: base_url 不应以 / 结尾", p.key);
            assert!(
                !url.contains("chat/completions") && !url.ends_with("messages"),
                "{}: base_url 不该带端点后缀",
                p.key
            );
            // 版本段可以在末尾（/v1、/v4），也可以在中间（Gemini 的 /v1beta/openai）
            let has_version = crate::endpoint::ends_with_version_segment(url)
                || url.contains("/v1beta/")
                || url.contains("/v1/");
            assert!(has_version, "{}: base_url 看不到版本段 → {}", p.key, url);
        }
    }

    /// key 重复会让回填逻辑静默错乱（`find` 只取第一个）。
    #[test]
    fn preset_keys_are_unique() {
        let mut seen: Vec<&str> = Vec::new();
        for p in presets() {
            assert!(!seen.contains(&p.key), "重复的 preset key: {}", p.key);
            seen.push(p.key);
        }
    }

    /// 本地服务不该有申请密钥的链接；云端服务（除自定义档）应当有。
    #[test]
    fn local_presets_have_no_apply_url() {
        for p in presets() {
            if p.is_local {
                assert!(p.apply_url.is_none(), "{}: 本地服务不需要申请密钥", p.key);
            }
        }
    }

    /// 反推要能把存量配置认回原模板 —— 包括**不带版本段**的旧写法。
    #[test]
    fn infer_preset_key_matches_by_host() {
        assert_eq!(
            infer_preset_key(
                Protocol::OpenAiCompatible,
                Some("https://api.deepseek.com/v1")
            ),
            "deepseek"
        );
        // 🔴 旧配置不带版本段，靠 host 匹配才认得回来
        assert_eq!(
            infer_preset_key(Protocol::OpenAiCompatible, Some("https://api.deepseek.com")),
            "deepseek"
        );
        assert_eq!(
            infer_preset_key(Protocol::Anthropic, Some("https://api.anthropic.com")),
            "anthropic_official"
        );
        assert_eq!(
            infer_preset_key(Protocol::Anthropic, None),
            "anthropic_official"
        );
        // 中转站 → Claude Code 档
        assert_eq!(
            infer_preset_key(Protocol::Anthropic, Some("https://cc.example.cn/v1")),
            "claude_code"
        );
        // 谁都不中 → 兜底
        assert_eq!(
            infer_preset_key(
                Protocol::OpenAiCompatible,
                Some("https://unknown.example/v1")
            ),
            CUSTOM_PRESET_KEY
        );
    }

    /// 兜底档必须存在，否则 `infer_preset_key` 会返回一个查不到的 key。
    #[test]
    fn custom_preset_exists() {
        assert!(
            preset_by_key(CUSTOM_PRESET_KEY).is_some(),
            "兜底档 {CUSTOM_PRESET_KEY} 必须在预置表里"
        );
    }
}
