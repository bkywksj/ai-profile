//! 下游定制服务商目录：在 crate 预置的基础上**增、删、改、筛**。
//!
//! # 为什么要有这一层
//!
//! crate 的预置是「所有下游都该看到」的公共部分。但总有只属于某个应用的条目 ——
//! 比如某个应用自己的合作渠道、内部网关，或者某个应用的调用实现只支持部分协议、要藏掉一些家。
//! 把这些塞进 crate 会让**所有**下游的下拉里都出现它；让下游整份抄走预置又回到了「各存一份、各自漂移」。
//!
//! 所以 crate 只放公共数据，下游用 [`PresetCatalog`] 在它上面叠自己的改动：
//!
//! ```
//! use ai_profile::preset::{PresetCatalog, ProviderPreset, ModelOption, GROUP_CHINA};
//! use ai_profile::Kind;
//!
//! // 应用私有的一条：只在本应用出现（const 构造，写成静态表）
//! static LOCAL: &[ProviderPreset] = &[
//!     ProviderPreset::new("my_gateway", Kind::Chat, "公司内网网关", Some("https://llm.corp.example/v1"))
//!         .with_group(GROUP_CHINA)
//!         .with_models("qwen-max", &[ModelOption::plain("qwen-max")]),
//! ];
//!
//! let list = PresetCatalog::new()
//!     .remove(&["groq"])                    // 本应用不想露出的
//!     .retain(|p| p.kind == Kind::Chat)     // 只要对话
//!     .extend(LOCAL)                           // 自己的条目（与 crate 同 key 则覆盖）
//!     .build();
//! assert!(list.iter().any(|p| p.key == "my_gateway"));
//! assert!(!list.iter().any(|p| p.key == "groq"));
//! ```
//!
//! # 🔴 协议是通用的，品牌不是
//!
//! 应用私有的条目只是**换了地址和模型名**，协议实现照样用 crate 的（OpenAI 兼容 / New API 视频……）。
//! 需要显式指定协议的（如 New API 视频中转站），用 [`ProviderPreset::with_default_extra`] 带上
//! `video_api=newapi`，crate 按它识别协议，不需要认识这个品牌。

use super::{presets, ProviderPreset};

/// 服务商目录构造器。见[模块文档](self)。
#[derive(Debug, Clone)]
pub struct PresetCatalog {
    list: Vec<ProviderPreset>,
}

impl Default for PresetCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetCatalog {
    /// 从 crate 的全部预置（本 build 开启的 kind）起步。
    pub fn new() -> Self {
        Self {
            list: presets().to_vec(),
        }
    }

    /// 从空目录起步（完全自定义；一般用不到）。
    pub fn empty() -> Self {
        Self { list: Vec::new() }
    }

    /// 按 key 删掉若干条。不认识的 key 忽略。
    pub fn remove(mut self, keys: &[&str]) -> Self {
        self.list.retain(|p| !keys.contains(&p.key));
        self
    }

    /// 只保留满足条件的条目（如「只留本应用调用实现支持的协议」）。
    pub fn retain(mut self, keep: impl Fn(&ProviderPreset) -> bool) -> Self {
        self.list.retain(|p| keep(p));
        self
    }

    /// 逐条改（如给某些条目补默认地址）。
    pub fn map(mut self, f: impl Fn(ProviderPreset) -> ProviderPreset) -> Self {
        self.list = self.list.into_iter().map(f).collect();
        self
    }

    /// 加入应用自己的条目。
    ///
    /// - key 与已有条目相同 → **原位覆盖**（应用想改某家 crate 预置的模型清单 / 说明时用）
    /// - 否则插到**同 kind、同分组**的最后一条之后，保证下拉分组连续；
    ///   该 kind 里没有这个分组 → 插到该 kind 的最后；该 kind 一条都没有 → 追加到末尾
    pub fn extend(mut self, extra: &[ProviderPreset]) -> Self {
        for p in extra {
            if let Some(slot) = self.list.iter_mut().find(|q| q.key == p.key) {
                *slot = *p;
                continue;
            }
            let same_group = self
                .list
                .iter()
                .rposition(|q| q.kind == p.kind && q.group_key == p.group_key);
            let same_kind = self.list.iter().rposition(|q| q.kind == p.kind);
            match same_group.or(same_kind) {
                Some(i) => self.list.insert(i + 1, *p),
                None => self.list.push(*p),
            }
        }
        self
    }

    /// 生成最终目录（顺序即下拉呈现顺序）。
    pub fn build(self) -> Vec<ProviderPreset> {
        self.list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::Kind;
    use crate::preset::{ModelOption, GROUP_CHINA, GROUP_INTERNATIONAL};

    static LOCAL: &[ProviderPreset] = &[
        ProviderPreset::new(
            "local_cn",
            Kind::Chat,
            "本地国内",
            Some("https://cn.example/v1"),
        )
        .with_group(GROUP_CHINA)
        .with_models("m1", &[ModelOption::plain("m1")]),
        ProviderPreset::new(
            "local_intl",
            Kind::Chat,
            "本地国际",
            Some("https://intl.example/v1"),
        )
        .with_group(GROUP_INTERNATIONAL),
    ];

    #[test]
    fn add_inserts_into_matching_group_keeping_it_contiguous() {
        let list = PresetCatalog::new().extend(LOCAL).build();
        let pos = |k: &str| list.iter().position(|p| p.key == k).unwrap();
        // 紧跟在各自分组的最后一条之后
        assert_eq!(list[pos("local_cn") - 1].group_key, GROUP_CHINA.0);
        assert_eq!(list[pos("local_intl") - 1].group_key, GROUP_INTERNATIONAL.0);
        // 同 kind 内分组仍连续
        let mut seen: Vec<&str> = Vec::new();
        let mut prev = "";
        for p in list.iter().filter(|p| p.kind == Kind::Chat) {
            if p.group_key != prev {
                assert!(!seen.contains(&p.group_key), "分组 {} 被拆开", p.group_key);
                seen.push(p.group_key);
                prev = p.group_key;
            }
        }
    }

    #[test]
    fn add_with_same_key_overrides_in_place() {
        static OVERRIDE: &[ProviderPreset] = &[ProviderPreset::new(
            "deepseek",
            Kind::Chat,
            "我的 DeepSeek",
            Some("https://api.deepseek.com/v1"),
        )
        .with_group(GROUP_CHINA)];
        let before = PresetCatalog::new().build();
        let after = PresetCatalog::new().extend(OVERRIDE).build();
        assert_eq!(before.len(), after.len(), "覆盖不增加条数");
        let i = before.iter().position(|p| p.key == "deepseek").unwrap();
        assert_eq!(after[i].label, "我的 DeepSeek", "原位覆盖");
    }

    #[test]
    fn remove_and_retain() {
        let list = PresetCatalog::new()
            .remove(&["deepseek", "不存在的 key"])
            .retain(|p| p.kind == Kind::Chat)
            .build();
        assert!(!list.iter().any(|p| p.key == "deepseek"));
        assert!(list.iter().all(|p| p.kind == Kind::Chat));
    }

    #[test]
    fn default_extra_and_builder_defaults() {
        const P: ProviderPreset = ProviderPreset::new("x", Kind::Chat, "X", None)
            .with_default_extra(&[("video_api", "newapi")]);
        assert_eq!(P.vendor_id, "x", "vendor_id 缺省取 key");
        assert_eq!(P.default_extra, &[("video_api", "newapi")]);
        assert!(P.models.is_empty() && P.model.is_empty());
    }
}
