//! 对话历史裁剪 —— 发请求之前把超出上下文窗口的旧消息丢掉，以及超长报错后的被动重试依据。
//!
//! # 为什么在 crate 里
//!
//! 最初写在 sigil（`llm_trim.rs`）。reeve 接入时发现两边的消息结构完全一样
//! （Anthropic 风格 `{ role, content }`，前端驱动工具循环、每轮把全部历史交给后端），
//! 按「第二个结构一致的使用方出现」这条判据收进来 —— 否则就是第二份会漂移的副本。
//!
//! 消息类型各应用自己定义，这里只要求实现 [`HistoryMessage`]（取角色、取内容）两行。
//!
//! # 两道防线
//!
//! | 场景 | 做法 |
//! |---|---|
//! | 窗口已知 | 发送前主动裁：[`history_budget`] → [`trim_history`] |
//! | 窗口未知 | 🔴 **不猜**，照常发；服务端报超长（[`is_context_overflow`]）后按 [`retry_budget`] 裁一半重试 |
//!
//! 第二道是为「会话无限膨胀」准备的：自定义中转站多数不报限额，只靠第一道的话
//! 这类配置永远不裁，直到每一轮都 400。被动重试不需要知道窗口大小 —— 服务端的报错就是事实。
//!
//! # 🔴 绝不打断 tool_use / tool_result 配对
//!
//! Anthropic 与 OpenAI 都要求：assistant 消息里出现的每个 `tool_use.id`，必须在**紧随其后**的
//! 消息里有对应的 `tool_result`。只裁掉其中一半会让请求直接 400，而报错只说
//! 「tool_use ids were found without tool_result blocks」，很难联想到是裁剪干的。
//! 所以按「轮次」裁：一个 assistant + 它的 tool_result 是不可分割的单元。
//!
//! # 估算而非精确计数
//!
//! 精确 token 数要按各家 tokenizer 算，引入 tokenizer 是几 MB 的体积代价，而这里只需回答
//! 「要不要裁」。用字符数估算并**刻意偏保守** —— 裁多了只是少几轮上下文，裁少了是整个请求失败。

use serde_json::Value;

use crate::limits::TokenLimits;

/// 每 token 约折合多少字符 —— 刻意取偏小的值，估出来的 token 数偏大，裁得更早。
///
/// 英文约 4 字符/token，中文约 1.5，混合内容取 2 偏保守。只用来决定「裁不裁」，不需要精确。
const CHARS_PER_TOKEN: usize = 2;

/// 给系统提示词 + 工具定义预留的余量（token）。
///
/// 这两块不参与裁剪但要占窗口。工具定义尤其大 —— 一百多个工具的全量 schema 能到几万 token。
pub const OVERHEAD_TOKENS: u32 = 8192;

/// 可被裁剪的一条消息。
///
/// 内容按 Anthropic 风格理解：纯文本是字符串，否则是 content block 数组
/// （`{"type":"text"}` / `{"type":"tool_use","id":…}` / `{"type":"tool_result","tool_use_id":…}`）。
///
/// ```
/// use ai_profile::history::HistoryMessage;
/// use serde_json::Value;
///
/// #[derive(Clone)]
/// struct ChatMessage { role: String, content: Value }
///
/// impl HistoryMessage for ChatMessage {
///     fn role(&self) -> &str { &self.role }
///     fn content(&self) -> &Value { &self.content }
/// }
/// ```
pub trait HistoryMessage: Clone {
    /// `"user"` / `"assistant"`
    fn role(&self) -> &str;
    /// 字符串或 content block 数组
    fn content(&self) -> &Value;
}

/// 裁剪结果。
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Trimmed<M> {
    /// 裁剪后的消息，可直接发出
    pub messages: Vec<M>,
    /// 丢掉了多少条（= 原条数 − 实际发出条数）。0 = 没裁。
    ///
    /// 🔴 调用方应当把它显示给用户：「模型不记得前面说过的话」如果没有任何提示，
    /// 用户只会觉得它变笨了，而且查不出原因。
    pub dropped: usize,
}

