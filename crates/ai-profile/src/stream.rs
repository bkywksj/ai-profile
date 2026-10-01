//! 流式解码 —— 把 OpenAI 兼容 / Anthropic 的 SSE 字节流解成统一事件。
//!
//! # 为什么在 crate 里
//!
//! 每个做对话的下游都要写一套 SSE 解析，而各家网关的怪癖（`\r\n`、keep-alive 注释行、
//! `tool_calls` 缺 `index`、只以 `[DONE]` 收尾、流断在半路、流内 `{"error":…}`……）随服务商变化，
//! 复制 N 份必然漏改。按「第二个使用方出现、事件结构能对齐」的判据收进来，与 [`history`](crate::history) 同理。
//!
//! # 🔴 sans-IO：只吃字节、吐事件
//!
//! 本模块**不发 HTTP、不持有连接、不绑任何异步运行时、没有新依赖**（默认的 `chat` feature 就有，能编到移动端）。
//! 这是它没有变成又一个 LLM SDK 的原因。留在应用的部分：
//!
//! | 留在应用 | 说明 |
//! |---|---|
//! | 请求体构造、HTTP 客户端 / 代理 / 超时 | 本模块只看响应字节 |
//! | 取消信号接线 | 取消后调用 [`StreamDecoder::abort`] 拿部分结果 |
//! | 推给前端、执行工具、存历史 | 事件与 [`StreamOutcome::content`] 是交接点 |
//! | 「某地址不带 `stream_options`」的记忆状态 | 本模块只给判定函数 [`is_stream_options_rejected`] |
//! | 「正文空则把思考提升为正文」「剥 `<think>` 标签」 | 产品取舍 |
//!
//! # 用法
//!
//! ```
//! use ai_profile::stream::{StreamDecoder, StreamEnd};
//! use ai_profile::Protocol;
//!
//! let mut dec = StreamDecoder::new(Protocol::OpenAiCompatible).with_stream_id("s1");
//! let mut events = dec.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"\xe4\xbd\xa0\"}}]}\n\n");
//! events.extend(dec.push(b"data: [DONE]\n\n"));
//! let (tail, outcome) = dec.finish();
//! events.extend(tail);
//!
//! assert!(matches!(outcome.end, StreamEnd::Complete));
//! assert_eq!(outcome.text(), "你");
//! ```
//!
//! # 事件与块号
//!
//! OpenAI 兼容：文字固定块 0，第 k 个工具调用是块 k+1。Anthropic：直接用协议自带的 `index`。
//! 思考（`reasoning_content` / `thinking_delta`）只发 [`StreamEvent::ReasoningDelta`]，事件不带块号，
//! 默认也不进 [`StreamOutcome::content`]。
//!
//! 🔴 **默认不保留 thinking 块及其 `signature`**；需要回传的应用用
//! [`StreamDecoder::with_thinking_blocks`] 显式开启，见下一节。
//!
//! # 思考块（opt-in，仅 Anthropic）
//!
//! 开启扩展思考并配合工具调用时，Anthropic 要求把上一轮 assistant 消息里的 thinking 块**连同 `signature` 原样回传**，
//! 否则下一轮请求被服务端拒绝。`with_thinking_blocks(true)` 之后：
//!
//! - [`StreamOutcome::content`] 里按流中出现的顺序带上 `{"type":"thinking","thinking":"<累积文字>","signature":"<签名>"}`
//!   与 `{"type":"redacted_thinking","data":"…"}`；签名来自 `signature_delta`，可能分多片，按到达顺序拼接；
//! - **块号 / 位置**：thinking 块沿用 Anthropic 的 `index`，与文字、工具调用同一套编号；`content` 按 `index` 升序，
//!   所以位置就是流里的位置。`content` 是稠密数组（空文字块照旧省略，不留空位），因此 `content[i]` 不等于 `index == i`，
//!   但回传给服务端不需要 `index`；
//! - **事件序列不变**：[`StreamEvent::ReasoningDelta`] 照发、仍不带块号，`TextDelta` 等的 `block_index` 与关闭时一样。
//!   开启与否只有 `content` 不同；
//! - 🔴 **只有 `Complete` 才带思考块**：断流 / 取消 / 流内错误时与工具调用同一条判据，`content` 只留文字，
//!   thinking 块签名不全、发回去服务端必拒；
//! - 没有签名的 thinking 块（个别 Anthropic 兼容网关不转 `signature_delta`）也不进 `content`：同样发不回官方端点。
//!   其思考文字仍在 [`StreamOutcome::reasoning`] 里；`redacted_thinking` 本身就是不透明负载，不要求签名；
//! - OpenAI 兼容协议：no-op（`reasoning_content` 没有签名，也没有回传要求）。
//!
//! # 用量里的 cache
//!
//! Anthropic 的 `cache_creation_input_tokens` / `cache_read_input_tokens`（`message_start` 与 `message_delta` 的 `usage`，
//! 累计值、覆盖不累加、0 不覆盖非零）默认就读进 [`StreamUsage`]，不影响 `content`。**只进 [`StreamOutcome::usage`]，
//! 不进 [`StreamEvent::Usage`]**，也不会因为只有 cache 变化而多发 `Usage` 事件。
//! OpenAI 兼容的对应字段（`prompt_tokens_details.cached_tokens`、DeepSeek 的 `prompt_cache_hit_tokens` 等）**尚未覆盖**。
//!
//! [`StreamEvent::ToolUseStart`] **延迟到工具名已知**（且 id 已到或参数已开始）才发，前端一收到就能显示
//! 「正在调用 xxx」；始终没等到的，在 [`StreamDecoder::finish`] 时补发。工具没给 id 的，补
//! `call_<流id>_<块号>`，且发出之后不再改变。
//!
//! # 收尾语义
//!
//! | 情形 | [`StreamEnd`] | 内容 |
//! |---|---|---|
//! | 有 `finish_reason` / `stop_reason` / `message_stop` | `Complete` | 全部块 |
//! | 只有 `[DONE]`、没有 `finish_reason` | `Complete` | 有工具调用推 `tool_use`，否则 `end_turn` |
//! | 既无结束原因也无 `[DONE]`（流断了） | `Truncated` | 🔴 只留文字，丢全部工具调用 |
//! | [`StreamDecoder::abort`] | `Cancelled` | 同上 |
//! | 流内 `{"error":…}` / Anthropic `error` 事件 | `Failed` | 只留文字，之后的输入忽略 |
//! | 整段响应里没有一行 SSE | `NotEventStream` | 带原始响应体（最多 1 MiB） |
//!
//! `Truncated` / `Cancelled` / `Failed` 时，此前已发出的 `ToolUseStart` 事件对应的工具调用不完整，
//! 应用按 [`StreamOutcome::end`] 收回即可，不要执行。
//!
//! 以 [`StreamOutcome::content`] 里有没有 `tool_use` 块为准判断要不要跑工具：个别网关带着
//! 工具调用却给 `finish_reason: "stop"`，[`StreamOutcome::stop_reason`] 照实透传，不替它改写。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};

use crate::kind::Protocol;

/// 非 SSE 响应体最多保留多少字节（供 [`StreamEnd::NotEventStream`]）。
const NON_EVENT_BODY_LIMIT: usize = 1 << 20;

/// 没有 [`StreamDecoder::with_stream_id`] 时，给补 id 用的流序号。
static NEXT_STREAM_SEQ: AtomicU64 = AtomicU64::new(1);

/// 归一化的结束原因，取 Anthropic 词表。
///
/// 线格式是一个字符串（[`as_str`](Self::as_str)），未知取值保留在 [`Other`](Self::Other)，不丢信息。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// 正常说完
    EndTurn,
    /// 要调用工具
    ToolUse,
    /// 撞到输出上限
    MaxTokens,
    /// 命中停止序列
    StopSequence,
    /// 模型拒绝
    Refusal,
    /// 服务端暂停（长任务续跑）
    PauseTurn,
    /// 其它取值，原样保留
    Other(String),
}

impl StopReason {
    /// 线格式拼写。
    pub fn as_str(&self) -> &str {
        match self {
            StopReason::EndTurn => "end_turn",
            StopReason::ToolUse => "tool_use",
            StopReason::MaxTokens => "max_tokens",
            StopReason::StopSequence => "stop_sequence",
            StopReason::Refusal => "refusal",
            StopReason::PauseTurn => "pause_turn",
            StopReason::Other(s) => s,
        }
    }

