# 任务：stream 保留 thinking 块（含 signature）与 cache 用量（opt-in）

**状态**: 🟢 进行中（代码与发版提交已在本地，等用户确认发布）
**创建时间**: 2026-10-01 11:26:07
**更新时间**: 2026-10-01 12:10:00
**Git 分支**: master
**版本位**: minor（兼容新增），0.x 阶段落在补丁位 → `0.1.5`
**约束**: 只做到「发版提交在本地 + 发版闸门全绿」。不发布、不打 tag、不推送、不跑任何联网 git

---

## 📋 需求描述

reeve 的 AI 助手要接流式，用 Anthropic 扩展思考 + 工具调用。Anthropic 要求带 `tool_use` 的多轮里，上一轮
assistant 消息的 thinking 块**连同 `signature` 原样回传**，否则下一轮被拒。0.1.4 的 `stream` 有意不保留
thinking / signature（`stream.rs` 模块文档、`content_block_start` 的 thinking 分支只发 `ReasoningDelta`、
`signature_delta` 被丢），`StreamUsage` 也没有 cache 读写 token。

🔴 已有 6 个下游把 `outcome.content` 原样存回历史，所以默认输出不能多出 thinking 块 → **opt-in**。

## 🎨 方案设计

### 读代码后确认 / 调整的点

1. **开关**：`StreamDecoder::with_thinking_blocks(bool)`，默认 `false`，风格同 `with_stream_id`（消费 `self` 返回 `Self`）。
   采用 `bool` 参数而不是无参 `enable_*`：规范用例里「选项」是一个布尔字段，下游从配置里读也方便。
2. **块号 / 位置规则**（不改任何事件）：
   - Anthropic 流的 thinking 块有自己的 `index`，与 text / tool_use 同一套编号，解码器内部 `blocks` 本来就是按
     `index` 排序的 `BTreeMap`。开启后 thinking 块直接用协议自带的 `index` 占位，`content` 数组按 `index` 升序，
     所以位置 = 它在流里出现的位置，回传顺序与服务端一致。
   - `content` 数组是**稠密**的（空文字块照旧省略，不留空位），所以 `content[i]` 不等于 `index == i`；
     应用要对账用 `index`，但 `content` 里不带 `index`——回传给服务端也不需要。
   - **事件的 `block_index` 不变**：`ReasoningDelta` 本来就不带块号，`TextDelta` / `ToolUse*` 本来就用协议的 `index`。
     开启与否，事件序列逐字节一致（只有 `outcome.content` 不同）。
   - OpenAI 兼容协议：no-op（`reasoning_content` 没有签名，也没有回传要求）。
3. **thinking 块的内容**：`{"type":"thinking","thinking":"<累积>","signature":"<拼接>"}`。
   - thinking 文字来自 `content_block_start.thinking`（通常为空）+ 全部 `thinking_delta`；
   - signature 来自 `content_block_start.signature`（通常为空）+ 全部 `signature_delta`（可能多片，按到达顺序拼接）；
   - 没有 `content_block_start` 直接来 delta 的（个别网关）：按 `index` 现建块，同样累积。
4. **`redacted_thinking`**：`content_block_start` 的 `content_block.data` 原样保存，输出 `{"type":"redacted_thinking","data":"…"}`；
   没有 `data` 字符串就忽略（没有可回传的东西）。它没有 delta，不产生任何事件。
5. **🔴 收尾处置**（与现有口径一致）：只有 `Complete` 才带 thinking / redacted_thinking；`Truncated` / `Cancelled` /
   `Failed` / `NotEventStream` 一律只留文字，与「丢全部工具调用」同一条判据（`build_content` 里同一个 `complete` 开关）。
   **调整（需说明理由）**：`Complete` 时，**没有 signature 的 thinking 块也丢**。理由：Anthropic 对没有签名的 thinking 块必拒，
   「签名不全不能回传」的判据对没有签名同样成立；个别 Anthropic 兼容网关根本不转 `signature_delta`，这类块保留
   也无法回传给官方端点，反而让应用以为拿到了可回传的块。丢掉后 `reasoning` 里仍有思考文字，界面展示不受影响。
   `redacted_thinking` 不要求签名（它自己就是不透明负载）。
