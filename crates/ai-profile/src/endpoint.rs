//! 端点拼接 —— **base_url 原样使用，不做任何版本段推断**。
//!
//! # 为什么不自动补 `/v1`
//!
//! 这里曾经是 `format!("{base}/v1/{path}")`，后来改成「末段是 `v<数字>` 就不补、
//! 否则补 /v1、末尾 `#` 可以强制不补」的推断。推断看着聪明，但它的例外一直在变多：
//!
//! - 智谱是 `/v4`，不是 `/v1`
//! - Gemini 的 OpenAI 兼容层是 `/v1beta/openai`，版本段不在末尾，只能靠 `#` 开后门
//! - 中转站还有 `/api/openai/v1`、`/proxy/anthropic` 这类各式各样的前缀
//!
//! **「需要一个转义符才能表达的规则」本身就说明规则不对。** 而推断错的代价是隐性的：
//! 用户照服务商文档填了正确的 base_url，我们悄悄加了一段，他只看到 404，
//! 也不知道真正请求的是哪个地址。
//!
//! 现在的契约很简单：**base_url 写什么就是什么，版本段由用户自己填**。
//! 预置清单里各家都已带上完整版本段（见 `preset` 模块）。
//!
//! # 唯一的例外：Anthropic 协议自动补 `/v1`
//!
//! 上面的理由只对 OpenAI 兼容这一侧成立 —— 那边各家版本段确实不统一。Anthropic 协议没有这个问题：
//! 它**只有 v1**，而且整个生态（官方 SDK、Claude Code 的 `ANTHROPIC_BASE_URL`、各家的 Anthropic
//! 兼容入口如 `https://api.deepseek.com/anthropic`）都约定 base 填到版本段之前、由客户端补
//! `/v1/messages`。按这个约定补，是**确定的协议翻译**，不是猜。
//!
//! 不补的代价是真实踩过的：从别的工具粘来 `https://x.com:8443` 这种地址，对话打到 `/messages`，
//! 中转网关对不认识的路径回 **200 + 前端网页**，报出来是「解析失败」，完全看不出是少了 `/v1`；
//! 而它的 `/models` 不带 `/v1` 也能用，于是「获取模型」是绿的、只有对话挂。
//!
//! 已带版本段的地址不受影响；末尾带 `#` 的原样保留（旧契约的「别替我补」标记，留给特殊网关）。

/// 把 base_url 和端点路径拼成完整 URL。
///
/// 仅有的两处容错：
/// - 剥掉误填的对话端点后缀（见 `strip_chat_endpoint`）——「同一个字段填了两种东西」
/// - 剥掉旧契约遗留的末尾 `#` 标记（存量配置里可能有）
pub fn join_api_path(base: &str, path: &str) -> String {
    let base = strip_chat_endpoint(base.trim().trim_end_matches('#').trim_end_matches('/'));
    format!("{base}/{path}")
}

/// 对话端点专用：用户把**完整端点**粘进 base_url 时原样使用。
///
/// 与 [`join_api_path`] 分开是有意的 —— 这条捷径只对对话端点成立。「获取模型」走的是
/// `<base>/models`，若共用一套判断，用户填了完整的 `/chat/completions` 会让它拿这个
/// 地址去 GET，必然失败（表现为"能聊天却拉不到模型"）。
///
/// `path` 传 `"chat/completions"`（OpenAI 兼容）或 `"messages"`（Anthropic）。
/// 传 `"messages"` 时按 Anthropic 约定补版本段，见 [`anthropic_base_url`]。
pub fn join_chat_endpoint(base: &str, path: &str) -> String {
    let probe = base.trim().trim_end_matches('#').trim_end_matches('/');
    // 同样按路径段认，与 strip_chat_endpoint 一致
    if probe.ends_with(&format!("/{path}")) {
        return probe.to_string();
    }
    if path == "messages" {
        return join_api_path(&anthropic_base_url(base), path);
    }
    join_api_path(base, path)
}