    /// 按线格式拼写解析；不认识的进 [`Other`](Self::Other)。
    pub fn parse(s: &str) -> Self {
        match s {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            "stop_sequence" => StopReason::StopSequence,
            "refusal" => StopReason::Refusal,
            "pause_turn" => StopReason::PauseTurn,
            other => StopReason::Other(other.to_string()),
        }
    }

    /// OpenAI 兼容的 `finish_reason` → 归一化。
    ///
    /// `stop → end_turn`、`tool_calls`（含旧的 `function_call`）`→ tool_use`、`length → max_tokens`，
    /// 其余透传（如 `content_filter`）。
    pub fn from_openai(finish_reason: &str) -> Self {
        match finish_reason {
            "stop" => StopReason::EndTurn,
            "tool_calls" | "function_call" => StopReason::ToolUse,
            "length" => StopReason::MaxTokens,
            other => StopReason::parse(other),
        }
    }
}

impl Serialize for StopReason {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StopReason {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(StopReason::parse(&s))
    }
}

/// 用量。取流里最后一次给出的值（覆盖，不累加）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct StreamUsage {
    /// 输入 token
    pub input_tokens: u32,
    /// 输出 token
    pub output_tokens: u32,
    /// 写入提示缓存的输入 token（Anthropic `cache_creation_input_tokens`）。
    ///
    /// 只读 Anthropic 协议；OpenAI 兼容的 `prompt_tokens_details.cached_tokens` 等**尚未覆盖**，恒为 0。
    /// 为 0 时序列化省略（缺省 = 0），没有缓存的流与 0.1.4 的 JSON 完全一致。
    #[serde(skip_serializing_if = "is_zero")]
    pub cache_creation_input_tokens: u32,
    /// 命中提示缓存而读出的输入 token（Anthropic `cache_read_input_tokens`）。口径同上。
    #[serde(skip_serializing_if = "is_zero")]
    pub cache_read_input_tokens: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 解出的统一事件。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    /// 正文增量
    TextDelta {
        /// 块号
        block_index: usize,
        /// 文字片段
        text: String,
    },
    /// 思考增量（`reasoning_content` / `reasoning` / `thinking_delta`）。不占块号。
    ReasoningDelta {
        /// 思考片段
        text: String,
    },
    /// 一个工具调用开始
    ToolUseStart {
        /// 块号
        block_index: usize,
        /// 调用 id（网关没给时是补的 `call_<流id>_<块号>`）
        id: String,
        /// 工具名
        name: String,
    },
    /// 工具参数的 JSON 片段（拼起来才是完整参数）
    ToolUseDelta {
        /// 块号
        block_index: usize,
        /// 参数 JSON 片段
        partial_json: String,
    },
    /// 用量有变化（累计值）
    Usage {
        /// 输入 token
        input_tokens: u32,
        /// 输出 token
        output_tokens: u32,
    },
    /// 流正常结束时的最后一个事件（`Complete`）；断流 / 取消 / 出错不发
    Finish {
        /// 归一化的结束原因
        reason: StopReason,
    },
    /// 流内错误。终态：之后的输入被忽略
    Error {
        /// 错误信息
        message: String,
    },
}

/// 流是怎么结束的。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEnd {
    /// 有结束标记，内容完整
    Complete,
    /// 流断在半路：没有结束原因，也没有 `[DONE]` / `message_stop`
    Truncated,
    /// 应用调用了 [`StreamDecoder::abort`]
    Cancelled,
    /// 流内报错
    Failed {
        /// 错误信息
        message: String,
    },
    /// 2xx 却整段没有一行 SSE：多半是回了网页，或网关忽略了 `stream: true` 直接回整段 JSON
    NotEventStream {
        /// 原始响应体（最多 1 MiB，行尾统一成 `\n`）
        body: String,
    },
}

/// 收尾结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct StreamOutcome {
    /// 流里上报的模型名
    pub model: Option<String>,
    /// 归一化结束原因。只有 [`StreamEnd::Complete`] 才有
    pub stop_reason: Option<StopReason>,
    /// 用量
    pub usage: StreamUsage,
    /// 结束方式
    pub end: StreamEnd,
    /// Anthropic 风格 content block 数组：`{"type":"text","text":…}` / `{"type":"tool_use","id","name","input"}`。
    /// 与 [`history::HistoryMessage`](crate::history::HistoryMessage) 同一套形状。
    /// 非 `Complete` 时只有文字块。工具参数解析失败按 `{}`。
    ///
    /// 默认不含思考块；[`StreamDecoder::with_thinking_blocks`] 开启后（仅 Anthropic）还会按流里的顺序带上
    /// `{"type":"thinking","thinking","signature"}` 与 `{"type":"redacted_thinking","data"}`，规则见模块文档。
    pub content: Value,
    /// 累积的思考文字（不进 `content`，与是否开启 `with_thinking_blocks` 无关）
    pub reasoning: String,
    /// 解析失败被跳过的 `data:` 帧数，排查网关问题用
    pub skipped_frames: usize,
}

impl StreamOutcome {
    /// 所有文字块拼起来的正文。
    pub fn text(&self) -> String {
        self.content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect()
    }
}

#[derive(Debug, Default)]
struct ToolBlock {
    id: String,
    name: String,
    input: String,
    started: bool,
}

#[derive(Debug)]
enum Block {
    Text(String),
    Tool(ToolBlock),
    /// 只在 `with_thinking_blocks(true)` 且协议为 Anthropic 时才会创建
    Thinking {
        thinking: String,
        signature: String,
    },
    /// 同上；`data` 是服务端给的不透明负载，原样保存
    Redacted(String),
}

/// 一帧 `data:` 的内容。
enum Frame {
    Done,
    Json(Value),
    Junk,
}

fn classify(data: &str) -> Frame {
    let t = data.trim();
    if t == "[DONE]" {
        return Frame::Done;
    }
    match serde_json::from_str::<Value>(t) {
        Ok(v) => Frame::Json(v),
        Err(_) => Frame::Junk,
    }
}

fn clamp_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 流式解码器：一次流一个实例。
///
/// 只持有自己的数据，`Send + 'static`。用法见[模块文档](self)。
#[derive(Debug)]
pub struct StreamDecoder {
    protocol: Protocol,
    stream_id: String,
    buf: Vec<u8>,
    first_line: bool,
    event_name: Option<String>,
    data: Vec<String>,
    saw_sse: bool,
    other: String,
    blocks: BTreeMap<usize, Block>,
    model: Option<String>,
    usage: StreamUsage,
    finish: Option<StopReason>,
    reasoning: String,
    keep_thinking: bool,
    done: bool,
    failed: Option<String>,
    skipped: usize,
}

impl StreamDecoder {
    /// 新建解码器。`protocol` 决定按哪种事件格式解。
    pub fn new(protocol: Protocol) -> Self {
        Self {
            protocol,
            stream_id: NEXT_STREAM_SEQ.fetch_add(1, Ordering::Relaxed).to_string(),
            buf: Vec::new(),
            first_line: true,
            event_name: None,
            data: Vec::new(),
            saw_sse: false,
            other: String::new(),
            blocks: BTreeMap::new(),
            model: None,
            usage: StreamUsage::default(),
            finish: None,
            reasoning: String::new(),
            keep_thinking: false,
            done: false,
            failed: None,
            skipped: 0,
        }
    }

    /// 指定流 id，只用于给没有 id 的工具调用补 `call_<流id>_<块号>`。
    ///
    /// 不设置时用进程内递增序号。想让补出来的 id 在应用重启后仍不重复，传应用自己的流 id。
    pub fn with_stream_id(mut self, id: impl Into<String>) -> Self {
        self.stream_id = id.into();
        self
    }

    /// 是否把 Anthropic 的 thinking / redacted_thinking 块（含 `signature`）保留进 [`StreamOutcome::content`]。
    ///
    /// **默认 `false`**，此时输出与 0.1.4 完全一致（思考只走 [`StreamEvent::ReasoningDelta`]）。
    /// 开启扩展思考并配合工具调用时，Anthropic 要求把上一轮的 thinking 块连同 `signature` 原样回传，
    /// 需要这样做的应用开启它。事件序列不受影响，只有 `content` 不同。规则见模块文档「思考块」一节。
    ///
    /// 对 OpenAI 兼容协议是 no-op：`reasoning_content` 没有签名，也没有回传要求。
    pub fn with_thinking_blocks(mut self, keep: bool) -> Self {
        self.keep_thinking = keep;
        self
    }

