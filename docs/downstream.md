# 下游登记表

> 🔴 **本 crate 每次推送后，都要回到这里过一遍。**
> 流程见技能 `downstream-sync`；本表只记「现在是什么状态」。

单向依赖最常见的失败是「crate 发了，没人升」—— 下游引用的版本停在几周前，
新模型、新修复都到不了用户手里，而且没有任何报错。这张表就是为了让它可见。

## 已接入

| 项目 | 仓库路径 | 引用方式 | 当前引用 | 最后同步 | 接入范围 | 本项目侧技能 |
|---|---|---|---|---|---|---|
| sigil | `E:/my/桌面软件tauri/sigil` | crates.io 版本（`src-tauri/Cargo.toml`） | `0.1.4` | 2026-10-01 | 预置 · 协议 · 端点 · 验证 · 限额 · 模型清洗 · 历史裁剪 · ai.profile 单条与打包 · **流式解码**（`stream`，本地 SSE 解析器已删） | `ai-profile-integration` |
| knowledge_base | `E:/my/桌面软件tauri/knowledge_base` | crates.io 版本（`src-tauri/Cargo.toml`，chat + client）；桌面与 Android 同一个 crate | `0.1.3` | 2026-09-25 | 预置（只用 OpenAI 兼容）· 端点 · 验证 · 限额 · 模型清洗 · ai.profile 单条与打包 · 超长识别（对话协议、RAG 预算、降档阶梯留在应用） | `ai-profile-integration` |
| onestop | `E:/my/backend_tauri/onestop` | crates.io 版本（`src-tauri/Cargo.toml`，chat + client + image + video + tts）；只有桌面端碰模型服务 | `0.1.3` | 2026-09-25 | 对话预置 · 协议 · 端点 · 验证（保留非对话模型）· 限额 · ai.profile 单条与打包 · 超长识别 · **生图 / 视频 / 配音预置**（只取它调得通的协议：OpenAI images / 火山方舟视频 / OpenAI speech）；多模态调用仍是它自己的实现 | `ai-profile-integration` |
| story_loom | `E:/my/桌面软件tauri/story_loom` | crates.io 版本（`src-tauri/Cargo.toml`，chat + client + image + video + tts） | `0.1.3` | 2026-09-25 | **四种能力全用**：预置 · 端点 · 验证 · 限额 · ai.profile 单条与打包 + 导出 · 超长识别 · 生图 / 视频 / 配音调用（`media`，实现即来自它）；任务编排、上下文组装、对话实现留在应用 | `ai-profile-integration` |
| reeve | `E:/my/桌面软件tauri/reeve` | crates.io 版本，**两处同值**：`src-tauri/Cargo.toml`（chat + client）与 `src-tauri/reeve-core/Cargo.toml`（只开 chat）；移动端经 `reeve_core::ai_profile` 跟随 | `0.1.3` | 2026-09-25 | 预置 · 协议 · 端点 · 验证 · 限额 · 模型清洗 · 历史裁剪 · ai.profile 单条与打包（桌面）；端点 · 预置 · 清洗（移动） | `ai-profile-integration` |
| prism | `E:/my/桌面软件tauri/prism` | crates.io 版本（`src-tauri/Cargo.toml`，chat + client + image）；`mobile-tauri` 壳不依赖 | `0.1.3` | 2026-09-26 | 对话预置 · 协议（OpenAI 兼容 + Anthropic）· 端点 · 验证 · 模型清洗 · 限额 · 历史裁剪与超长重试 · ai.profile 单条与打包 + 导出 · **生图预置与调用**（`media::image`）；对话实现、配图落盘入库留在应用 | `ai-profile-integration` |
| aibid | `E:/my/backend_tauri/aibid` | crates.io 版本（`desktop/src-tauri/Cargo.toml`，`client` + 默认 `chat`）；桌面端 | `0.1.3` | 2026-09-26 | 对话预置 24 家 · 协议（OpenAI 兼容 + Anthropic）· 端点 · 验证 · 模型清洗 · 限额 · 超长识别 · ai.profile 单条与打包 + 导出（不带密钥）；对话 / 流式 / Anthropic 线格式 / 重试 / 代理 / 密钥存储留在应用；**视觉 / 向量维度**（crate 没有）做成 `model_caps.rs` 叠加表。未发布过，按用户决定不写存量迁移（提交 `bc15d13` / `bb76bfa` / `51aa48e`） | `ai-profile-integration` |