/// Anthropic 协议的 base：末段不是版本段就补 `/v1`（见模块文档「唯一的例外」）。
///
/// - `https://api.anthropic.com` → `https://api.anthropic.com/v1`
/// - `https://api.deepseek.com/anthropic` → `…/anthropic/v1`
/// - 已有版本段（`…/v1`）、误填了完整端点（`…/v1/messages`）→ 不动
/// - 末尾带 `#` → 原样（去掉 `#`），不补
///
/// 「获取模型」与对话都要走它，两边口径一致才不会出现「获取通过、对话 404」。
pub fn anthropic_base_url(base: &str) -> String {
    let t = base.trim();
    let pinned = t.ends_with('#');
    let b = strip_chat_endpoint(t.trim_end_matches('#').trim_end_matches('/'));
    if pinned || b.is_empty() || ends_with_version_segment(b) {
        return b.to_string();
    }
    format!("{b}/v1")
}

/// 剥掉 base 末尾误填的对话端点后缀，把完整端点还原成 base。
fn strip_chat_endpoint(base: &str) -> &str {
    for suffix in ["chat/completions", "messages"] {
        // 按路径段认：前面必须紧跟 `/`，否则 `…/mymessages` 会被剥成 `…/my`
        if let Some(rest) = base.strip_suffix(suffix) {
            if rest.ends_with('/') {
                return rest.trim_end_matches('/');
            }
        }
    }
    base
}

