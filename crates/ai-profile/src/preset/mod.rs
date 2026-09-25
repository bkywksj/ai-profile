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
//! 下拉按 `group_key` 分组，**同一 kind 内同组必须连续排列** —— 不连续会切出重复的分组标题。
//! 有 `preset_groups_are_contiguous` 守着。不同 kind 分开渲染，各自从「国内」起排。

use serde::Serialize;

use crate::kind::{Kind, Protocol};

pub mod vendor;

pub mod catalog;
mod chat;
mod docgen;
#[cfg(feature = "image")]
mod image;
#[cfg(feature = "tts")]
mod tts;
#[cfg(feature = "video")]
mod video;

pub use catalog::PresetCatalog;
pub use docgen::render_providers_markdown;
pub use vendor::{vendors, vendors_all, vendors_in, Vendor};

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
    /// 上下文窗口（输入 + 输出总量）的**静态兜底值**，端点不报时用。
    ///
    /// 🔴 只在**官方文档明确写了**时填，且宁可保守 —— 猜大了会让裁历史裁不够，
    /// 表现为「明明裁过还是超限」；猜小了只是浪费一点窗口，代价小得多。
    ///
    /// `None` = 不知道。调用方应当让用户手填，而不是自己兜一个默认值。
    pub context_window: Option<u32>,
    /// 单次输出上限的静态兜底值。同上，只填文档明确的。
    pub max_output: Option<u32>,
}

impl ModelOption {
    /// 标签与 id 相同、且不带限额信息的快捷构造。
    ///
    /// 绝大多数预置用它 —— 限额优先从端点实时取，静态值只给
    /// 「端点不报 + 官方文档明确」的那些模型补。
    pub const fn plain(value: &'static str) -> Self {
        Self {
            value,
            label: value,
            context_window: None,
            max_output: None,
        }
    }

    /// 带静态限额兜底的构造。
    ///
    /// 用于 OpenAI 规范兼容端点 —— 它们的 `/models` 只返回
    /// `{id, object, owned_by}`，一个限额字段都没有（LM Studio /
    /// Ollama 实测如此），不给静态值的话这个能力在那些端点上等于不存在。
    ///
    /// ⚠️ 这些数字**会过时**。DeepSeek V3 时代是 128K/8K，V4 已经是 1M/384K ——
    /// 半年翻了八倍。所以：端点报了就用端点的，静态值只是端点沉默时的下限保证。
    ///
    /// ```
    /// # use ai_profile::preset::ModelOption;
    /// // DeepSeek V4 系官方标称：1M 上下文、384K 输出（2026-09-22 核对）
    /// let m = ModelOption::with_limits("deepseek-flash", 1_000_000, 384_000);
    /// assert_eq!(m.context_window, Some(1_000_000));
    /// ```
    pub const fn with_limits(value: &'static str, context_window: u32, max_output: u32) -> Self {
        Self {
            value,
            label: value,
            context_window: Some(context_window),
            max_output: Some(max_output),
        }
    }

    /// 只知道上下文窗口、不知道输出上限时用。
    ///
    /// 这种情况很常见：多数服务商文档会写「支持 200K 上下文」，
    /// 却不单独说明单次输出上限。**不知道就是不知道**，别拿窗口大小去猜输出上限。
    pub const fn with_context(value: &'static str, context_window: u32) -> Self {
        Self {
            value,
            label: value,
            context_window: Some(context_window),
            max_output: None,
        }
    }