> ⏳ sigil / knowledge_base / onestop / story_loom / reeve 的界面改动都**尚未实机验证**；代码测试全绿。
> prism 已在 tauri dev 里验证「获取」（文本 / 图片），测试连接、导入导出、生图还没实测。
>
> 2026-10-01：**`0.1.4` 已发布到 crates.io**（tag `v0.1.4` 指向 `171e70b`，docs.rs 的 `stream` 与 `infer_preset_key_for` 已核对，文档站已同步）。
> **sigil 已升到 `0.1.4` 并迁移流式**（提交 `841b309`，2026-10-01，本地未推送）：删掉 `OaStream` / `parse_sse_event` /
> `handle_anthropic_sse_event` / `openai_stop_reason` / `looks_like_html`，两个协议共用一个 `pump_stream`；
> 全量测试 lib 1551 条通过。**界面行为待实机验证**：中文跨包不乱码、Anthropic 流内错误、整段 JSON 响应报错、停止按钮。
> 其余五家（knowledge_base / onestop / story_loom / reeve / prism）**仍引用 `0.1.3`**；`0.1.4` 是兼容补丁，只升版本号零代码改动，
> 但要用 `stream` / `infer_preset_key_for` 的下游必须先升。
>
> 迁移时的两个提醒（sigil 踩到的）：
> ① 构建脚本会撞 `tauri dev` 开发版占用的 `dokan2.dll`（os error 32），验证用独立的 `CARGO_TARGET_DIR` 完整重编，别去关别人的开发版；
> ② `cargo update -p ai-profile` 可能把别的包（errno / rustix / tempfile 等）的 `windows-sys` 引用顺手改旧，
> 提交前看锁文件 diff，只应动 ai-profile 一个包，否则从 HEAD 取回原文件、手改版本和校验和两行。
>
> prism 升级到 `0.1.4`（`infer_preset_key_for` 所在版本）时要做两件事，见 CHANGELOG `0.1.4`：
> ① 删掉本地的 `infer_image_preset_key`，改调 `preset::infer_preset_key_for(Kind::Image, …)`（按 `crate-boundary` 删副本）；
> ② 素材库记的图片尺寸改为从出图字节读 —— 请求的 `IMAGE_SIZE` 对火山方舟 / OpenAI 官方不生效（方舟按模型默认尺寸出图）。
> 它反馈的另两条（生图请求体补 `size`、按能力分类模型）结论与待办见 `docs/tasks/` 里的「prism接入反馈三条」。
>
> 🔴 **流式解码（`stream`，0.1.4 起可用）的迁移计划**（任务见 `docs/tasks/` 里的「流式解码」；迁移顺序 sigil → prism → aibid → onestop，一次一家）：
> - **sigil 先改**：删本地 `OaStream`、`parse_sse_event`、`handle_anthropic_sse_event`、`openai_stop_reason`、`looks_like_html` 及其测试，
>   用 `StreamDecoder::push` 的事件映射到现有前端事件，`Done.response` 用 `StreamOutcome::content`；
>   取消接线、`stream_options` 重试与「按地址记住」留在应用。注意三处行为变化：`ToolUseStart` 延迟到工具名已知才发；
>   Anthropic 流内 `error` 变成终态；多字节字符跨包不再丢字。
> - **prism 优先**：它已有字节缓冲、`[DONE]`、流内 error、role-only 首块，缺 usage、finish_reason、断流检测（流断了当成功）、2xx 回网页、`stream_options` 重试。
> - **其余（aibid / onestop / story_loom / knowledge_base）不强制**，下次动对话功能时迁；aibid、onestop 逐块 `from_utf8_lossy` 的乱码会随迁移自然修好。
>   knowledge_base 的 Ollama 原生 NDJSON 不在 `stream` 范围内，留在应用。
> - **aibid 迁移时注意**（2026-10-01 摸底，`desktop/src-tauri/src/llm.rs` 的 `chat_stream`）：
>   ① 只有文本流，无工具调用；回调是 `on_delta(&str)`，返回 `Err` 即取消，收尾用 `ChatReply { content, finish_reason, usage }`；
>   ② 🔴 **`finish_reason` 的词表**：aibid 用 OpenAI 词表（`stop` / `length` / `tool_calls`）并有截断闸门（AI-04，认 `length`），
>   crate 的 `StopReason` 是 Anthropic 词表（`end_turn` / `max_tokens` / `tool_use`），迁移时必须换算回去，闸门语义不能变；
>   ③ `Truncated`（断流）当前会表现为 `finish_reason` 为空，要核对闸门对空值的处置，别让断流变成静默成功；
>   ④ `stream_options` 重试现在是按**错误字符串**判（`is_stream_options_unsupported`），`stream_send` 在判之前已把响应压成字符串，
>   换成 crate 的 `is_stream_options_rejected(status, body)` 需要把状态码带出来；
>   ⑤ 保留：429/5xx 退避重试、`is_stream_unsupported` 回落非流式（`commands::stream_or_chat`）、`report()` 记账；
>   ⑥ 逐块 `from_utf8_lossy` 的中文乱码会随迁移修好；`ai-profile` 依赖 features 是 `["client"]` 且没关默认特性，所以 `chat`（含 `stream`）已开。
>
> story_loom 的本机 lib 单测有 WebView2 入口崩溃（环境问题），
> 应用侧纯函数测试（`legacy_endpoint` / `provider_share`）是在临时 crate 里 `#[path]` 引入实跑的。
>
> 🔴 **只属于个别应用的服务商不进 crate**（2026-09-24 撤回中宇AI智行）：那个应用用 `PresetCatalog::new().extend(&LOCAL)` 自己加，
> 目前只有 story_loom 在用（`local_presets.rs`，四种能力各一条，视频带 `video_api=newapi`）；onestop 不接中宇，它的 `local_presets.rs` 是空表。
>
> onestop 的多模态预置已改用 crate（`850eb77`）。它自己的生图 / 视频 / 配音调用只支持三种协议，所以预置按协议过滤；
> 要支持海螺 / Vidu / 火山语音等，得先把调用换成 `media`（StoryLoom 在用），再放开过滤。
>
> 2026-09-25：五家升到 **`0.1.3`**，做法同 0.1.2，零代码改动，锁文件只动 ai-profile 一个包，2026-09-25 已推送（连同技能更新提交；sigil 同批推了 crates.io 发布能力的 7 个提交）。
> 测试：sigil 1468 / reeve 1104 / knowledge_base 890 / onestop 199 全过；story_loom 测试代码编译通过。
> 提交：sigil `d14e3a9`、reeve `e9d3b81`、knowledge_base `f81e2f7`、onestop `6279cb3`、story_loom `26bd357`。
> 用户能感知的变化：「Anthropic 官方」档不填地址也能「获取模型」；地址填到网站根目录时报「地址不对」而不是假成功。
>
> 2026-09-25：五家升到 **`0.1.2`**（`Cargo.toml` 最低版本改为 0.1.2 + `cargo update -p ai-profile`，
> Cargo.lock 只动 ai-profile 一个包），零代码改动，均只本地提交、未推送。
> 测试：sigil 1419 / reeve 1104 / knowledge_base 890 / onestop 198 全过（knowledge_base 此前 2 条既有失败已不复现）；
> story_loom 测试代码编译通过（lib 单测仍受 WebView2 环境所限）。
> 提交：sigil `c96b16a`、reeve `15b08f0`、knowledge_base `933cb96`、onestop `024831b`、story_loom `8fb20f4`。
>
> 2026-09-24：五家统一从 git 提交号切到 **crates.io `0.1.1`**（首个正式发布版），均只本地提交、未推送。
> 各家测试：sigil 1419 / reeve 1104 / onestop 198 全过；story_loom 编译检查通过（lib 单测受 WebView2 环境所限，
> 迁移对照测试在临时 crate 实跑 13/13）；knowledge_base 有 2 条**与本库无关**的既有失败
> （`dataview_recent_notes_orders_by_updated_at`、`tag_path_segments_independent_namespace`，Cargo.lock 只变了 ai-profile）。
>
> ⚠️ 0.1.1 起 Anthropic 协议自动补 `/v1`。onestop 的 v16 迁移因此把「带路径、无版本段」的 Anthropic 地址钉上 `#`，
> 保证升级前后请求同一地址；reeve / knowledge_base / sigil / story_loom 的旧规则本来就补 `/v1`，不受影响。