6. **cache 用量**：`StreamUsage` 增加 `cache_creation_input_tokens` / `cache_read_input_tokens`（`u32`，名字与 Anthropic 线格式一致，
   camelCase 序列化为 `cacheCreationInputTokens` / `cacheReadInputTokens`）。
   - 来源：`message_start.message.usage` 与 `message_delta.usage`，覆盖不累加；沿用「0 不覆盖已有非零值」的口径。
   - 🔴 **序列化时为 0 就省略**（`skip_serializing_if`）：这样没有 cache 的流，`outcome.usage` 的 JSON 与 0.1.4 逐字节一致，
     已有 325 条规范用例一个字都不用动；缺省 = 0，其他语言实现者按缺省 0 读。
   - 🔴 **`StreamEvent::Usage` 不加字段**：它是带命名字段的 enum 变体，`#[non_exhaustive]` 只挡 enum 本身，不挡变体里加字段；
     下游 `StreamEvent::Usage { input_tokens, output_tokens }` 不带 `..` 的模式会编译失败。所以只进 `outcome.usage`。
   - 🔴 **`Usage` 事件的触发条件不变**：只在 input / output 变化时发；仅 cache 变化不发事件（否则默认行为会与 0.1.4 不一致）。
   - 默认开启（不改 `content`）。**只读 Anthropic 协议**；OpenAI 兼容的 `prompt_tokens_details.cached_tokens` 等**本次不做**，文档记「未覆盖」。
7. **规范表达「开启选项」**：decode 用例 `input` 里加可选 `thinkingBlocks: true`，缺省 = 关闭；生成器只在为 `true` 时才写这个键，
   已有用例的 JSON 不变。`description` 与 `rules` 里写完整规则。

### 被否的做法

- **thinking 块进 `StreamEvent`（加 `ThinkingBlock` 事件）**：多一个事件变体，且下游只需要收尾结果，没必要。
- **`content_block_stop` 作为「块完整」的判据**：多个网关不发它（现有代码就忽略它），要求它会把能用的块误丢。
  `Complete` 已意味着 `message_stop` / `stop_reason` 到了，此前的块都已结束。
- **默认开启 + 让下游关**：违背「默认行为不变」。

## 🎯 实现步骤

- [x] 1. `stream.rs`：`Block` 增 `Thinking` / `Redacted`；`with_thinking_blocks`；thinking / signature / redacted 的累积；`build_content` 按 `complete` 取舍
- [x] 2. `stream.rs`：`StreamUsage` 加 cache 字段与序列化省略；`read_usage` 读 Anthropic 的 cache 字段
- [x] 3. 单测：关闭时与 0.1.4 一致（现有测试不改）；开启时 thinking + signature + tool_use 交错；signature 分片；redacted；无签名丢弃；
      断流 / 取消 / 失败丢 thinking；OpenAI no-op；cache 用量；任意分包等价（开启态）——新增 12 条，stream 测试 43 → 55
- [x] 4. `tests/smoke.rs`：从外部调用 `with_thinking_blocks`、读 cache 字段
- [x] 5. `xtask/src/spec.rs`：decode 用例支持 `thinkingBlocks`；新用例；`description` / `rules` 写全；`spec/README.md`
- [x] 6. 版本号 0.1.5、`cargo xtask gen-spec`；脚本核对 `git diff spec/`：**旧的 53 条 stream 用例逐条完全相同**，其余 9 个 spec 文件只有 `crateVersion` 一行
- [x] 7. 文档：模块文档、CHANGELOG 0.1.5、`docs/versioning.md`、`docs/downstream.md`（各加一行）、项目 CLAUDE.md 守卫表
- [x] 8. 影响面核对（见下）
- [x] 9. 反证（见下）
- [x] 10. 发版闸门全套（见下）
- [x] 11. 本地提交（代码与测试 / 发版提交 / 文档，分开），不推送、不发布

## 🧪 反证记录

| 反证 | 做法 | 结果 |
|---|---|---|
| (a) 默认值改成开启 | `new()` 里 `keep_thinking: true` | 🔴 单测 3 条红：**已有的** `anthropic_thinking_goes_to_reasoning_and_signature_is_dropped` + 两条新的默认值守卫；还原后 55 条全绿 |
| (a′) 同上，看规范守卫 | 同上，跑 `cargo test -p xtask` | 第一次**仍是绿的** —— `decode_case` 显式传了 `with_thinking_blocks(false)`，把默认值遮住了（见问题 1）。改成「选项缺省时不调用」后，`spec_files_in_sync` 变红；还原后绿 |
| (b) 去掉断流丢 thinking 的守卫 | `build_content` 里 `Thinking` / `Redacted` 分支去掉 `complete` | 🔴 `incomplete_streams_drop_thinking_blocks_and_keep_only_text` 与 `spec_files_in_sync` 同时红；还原后 55 条与 xtask 3 条全绿 |

## 🔎 影响面（change-impact）

默认关闭下 6 个下游全部不受影响。逐个搜了调用点：

| 下游 | 读到的 | 结论 |
|---|---|---|
| sigil `services/vault/llm.rs` | `outcome.content`（1808 行，原样存回历史）、`usage.input/output_tokens` | `content` 默认不变；用量字段按名读，不受新字段影响 |
| onestop `skill/agent.rs` | `outcome.content`（1910 / 1923 / 1928，作 assistant 回合历史）、`usage.input/output_tokens` | 同上。🔴 这正是必须 opt-in 的原因：默认带 thinking 块会改变它回填的历史 |
| onestop `chat/mod.rs` | `usage.input/output_tokens`、`end` | 无感 |
| knowledge_base `services/ai.rs` | `outcome.text()`、`end`、`skipped_frames`、`stop_reason` | 无感 |
| prism `ai_client.rs`、story_loom `provider.rs` | 只读 `end`、`skipped_frames` | 无感 |
| aibid `llm.rs` | `usage.input/output_tokens`、`end` | 无感 |
| reeve | 未使用 `stream` | 本次的目标使用方；等 0.1.5 发布后再升 |