    /// 这条模型的静态兜底限额；两者都没有时返回 `None`。
    ///
    /// 🔴 返回的 [`TokenLimits`](crate::limits::TokenLimits) 带
    /// `source: Preset` 标记 —— 调用方据此知道这是**估计值**、该允许用户修改，
    /// 而不是端点保证的事实。
    pub fn preset_limits(&self) -> Option<crate::limits::TokenLimits> {
        if self.context_window.is_none() && self.max_output.is_none() {
            return None;
        }
        Some(crate::limits::TokenLimits::from_preset(
            self.context_window,
            self.max_output,
        ))
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

    /// 用这条预置新建配置时，自动写进调用方 `extra` 的固定键值（用户不用填、也不该改）。
    ///
    /// 用途是**显式指定协议**，让 crate 不必认识品牌：比如某 New API 视频中转站带
    /// `("video_api", "newapi")`，`media::video::VideoProtocol::detect`（`video` feature）就按 New API 处理。
    /// 与 [`Self::extra_fields`] 的区别：那是让用户**填**的，这是预置**定死**的。
    /// 线格式是 `[[key, value], …]`。
    pub default_extra: &'static [(&'static str, &'static str)],

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

impl ProviderPreset {
    /// 用户没填地址时，这条预置实际该请求的端点。
    ///
    /// - 预置写了 `base_url` → 就是它
    /// - 没写，但**协议的官方端点落在它声明的 `match_hosts` 里** → 协议官方端点
    ///   （「Anthropic 官方」这类固定走官方地址、界面隐藏地址框的预置）
    /// - 其余（自定义端点、各类中转档，`match_hosts` 为空）→ `None`，必须由用户填
    ///
    /// 🔴 「Anthropic 官方」的 `base_url` 刻意留空（调用方据此隐藏输入框），但请求总得有个地址。
    /// 此前「获取模型」验证（`client::Verifier::verify`）只看 `base_url`，用户不填地址时直接报
    /// 「缺 base_url」—— 官方档恰恰是最不该让用户填地址的那一档。
    pub fn endpoint(&self) -> Option<&'static str> {
        self.base_url.or_else(|| {
            let official = self.protocol.default_base_url();
            self.match_hosts
                .iter()
                .any(|h| official.contains(h))
                .then_some(official)
        })
    }
}

/// 下游自建预置用的 const 构造器（crate 自己的预置照旧写字面量）。
///
/// `ProviderPreset` 是 `#[non_exhaustive]`，下游不能写字面量；这组 `const fn` 让下游照样能写**静态表**，
/// 再交给 [`PresetCatalog::extend`] 合并进目录。见 [`catalog`] 模块文档。
///
/// 缺省值：`vendor_id` = key、分组「本地 / 自建」、OpenAI 兼容协议、无模型、无 i18n key（`label_key` 为空，
/// 接了 i18n 的调用方遇到空 key 应直接显示 `label`）。
impl ProviderPreset {
    /// 最小构造：key、能力、名字、地址（`None` = 让用户自填）。
    pub const fn new(
        key: &'static str,
        kind: Kind,
        label: &'static str,
        base_url: Option<&'static str>,
    ) -> Self {
        Self {
            key,
            vendor_id: key,
            kind,
            group_key: GROUP_LOCAL.0,
            group_label: GROUP_LOCAL.1,
            label_key: "",
            label,
            hint_key: None,
            hint: None,
            base_url,
            model: "",
            models: &[],
            protocol: Protocol::OpenAiCompatible,
            match_hosts: &[],
            extra_fields: &[],
            default_extra: &[],
            apply_url: None,
            is_local: false,
            verified_at: None,
        }
    }
    /// 服务商聚合 id（同一家多种能力共用；缺省 = key）
    pub const fn with_vendor(mut self, vendor_id: &'static str) -> Self {
        self.vendor_id = vendor_id;
        self
    }
    /// 分组（传 `GROUP_CHINA` 等常量）
    pub const fn with_group(mut self, group: (&'static str, &'static str)) -> Self {
        self.group_key = group.0;
        self.group_label = group.1;
        self
    }
    /// 下拉第二行的要点
    pub const fn with_hint(mut self, hint: &'static str) -> Self {
        self.hint = Some(hint);
        self
    }
    /// 默认模型 + 候选清单
    pub const fn with_models(
        mut self,
        model: &'static str,
        models: &'static [ModelOption],
    ) -> Self {
        self.model = model;
        self.models = models;
        self
    }
    /// 对话协议（缺省 OpenAI 兼容）
    pub const fn with_protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = protocol;
        self
    }
    /// 反推模板用的主机名片段
    pub const fn with_match_hosts(mut self, hosts: &'static [&'static str]) -> Self {
        self.match_hosts = hosts;
        self
    }
    /// 让用户填的专有字段
    pub const fn with_extra_fields(mut self, fields: &'static [ExtraField]) -> Self {
        self.extra_fields = fields;
        self
    }
    /// 预置定死的 extra 键值（如 `("video_api", "newapi")`）
    pub const fn with_default_extra(mut self, kv: &'static [(&'static str, &'static str)]) -> Self {
        self.default_extra = kv;
        self
    }
    /// 密钥申请页
    pub const fn with_apply_url(mut self, url: &'static str) -> Self {
        self.apply_url = Some(url);
        self
    }
    /// 本地推理服务（需用户先把服务跑起来）
    pub const fn local(mut self) -> Self {
        self.is_local = true;
        self
    }
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
///
/// 顺序：chat → image → video → tts（只含本 build 开启的 kind）。
/// 只开 `chat` 时直接返回静态数组；开了多模态才在首次调用时拼接一次。
pub fn presets() -> &'static [ProviderPreset] {
    #[cfg(not(any(feature = "image", feature = "video", feature = "tts")))]
    {
        chat::CHAT_PRESETS
    }
    #[cfg(any(feature = "image", feature = "video", feature = "tts"))]
    {
        static ALL: std::sync::OnceLock<Vec<ProviderPreset>> = std::sync::OnceLock::new();
        ALL.get_or_init(|| {
            let mut v = chat::CHAT_PRESETS.to_vec();
            #[cfg(feature = "image")]
            v.extend_from_slice(image::IMAGE_PRESETS);
            #[cfg(feature = "video")]
            v.extend_from_slice(video::VIDEO_PRESETS);
            #[cfg(feature = "tts")]
            v.extend_from_slice(tts::TTS_PRESETS);
            v
        })
    }
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
        // 按 host 判，不按全串：官方地址带不带 `/v1` 都是官方档。
        // 此前全串比较 `https://api.anthropic.com`，而默认端点已改成带 `/v1` 的写法，
        // 用户照抄官方文档填进来就会被误判成「Claude Code 中转」档。
        if url.is_empty() || url.contains("://api.anthropic.com") {
            return "anthropic_official";
        }
        return "claude_code";
    }
    if url.is_empty() {
        return CUSTOM_PRESET_KEY;
    }
    // 🔴 只在对话预置里反推：开了多模态后，硅基流动 / 火山方舟等同 host 的
    //    生图、视频预置也在 presets() 里，不过滤会把一条对话配置认成生图档
    for p in presets_for(Kind::Chat) {
        if p.protocol != Protocol::OpenAiCompatible {
            continue;
        }
        if p.match_hosts.iter().any(|h| url.contains(h)) {
            return p.key;
        }
    }
    CUSTOM_PRESET_KEY
}