> 升级一个下游 = 改它 `Cargo.toml` 里的 `version`（补丁版本也改）+ `cargo update -p ai-profile`（锁文件只动这一个包）→
> 在该项目跑全量测试 → 按它自己的节奏发版 → **回来改上表的「当前引用」和「最后同步」**。
> 漏了最后一步，下次就不知道它落后多少。

## 计划接入

按顺序。每家接入时在 `docs/tasks/active/` 建一份任务文档，完成后归档，并把它移到上表。

| 顺序 | 项目 | 现状（2026-09-23 盘点） | 接入时必须处理 |
|---|---|---|---|
| 1 | sku_lane（`E:/my/桌面软件tauri/sku_lane`，品道 · 电商铺货） | **2026-09-29 接入中**（由 sku_lane 仓库里的会话实施）。未发布（无 tag）。`services/ai.rs`（169 行）**写死 DeepSeek 一家、模型 `deepseek-chat`** —— 该别名 2026-07-24 已下线，**AI 选品评分 / 改写标题现在就是坏的**；无服务商选择、无 `ai.profile` | ① 用户只能填一个 DeepSeek 密钥：接入即新增「模型服务」设置（选服务商 / 获取模型 / 测试连接），属新增界面 ② 未发布，不写迁移 ③ 优先级最高：现在是坏的 |
| 2 | reka（`E:/my/桌面软件tauri/reka`，HTTP 接口调试工具） | **2026-09-30 接入中**（由 reka 仓库里的会话实施）。未发布（无 tag）。后端 `services/ai_llm.rs`（244 行）按 provider 分派 Anthropic / OpenAI 两套对话，OpenAI 分支**自动补 `/v1`**；前端 `AiSection.tsx` 写死 5 家预置（DeepSeek 默认 `deepseek-chat`，别名已下线）；另有 **TS 版 `lib/aiProfile.ts` 解析器**（117 行）+ `commands/ai_profile.rs` | ① 前端预置表、TS 解析器删掉，改用 crate ② 地址规则换成 crate 的（原样使用），未发布，不写迁移 ③ 对话实现（两套协议）留应用 |
| 3 | shop_sage（`E:/my/桌面软件tauri/shop_sage`，购物参谋） | **2026-09-30 接入中**（由 shop_sage 仓库里的会话实施）。未发布（无 tag）。服务商是手填自由文本（无预置），协议靠 provider 字符串是否含 "anthropic" 判断；`ModelProfilesSection` / `ImportDialog` / `ShareDialog` 与 **TS 版 `lib/aiProfile.ts` 解析器**（sigil 接入时删掉的那种漂移副本）；`services/ai/client.rs` 手写端点拼接与 Anthropic 分支 | ① 删 TS 解析器，导入导出走 crate 的 `parse_profile` / `to_profile` ② 未发布，不写迁移 |
| 4 | cross_pilot（`E:/my/桌面软件tauri/cross_pilot`，跨翼 · 亚马逊多店驾驶舱） | 2026-09-26 盘点，未开工。🔴 **已发布**（tag `v0.1.0`，updater 指向 R2，文档站有下载页）。前端 `providerPresets.ts`（285 行，9 家）+ TS 版 `aiProfile.ts`；后端 `services/provider/llm.rs` | ① 🔴 已发布：存量配置的地址写法 / 预置 key 变了要写迁移 + 对照测试（照 reeve / knowledge_base 的范例） ② 工作区有 9 个未提交文件（license / store，2026-07-11 起搁置，非进行中的会话）：接入时不碰、不提交 |
| 5 | crosspilot_erp（`E:/my/backend_tauri/crosspilot_erp`，跨翼 ERP，Aegis 派生；私有化部署） | 2026-09-30 盘点，未开工。**服务端**有 AI 功能但全是 mock：AI Listing 工厂（生成 / 优化 / 翻译，`server/src/listing.rs`）、差评 AI 回复草稿（`server/src/api/review.rs`）；真实调用待办 B-016（Key 走 env `AEGIS_LLM_KEY`，未接）。桌面端 / 移动端无模型调用 | ① 🔴 首个**服务端**下游：Key 只在服务端、绝不进 WebView；配置放 env 还是 admin 后台 + 服务端加密存储，照 Aegis 的 `ai-profile-onboarding`（2026-09-30 新增）定 ② 这是把 mock 换成真实调用 = 新功能，先出方案给用户确认再动手 ③ 未配置时保留 mock 降级 |