没有任何下游引用 `StreamUsage` 类型本身或调用 `with_thinking_blocks`；`StreamUsage` 是 `#[non_exhaustive]`，下游本来就不能字面量构造 / 无 `..` 解构它，加字段兼容。

## 🧰 发版闸门结果（本地，读的是 `test result:` 行）

| 项 | 结果 |
|---|---|
| `cargo fmt --all --check` | 通过（先 `cargo fmt --all` 整理了自己新增的几处换行） |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过（先修了 2 处 `useless_concat`） |
| `cargo test -p ai-profile`（默认 feature） | lib 136 / smoke 8 / verify_mock 0 / doc 10，0 失败 |
| `cargo test -p ai-profile --no-default-features --features chat` | lib 136 / smoke 8 / doc 10，0 失败 |
| `cargo test -p ai-profile --features client` | lib 146 / smoke 12 / verify_mock 2 / doc 13，0 失败 |
| `cargo test --workspace --all-features` | lib 173 / media_mock 19 / smoke 12 / verify_mock 2 / xtask 3 / doc 14，0 失败 |
| `cargo doc`（`-D warnings --cfg docsrs --all-features` 与默认 feature `-D warnings`） | 两种都通过 |
| `cargo xtask gen-docs` + `git diff --exit-code docs/providers.md` | 无变化 |
| `cargo xtask gen-spec` | 重复生成 0 个文件变化 |
| `cargo package -p ai-profile` | 35 个文件，含 `LICENSE` / `README.md`，无 `.secrets/`；包内 `Cargo.toml` 有 `[package.metadata.docs.rs] all-features = true`、`rust-version = "1.88"`、`repository` |
| 解包后 `cargo test --all-features`（独立纯 ASCII `CARGO_TARGET_DIR`） | lib 173 / media_mock 19 / smoke 12 / verify_mock 2 / doc 14，0 失败 |
| MSRV 1.88 | 本机没装，要靠推送后 CI 的 `msrv` 任务（本次新增代码没用新语法） |

规范用例总数：**325 → 336**（只有 `stream.json` 53 → 64，新增 11 条，其中 8 条 `thinkingBlocks: true`）。

## 📝 关键决策

- **架构归属**: crate（`stream`，sans-IO 解码的一部分；「保留什么块」是协议解码，不是应用取舍）。回传 thinking 块、拼请求体、Agent 循环仍在应用
- **版本位**: 兼容新增 → `0.1.5`（0.x 补丁位）
- **影响的下游**: 见下方「影响面」，默认关闭时全部无感

## 🐛 问题记录

### 问题 1: 规范生成器显式传 `false` 会遮住「默认值被改」
- **发现时间**: 2026-10-01（做反证 a 时）
- **现象**: 把 `keep_thinking` 默认改成 `true`，单测红了，但 `cargo test -p xtask` 的 `spec_files_in_sync` 仍绿
- **根本原因**: `decode_case` 一开始写成 `.with_thinking_blocks(thinking)`，对「缺省 = 关闭」的用例也显式传了 `false`，规范用例钉住的就不再是**默认行为**
- **解决方案**: 选项缺省时不调用 `with_thinking_blocks`，只在 `thinkingBlocks: true` 时才调用；复测反证 (a) 后 `spec_files_in_sync` 变红
- **状态**: 🟢 已解决

## 🔄 当前进度

**已完成**: 11 / 11 步骤（100%）。**尚未发布**：不运行发布命令、不打 tag、不推送，等用户当次明确确认

## 📁 相关文件

- `crates/ai-profile/src/stream.rs`
- `crates/ai-profile/tests/smoke.rs`
- `xtask/src/spec.rs`、`spec/conformance/stream.json`、`spec/README.md`
- `CHANGELOG.md`、`docs/versioning.md`、`docs/downstream.md`

## ⚠️ 注意事项

- 🔴 已有规范用例**不许改**：黄金样本红了绝不能靠重新生成来「修」，先查是不是自己改了默认行为
- 文档站 `ai-profile-docs` 不碰（用户另行同步）
- 全部文件 LF、UTF-8 无 BOM

## 💬 变更记录

### 2026-10-01 11:26
**变更类型**: 方案设计
**变更内容**: 读完 `stream.rs` / `xtask/src/spec.rs` 后定稿；相对任务书的调整：① `Complete` 时无签名 thinking 块也丢；② cache 字段为 0 时省略序列化以保证已有用例不变；③ `StreamEvent::Usage` 不加字段。
