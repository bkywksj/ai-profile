//! 按厂商聚合 —— 服务商目录的数据源。
//!
//! # 🔴 为什么要按厂商聚合，而不是把预置平铺给界面
//!
//! 实测各家 endpoint 的 host：硅基流动覆盖 4 种能力（对话/生图/视频/配音）、
//! 阿里百炼 3 种、智谱与火山方舟各 2 种。**同一个 host 用同一个 key。**
//!
//! 按 kind 平铺会让用户把硅基流动的密钥填四遍；按厂商聚合则是
//! 「填一次密钥，勾选要启用哪几种能力」。
//!
//! 平铺还有个更糟的后果：**用户看不出「配了硅基流动就等于四种能力全有了」** ——
//! 而这恰恰是他最该知道的信息。

use serde::Serialize;

use super::{presets, ProviderPreset};
use crate::kind::Kind;

/// 一家服务商（跨 kind 聚合后的视图）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Vendor {
    /// 与 [`ProviderPreset::vendor_id`] 对应
    pub id: &'static str,
    /// 展示名，取该厂商第一条预置的 `label`
    pub label: &'static str,
    /// 分组 key（i18n）
    pub group_key: &'static str,
    /// 分组纯文本名
    pub group_label: &'static str,
    /// 这家覆盖哪几种能力（按预置数组顺序去重）
    pub kinds: Vec<Kind>,
    /// 该厂商下所有预置的 key，调用方点卡片后据此打开对应表单
    pub preset_keys: Vec<&'static str>,
    /// base_url 的 host（展示用，也是「同密钥多能力」的依据）；
    /// `None` = 自定义端点档，没有固定 host
    pub host: Option<&'static str>,
    /// 密钥申请页
    pub apply_url: Option<&'static str>,
    /// 本地服务（需用户先把服务跑起来）
    pub is_local: bool,
}

/// 从 base_url 抽出 host（含端口），用于判定「是不是同一家」。
fn host_of(base_url: Option<&'static str>) -> Option<&'static str> {
    let url = base_url?;
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    Some(match rest.find('/') {
        Some(i) => &rest[..i],
        None => rest,
    })
}

/// 按厂商聚合，仅返回与 `allow_kinds` 有交集的厂商。
///
/// 调用方传入本应用声明的 kinds —— 只做对话的应用不会看到只提供视频的厂商。
/// 返回顺序沿用预置数组顺序（即分组顺序）。
pub fn vendors(allow_kinds: &[Kind]) -> Vec<Vendor> {
    let mut out: Vec<Vendor> = Vec::new();
    for p in presets() {
        if !allow_kinds.contains(&p.kind) {
            continue;
        }
        match out.iter_mut().find(|v| v.id == p.vendor_id) {
            Some(v) => {
                if !v.kinds.contains(&p.kind) {
                    v.kinds.push(p.kind);
                }
                v.preset_keys.push(p.key);
            }
            None => out.push(Vendor {
                id: p.vendor_id,
                label: p.label,
                group_key: p.group_key,
                group_label: p.group_label,
                kinds: vec![p.kind],
                preset_keys: vec![p.key],
                host: host_of(p.base_url),
                apply_url: p.apply_url,
                is_local: p.is_local,
            }),
        }
    }
    out
}

/// 该厂商下的全部预置。
pub fn presets_of_vendor(vendor_id: &str) -> impl Iterator<Item = &'static ProviderPreset> + '_ {
    presets().iter().filter(move |p| p.vendor_id == vendor_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 同 `vendor_id` 的 host 必须一致 —— 否则服务商目录会把两家并成一张卡，
    /// 用户以为填一次密钥两边都能用，实际上另一家根本不认。
    #[test]
    fn vendor_ids_consistent() {
        for p in presets() {
            let host = host_of(p.base_url);
            for q in presets_of_vendor(p.vendor_id) {
                let qh = host_of(q.base_url);
                // 两边都有 host 时必须相等；任一为 None（自定义端点档）则跳过
                if let (Some(a), Some(b)) = (host, qh) {
                    assert_eq!(
                        a, b,
                        "vendor_id={} 下 {} 与 {} 的 host 不一致（{} vs {}）",
                        p.vendor_id, p.key, q.key, a, b
                    );
                }
            }
        }
    }

    /// 只声明 chat 的应用不该看到纯视频厂商；聚合后每家至少有一条预置。
    #[test]
    fn vendors_filtered_by_kind() {
        let all = vendors(&[Kind::Chat]);
        assert!(!all.is_empty(), "chat 应当有厂商");
        for v in &all {
            assert!(!v.preset_keys.is_empty(), "{} 没有任何预置", v.id);
            assert!(v.kinds.contains(&Kind::Chat));
        }
        // 传空 kinds → 什么都不返回（应用没声明任何能力）
        assert!(vendors(&[]).is_empty());
    }

    /// 本地服务与云端服务必须可区分 —— 目录卡片据此给「先启动它」而非「去申请密钥」。
    #[test]
    fn local_vendors_are_marked() {
        let v = vendors(&[Kind::Chat]);
        let ollama = v.iter().find(|x| x.id == "ollama").expect("应有 ollama");
        assert!(ollama.is_local);
        assert!(ollama.apply_url.is_none(), "本地服务不需要申请密钥");

        let ds = v
            .iter()
            .find(|x| x.id == "deepseek")
            .expect("应有 deepseek");
        assert!(!ds.is_local);
        assert!(ds.apply_url.is_some(), "云端服务应给申请入口");
    }

    #[test]
    fn host_extraction() {
        assert_eq!(
            host_of(Some("https://api.deepseek.com/v1")),
            Some("api.deepseek.com")
        );
        assert_eq!(
            host_of(Some("http://localhost:11434/v1")),
            Some("localhost:11434")
        );
        assert_eq!(
            host_of(Some("https://open.bigmodel.cn/api/paas/v4")),
            Some("open.bigmodel.cn")
        );
        assert_eq!(host_of(None), None);
    }
}