### 🔴 存量地址修正（已发布过的下游都要做；reeve、knowledge_base 已做完，可作范例）

这三家的旧逻辑会**自动补 `/v1`**（reeve、knowledge_base 末尾加 `#` 可以禁止补），
crate 的规则是**原样使用、绝不推断**。三家都已发布过，用户存下的配置里一定有不带 `/v1` 的地址 ——
直接换成 crate，这些配置全部 404，而且用户看不出原因。

做法：接入时写一次性数据修正 —— **用该应用自己的旧补全逻辑**把存量 `base_url` 规范成完整地址
（补上 `/v1`、去掉末尾 `#`），修正完成后再删旧逻辑。

- 修正逻辑留在应用侧，**不进 crate**：它是各家历史包袱，不是通用知识
- sigil 没做这一步，是因为它当时还没发布过（未发布期不加迁移代码）
- onestop 的旧规则最简单（只有主机名时补 `/v1`），见 `src-tauri/src/provider/legacy_base_url.rs`，入口是 schema v16
- knowledge_base 的做法见 `src-tauri/src/services/legacy_api_url.rs`（它的旧规则把 `v1.5` 也算版本段，与 reeve 不同 ——
  **各家要冻结自己的旧规则**，不能共用一份）
- **reeve 的做法可直接照搬**（`reeve-core/src/legacy_base_url.rs`）：
  - 对照测试里冻结一份旧拼接规则，断言「旧规则(原值) == crate(修正值)」—— 以它为判据
  - 🔴 修正函数必须**幂等**：带 `#` 的地址原样保留（去掉 `#` 再修一次会被补成 `…/v1`）
  - 🔴 只对**旧来源**修正：升级迁移一次；备份恢复看备份清单的 schema 版本；
    跨端同步看载荷标记。新数据再修正会把用户刻意不带版本段的地址错补 `/v1`
  - 默认端点若是不带 `/v1` 的根地址，也要改用 `Protocol::default_base_url`
  - 应用若在「留空」时把默认地址写进库（reeve 移动端就是），迁移也要覆盖它