/// 一条已存配置的模型在预置里登记的静态限额（`source: Preset`）。
///
/// 先按 [`infer_preset_key`] 找到预置，再按 model id 精确匹配。
/// 查不到返回 `None` —— 自定义端点、预置外的模型都是这种情况，**不猜**。
///
/// 这是分层回退里的第 2 层；调用方把用户设置 / 端点上报用
/// [`TokenLimits::or`](crate::limits::TokenLimits::or) 叠在它上面。
///
/// ```
/// # use ai_profile::{preset, Protocol};
/// let l = preset::model_limits(
///     Protocol::OpenAiCompatible,
///     Some("https://api.deepseek.com/v1"),
///     "deepseek-flash",
/// ).unwrap();
/// assert!(l.context_window.is_some());
/// assert!(preset::model_limits(Protocol::OpenAiCompatible, None, "whatever").is_none());
/// ```
pub fn model_limits(
    protocol: Protocol,
    base_url: Option<&str>,
    model: &str,
) -> Option<crate::limits::TokenLimits> {
    let model = model.trim();
    preset_by_key(infer_preset_key(protocol, base_url))?
        .models
        .iter()
        .find(|m| m.value == model)
        .and_then(ModelOption::preset_limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 官方档没写地址也要有端点；自定义档没写地址就是没有，必须让用户填。
    #[test]
    fn endpoint_falls_back_to_official_only_for_fixed_presets() {
        let official = preset_by_key("anthropic_official").unwrap();
        assert!(
            official.base_url.is_none(),
            "官方档的 base_url 要留空（界面据此隐藏地址框）"
        );
        assert_eq!(official.endpoint(), Some("https://api.anthropic.com/v1"));

        for key in ["claude_code", "codex", CUSTOM_PRESET_KEY] {
            let p = preset_by_key(key).unwrap();
            assert_eq!(p.endpoint(), None, "{key} 是让用户自填地址的档，不能替他猜");
        }
        let deepseek = preset_by_key("deepseek").unwrap();
        assert_eq!(deepseek.endpoint(), deepseek.base_url);
    }

    /// 守卫：所有「没写地址却声明了主机名」的预置，都必须能解析出端点 ——
    /// 否则它既不让用户填（界面隐藏地址框），又没有地址可请求。
    #[test]
    fn fixed_presets_without_base_url_resolve_an_endpoint() {
        for p in presets() {
            if p.base_url.is_none() && !p.match_hosts.is_empty() {
                assert!(
                    p.endpoint().is_some(),
                    "{} 没写地址也解析不出官方端点",
                    p.key
                );
            }
        }
    }

    /// 🔴 同一 kind 内同组必须连续 —— 不连续会让下拉切出两个同名分组标题。
    ///
    /// 按 kind 分别判：各 kind 分开渲染（对话下拉、生图下拉……），各自从「国内」起排。
    #[test]
    fn preset_groups_are_contiguous() {
        let mut kinds: Vec<Kind> = Vec::new();
        for p in presets() {
            if !kinds.contains(&p.kind) {
                kinds.push(p.kind);
            }
        }
        for kind in kinds {
            let mut seen: Vec<&str> = Vec::new();
            let mut prev = "";
            for p in presets_for(kind) {
                if p.group_key == prev {
                    continue;
                }
                assert!(
                    !seen.contains(&p.group_key),
                    "{:?} 的分组 {} 被拆成了不连续的多段",
                    kind,
                    p.group_key
                );
                seen.push(p.group_key);
                prev = p.group_key;
            }
        }
    }

    /// 同一 kind 的预置必须连续 —— `presets_for` 过滤无所谓，但文档与服务商目录按数组顺序走。
    #[test]
    fn preset_kinds_are_contiguous() {
        let mut seen: Vec<Kind> = Vec::new();
        let mut prev: Option<Kind> = None;
        for p in presets() {
            if prev == Some(p.kind) {
                continue;
            }
            assert!(
                !seen.contains(&p.kind),
                "{:?} 的预置被拆成了不连续的多段",
                p.kind
            );
            seen.push(p.kind);
            prev = Some(p.kind);
        }
    }

    /// 🔴 对话配置不能被反推成同 host 的生图 / 视频档。
    #[test]
    fn infer_preset_key_only_returns_chat_presets() {
        for url in [
            "https://api.siliconflow.cn/v1",
            "https://ark.cn-beijing.volces.com/api/v3",
        ] {
            let key = infer_preset_key(Protocol::OpenAiCompatible, Some(url));
            assert_eq!(
                preset_by_key(key).map(|p| p.kind),
                Some(Kind::Chat),
                "{url} → {key}"
            );
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
        // 🔴 官方文档写法带 /v1，同样是官方档（此前被误判成中转）
        assert_eq!(
            infer_preset_key(Protocol::Anthropic, Some("https://api.anthropic.com/v1")),
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