    /// 喂入一段响应字节（任意切分都行，包括把多字节字符或 `\r\n` 切在两包之间），返回这一段解出的事件。
    pub fn push(&mut self, bytes: &[u8]) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        self.buf.extend_from_slice(bytes);
        self.drain_lines(false, &mut out);
        out
    }

    /// 流读到 EOF 后收尾：返回最后的事件（补发的 `ToolUseStart`、`Finish`）和结果。
    pub fn finish(mut self) -> (Vec<StreamEvent>, StreamOutcome) {
        let mut out = Vec::new();
        self.flush(&mut out);

        let end = if let Some(message) = self.failed.clone() {
            StreamEnd::Failed { message }
        } else if !self.saw_sse && !self.other.trim().is_empty() && self.blocks.is_empty() {
            StreamEnd::NotEventStream {
                body: self.other.trim().to_string(),
            }
        } else if self.done || self.finish.is_some() {
            StreamEnd::Complete
        } else {
            StreamEnd::Truncated
        };

        let mut stop_reason = None;
        if matches!(end, StreamEnd::Complete) {
            self.start_pending_tools(&mut out);
            let reason = self.finish.clone().unwrap_or_else(|| {
                if self.has_tool() {
                    StopReason::ToolUse
                } else {
                    StopReason::EndTurn
                }
            });
            out.push(StreamEvent::Finish {
                reason: reason.clone(),
            });
            stop_reason = Some(reason);
        }
        let outcome = self.outcome(end, stop_reason);
        (out, outcome)
    }

    /// 应用侧取消后调用：丢掉没读完的半行，返回已收到的文字（工具调用一律丢）。
    pub fn abort(self) -> StreamOutcome {
        self.outcome(StreamEnd::Cancelled, None)
    }

    fn outcome(&self, end: StreamEnd, stop_reason: Option<StopReason>) -> StreamOutcome {
        let complete = matches!(end, StreamEnd::Complete);
        StreamOutcome {
            model: self.model.clone(),
            stop_reason,
            usage: self.usage,
            end,
            content: self.build_content(complete),
            reasoning: self.reasoning.clone(),
            skipped_frames: self.skipped,
        }
    }

    /// `complete` 为假（断流 / 取消 / 出错）时只留文字：工具调用参数不全、thinking 块签名不全，
    /// 发回去服务端必拒，所以与工具调用同一条判据一并丢掉。
    fn build_content(&self, complete: bool) -> Value {
        let mut blocks = Vec::new();
        for block in self.blocks.values() {
            match block {
                Block::Text(s) if !s.is_empty() => {
                    blocks.push(json!({ "type": "text", "text": s }));
                }
                // 没有签名的 thinking 块 Anthropic 必拒（个别兼容网关不转 signature_delta），不回传
                Block::Thinking {
                    thinking,
                    signature,
                } if complete && !signature.is_empty() => {
                    blocks.push(json!({
                        "type": "thinking", "thinking": thinking, "signature": signature
                    }));
                }
                Block::Redacted(data) if complete => {
                    blocks.push(json!({ "type": "redacted_thinking", "data": data }));
                }
                Block::Tool(t) if complete => {
                    let input = if t.input.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&t.input).unwrap_or_else(|_| json!({}))
                    };
                    blocks.push(json!({
                        "type": "tool_use", "id": t.id, "name": t.name, "input": input
                    }));
                }
                _ => {}
            }
        }
        Value::Array(blocks)
    }

    fn has_tool(&self) -> bool {
        self.blocks.values().any(|b| matches!(b, Block::Tool(_)))
    }

    // ── 行 → SSE 帧 ─────────────────────────────────────────────

    /// 按 `\n` / `\r\n` / 单独的 `\r` 切行。`\r` 落在缓冲末尾时要等下一包（可能是 `\r\n` 的前半），EOF 才当行尾。
    fn drain_lines(&mut self, eof: bool, out: &mut Vec<StreamEvent>) {
        let mut start = 0;
        loop {
            let (line, consumed) = {
                let rest = &self.buf[start..];
                let Some(i) = rest.iter().position(|b| *b == b'\n' || *b == b'\r') else {
                    break;
                };
                let mut consumed = i + 1;
                if rest[i] == b'\r' {
                    if i + 1 < rest.len() {
                        if rest[i + 1] == b'\n' {
                            consumed += 1;
                        }
                    } else if !eof {
                        break;
                    }
                }
                (rest[..i].to_vec(), consumed)
            };
            start += consumed;
            self.on_line(&line, out);
        }
        self.buf.drain(..start);
    }

    fn flush(&mut self, out: &mut Vec<StreamEvent>) {
        self.drain_lines(true, out);
        if !self.buf.is_empty() {
            let line = std::mem::take(&mut self.buf);
            self.on_line(&line, out);
        }
        self.dispatch(out);
    }

    fn on_line(&mut self, raw: &[u8], out: &mut Vec<StreamEvent>) {
        // 只在完整的一行上解码：行以 ASCII 换行结尾，多字节字符不会被切开
        let text = String::from_utf8_lossy(raw);
        let mut line: &str = &text;
        if self.first_line {
            self.first_line = false;
            line = line.trim_start_matches('\u{feff}');
        }
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "data" => {
                self.mark_sse();
                self.data.push(value.to_string());
            }
            "event" => {
                self.mark_sse();
                self.event_name = Some(value.trim().to_string());
            }
            "id" | "retry" => {}
            _ => {
                if !self.saw_sse && self.other.len() < NON_EVENT_BODY_LIMIT {
                    self.other.push_str(line);
                    self.other.push('\n');
                }
            }
        }
    }

    fn mark_sse(&mut self) {
        if !self.saw_sse {
            self.saw_sse = true;
            self.other = String::new();
        }
    }

    /// 遇到空行（或 EOF）时把攒的 `data:` 交出去。
    ///
    /// 多行 `data:` 按规范用 `\n` 拼成一帧；拼起来不是合法 JSON、但每行单独是，
    /// 说明网关把几个事件之间漏了空行，逐行各当一帧。
    fn dispatch(&mut self, out: &mut Vec<StreamEvent>) {
        let name = self.event_name.take();
        if self.data.is_empty() {
            return;
        }
        let lines = std::mem::take(&mut self.data);
        let joined = lines.join("\n");
        if joined.trim().is_empty() {
            return;
        }
        match classify(&joined) {
            Frame::Junk if lines.len() > 1 => {
                for line in &lines {
                    if !line.trim().is_empty() {
                        self.apply(name.as_deref(), classify(line), out);
                    }
                }
            }
            frame => self.apply(name.as_deref(), frame, out),
        }
    }

    // ── 帧 → 事件 ───────────────────────────────────────────────

    fn terminal(&self) -> bool {
        self.done || self.failed.is_some()
    }

    fn fail(&mut self, message: String, out: &mut Vec<StreamEvent>) {
        self.failed = Some(message.clone());
        out.push(StreamEvent::Error { message });
    }

    fn apply(&mut self, event: Option<&str>, frame: Frame, out: &mut Vec<StreamEvent>) {
        if self.terminal() {
            return;
        }
        let v = match frame {
            Frame::Done => {
                self.done = true;
                return;
            }
            Frame::Junk => {
                self.skipped += 1;
                return;
            }
            Frame::Json(v) => v,
        };
        if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
            self.fail(error_message(err), out);
            return;
        }
        if event == Some("error") {
            let msg = v
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string());
            self.fail(msg, out);
            return;
        }
        match self.protocol {
            Protocol::Anthropic => {
                // 有的网关不发 `event:` 行，data 里的 type 才可靠
                let kind = v
                    .get("type")
                    .and_then(Value::as_str)
                    .or(event)
                    .unwrap_or("")
                    .to_string();
                self.apply_anthropic(&kind, &v, out);
            }
            _ => self.apply_openai(&v, out),
        }
    }

    fn set_model(&mut self, v: Option<&Value>) {
        if let Some(m) = v.and_then(Value::as_str).filter(|m| !m.is_empty()) {
            self.model = Some(m.to_string());
        }
    }

    /// 用量取最后一次出现的值（覆盖）；为 0 的不覆盖已有的非零值 —— 有的网关末帧把 input 报成 0。
    fn read_usage(&mut self, u: &Value, out: &mut Vec<StreamEvent>) {
        let pick = |keys: &[&str]| {
            keys.iter()
                .find_map(|k| u.get(*k).and_then(Value::as_u64).filter(|n| *n > 0))
                .map(clamp_u32)
        };
        let mut next = self.usage;
        if let Some(n) = pick(&["prompt_tokens", "input_tokens"]) {
            next.input_tokens = n;
        }
        if let Some(n) = pick(&["completion_tokens", "output_tokens"]) {
            next.output_tokens = n;
        }
        // cache 只读 Anthropic；它不进 `Usage` 事件（给带命名字段的变体加字段会破坏下游的模式匹配），
        // 所以也不触发事件：是否发事件只看 input / output 有没有变，与 0.1.4 一致
        if self.protocol == Protocol::Anthropic {
            if let Some(n) = pick(&["cache_creation_input_tokens"]) {
                next.cache_creation_input_tokens = n;
            }
            if let Some(n) = pick(&["cache_read_input_tokens"]) {
                next.cache_read_input_tokens = n;
            }
        }

        let event_changed = next.input_tokens != self.usage.input_tokens
            || next.output_tokens != self.usage.output_tokens;
        self.usage = next;
        if event_changed {
            out.push(StreamEvent::Usage {
                input_tokens: next.input_tokens,
                output_tokens: next.output_tokens,
            });
        }
    }

    fn text_delta(&mut self, key: usize, text: &str, out: &mut Vec<StreamEvent>) {
        if text.is_empty() {
            return;
        }
        match self
            .blocks
            .entry(key)
            .or_insert_with(|| Block::Text(String::new()))
        {
            Block::Text(s) => s.push_str(text),
            _ => return,
        }
        out.push(StreamEvent::TextDelta {
            block_index: key,
            text: text.to_string(),
        });
    }

    fn reasoning_delta(&mut self, text: &str, out: &mut Vec<StreamEvent>) {
        if text.is_empty() {
            return;
        }
        self.reasoning.push_str(text);
        out.push(StreamEvent::ReasoningDelta {
            text: text.to_string(),
        });
    }

    /// 累积 thinking 块（仅 `with_thinking_blocks(true)`）。块号沿用 Anthropic 的 `index`；
    /// 没有 `content_block_start` 直接来 delta 的网关也现建块。该块号已被别的类型占用则忽略。
    fn thinking_append(&mut self, key: usize, thinking: &str, signature: &str) {
        if !self.keep_thinking {
            return;
        }
        if let Block::Thinking {
            thinking: t,
            signature: s,
        } = self.blocks.entry(key).or_insert_with(|| Block::Thinking {
            thinking: String::new(),
            signature: String::new(),
        }) {
            t.push_str(thinking);
            s.push_str(signature);
        }
    }

    fn synth_id(&self, key: usize) -> String {
        format!("call_{}_{}", self.stream_id, key)
    }

    /// 更新一个工具调用块，并在条件满足时发 `ToolUseStart`。
    ///
    /// 条件：名字已知，且（id 已到 或 参数已开始）。真实协议里 id 一定先于参数，
    /// 所以「参数已开始还没 id」= 这个网关不给 id，此时补一个。
    fn tool_update(
        &mut self,
        key: usize,
        id: &str,
        name: &str,
        args: &str,
        out: &mut Vec<StreamEvent>,
    ) {
        let synth = self.synth_id(key);
        let Block::Tool(t) = self
            .blocks
            .entry(key)
            .or_insert_with(|| Block::Tool(ToolBlock::default()))
        else {
            return;
        };
        if !t.started && t.id.is_empty() && !id.is_empty() {
            t.id = id.to_string();
        }
        if t.name.is_empty() && !name.is_empty() {
            t.name = name.to_string();
        }
        if !args.is_empty() {
            t.input.push_str(args);
            if t.started {
                out.push(StreamEvent::ToolUseDelta {
                    block_index: key,
                    partial_json: args.to_string(),
                });
            }
        }
        if !t.started && !t.name.is_empty() && (!t.id.is_empty() || !t.input.is_empty()) {
            t.started = true;
            if t.id.is_empty() {
                t.id = synth;
            }
            out.push(StreamEvent::ToolUseStart {
                block_index: key,
                id: t.id.clone(),
                name: t.name.clone(),
            });
            if !t.input.is_empty() {
                out.push(StreamEvent::ToolUseDelta {
                    block_index: key,
                    partial_json: t.input.clone(),
                });
            }
        }
    }

    /// 收尾时把还没发 `ToolUseStart` 的补发（名字可能仍为空）。
    fn start_pending_tools(&mut self, out: &mut Vec<StreamEvent>) {
        let synth: Vec<(usize, String)> = self
            .blocks
            .keys()
            .map(|k| (*k, self.synth_id(*k)))
            .collect();
        for (key, synth) in synth {
            let Some(Block::Tool(t)) = self.blocks.get_mut(&key) else {
                continue;
            };
            if t.started {
                continue;
            }
            t.started = true;
            if t.id.is_empty() {
                t.id = synth;
            }
            out.push(StreamEvent::ToolUseStart {
                block_index: key,
                id: t.id.clone(),
                name: t.name.clone(),
            });
            if !t.input.is_empty() {
                out.push(StreamEvent::ToolUseDelta {
                    block_index: key,
                    partial_json: t.input.clone(),
                });
            }
        }
    }

    fn next_tool_key(&self) -> usize {
        self.blocks
            .iter()
            .filter(|(_, b)| matches!(b, Block::Tool(_)))
            .map(|(k, _)| *k)
            .max()
            .map_or(1, |k| k + 1)
    }

    fn find_tool_by_id(&self, id: &str) -> Option<usize> {
        self.blocks.iter().find_map(|(k, b)| match b {
            Block::Tool(t) if t.id == id => Some(*k),
            _ => None,
        })
    }

    // ── OpenAI 兼容 ─────────────────────────────────────────────

    fn apply_openai(&mut self, v: &Value, out: &mut Vec<StreamEvent>) {
        self.set_model(v.get("model"));
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            self.read_usage(u, out);
        }
        // 多 choices 只读第一个；末尾只带 usage 的帧 choices 为空
        let Some(choice) = v
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            return;
        };
        // 有的网关在流里回完整 message 而不是 delta
        let delta = choice
            .get("delta")
            .filter(|d| d.is_object())
            .or_else(|| choice.get("message").filter(|m| m.is_object()));
        if let Some(d) = delta {
            for key in ["reasoning_content", "reasoning"] {
                if let Some(s) = d.get(key).and_then(Value::as_str) {
                    self.reasoning_delta(s, out);
                }
            }
            if let Some(s) = d.get("content").and_then(Value::as_str) {
                self.text_delta(0, s, out);
            }
            if let Some(calls) = d.get("tool_calls").and_then(Value::as_array) {
                self.openai_tool_calls(calls, out);
            }
        }
        // 空串、null 不覆盖已有值
        if let Some(r) = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty())
        {
            self.finish = Some(StopReason::from_openai(r));
        }
    }

    fn openai_tool_calls(&mut self, calls: &[Value], out: &mut Vec<StreamEvent>) {
        for (pos, tc) in calls.iter().enumerate() {
            let id = tc.get("id").and_then(Value::as_str).unwrap_or("");
            let func = tc.get("function");
            let name = func
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = match func.and_then(|f| f.get("arguments")) {
                Some(Value::String(s)) => s.clone(),
                Some(v) if v.is_object() || v.is_array() => v.to_string(),
                _ => String::new(),
            };
            let key = match tc.get("index").and_then(Value::as_u64) {
                Some(i) => {
                    let key = usize::try_from(i).unwrap_or(0).saturating_add(1);
                    // 有的网关并行调用的 index 全是 0，只有 id 不同：id 冲突就另起一块
                    let clash = matches!(
                        self.blocks.get(&key),
                        Some(Block::Tool(t)) if !id.is_empty() && !t.id.is_empty() && t.id != id
                    );
                    if clash {
                        self.find_tool_by_id(id)
                            .unwrap_or_else(|| self.next_tool_key())
                    } else {
                        key
                    }
                }
                // 缺 index：有 id 按 id 认，没有就按本帧内的位置算
                None if !id.is_empty() => self
                    .find_tool_by_id(id)
                    .unwrap_or_else(|| self.next_tool_key()),
                None => pos + 1,
            };
            self.tool_update(key, id, name, &args, out);
        }
    }

    // ── Anthropic ───────────────────────────────────────────────

    fn apply_anthropic(&mut self, kind: &str, v: &Value, out: &mut Vec<StreamEvent>) {
        let index = || {
            v.get("index")
                .and_then(Value::as_u64)
                .map(|i| usize::try_from(i).unwrap_or(0))
        };
        match kind {
            "message_start" => {
                if let Some(m) = v.get("message") {
                    self.set_model(m.get("model"));
                    if let Some(u) = m.get("usage").filter(|u| u.is_object()) {
                        self.read_usage(u, out);
                    }
                }
            }
            "content_block_start" => {
                let Some(key) = index() else { return };
                let Some(b) = v.get("content_block") else {
                    return;
                };
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let s = b.get("text").and_then(Value::as_str).unwrap_or("");
                        self.blocks
                            .entry(key)
                            .or_insert_with(|| Block::Text(String::new()));
                        self.text_delta(key, s, out);
                    }
                    Some("tool_use") => {
                        let id = b.get("id").and_then(Value::as_str).unwrap_or("");
                        let name = b.get("name").and_then(Value::as_str).unwrap_or("");
                        // start 里的 input 通常是 `{}`；网关直接给了完整对象就当作整段参数
                        let args = match b.get("input") {
                            Some(i) if i.as_object().is_some_and(|o| !o.is_empty()) => {
                                i.to_string()
                            }
                            _ => String::new(),
                        };
                        self.tool_update(key, id, name, &args, out);
                    }
                    Some("thinking") => {
                        let s = b.get("thinking").and_then(Value::as_str);
                        if let Some(s) = s {
                            self.reasoning_delta(s, out);
                        }
                        let sig = b.get("signature").and_then(Value::as_str);
                        self.thinking_append(key, s.unwrap_or(""), sig.unwrap_or(""));
                    }
                    Some("redacted_thinking") => {
                        if let Some(data) = b.get("data").and_then(Value::as_str) {
                            if self.keep_thinking {
                                self.blocks
                                    .entry(key)
                                    .or_insert_with(|| Block::Redacted(data.to_string()));
                            }
                        }
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let Some(key) = index() else { return };
                let Some(d) = v.get("delta") else { return };
                match d.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(s) = d.get("text").and_then(Value::as_str) {
                            self.text_delta(key, s, out);
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(s) = d.get("partial_json").and_then(Value::as_str) {
                            self.tool_update(key, "", "", s, out);
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(s) = d.get("thinking").and_then(Value::as_str) {
                            self.reasoning_delta(s, out);
                            self.thinking_append(key, s, "");
                        }
                    }
                    // 签名可能分多片，按到达顺序拼接；没开 with_thinking_blocks 时丢弃
                    Some("signature_delta") => {
                        if let Some(s) = d.get("signature").and_then(Value::as_str) {
                            self.thinking_append(key, "", s);
                        }
                    }
                    // citations_delta 等：不保留
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(r) = v
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(Value::as_str)
                    .filter(|r| !r.is_empty())
                {
                    self.finish = Some(StopReason::parse(r));
                }
                if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
                    self.read_usage(u, out);
                }
            }
            "message_stop" => self.done = true,
            // ping / content_block_stop / 未知事件：忽略
            _ => {}
        }
    }
}