/// base 的最后一段是否形如 `v<数字>`（`/v1`、`/v4`…）。
///
/// 只认「`v` + 全数字」，所以 `https://v4.example.com` 这种**主机名**以 v4 开头的
/// 不会被误判（末段是 `v4.example.com`，含非数字字符）。
///
/// 🔴 端点拼接**不用它** —— 保留是为了两件事：守住「预置 base_url 必须自带版本段」
/// 这条契约，以及给前端「这个地址看着缺版本段」的提醒保持同一口径。
pub fn ends_with_version_segment(base: &str) -> bool {
    base.trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|seg| seg.strip_prefix('v'))
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 端点后缀按**路径段**认，不按字符串：`…/mymessages` 不是误填的 `/messages`。
    /// 此前按字符串剥，会把它剥成 `…/my` 再拼路径。外部实现者照规范写 Python 版时发现的。
    #[test]
    fn chat_suffix_is_matched_by_path_segment() {
        assert_eq!(
            join_api_path("https://relay.example.com/v1/mymessages", "models"),
            "https://relay.example.com/v1/mymessages/models"
        );
        assert_eq!(
            join_chat_endpoint("https://relay.example.com/v1/mymessages", "messages"),
            "https://relay.example.com/v1/mymessages/v1/messages"
        );
        // 真正误填的完整端点照旧剥掉
        assert_eq!(
            join_api_path("https://relay.example.com/v1/messages", "models"),
            "https://relay.example.com/v1/models"
        );
    }

    /// 🔴 本 crate 最容易被"好心"改回去的地方：看到用户填了
    /// `https://api.deepseek.com` 很容易想顺手补个 /v1。**不要补。**
    #[test]
    fn join_api_path_never_infers_version_segment() {
        // 不带版本段 = 原样拼，不替用户补
        assert_eq!(
            join_api_path("https://api.deepseek.com", "chat/completions"),
            "https://api.deepseek.com/chat/completions"
        );
        // 带 /v1
        assert_eq!(
            join_api_path("https://api.deepseek.com/v1", "chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        // 智谱的 /v4 —— 曾经被无脑补成 /v1 而 404
        assert_eq!(
            join_api_path("https://open.bigmodel.cn/api/paas/v4", "chat/completions"),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        // 版本段不在末尾（Gemini）：过去只能靠末尾加 # 开后门，现在天然就对
        assert_eq!(
            join_api_path(
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "chat/completions"
            ),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }

    /// 末尾多余的 `/` 与旧契约遗留的 `#` 都要吃掉，不能拼出双斜杠或把 # 带进 URL。
    #[test]
    fn join_api_path_trims_slash_and_legacy_hash() {
        assert_eq!(
            join_api_path("https://api.deepseek.com/v1/", "models"),
            "https://api.deepseek.com/v1/models"
        );
        assert_eq!(
            join_api_path(
                "https://generativelanguage.googleapis.com/v1beta/openai#",
                "models"
            ),
            "https://generativelanguage.googleapis.com/v1beta/openai/models"
        );
        assert_eq!(
            join_api_path("  https://api.deepseek.com/v1  ", "models"),
            "https://api.deepseek.com/v1/models"
        );
    }

    /// 用户把完整对话端点填进 base_url：对话原样用，但「获取模型」要先把端点剥掉，
    /// 否则会 GET `…/chat/completions/models`，表现成「能聊天却拉不到模型」。
    #[test]
    fn join_chat_endpoint_accepts_full_endpoint() {
        let full = "https://proxy.example.com/v1/chat/completions";
        assert_eq!(join_chat_endpoint(full, "chat/completions"), full);
        assert_eq!(
            join_api_path(full, "models"),
            "https://proxy.example.com/v1/models"
        );
        let anth = "https://proxy.example.com/v1/messages";
        assert_eq!(join_chat_endpoint(anth, "messages"), anth);
    }

    /// Anthropic 协议：用户照 Claude Code / 官方 SDK 的习惯只填到主机名（或 `/anthropic`）时自动补 `/v1`。
    #[test]
    fn anthropic_base_gets_v1_when_missing() {
        for (raw, want) in [
            (
                "https://cc.example.top:8443",
                "https://cc.example.top:8443/v1",
            ),
            ("https://api.anthropic.com/", "https://api.anthropic.com/v1"),
            (
                "https://api.deepseek.com/anthropic",
                "https://api.deepseek.com/anthropic/v1",
            ),
            // 已有版本段 / 误填完整端点 → 不动
            (
                "https://api.anthropic.com/v1",
                "https://api.anthropic.com/v1",
            ),
            (
                "https://proxy.example.com/v1/messages",
                "https://proxy.example.com/v1",
            ),
            // 显式 # = 别替我补
            (
                "https://odd.gateway.com/raw#",
                "https://odd.gateway.com/raw",
            ),
        ] {
            assert_eq!(anthropic_base_url(raw), want, "raw={raw}");
        }
        assert_eq!(
            join_chat_endpoint("https://cc.example.top:8443", "messages"),
            "https://cc.example.top:8443/v1/messages"
        );
        // 幂等：补过的再补一次不变
        let once = anthropic_base_url("https://cc.example.top:8443");
        assert_eq!(anthropic_base_url(&once), once);
    }

    /// 🔴 补 /v1 只对 Anthropic 生效；OpenAI 兼容一侧仍然原样拼（智谱 /v4、Gemini 等靠这条活着）。
    #[test]
    fn openai_side_still_never_infers() {
        assert_eq!(
            join_chat_endpoint("https://api.deepseek.com", "chat/completions"),
            "https://api.deepseek.com/chat/completions"
        );
    }

    /// 版本段判定只认「v + 全数字」；主机名以 v4 开头的不算。
    #[test]
    fn version_segment_detection() {
        assert!(ends_with_version_segment("https://api.deepseek.com/v1"));
        assert!(ends_with_version_segment(
            "https://open.bigmodel.cn/api/paas/v4"
        ));
        assert!(ends_with_version_segment("https://api.deepseek.com/v1/"));
        assert!(!ends_with_version_segment("https://api.deepseek.com"));
        assert!(!ends_with_version_segment("https://v4.example.com"));
        assert!(!ends_with_version_segment("https://x.com/v1beta/openai"));
    }
}
