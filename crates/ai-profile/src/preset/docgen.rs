//! `docs/providers.md` 的生成器。
//!
//! # 为什么生成而不是手写
//!
//! 20 家服务商散在按 kind 分的多个文件里，没有一处能一眼看全 —— 开源后这是
//! README 之外最该有的文档。但手写就是**第二份数据源**，而本 crate 存在的
//! 全部理由正是「消掉重复的数据源」，再引入一份说不过去。
//!
//! 所以：
//! - `cargo xtask gen-docs` 调 [`render_providers_markdown`] 写文件
//! - 守卫测试 `providers_md_in_sync` 调同一个函数比对
//!
//! 两边共用一份逻辑，文档漂移会让 CI 直接红。

use super::{presets, vendor::vendors_all};
use crate::kind::Kind;

/// 文件头：说明这是生成物，别手改。
const HEADER: &str = "<!-- 🔴 本文件由 `cargo xtask gen-docs` 生成，请勿手改。\n     \
改预置请编辑 crates/ai-profile/src/preset/*.rs，再重新生成。\n     \
守卫测试 `providers_md_in_sync` 会比对一致性。 -->\n";

/// 渲染 `docs/providers.md` 的完整内容。
///
/// `#[doc(hidden)]`：这是给 xtask 与守卫测试用的内部设施，不是给下游的 API。
#[doc(hidden)]
pub fn render_providers_markdown() -> String {
    let mut s = String::with_capacity(8 * 1024);
    s.push_str(HEADER);
    s.push_str("\n# 支持的服务商\n\n");

    let all = presets();
    let vs = vendors_all();
    s.push_str(&format!(
        "当前共 **{}** 家服务商、**{}** 条预置配置。\n\n",
        vs.len(),
        all.len()
    ));

    s.push_str(
        "> `base_url` 一律是**服务商文档里的原文**（含版本段、不含端点后缀）。\n\
         > 本 crate 原样使用它、不做任何推断 —— 所以各家的版本段不统一（多数 `/v1`、\n\
         > 智谱 `/v4`、Gemini 的 `/v1beta/openai` 还不在末尾）也不影响。\n\n",
    );

    // ── 按厂商汇总 ─────────────────────────────────────────────
    s.push_str("## 按服务商\n\n");
    s.push_str(
        "同一家的多种能力**共用一个密钥** —— 配一次就能全部启用。\n\n\
         | 服务商 | 能力 | Host | 类型 |\n|---|---|---|---|\n",
    );
    for v in &vs {
        let kinds = v
            .kinds
            .iter()
            .map(|k| k.label())
            .collect::<Vec<_>>()
            .join(" / ");
        let host = v.host.unwrap_or("（自定义）");
        let ty = if v.is_local {
            "本地，需先启动服务"
        } else if v.apply_url.is_some() {
            "云端"
        } else {
            "自定义端点"
        };
        s.push_str(&format!(
            "| {} | {} | `{}` | {} |\n",
            v.label, kinds, host, ty
        ));
    }

    // ── 按能力分组的明细 ───────────────────────────────────────
    for kind in ALL_KINDS {
        let items: Vec<_> = all.iter().filter(|p| p.kind == *kind).collect();
        if items.is_empty() {
            continue;
        }
        s.push_str(&format!("\n## {}\n\n", kind.label()));
        s.push_str("| 预置 key | 名称 | Base URL | 默认模型 | 协议 | 核实于 |\n");
        s.push_str("|---|---|---|---|---|---|\n");
        for p in items {
            let base = p.base_url.unwrap_or("—");
            let model = if p.model.is_empty() { "—" } else { p.model };
            let proto = match p.protocol {
                crate::kind::Protocol::Anthropic => "Anthropic",
                _ => "OpenAI 兼容",
            };
            // 🔴 verified_at 为空 = 未实际调通过，明确标出来而不是留白 ——
            //    留白会让读者以为"没这个概念"，标出来才知道该自己验一下
            let verified = p.verified_at.unwrap_or("未核实");
            s.push_str(&format!(
                "| `{}` | {} | `{}` | `{}` | {} | {} |\n",
                p.key, p.label, base, model, proto, verified
            ));
        }
    }

    s.push_str("\n## 加一家服务商\n\n");
    s.push_str(
        "见 [`CLAUDE.md`](../CLAUDE.md) 的「加一家 provider 的完整流程」。要点：\n\n\
         1. `base_url` 照抄文档原文，含版本段\n\
         2. 默认 `model` 选**够用档**而非最强档\n\
         3. `models` 只放核对过的 id，并填 `verified_at`\n\
         4. 同一厂商复用同一个 `vendor_id`\n\
         5. `cargo test` 七个守卫测试必须全绿\n\
         6. `cargo xtask gen-docs` 重新生成本文件\n",
    );
    s
}

/// 本 build 开启的所有 kind —— 生成文档时按这个顺序分节。
const ALL_KINDS: &[Kind] = &[
    Kind::Chat,
    #[cfg(feature = "image")]
    Kind::Image,
    #[cfg(feature = "video")]
    Kind::Video,
    #[cfg(feature = "tts")]
    Kind::Tts,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 `docs/providers.md` 必须与代码一致。
    ///
    /// 不一致时的修法：`cargo xtask gen-docs`。
    ///
    /// 为什么要有这条：文档一旦手改就变成第二份数据源，而本 crate 存在的全部
    /// 理由正是消掉重复数据源 —— 自己再引入一份说不过去。
    ///
    /// 只在四种 kind 全开时比对：文档按本 build 的 kind 生成，`cargo xtask gen-docs`
    /// 开的是全部 feature；只开 chat 的 build 生成的内容本来就少几节。
    #[test]
    #[cfg(all(feature = "image", feature = "video", feature = "tts"))]
    fn providers_md_in_sync() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/providers.md");
        let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "读不到 {}：{e}\n跑 `cargo xtask gen-docs` 生成它",
                path.display()
            )
        });
        let expected = render_providers_markdown();
        // 行尾归一：仓库里可能是 CRLF（Windows 检出），生成的是 LF
        let norm = |s: &str| s.replace("\r\n", "\n");
        assert_eq!(
            norm(&on_disk),
            norm(&expected),
            "docs/providers.md 与代码不一致 —— 跑 `cargo xtask gen-docs` 重新生成"
        );
    }

    /// 生成物要包含每一条预置，不能漏。
    #[test]
    fn rendered_doc_covers_every_preset() {
        let md = render_providers_markdown();
        for p in presets() {
            assert!(
                md.contains(&format!("`{}`", p.key)),
                "生成的文档漏了预置 {}",
                p.key
            );
        }
    }

    /// 未核实的预置要明确标出，而不是留白。
    #[test]
    fn unverified_presets_are_labeled() {
        let md = render_providers_markdown();
        let unverified = presets().iter().filter(|p| p.verified_at.is_none()).count();
        if unverified > 0 {
            assert!(
                md.contains("未核实"),
                "有 {unverified} 条未核实的预置，文档里应当标出来"
            );
        }
    }
}