/// 估算一条消息占多少 token。
///
/// 量内容序列化后的 JSON 长度 —— 工具调用参数、tool_result 内容都在里面，只数 text 会严重低估。
fn message_tokens<M: HistoryMessage>(msg: &M) -> u32 {
    let len = serde_json::to_string(msg.content())
        .map(|s| s.len())
        .unwrap_or(0)
        + msg.role().len();
    // 每条消息还有角色标记等固定开销，加一点常数
    u32::try_from(len / CHARS_PER_TOKEN + 8).unwrap_or(u32::MAX)
}

/// 估算整段历史占多少 token（偏保守）。
pub fn estimate_tokens<M: HistoryMessage>(messages: &[M]) -> u32 {
    messages
        .iter()
        .map(message_tokens)
        .fold(0u32, u32::saturating_add)
}

fn has_block<M: HistoryMessage>(msg: &M, kind: &str) -> bool {
    msg.content().as_array().is_some_and(|blocks| {
        blocks
            .iter()
            .any(|b| b.get("type").and_then(Value::as_str) == Some(kind))
    })
}

/// 这条消息发起了 `tool_use`（在等一个 tool_result）。
fn has_tool_use<M: HistoryMessage>(msg: &M) -> bool {
    has_block(msg, "tool_use")
}

/// 这条消息是 `tool_result`（上一条的应答）。
fn has_tool_result<M: HistoryMessage>(msg: &M) -> bool {
    has_block(msg, "tool_result")
}

