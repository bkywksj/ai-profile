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

/// 把 base_url 和端点路径拼成完整 URL。
///
/// 仅有的两处容错：
/// - 剥掉误填的对话端点后缀（见 [`strip_chat_endpoint`]）——「同一个字段填了两种东西」
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
pub fn join_chat_endpoint(base: &str, path: &str) -> String {
    let probe = base.trim().trim_end_matches('#').trim_end_matches('/');
    if probe.ends_with(path) {
        return probe.to_string();
    }
    join_api_path(base, path)
}

/// 剥掉 base 末尾误填的对话端点后缀，把完整端点还原成 base。
fn strip_chat_endpoint(base: &str) -> &str {
    for suffix in ["chat/completions", "messages"] {
        if let Some(rest) = base.strip_suffix(suffix) {
            return rest.trim_end_matches('/');
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