fn error_message(err: &Value) -> String {
    err.get("message")
        .and_then(Value::as_str)
        .or_else(|| err.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| err.to_string())
}

/// 请求带了 `stream_options: {"include_usage": true}` 被中转站拒收吗？
///
/// 状态码 400 / 422，且响应体（不分大小写）提到 `stream_options` 或 `include_usage`。
/// 命中时应用应去掉 `stream_options` 重试一次，并按地址记住这个结果。
pub fn is_stream_options_rejected(status: u16, body: &str) -> bool {
    if !matches!(status, 400 | 422) {
        return false;
    }
    let b = body.to_ascii_lowercase();
    b.contains("stream_options") || b.contains("include_usage")
}

/// 响应体像不像一张网页？
///
/// base_url 指到网站根目录时，网站把任何未知路径回成首页 HTML、状态码 200。
/// 看开头 512 个字符：`<!doctype html`、`<html`，或以 `<` 开头且很快出现 `<html`。
pub fn looks_like_html(body: &str) -> bool {
    let head: String = body
        .trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .chars()
        .take(512)
        .collect::<String>()
        .to_lowercase();
    head.starts_with("<!doctype html")
        || head.starts_with("<html")
        || (head.starts_with('<') && head.contains("<html"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OA: Protocol = Protocol::OpenAiCompatible;
    const AN: Protocol = Protocol::Anthropic;

    fn run(p: Protocol, chunks: &[&[u8]]) -> (Vec<StreamEvent>, StreamOutcome) {
        let mut d = StreamDecoder::new(p).with_stream_id("t");
        let mut ev = Vec::new();
        for c in chunks {
            ev.extend(d.push(c));
        }
        let (tail, out) = d.finish();
        ev.extend(tail);
        (ev, out)
    }

    fn one(p: Protocol, s: &str) -> (Vec<StreamEvent>, StreamOutcome) {
        run(p, &[s.as_bytes()])
    }

    fn starts(ev: &[StreamEvent]) -> Vec<(usize, String, String)> {
        ev.iter()
            .filter_map(|e| match e {
                StreamEvent::ToolUseStart {
                    block_index,
                    id,
                    name,
                } => Some((*block_index, id.clone(), name.clone())),
                _ => None,
            })
            .collect()
    }

    fn texts(ev: &[StreamEvent]) -> Vec<String> {
        ev.iter()
            .filter_map(|e| match e {
                StreamEvent::TextDelta { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn tool_uses(o: &StreamOutcome) -> Vec<Value> {
        o.content
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .cloned()
            .collect()
    }

    // ── OpenAI 兼容 ─────────────────────────────────────────────

    /// 含 `\r\n`、keep-alive 注释行、usage 末帧的完整流。
    const OA_FULL: &str = concat!(
        ": keep-alive\r\n\r\n",
        "data: {\"model\":\"m1\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"你好\"}}]}\r\n\r\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"，世界\"}}]}\r\n\r\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"function\":{\"name\":\"read\",\"arguments\":\"{\\\"p\\\":\"}}]}}]}\r\n\r\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"x\\\"}\"}}]}}]}\r\n\r\n",
        ": keep-alive\r\n\r\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\r\n\r\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7}}\r\n\r\n",
        "data: [DONE]\r\n\r\n",
    );

    #[test]
    fn openai_assembles_text_tool_calls_and_usage() {
        let (ev, o) = one(OA, OA_FULL);
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(o.stop_reason, Some(StopReason::ToolUse));
        assert_eq!(o.model.as_deref(), Some("m1"));
        assert_eq!((o.usage.input_tokens, o.usage.output_tokens), (11, 7));
        assert_eq!(texts(&ev), ["你好", "，世界"]);
        assert_eq!(o.text(), "你好，世界");
        assert_eq!(starts(&ev), [(1, "call_a".into(), "read".into())]);
        let tools = tool_uses(&o);
        assert_eq!(tools[0]["input"], json!({"p": "x"}));
        let deltas: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ToolUseDelta { partial_json, .. } => Some(partial_json.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas.concat(), "{\"p\":\"x\"}");
        assert!(
            matches!(ev.last(), Some(StreamEvent::Finish { reason }) if *reason == StopReason::ToolUse)
        );
    }

    #[test]
    fn openai_done_without_finish_reason_infers_tool_use_and_ignores_tail() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]}}]}\n\n",
                "data: [DONE]\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"[DONE] 之后的内容\"}}]}\n\n",
            ),
        );
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(o.stop_reason, Some(StopReason::ToolUse));
        assert_eq!(tool_uses(&o).len(), 1);
        assert_eq!(o.text(), "");
    }

    #[test]
    fn openai_done_without_tools_infers_end_turn() {
        let (_, o) = one(
            OA,
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n",
        );
        assert_eq!(o.stop_reason, Some(StopReason::EndTurn));
    }

    #[test]
    fn openai_error_chunk_fails_the_round() {
        let (ev, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"半\"}}]}\n\n",
                "data: {\"error\":{\"message\":\"rate limited\",\"type\":\"x\"}}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"忽略\"}}]}\n\n",
            ),
        );
        assert_eq!(
            o.end,
            StreamEnd::Failed {
                message: "rate limited".into()
            }
        );
        assert_eq!(o.text(), "半");
        assert!(ev
            .iter()
            .any(|e| matches!(e, StreamEvent::Error { message } if message == "rate limited")));
        assert!(!ev.iter().any(|e| matches!(e, StreamEvent::Finish { .. })));
    }

    #[test]
    fn openai_error_null_is_not_an_error() {
        let (_, o) = one(
            OA,
            "data: {\"error\":null,\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"stop\"}]}\n\n",
        );
        assert_eq!(o.end, StreamEnd::Complete);
    }

    #[test]
    fn openai_late_id_and_name_start_once_after_name_known() {
        let (ev, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_late\"}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"late_tool\",\"arguments\":\"{}\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            ),
        );
        assert_eq!(starts(&ev), [(1, "call_late".into(), "late_tool".into())]);
        assert_eq!(tool_uses(&o)[0]["id"], "call_late");
    }

    #[test]
    fn openai_tool_start_waits_for_id_but_not_forever() {
        // 名字先到、id 还没来、参数也没开始：先不发
        let mut d = StreamDecoder::new(OA).with_stream_id("t");
        let ev = d.push(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"f\"}}]}}]}\n\n");
        assert!(starts(&ev).is_empty());
        // 参数开始了还没有 id：这个网关不给 id，补一个
        let ev = d.push(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{}\"}}]}}]}\n\n");
        assert_eq!(starts(&ev), [(1, "call_t_1".into(), "f".into())]);
    }

    #[test]
    fn openai_truncated_drops_partial_tool_calls_keeps_text() {
        let (ev, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"先说点\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"a\"}}]}}]}\n\n",
            ),
        );
        assert_eq!(o.end, StreamEnd::Truncated);
        assert_eq!(o.stop_reason, None);
        assert_eq!(o.text(), "先说点");
        assert!(tool_uses(&o).is_empty());
        assert!(!ev.iter().any(|e| matches!(e, StreamEvent::Finish { .. })));
    }

    #[test]
    fn openai_clean_end_without_index_or_id_fills_distinct_ids() {
        let (ev, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[",
                "{\"function\":{\"name\":\"a\",\"arguments\":\"{}\"}},",
                "{\"function\":{\"name\":\"b\",\"arguments\":\"{}\"}}]}}]}\n\n",
                "data: [DONE]\n\n",
            ),
        );
        assert_eq!(o.stop_reason, Some(StopReason::ToolUse));
        let s = starts(&ev);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].1, "call_t_1");
        assert_eq!(s[1].1, "call_t_2");
        assert_ne!(s[0].1, s[1].1);
    }

    #[test]
    fn openai_same_index_different_ids_become_separate_calls() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"b\",\"function\":{\"name\":\"g\",\"arguments\":\"{}\"}}]}}]}\n\n",
                "data: [DONE]\n\n",
            ),
        );
        let t = tool_uses(&o);
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0]["id"].as_str(), t[1]["id"].as_str()),
            (Some("a"), Some("b"))
        );
    }

    #[test]
    fn openai_no_index_but_id_matches_existing_call() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"a\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"k\\\":\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"a\",\"function\":{\"arguments\":\"1}\"}}]}}]}\n\n",
                "data: [DONE]\n\n",
            ),
        );
        let t = tool_uses(&o);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0]["input"], json!({"k": 1}));
    }

    #[test]
    fn openai_arguments_as_object_and_whole_at_once() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"f\",\"arguments\":{\"x\":1}}}]}}]}\n\n",
                "data: [DONE]\n\n",
            ),
        );
        assert_eq!(tool_uses(&o)[0]["input"], json!({"x": 1}));
    }

    #[test]
    fn openai_invalid_tool_arguments_fall_back_to_empty_object() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"x\"}}]}}]}\n\n",
                "data: [DONE]\n\n",
            ),
        );
        assert_eq!(tool_uses(&o)[0]["input"], json!({}));
    }

    #[test]
    fn openai_reasoning_content_is_separate_from_text() {
        let (ev, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"想一想\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"reasoning\":\"再想\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"答案\"},\"finish_reason\":\"stop\"}]}\n\n",
            ),
        );
        assert_eq!(o.reasoning, "想一想再想");
        assert_eq!(o.text(), "答案");
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, StreamEvent::ReasoningDelta { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn openai_empty_finish_reason_and_usage_frame_keep_finish() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"length\"}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"\"}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":null}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}\n\n",
            ),
        );
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(o.stop_reason, Some(StopReason::MaxTokens));
        assert_eq!(o.usage.output_tokens, 4);
    }

    #[test]
    fn usage_is_overwritten_not_summed_and_zero_does_not_wipe() {
        let (ev, o) = one(
            OA,
            concat!(
                "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":1},\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
                "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5},\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n",
                "data: {\"usage\":{\"prompt_tokens\":0,\"completion_tokens\":9},\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            ),
        );
        assert_eq!((o.usage.input_tokens, o.usage.output_tokens), (10, 9));
        let n = ev
            .iter()
            .filter(|e| matches!(e, StreamEvent::Usage { .. }))
            .count();
        assert_eq!(n, 3);
    }

    #[test]
    fn openai_full_message_instead_of_delta_is_accepted() {
        let (_, o) = one(
            OA,
            "data: {\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"整段\"},\"finish_reason\":\"stop\"}]}\n\n",
        );
        assert_eq!(o.text(), "整段");
    }

    #[test]
    fn openai_only_first_choice_is_read() {
        let (_, o) = one(
            OA,
            "data: {\"choices\":[{\"delta\":{\"content\":\"A\"}},{\"delta\":{\"content\":\"B\"}}]}\n\ndata: [DONE]\n\n",
        );
        assert_eq!(o.text(), "A");
    }

    // ── SSE 行层 ────────────────────────────────────────────────

    #[test]
    fn data_without_space_lone_cr_and_bom_are_accepted() {
        let (_, o) = one(
            OA,
            "\u{feff}data:{\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\r\rdata:[DONE]\r\r",
        );
        assert_eq!(o.text(), "a");
        assert_eq!(o.end, StreamEnd::Complete);
    }

    #[test]
    fn multi_line_data_is_joined_with_newline() {
        let (_, o) = one(
            OA,
            "data: {\"choices\":[{\"delta\":\ndata: {\"content\":\"x\"}}]}\n\ndata: [DONE]\n\n",
        );
        assert_eq!(o.text(), "x");
        assert_eq!(o.skipped_frames, 0);
    }

    #[test]
    fn events_missing_blank_line_between_them_are_split_per_line() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n",
                "data: [DONE]\n\n",
            ),
        );
        assert_eq!(o.text(), "ab");
    }

    #[test]
    fn unparsable_frame_is_skipped_not_fatal() {
        let (_, o) = one(
            OA,
            concat!(
                "data: {not json\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
            ),
        );
        assert_eq!(o.text(), "ok");
        assert_eq!(o.skipped_frames, 1);
        assert_eq!(o.end, StreamEnd::Complete);
    }

    #[test]
    fn last_event_without_trailing_blank_line_is_still_dispatched() {
        let (_, o) = one(
            OA,
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\ndata: [DONE]",
        );
        assert_eq!(o.end, StreamEnd::Complete);
    }

    #[test]
    fn html_response_is_not_an_event_stream() {
        let html = "<!DOCTYPE html>\n<html><head><title>x</title></head><body>hi</body></html>";
        let (_, o) = one(OA, html);
        match &o.end {
            StreamEnd::NotEventStream { body } => assert!(looks_like_html(body)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn non_stream_json_body_is_returned_whole() {
        let body = "{\"choices\":[{\"message\":{\"content\":\"整段\"}}]}";
        let (_, o) = one(OA, body);
        assert_eq!(
            o.end,
            StreamEnd::NotEventStream {
                body: body.to_string()
            }
        );
    }

    #[test]
    fn empty_stream_is_truncated() {
        let (_, o) = one(OA, "");
        assert_eq!(o.end, StreamEnd::Truncated);
    }

    #[test]
    fn abort_keeps_text_and_drops_tools() {
        let mut d = StreamDecoder::new(OA).with_stream_id("t");
        d.push("data: {\"choices\":[{\"delta\":{\"content\":\"已收到\"}}]}\n\n".as_bytes());
        d.push(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c\",\"function\":{\"name\":\"f\",\"arguments\":\"{\"}}]}}]}\n\n");
        d.push("data: {\"choices\":[{\"delta\":{\"content\":\"半".as_bytes());
        let o = d.abort();
        assert_eq!(o.end, StreamEnd::Cancelled);
        assert_eq!(o.stop_reason, None);
        assert_eq!(o.text(), "已收到");
        assert!(tool_uses(&o).is_empty());
    }

    #[test]
    fn abort_before_first_byte_is_empty() {
        let o = StreamDecoder::new(OA).abort();
        assert_eq!(o.end, StreamEnd::Cancelled);
        assert_eq!(o.content, json!([]));
    }

    // ── Anthropic ───────────────────────────────────────────────

    const AN_FULL: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-x\",\"usage\":{\"input_tokens\":25,\"output_tokens\":1}}}\n\n",
        "event: ping\ndata: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"你好\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"read\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"p\\\":\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"x\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":42}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    #[test]
    fn anthropic_assembles_text_tool_use_and_usage() {
        let (ev, o) = one(AN, AN_FULL);
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(o.stop_reason, Some(StopReason::ToolUse));
        assert_eq!(o.model.as_deref(), Some("claude-x"));
        assert_eq!((o.usage.input_tokens, o.usage.output_tokens), (25, 42));
        assert_eq!(o.text(), "你好");
        assert_eq!(starts(&ev), [(1, "toolu_1".into(), "read".into())]);
        assert_eq!(tool_uses(&o)[0]["input"], json!({"p": "x"}));
    }

    #[test]
    fn anthropic_crlf_is_handled() {
        let (_, o) = one(AN, &AN_FULL.replace('\n', "\r\n"));
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(o.text(), "你好");
    }

    #[test]
    fn anthropic_dispatches_by_data_type_when_event_line_is_missing() {
        let no_event: String = AN_FULL
            .lines()
            .filter(|l| !l.starts_with("event:"))
            .collect::<Vec<_>>()
            .join("\n");
        let (_, o) = one(AN, &no_event);
        assert_eq!(o.text(), "你好");
        assert_eq!(o.stop_reason, Some(StopReason::ToolUse));
    }

    #[test]
    fn anthropic_thinking_goes_to_reasoning_and_signature_is_dropped() {
        let (ev, o) = one(
            AN,
            concat!(
                "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"嗯\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig\"}}\n\n",
                "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"答\"}}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
                "data: {\"type\":\"message_stop\"}\n\n",
            ),
        );
        assert_eq!(o.reasoning, "嗯");
        assert_eq!(o.text(), "答");
        assert_eq!(o.content.as_array().unwrap().len(), 1);
        assert!(ev
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta { block_index: 1, .. })));
    }

    // ── 思考块（opt-in）与 cache 用量 ───────────────────────────

    /// 思考(0，签名分两片) → 文字(1) → 思考(2) → redacted(3) → 工具(4)。
    const AN_THINK: &str = concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-x\",\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_creation_input_tokens\":100,\"cache_read_input_tokens\":2000}}}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"先\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"想\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"AbC\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"dEf==\"}}\n\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"我来查\"}}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"再想\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"s2\"}}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":3,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"EncRyPt\"}}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":4,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"read\",\"input\":{}}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":4,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"p\\\":1}\"}}\n\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":42,\"cache_read_input_tokens\":2500}}\n\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );

    fn run_thinking(p: Protocol, s: &str) -> (Vec<StreamEvent>, StreamOutcome) {
        let mut d = StreamDecoder::new(p)
            .with_stream_id("t")
            .with_thinking_blocks(true);
        let mut ev = d.push(s.as_bytes());
        let (tail, out) = d.finish();
        ev.extend(tail);
        (ev, out)
    }

    fn types(o: &StreamOutcome) -> Vec<String> {
        o.content
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["type"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn thinking_blocks_are_off_by_default_and_explicit_false_is_the_same() {
        let (ev, o) = one(AN, AN_THINK);
        assert_eq!(types(&o), ["text", "tool_use"]);
        assert_eq!(o.reasoning, "先想再想");
        let mut d = StreamDecoder::new(AN)
            .with_stream_id("t")
            .with_thinking_blocks(false);
        let mut ev2 = d.push(AN_THINK.as_bytes());
        let (tail, o2) = d.finish();
        ev2.extend(tail);
        assert_eq!((ev, o), (ev2, o2));
    }

    #[test]
    fn thinking_blocks_keep_stream_order_signature_and_redacted() {
        let (_, o) = run_thinking(AN, AN_THINK);
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(
            o.content,
            json!([
                {"type":"thinking","thinking":"先想","signature":"AbCdEf=="},
                {"type":"text","text":"我来查"},
                {"type":"thinking","thinking":"再想","signature":"s2"},
                {"type":"redacted_thinking","data":"EncRyPt"},
                {"type":"tool_use","id":"toolu_1","name":"read","input":{"p":1}},
            ])
        );
        // 思考文字照旧累积进 reasoning
        assert_eq!(o.reasoning, "先想再想");
    }

    #[test]
    fn thinking_option_changes_only_content_not_events() {
        let (ev_off, o_off) = one(AN, AN_THINK);
        let (ev_on, o_on) = run_thinking(AN, AN_THINK);
        assert_eq!(ev_off, ev_on);
        assert_ne!(o_off.content, o_on.content);
        let mut o_on = o_on;
        o_on.content = o_off.content.clone();
        assert_eq!(o_off, o_on);
        // 块号沿用协议自带的 index：文字 1、工具 4，不因思考块而移位
        assert!(ev_on
            .iter()
            .any(|e| matches!(e, StreamEvent::TextDelta { block_index: 1, .. })));
        assert_eq!(starts(&ev_on), [(4, "toolu_1".into(), "read".into())]);
    }

    #[test]
    fn thinking_without_start_event_builds_the_block_from_deltas() {
        let (_, o) = run_thinking(
            AN,
            concat!(
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"嗯\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sg\"}}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
            ),
        );
        assert_eq!(
            o.content,
            json!([{"type":"thinking","thinking":"嗯","signature":"sg"}])
        );
    }

    #[test]
    fn thinking_block_without_signature_is_dropped_but_reasoning_stays() {
        let (_, o) = run_thinking(
            AN,
            concat!(
                "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"无签名\"}}\n\n",
                "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"答\"}}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
            ),
        );
        assert_eq!(o.end, StreamEnd::Complete);
        assert_eq!(types(&o), ["text"]);
        assert_eq!(o.reasoning, "无签名");
    }

    /// 🔴 守卫：断流 / 取消 / 流内错误时 thinking 与 redacted_thinking 一并丢弃（签名不全服务端必拒），只留文字
    #[test]
    fn incomplete_streams_drop_thinking_blocks_and_keep_only_text() {
        // 截掉 message_delta / message_stop → 断流
        let cut = AN_THINK
            .split("data: {\"type\":\"message_delta\"")
            .next()
            .unwrap();
        let (_, o) = run_thinking(AN, cut);
        assert_eq!(o.end, StreamEnd::Truncated);
        assert_eq!(o.content, json!([{"type":"text","text":"我来查"}]));

        // 取消：停在第二个 thinking 的签名到达之前
        let upto = AN_THINK
            .split("data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"signature_delta\"")
            .next()
            .unwrap();
        let mut d = StreamDecoder::new(AN).with_thinking_blocks(true);
        d.push(upto.as_bytes());
        let o = d.abort();
        assert_eq!(o.end, StreamEnd::Cancelled);
        assert_eq!(o.content, json!([{"type":"text","text":"我来查"}]));

        // 流内错误
        let failed = format!(
            "{cut}event: error\ndata: {{\"type\":\"error\",\"error\":{{\"message\":\"Overloaded\"}}}}\n\n"
        );
        let (_, o) = run_thinking(AN, &failed);
        assert!(matches!(o.end, StreamEnd::Failed { .. }));
        assert_eq!(o.content, json!([{"type":"text","text":"我来查"}]));
    }

    #[test]
    fn thinking_option_is_a_noop_for_openai_compatible() {
        let s = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"想\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"答\"},\"finish_reason\":\"stop\"}]}\n\n",
        );
        assert_eq!(one(OA, s), run_thinking(OA, s));
    }

    #[test]
    fn thinking_option_keeps_chunking_invariance() {
        let bytes = AN_THINK.as_bytes();
        let chunked = |parts: &[&[u8]]| {
            let mut d = StreamDecoder::new(AN)
                .with_stream_id("t")
                .with_thinking_blocks(true);
            let mut ev = Vec::new();
            for p in parts {
                ev.extend(d.push(p));
            }
            let (tail, o) = d.finish();
            ev.extend(tail);
            (ev, o)
        };
        let whole = chunked(&[bytes]);
        for cut in 0..=bytes.len() {
            assert_eq!(whole, chunked(&[&bytes[..cut], &bytes[cut..]]), "{cut}");
        }
    }

    #[test]
    fn anthropic_cache_usage_is_overwritten_and_zero_does_not_wipe() {
        let (ev, o) = one(AN, AN_THINK);
        assert_eq!(o.usage.input_tokens, 25);
        assert_eq!(o.usage.output_tokens, 42);
        assert_eq!(o.usage.cache_creation_input_tokens, 100);
        // message_delta 的累计值覆盖 message_start 的
        assert_eq!(o.usage.cache_read_input_tokens, 2500);
        // 只有 input / output 变化才发 Usage 事件：message_start 一次、message_delta（output 1→42）一次，
        // message_delta 里单独变化的 cache_read 不额外发
        let usage_events = ev
            .iter()
            .filter(|e| matches!(e, StreamEvent::Usage { .. }))
            .count();
        assert_eq!(usage_events, 2);

        let (_, o) = one(
            AN,
            concat!(
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"cache_read_input_tokens\":300}}}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":9,\"cache_read_input_tokens\":0}}\n\n",
            ),
        );
        assert_eq!(o.usage.cache_read_input_tokens, 300);
    }

    #[test]
    fn cache_only_change_emits_no_usage_event() {
        let (ev, _) = one(
            AN,
            concat!(
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{},\"usage\":{\"cache_read_input_tokens\":77}}\n\n",
            ),
        );
        let n = ev
            .iter()
            .filter(|e| matches!(e, StreamEvent::Usage { .. }))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn openai_does_not_read_cache_fields() {
        let (_, o) = one(
            OA,
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4,\"cache_read_input_tokens\":9,\"prompt_tokens_details\":{\"cached_tokens\":8}}}\n\n",
        );
        assert_eq!((o.usage.input_tokens, o.usage.output_tokens), (3, 4));
        assert_eq!(
            (
                o.usage.cache_creation_input_tokens,
                o.usage.cache_read_input_tokens
            ),
            (0, 0)
        );
    }

    #[test]
    fn zero_cache_fields_are_omitted_from_the_wire_shape() {
        let (_, o) = one(OA, "data: {\"choices\":[{\"delta\":{\"content\":\"a\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}\n\n");
        assert_eq!(
            serde_json::to_value(o.usage).unwrap(),
            json!({"inputTokens":3,"outputTokens":4})
        );
        let (_, o) = one(AN, AN_THINK);
        assert_eq!(
            serde_json::to_value(o.usage).unwrap(),
            json!({"inputTokens":25,"outputTokens":42,
                   "cacheCreationInputTokens":100,"cacheReadInputTokens":2500})
        );
    }

    #[test]
    fn anthropic_error_event_is_terminal_failed() {
        let (ev, o) = one(
            AN,
            concat!(
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"半\"}}\n\n",
                "event: error\n",
                "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"忽略\"}}\n\n",
            ),
        );
        assert_eq!(
            o.end,
            StreamEnd::Failed {
                message: "Overloaded".into()
            }
        );
        assert_eq!(o.text(), "半");
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, StreamEvent::Error { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn anthropic_error_event_without_error_field_uses_message() {
        let (_, o) = one(AN, "event: error\ndata: {\"message\":\"boom\"}\n\n");
        assert_eq!(
            o.end,
            StreamEnd::Failed {
                message: "boom".into()
            }
        );
    }

    #[test]
    fn anthropic_tool_use_without_id_gets_synthetic_id() {
        let (ev, o) = one(
            AN,
            concat!(
                "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"name\":\"f\"}}\n\n",
                "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
            ),
        );
        assert_eq!(starts(&ev), [(1, "call_t_1".into(), "f".into())]);
        assert_eq!(tool_uses(&o)[0]["id"], "call_t_1");
    }

    #[test]
    fn anthropic_truncated_without_message_stop() {
        let (_, o) = one(
            AN,
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"断\"}}\n\n",
        );
        assert_eq!(o.end, StreamEnd::Truncated);
        assert_eq!(o.text(), "断");
    }

    #[test]
    fn anthropic_stop_reasons_pass_through() {
        for (raw, want) in [
            ("end_turn", StopReason::EndTurn),
            ("max_tokens", StopReason::MaxTokens),
            ("stop_sequence", StopReason::StopSequence),
            ("pause_turn", StopReason::PauseTurn),
            ("refusal", StopReason::Refusal),
            ("weird", StopReason::Other("weird".into())),
        ] {
            let (_, o) = one(
                AN,
                &format!("data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"{raw}\"}}}}\n\n"),
            );
            assert_eq!(o.stop_reason, Some(want));
        }
    }

    // ── 守卫 ────────────────────────────────────────────────────

    /// 🔴 同一段流无论怎么切包，事件与结果必须完全一致 —— 网络分包是不可控的。
    #[test]
    fn any_chunking_yields_identical_events_and_outcome() {
        let cjk = "data: {\"choices\":[{\"delta\":{\"content\":\"你好，世界🙂\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n";
        for (p, s) in [(OA, OA_FULL), (AN, AN_FULL), (OA, cjk)] {
            let whole = run(p, &[s.as_bytes()]);
            let bytes = s.as_bytes();
            for cut in 0..=bytes.len() {
                let split = run(p, &[&bytes[..cut], &bytes[cut..]]);
                assert_eq!(whole, split, "在 {cut} 处切开结果不同");
            }
            let each: Vec<&[u8]> = bytes.chunks(1).collect();
            assert_eq!(whole, run(p, &each), "逐字节喂入结果不同");
        }
    }

    #[test]
    fn stream_options_rejection_is_recognised() {
        assert!(is_stream_options_rejected(
            400,
            r#"{"error":{"message":"Unknown parameter: 'stream_options'."}}"#
        ));
        assert!(is_stream_options_rejected(
            422,
            r#"{"detail":"extra fields not permitted: STREAM_OPTIONS"}"#
        ));
        assert!(is_stream_options_rejected(
            400,
            "invalid field include_usage"
        ));
        assert!(!is_stream_options_rejected(
            401,
            r#"{"error":"stream_options"}"#
        ));
        assert!(!is_stream_options_rejected(
            400,
            r#"{"error":{"message":"max_tokens is too large"}}"#
        ));
        assert!(!is_stream_options_rejected(500, "stream_options"));
    }

    #[test]
    fn html_detection() {
        assert!(looks_like_html("<!DOCTYPE html><html></html>"));
        assert!(looks_like_html("  \n<HTML lang=en>"));
        assert!(looks_like_html("\u{feff}<?xml version=\"1.0\"?><html>"));
        assert!(!looks_like_html("{\"error\":\"x\"}"));
        assert!(!looks_like_html("data: <html>"));
        assert!(!looks_like_html(""));
    }

    #[test]
    fn stop_reason_spellings_agree() {
        for r in [
            StopReason::EndTurn,
            StopReason::ToolUse,
            StopReason::MaxTokens,
            StopReason::StopSequence,
            StopReason::Refusal,
            StopReason::PauseTurn,
            StopReason::Other("content_filter".into()),
        ] {
            assert_eq!(StopReason::parse(r.as_str()), r);
            let j = serde_json::to_string(&r).unwrap();
            assert_eq!(j, format!("\"{}\"", r.as_str()));
            assert_eq!(serde_json::from_str::<StopReason>(&j).unwrap(), r);
        }
        assert_eq!(StopReason::from_openai("stop"), StopReason::EndTurn);
        assert_eq!(StopReason::from_openai("tool_calls"), StopReason::ToolUse);
        assert_eq!(
            StopReason::from_openai("function_call"),
            StopReason::ToolUse
        );
        assert_eq!(StopReason::from_openai("length"), StopReason::MaxTokens);
        assert_eq!(
            StopReason::from_openai("content_filter"),
            StopReason::Other("content_filter".into())
        );
    }

    #[test]
    fn wire_shapes_are_camel_case_with_kind_tag() {
        let e = StreamEvent::ToolUseStart {
            block_index: 1,
            id: "a".into(),
            name: "f".into(),
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"kind":"tool_use_start","blockIndex":1,"id":"a","name":"f"})
        );
        let e = StreamEvent::Usage {
            input_tokens: 1,
            output_tokens: 2,
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"kind":"usage","inputTokens":1,"outputTokens":2})
        );
        assert_eq!(
            serde_json::to_value(StreamEnd::Failed {
                message: "m".into()
            })
            .unwrap(),
            json!({"kind":"failed","message":"m"})
        );
    }

    #[test]
    fn decoder_is_send_and_static() {
        fn assert_send<T: Send + 'static>() {}
        assert_send::<StreamDecoder>();
        assert_send::<StreamEvent>();
        assert_send::<StreamOutcome>();
    }
}