/// 按 token 预算裁剪历史，**保留最近的**。
///
/// # 规则
///
/// 1. 从最新往回累加，超预算就停 —— 最近的上下文最有价值
/// 2. **第一条 user 消息永远保留**：它通常是任务描述，丢了模型会不知道在干什么
/// 3. 不切断 tool_use / tool_result 配对：要么一起留，要么一起丢
///
/// `budget` 为 `None`（窗口未知）时**原样返回** —— 猜一个数字去裁用户的历史比不裁更糟。
/// 窗口未知时的保护靠被动重试，见 [`is_context_overflow`] / [`retry_budget`]。
///
/// ```
/// use ai_profile::history::{trim_history, HistoryMessage};
/// # use serde_json::{json, Value};
/// # #[derive(Clone)] struct M { role: String, content: Value }
/// # impl HistoryMessage for M {
/// #     fn role(&self) -> &str { &self.role }
/// #     fn content(&self) -> &Value { &self.content }
/// # }
/// let msgs = vec![
///     M { role: "user".into(), content: json!("x".repeat(10_000)) },
///     M { role: "assistant".into(), content: json!("ok") },
/// ];
/// assert_eq!(trim_history(msgs.clone(), None).dropped, 0);  // 窗口未知：不裁
/// assert!(trim_history(msgs, Some(100)).dropped > 0);
/// ```
pub fn trim_history<M: HistoryMessage>(messages: Vec<M>, budget: Option<u32>) -> Trimmed<M> {
    let Some(budget) = budget else {
        return Trimmed {
            messages,
            dropped: 0,
        };
    };
    if messages.is_empty() || estimate_tokens(&messages) <= budget {
        return Trimmed {
            messages,
            dropped: 0,
        };
    }

    // 第一条 user 消息是任务描述，丢了模型会不知道在干什么 —— 先给它**预留**位置，
    // 再用剩下的预算从最新往回收。
    //
    // 🔴 只在它不超过预算一半时预留：用户第一条就贴了一大段日志时，无条件保留会让结果
    //    永远超预算 —— 主动裁剪降不下来，被动重试又因「没有可裁的」而停下，
    //    会话从此每一轮都 400（sigil 此前的实现就有这个缺陷）。占一半以上时，最近的上下文更要紧。
    let first_user = messages
        .iter()
        .position(|m| m.role() == "user" && !has_tool_result(m));
    let reserve = first_user
        .map(|i| message_tokens(&messages[i]))
        .filter(|t| *t <= budget / 2)
        .unwrap_or(0);
    let recent_budget = budget - reserve;

    // 从最新往回收，凑够预算就停
    let mut keep_from = messages.len();
    let mut used = 0u32;
    for (i, m) in messages.iter().enumerate().rev() {
        let t = message_tokens(m);
        if used.saturating_add(t) > recent_budget {
            break;
        }
        used += t;
        keep_from = i;
    }

    // 🔴 保留区不能以「孤儿 tool_result」开头 —— 它的 tool_use 在被裁掉的那半边
    while keep_from < messages.len() && has_tool_result(&messages[keep_from]) {
        keep_from += 1;
    }

    // 🔴 保留区不能以「发起了 tool_use 却没有应答」结尾（裁剪点恰好落在一轮中间）
    let mut keep_to = messages.len();
    if keep_from < keep_to && has_tool_use(&messages[keep_to - 1]) {
        keep_to -= 1;
    }

    if keep_from >= keep_to {
        // 极端情况：预算小到连一轮都放不下。
        //
        // 🔴 即便如此也不能留下残缺的 tool 配对 —— 那是确定被拒收的形状。
        // 从后往前找第一条**自身完整**的消息（纯文本）留下；找不到就留空，
        // 让上层参数校验报一个能读懂的错。
        let fallback: Vec<M> = messages
            .iter()
            .rev()
            .find(|m| !has_tool_use(*m) && !has_tool_result(*m))
            .cloned()
            .into_iter()
            .collect();
        return Trimmed {
            dropped: messages.len() - fallback.len(),
            messages: fallback,
        };
    }

    let mut kept: Vec<M> = messages[keep_from..keep_to].to_vec();

    // 预留过位置、且它确实落在被裁掉的那一段里 → 补回最前面
    if let Some(i) = first_user {
        if reserve > 0 && i < keep_from {
            kept.insert(0, messages[i].clone());
        }
    }

    // 🔴 按「实际发出的条数」算 —— 首条 user 可能被补回，按区间算会多报一条
    //    （sigil 真实 DeepSeek 实测暴露：10 条裁到 4 条却报「丢 7」）
    let dropped = messages.len() - kept.len();
    Trimmed {
        messages: kept,
        dropped,
    }
}

/// 从限额算出留给历史消息的 token 预算：`窗口 − 本次 max_tokens − OVERHEAD_TOKENS`。
///
/// 与 [`TokenLimits::input_budget`] 的区别：这里额外扣掉系统提示与工具定义的余量，
/// 是给 [`trim_history`] 直接用的数字。窗口未知返回 `None`（调用方据此不裁）。
///
/// ```
/// use ai_profile::history::{history_budget, OVERHEAD_TOKENS};
/// use ai_profile::TokenLimits;
///
/// let l = TokenLimits::from_endpoint(Some(128_000), Some(8192));
/// assert_eq!(history_budget(Some(&l), 4096), Some(128_000 - 4096 - OVERHEAD_TOKENS));
/// assert_eq!(history_budget(None, 4096), None);
/// ```
pub fn history_budget(limits: Option<&TokenLimits>, max_tokens: u32) -> Option<u32> {
    limits?
        .input_budget(max_tokens)
        .map(|b| b.saturating_sub(OVERHEAD_TOKENS))
}

/// 服务端报「上下文超长」后，重试该用的预算：当前历史估算值的一半。
///
/// 🔴 调用方的重试循环要设上限（建议 3 次），并在 `trim_history` 返回 `dropped == 0`
/// 时停止 —— 那说明已经裁无可裁，再试只是重复同一个失败。
///
/// 为什么是一半：不知道窗口多大，只知道「当前这么多放不下」。对半收敛最快，
/// 三次内能从 8 倍超额降到窗口以内；裁多了只是少几轮上下文。
pub fn retry_budget<M: HistoryMessage>(messages: &[M]) -> u32 {
    estimate_tokens(messages) / 2
}