## 不接入

| 项目 | 原因 | 决定时间 |
|---|---|---|
| tauri-cc（智码 AICoder） | 模型服务与登录 / 账号体系耦合；**完全不接**，包括 ai.profile 协议层 | 2026-09-23 |
| ai-station | 项目**不再维护**，不接入（原计划排第 1） | 2026-09-24 |

## 未登记的 ai.profile 实现

盘点时发现另有 9 个项目自带 ai.profile 实现：quant_helm、reka、cup_watch、db_pilot、lumafind、
hindsight、cross_pilot、shop_sage、zhongyu_comic。其中 reka、cross_pilot、shop_sage 已于 2026-09-26 移入上方「计划接入」，
剩下 6 个仍按下面的规则处理。

🔴 **不预先登记**。哪个项目要改时，先确认它要不要接入、接到哪一层，确定了再加进上面的表。
（用户 2026-09-23 的决定：每次改前确定了再登记。）

## 发布形态

已发布到 crates.io（`0.1.0` 起，最新以 `docs/versioning.md` 与 crates.io 为准），「已接入」表里的下游都按**版本号**引用。
发版流程见技能 `crate-release`；0.x 阶段 minor 视为破坏性版本，判定规则见 `docs/versioning.md`。

发布前（2026-09-22 ~ 24）下游按 git 提交号引用 —— 那段时间改 API 不必背 semver 包袱，
五个应用都接入跑通后才发了 `0.1.0`。
