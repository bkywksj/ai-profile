//! 发对话请求之前：按上下文窗口裁历史、给 max_tokens 封顶。纯函数，不联网。
//!
//! ```text
//! cargo run -p ai-profile --example trim_history
//! ```

use ai_profile::history::{
    history_budget, is_context_overflow, retry_budget, trim_history, HistoryMessage,
};
use ai_profile::{preset, Protocol, TokenLimits};
use serde_json::{json, Value};

/// 应用自己的消息结构 —— 实现 HistoryMessage 就能交给本库裁剪
#[derive(Clone)]
struct ChatMessage {
    role: String,
    content: Value,
}

impl HistoryMessage for ChatMessage {
    fn role(&self) -> &str {
        &self.role
    }
    fn content(&self) -> &Value {
        &self.content
    }
}

fn main() {
    let model = "deepseek-flash";
    let requested_max_tokens = 4096;

    // 1. 限额按「用户手填 > 端点上报 > 预置」逐字段取值
    let user = TokenLimits::from_user(Some(64_000), None); // 用户只知道窗口
    let preset_l = preset::model_limits(
        Protocol::OpenAiCompatible,
        Some("https://api.deepseek.com/v1"),
        model,
    );
    let limits = [Some(user), preset_l]
        .into_iter()
        .flatten()
        .reduce(|hi, lo| hi.or(lo))
        .expect("至少有用户那一层");
    println!(
        "生效限额：窗口 {:?}（来源 {:?}） / 输出 {:?}",
        limits.context_window, limits.source, limits.max_output
    );

    // 2. max_tokens 只收窄、不放大
    let max_tokens = limits
        .max_output
        .map_or(requested_max_tokens, |m| requested_max_tokens.min(m));

    // 3. 按窗口裁历史（扣掉本次要用的 max_tokens 与系统提示余量）
    let history: Vec<ChatMessage> = (0..200)
        .map(|i| ChatMessage {
            role: if i % 2 == 0 { "user" } else { "assistant" }.into(),
            content: json!("很长的一段对话内容 ".repeat(200)),
        })
        .collect();
    let budget = history_budget(Some(&limits), max_tokens);
    let trimmed = trim_history(history, budget);
    println!(
        "发送 {} 条，裁掉 {} 条",
        trimmed.messages.len(),
        trimmed.dropped
    );
    if trimmed.dropped > 0 {
        // 🔴 要告诉用户：否则「模型不记得前面的话」看起来就是它变笨了
        println!("界面提示：较早的 {} 条消息未发送给模型", trimmed.dropped);
    }

    // 4. 服务端仍报超长（窗口未知或估算偏小）：按估算的一半再裁一次重试
    let status = 400;
    let body = r#"{"error":{"message":"This model's maximum context length is 65536 tokens"}}"#;
    if is_context_overflow(status, body) {
        // retry_budget 按「刚才超长的那批消息」的估算取一半
        let budget = retry_budget(&trimmed.messages);
        let retry = trim_history(trimmed.messages, Some(budget));
        println!("超长重试：再裁到 {} 条", retry.messages.len());
    }
}