/// 这个错误是不是「输入超出上下文窗口」—— 是的话裁掉旧历史重试能解决。
///
/// 只认明确的超长报错，**宁可漏判不可误判**：误判会把用户的历史无谓地裁掉一半。
/// 特别排除：
/// - `429`：限流报错里常出现「tokens per min」，与窗口无关
/// - 「max_tokens 太大」：那是**输出**上限，裁历史解决不了
///
/// 覆盖的真实报错形状见单元测试（OpenAI / Anthropic / DeepSeek / Gemini / OpenRouter /
/// Kimi / 通义 / 智谱 / vLLM）。`413` 视为超长（请求体过大，裁历史同样有效）。
pub fn is_context_overflow(status: u16, body: &str) -> bool {
    if status == 413 {
        return true;
    }
    if !matches!(status, 400 | 422) {
        return false;
    }
    let b = body.to_ascii_lowercase();

    // 输出上限超了 —— 裁历史没用。这类报错不提 context / prompt / input。
    let about_output_only = (b.contains("max_tokens") || b.contains("max_completion_tokens"))
        && !b.contains("context")
        && !b.contains("prompt")
        && !b.contains("input");
    if about_output_only {
        return false;
    }

    const PATTERNS: &[&str] = &[
        "context_length_exceeded",
        "maximum context length",
        "context length",
        "context window",
        "prompt is too long",
        "prompt too long",
        "too many tokens",
        "exceeds the maximum number of tokens",
        "exceeded model token limit",
        "input is too long",
        "input length",
        "input tokens exceed",
        "请求内容过长",
        "超长",
        "输入长度",
        "超过最大长度",
        "上下文长度",
        "超出模型",
    ];
    // 中文模式是原文比较（to_ascii_lowercase 不动非 ASCII）
    PATTERNS.iter().any(|p| b.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq)]
    struct Msg {
        role: String,
        content: Value,
    }

    impl HistoryMessage for Msg {
        fn role(&self) -> &str {
            &self.role
        }
        fn content(&self) -> &Value {
            &self.content
        }
    }

    fn text(role: &str, s: &str) -> Msg {
        Msg {
            role: role.into(),
            content: json!(s),
        }
    }

    fn tool_use_msg(id: &str) -> Msg {
        Msg {
            role: "assistant".into(),
            content: json!([{ "type": "tool_use", "id": id, "name": "x", "input": {} }]),
        }
    }

    fn tool_result_msg(id: &str, payload: &str) -> Msg {
        Msg {
            role: "user".into(),
            content: json!([{ "type": "tool_result", "tool_use_id": id, "content": payload }]),
        }
    }

    /// 窗口未知时**不裁** —— 猜一个数字去裁用户的历史比不裁更糟。
    #[test]
    fn unknown_budget_never_trims() {
        let msgs = vec![text("user", &"x".repeat(100_000))];
        let out = trim_history(msgs.clone(), None);
        assert_eq!(out.dropped, 0);
        assert_eq!(out.messages, msgs);
    }

    #[test]
    fn under_budget_is_untouched() {
        let msgs = vec![text("user", "hi"), text("assistant", "hello")];
        let out = trim_history(msgs.clone(), Some(100_000));
        assert_eq!(out.dropped, 0);
        assert_eq!(out.messages, msgs);
    }

    #[test]
    fn keeps_recent_drops_old() {
        let msgs = vec![
            text("user", &"a".repeat(4000)),
            text("assistant", &"b".repeat(4000)),
            text("user", &"c".repeat(4000)),
            text("assistant", &"d".repeat(40)),
        ];
        let out = trim_history(msgs, Some(2100));
        assert!(out.dropped > 0, "该裁却没裁");
        let last = out.messages.last().unwrap();
        assert_eq!(last.role, "assistant");
        assert!(last.content.as_str().unwrap().contains("dddd"));
    }

    /// 🔴 保留区不能以孤儿 tool_result 开头。
    #[test]
    fn never_starts_with_orphan_tool_result() {
        let msgs = vec![
            text("user", &"a".repeat(8000)),
            tool_use_msg("t1"),
            tool_result_msg("t1", &"r".repeat(8000)),
            text("assistant", "done"),
        ];
        let out = trim_history(msgs, Some(4200));
        assert!(!has_tool_result(&out.messages[0]), "{:?}", out.messages[0]);
    }

    /// 🔴 保留区不能以「发起了 tool_use 却没有应答」结尾。
    #[test]
    fn never_ends_with_unanswered_tool_use() {
        let msgs = vec![
            text("user", "start"),
            text("assistant", &"x".repeat(20_000)),
            tool_use_msg("t9"),
        ];
        let out = trim_history(msgs, Some(200));
        assert!(!out.messages.last().is_some_and(has_tool_use));
    }

    /// 首条 user（任务描述）被裁掉时要补回来；🔴 dropped 按实际发出条数算。
    #[test]
    fn preserves_first_user_message_and_counts_correctly() {
        let msgs = vec![
            text("user", "把这个仓库的测试全部跑一遍"),
            text("assistant", &"x".repeat(20_000)),
            text("user", "继续"),
        ];
        let total = msgs.len();
        let out = trim_history(msgs, Some(600));
        assert!(out.dropped > 0);
        assert_eq!(out.dropped, total - out.messages.len());
        assert!(out.messages[0]
            .content
            .as_str()
            .unwrap()
            .contains("把这个仓库"));
    }

    /// 🔴 首条 user 本身就超预算时**不补回** —— 否则永远降不到预算以内，被动重试也救不了。
    #[test]
    fn oversized_first_user_is_not_forced_back() {
        let msgs = vec![
            text("user", &"日志".repeat(20_000)),
            text("assistant", "看完了"),
            text("user", "那第三行什么意思"),
        ];
        let out = trim_history(msgs, Some(200));
        assert!(estimate_tokens(&out.messages) <= 200, "{:?}", out.messages);
        assert!(!out
            .messages
            .iter()
            .any(|m| m.content.as_str().is_some_and(|c| c.len() > 1000)));
        assert_eq!(
            out.messages.last().unwrap().content,
            json!("那第三行什么意思")
        );
    }

    #[test]
    fn extreme_budget_keeps_last_intact_message() {
        let msgs = vec![
            text("user", &"a".repeat(10_000)),
            text("assistant", &"b".repeat(10_000)),
        ];
        let out = trim_history(msgs, Some(1));
        assert_eq!(out.messages.len(), 1);
        assert_eq!(out.dropped, 1);
        assert_eq!(out.messages[0].role, "assistant");
    }

    /// 🔴 极端预算下也不能留下残缺的 tool 配对（sigil 上一版的真实 bug）。
    #[test]
    fn extreme_budget_still_respects_tool_pairing() {
        let msgs = vec![text("user", &"a".repeat(10_000)), tool_use_msg("t1")];
        let out = trim_history(msgs, Some(1));
        assert!(!out.messages.iter().any(has_tool_use));
        assert!(!out.messages.iter().any(has_tool_result));
    }

    #[test]
    fn budget_subtracts_output_and_overhead() {
        let l = TokenLimits::from_endpoint(Some(128_000), Some(8192));
        assert_eq!(
            history_budget(Some(&l), 4096),
            Some(128_000 - 4096 - OVERHEAD_TOKENS)
        );
        let tiny = TokenLimits::from_preset(Some(1000), None);
        assert_eq!(history_budget(Some(&tiny), 4096), Some(0), "不下溢");
        assert_eq!(history_budget(None, 4096), None);
        let no_window = TokenLimits::from_preset(None, Some(8192));
        assert_eq!(history_budget(Some(&no_window), 4096), None);
    }

    /// 被动重试：每次按当前估算减半，预算单调下降；裁无可裁时 dropped 为 0，调用方据此停止。
    #[test]
    fn retry_budget_halves_until_nothing_left_to_drop() {
        let mut cur = vec![text("user", "任务")];
        for _ in 0..8 {
            cur.push(text("assistant", &"x".repeat(2000)));
            cur.push(text("user", &"y".repeat(2000)));
        }
        let before = estimate_tokens(&cur);
        let mut last = u32::MAX;
        let mut rounds = 0;
        for _ in 0..3 {
            let b = retry_budget(&cur);
            assert!(b < last, "预算必须单调减小");
            last = b;
            let next = trim_history(cur.clone(), Some(b));
            if next.dropped == 0 {
                break;
            }
            assert_eq!(
                next.messages[0].content,
                json!("任务"),
                "任务描述必须一直在"
            );
            cur = next.messages;
            rounds += 1;
        }
        assert!(rounds >= 1);
        assert!(estimate_tokens(&cur) <= before / 2, "一轮后至少减半");
    }

    /// 真实报错形状：这些**必须**判为超长。
    #[test]
    fn detects_real_overflow_errors() {
        let cases: &[(u16, &str)] = &[
            // OpenAI / DeepSeek / vLLM
            (
                400,
                r#"{"error":{"message":"This model's maximum context length is 131072 tokens. However, you requested 140000 tokens (135904 in the messages, 4096 in the completion).","type":"invalid_request_error","code":"context_length_exceeded"}}"#,
            ),
            // Anthropic
            (
                400,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 215000 tokens > 200000 maximum"}}"#,
            ),
            // Gemini（OpenAI 兼容层）
            (
                400,
                r#"[{"error":{"code":400,"message":"The input token count (1200000) exceeds the maximum number of tokens allowed (1048576).","status":"INVALID_ARGUMENT"}}]"#,
            ),
            // OpenRouter
            (
                400,
                r#"{"error":{"message":"This endpoint's maximum context length is 200000 tokens. However, you requested about 230000 tokens","code":400}}"#,
            ),
            // Kimi
            (
                400,
                r#"{"error":{"message":"Invalid request: Your request exceeded model token limit: 131072","type":"invalid_request_error"}}"#,
            ),
            // 通义（DashScope 兼容模式）
            (
                400,
                r#"{"error":{"message":"Range of input length should be [1, 129024]","type":"invalid_request_error","code":"invalid_parameter_error"}}"#,
            ),
            // 智谱
            (400, r#"{"error":{"code":"1261","message":"Prompt 超长"}}"#),
            // 请求体过大
            (413, "Request Entity Too Large"),
        ];
        for (status, body) in cases {
            assert!(is_context_overflow(*status, body), "漏判：{status} {body}");
        }
    }

    /// 这些**不能**判为超长 —— 误判会把用户历史无谓地裁掉一半。
    #[test]
    fn does_not_misfire() {
        let cases: &[(u16, &str)] = &[
            // 限流：提到 tokens 但与窗口无关
            (
                429,
                r#"{"error":{"message":"Rate limit reached for gpt-x on tokens per min (TPM): Limit 30000, Used 29000","type":"tokens"}}"#,
            ),
            // 输出上限太大：裁历史解决不了
            (
                400,
                r#"{"error":{"message":"max_tokens is too large: 50000. This model supports at most 8192 completion tokens.","type":"invalid_request_error"}}"#,
            ),
            (
                400,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 64000 > 32000, which is the maximum allowed number of output tokens"}}"#,
            ),
            // 鉴权、模型不存在
            (401, r#"{"error":{"message":"Incorrect API key provided"}}"#),
            (
                400,
                r#"{"error":{"message":"The model `gpt-9` does not exist"}}"#,
            ),
            // 服务端故障
            (500, "maximum context length"),
        ];
        for (status, body) in cases {
            assert!(!is_context_overflow(*status, body), "误判：{status} {body}");
        }
    }
}
