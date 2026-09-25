# Changelog

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)，版本位的判定规则见
[`docs/versioning.md`](docs/versioning.md)。

## [未发布]

## [0.1.2] - 2026-09-25

**升级只需 `cargo update -p ai-profile`，不用改代码。** 行为变化只涉及下面列出的输入，正常配置结果不变。

### 修复
- **模型清洗漏掉了本库自己的生图 / 视频 / 配音模型**：`dall-e-3`、`doubao-seedream-*`、`doubao-seedance-*`、
  `wan*-t2i-*`、`*-I2V-*`、`vidu/*_img2video`、`cogvideox-*`、`MiniMax-Hailuo-*`、`fish-speech-*`，
  以及 OpenAI 的语音转写模型 `gpt-4o-transcribe` / `gpt-4o-mini-transcribe`，都会出现在「获取模型」的对话下拉里。
  新增守卫测试用预置数据双向校验：以后新增非对话预置而特征词没覆盖到，测试直接失败。
  下游影响：对话下拉变干净；`dropped_models` 变多 —— 按它排生图 / 视频 / 配音下拉的应用，这些模型会排到正确位置
- `suggest_url` 对以 `/v1beta` 结尾的地址（带不带结尾 `/`）会建议出 `…/v1beta/v1` 这种错地址，现返回 `None`
- 误填的端点后缀改按**路径段**识别：此前按字符串，`…/v1/mymessages` 会被剥成 `…/v1/my`
  （`join_api_path` / `join_chat_endpoint` / `anthropic_base_url` 共用这条规则）
- `ai.profile` 信封必须是 JSON 对象：此前 serde 默认允许按字段顺序从数组反序列化，
  `["ai.profile",1,{…}]` 也能导入，协议里没有这种写法；现返回 `invalid_json`

后四处是请外部实现者只凭公开规范写 Python 版时发现的。

### 新增
- 公开常量 `model_filter::{NON_CHAT_MARKERS, NON_CHAT_PREFIXES}`、`history::CONTEXT_OVERFLOW_PATTERNS`、
  `limits::{CONTEXT_WINDOW_FIELDS, MAX_OUTPUT_FIELDS}`：模型清洗、超长识别、限额解析用到的数据表，
  原本是私有的，公开后行为不变

### 仓库层面（不影响 crate 代码）
- **其他语言可以对照实现了**：`spec/` 发布与语言无关的预置数据 `presets.json` 和 8 组共 246 条一致性用例，
  规则用到的数据表随用例一起发布。由 `cargo xtask gen-spec` 从本库生成，守卫测试保证与代码同步。
  文档站按版本存档：<https://ai-profile.ruoyi.plus/spec/v0.1.2/>，说明见「其他语言实现」页

## [0.1.1] - 2026-09-24

### 修复
- docs.rs 按全部 feature 构建文档（`[package.metadata.docs.rs] all-features = true`）。
  0.1.0 在 docs.rs 上只有默认的 `chat`，`client` 与 `media` 整块缺失

## [0.1.0] - 2026-09-24

首个 crates.io 版本。以下为 0.1.0 之前的开发记录。

### 骨架
- 建仓：workspace + `crates/ai-profile` + `xtask`
- `endpoint` —— base_url 原样拼接，**不推断版本段**（含 4 个守卫测试）
- `model_filter` —— 拉回清单的清洗，排除法（含 4 个测试）
- `kind` —— `Kind` / `Protocol` 枚举，按 feature 切分
- `error` —— 结构化 `VerifyError`，每个变体对应一个 UI 动作
- 规划与交互原型迁入 `docs/`

### 数据与协议
- `preset` —— 19 家 chat 预置，按 `vendor_id` 聚合成服务商目录
  - 含火山方舟（豆包）/ 腾讯 TokenHub / xAI Grok，三家地址均经 `probe` 实测可达；
    model id 未经真实密钥验证，故 `verified_at` 留空
  - 火山方舟的 `vendor_id` 刻意与后续 image / video 预置共用 ——
    三种能力同一个 base_url，用户配一次密钥即可全开
- `protocol` —— `ai.profile` 解析 / 生成，统一四份既有实现
  （修掉其中一份漏 `baseUrl` 别名导致的静默不兼容）
- `client::Verifier` —— 持有可复用的 HTTP 客户端，零成本验证，
  六个错误变体各对应一个 UI 动作
  - `Verifier::from_builder` 接收调用方自备的 builder（代理 / 证书从这里进），
    超时与禁重定向由本 crate 在其后强制施加，调用方覆盖不掉
  - 自由函数 `client::verify` 仍在，走进程级共享的默认实例；**需要代理就不能用它**

### 历史裁剪（2026-09-23）
- 新增 `history` 模块（不依赖 client，移动端可用）：`HistoryMessage` trait、`trim_history`、
  `history_budget`、`estimate_tokens`、`retry_budget`、`is_context_overflow`
  - 从 sigil 的 `llm_trim.rs` 收入 —— reeve 接入时出现第二个消息结构一致的使用方
  - **修正**：首条 user 消息此前无条件补回，它本身超预算时结果永远降不下来、被动重试也救不了；
    现在先为它预留位置，且只在不超过预算一半时预留
  - 新增「窗口未知」时的被动防线：识别各家「上下文超长」报错，按估算的一半重试

### 新机型（2026-09-23）
- Claude Opus 5.5（`claude-opus-5-5`）：加入 Anthropic 官方 / Claude Code 中转两档；
  官方档默认改为它（更强且更便宜），中转档默认仍是 Opus 5（中转上新滞后）
- GPT-6 Sol / Luna（`gpt-6-sol` / `gpt-6-luna`）：加入 OpenAI 官方与 Codex 档；
  Codex 档补齐 GPT-6 三款，默认仍是 `gpt-5.6-terra`（GPT-6 仍在分批放量）
- OpenRouter 补 `anthropic/claude-opus-5.5`、`openai/gpt-6-sol`、`openai/gpt-6-luna`

### 限额分层与协议拼写（2026-09-23）
- ⚠️ `Protocol` 线格式由 `"open_ai_compatible"` 改为 `"openai_compatible"`，
  并新增 `as_str` / `parse` / `default_base_url` —— 线格式、持久化、默认端点收成一处
- `LimitSource::User` + `TokenLimits::from_user` / `with_source` / `or`（逐字段回退）
- `preset::model_limits(protocol, base_url, model)` —— 查预置静态限额
- 修：`infer_preset_key` 把带 `/v1` 的 Anthropic 官方地址误判成中转档

### 多条打包（2026-09-23）
- `parse_profiles` —— 单条与 `ai.profile.bundle` 统一返回列表（`ParsedProfiles`）；
  `parse_profile` 签名不变，遇到打包仍报 `not_ai_profile`（minor）
- 兼容智码已发出去的打包写法：`data.api_profiles` + snake_case 档案 + 顶层 `tool_id`；
  `auth_type = "oauth"` 的条目跳过并计入 `skipped`（OAuth 凭据与设备绑定）
- `ParseError::EmptyBundle` —— 打包里没有一条可导入的配置
- 修：`parse_profiles` 先认 `kind` 再要 `data`，粘了别的 JSON 时报 `not_ai_profile` 而不是 `missing_data`（reeve 测试抓到）

### 新增 6 家服务商（2026-09-23）
- MiniMax / 百度千帆 / 阶跃星辰 / 百川 / 小米 MiMo / Together，随 knowledge_base 接入搬入
  （地址与模型取自它已发布的预置表），共 25 家；`probe` 实测地址均可达，model id 未经真实密钥验证
- 没有搬的两家：腾讯混元（老地址已停止上新模型，引导走 TokenHub）、
  零一万物（`probe` 返回「Model service has been discontinued」）
- 下游升级时要补 6 家的 `providerTemplate.*` 翻译（sigil `check-preset-i18n.py`、
  reeve 守卫 `ai_profile_preset_i18n_keys_exist` 会报缺）

### 撤回聚合站预置（2026-09-24）
- 中宇AI智行（`ipsunion`）的对话 / 生图 / 视频 / 配音 4 条预置**不进 crate**（用户决定）：它是个别应用自己的合作渠道，
  不该出现在所有下游的服务商下拉里。需要它的应用（onestop、StoryLoom）在本地补这几条
- 对话预置回到 25 家；此前升级到 `844a755`～`ee1731e` 的下游若补过 `providerTemplate.ipsunion.*` 翻译，可以删掉

### 下游定制目录（2026-09-24）
- `preset::PresetCatalog`：在 crate 预置上 `remove` / `retain` / `map` / `extend`（同 key 覆盖、新条目插到同 kind 同分组末尾）后 `build`
- `ProviderPreset::new` + `with_*` 一组 `const fn`：下游不能写字面量（`non_exhaustive`），靠它写自己的静态预置表
- `ProviderPreset::default_extra`：预置定死的 extra 键值，新建配置时写进调用方 `extra`（线格式 `[[key, value], …]`）
- ⚠️ 行为变化：视频 New API 协议**只认** `extra.video_api=newapi`，不再按域名（原先 `ipsunion`）兜底；
  依赖它的存量配置由下游迁移补标记（StoryLoom v31）

### 多模态调用支持代理；服务商卡片跟随定制目录（2026-09-24）
- `media::MediaHttp::from_fn(|| builder)`：生图 / 视频 / 配音调用接入调用方的 HTTP 底座（代理 / 证书），
  超时策略仍由本 crate 在其后施加。各 provider 新增 `with_http`，统一入口新增 `from_config_with` / `synthesize_with`；
  原有 `new` / `from_config` / `synthesize` 不变
- `preset::vendors_in(list, kinds)` 与 `PresetCatalog::vendors`：按调用方的定制目录聚合服务商卡片

### 被滤掉的模型 id 也带回（2026-09-24）
- `VerifyOk::dropped_models` / `CleanedModels::dropped_models`：清洗时滤掉的非对话模型 id（端点顺序），
  `len() == dropped`（minor，两个结构都是 `non_exhaustive`）
- 起因是 onestop：一条配置同时挂对话与生图 / 配音 / 向量模型，要的是「能聊天的排前面、其余也别丢」，
  而此前只拿得到条数、拿不到 id。只做对话的调用方忽略它即可

### 多模态：生图 / 视频 / 配音（2026-09-24，随 StoryLoom 接入反向搬入）
- 预置：生图 5、视频 8、配音 4 条（含各自「自定义」档），挂 `image` / `video` / `tts` feature；
  `vendor_id` 与同 host 的对话预置共用。只开 `chat` 的下游不受影响（`presets()` 仍是静态数组）
- ⚠️ 行为变化：`infer_preset_key` 只在对话预置里反推（开了多模态后不会把对话配置认成生图档）；
  分组连续性守卫改为按 kind 判
- 调用：新增 `media` 模块（要同时开 `client` 与对应 kind），整体搬自 StoryLoom v0.9.0 的生产实现，只做机械替换：
  - `media::image`：OpenAI `/images/generations` 兼容 + 通义万相 DashScope 异步；`AnyImageProvider::from_config`
  - `media::video`：火山方舟 / 海螺 / Vidu / 智谱 / 硅基流动 / New API 六套 submit + poll；
    `VideoProtocol::detect`、`AnyVideoProvider`、`supports_last_frame`、`explain_error`
  - `media::image::ImageProtocol::detect`：只判协议、不构造 provider（onestop 据此过滤它调不通的生图预置）
  - `media::tts`：火山语音专有协议 + OpenAI `/audio/speech`；`TtsProtocol::detect`、`synthesize`、音色目录 `voice_catalog`
  - 错误类型 `MediaError`，`Display` 与 StoryLoom 原 `AppError` 逐字一致
  - **任务编排不进 crate**：轮询循环、取消、落盘、进度事件归调用方
- 依赖：`client` 新增 `base64`、`log`；`video` 带图像解码库（依赖名 `img`，New API 中转站要先压缩大首帧）

### Anthropic 协议自动补 `/v1`（2026-09-24）
- `endpoint::anthropic_base_url`：Anthropic 协议的 base 末段不是版本段就补 `/v1`；已带版本段原样用，
  末尾 `#` 表示「别替我补」。`join_chat_endpoint(base, "messages")` 与 `Verifier`（Anthropic）都走它
- 理由：Anthropic 只有 v1，整个生态（官方 SDK、Claude Code 的 `ANTHROPIC_BASE_URL`、各家 `/anthropic` 入口）
  都约定 base 填到版本段之前 —— 这是确定的协议翻译，与 OpenAI 兼容一侧「不推断」的契约不冲突
- 起因：粘贴导入 `baseURL: https://x.com:8443` 的 Anthropic 配置，对话打到 `/messages`，中转网关回
  200 + 前端网页，报「解析失败」；而它的 `/models` 不带 `/v1` 也能用，「获取模型」反而是绿的
- 404 诊断改用补过的 base，不再对已补 `/v1` 的地址建议「补 /v1」
- `tests/verify_mock.rs`：守住验证请求真实打到 `/v1/models`，以及 OpenAI 兼容一侧仍原样拼

### 发布前收尾（2026-09-24）
- 🔴 多模态 HTTP 客户端建失败时**不再静默退回 `reqwest::Client::new()`** ——
  那会连同调用方配的代理与超时一起丢掉、改走直连且不报错。现在失败原因留到发请求时以
  `MediaError::Failed` 报出；公开构造函数签名不变
- rustdoc 零警告（修掉 5 处失效的文档内链接），CI 以 `-D warnings` 守住
- 声明 `rust-version = "1.88"`，CI 新增 msrv 任务用 1.88 真编一遍
- 包内补 `LICENSE`；README 按现状重写（原文还停在「规划阶段」，且含 crates.io 上会失效的相对链接）
- `providers_md_in_sync` 在 crates.io 下载的包里（没有仓库 `docs/`）跳过，仓库内仍严格校验
- `tests/media_mock.rs`：18 条模拟服务端测试（wiremock），覆盖全部 12 个多模态 provider 的
  「提交 → 轮询 → 取结果」与错误翻译。此前 media 只有分发与解析单测，没有一条真正发过请求
  - 守住的协议契约举例：New API 轮询用 `id` 而非 `task_id`、海螺成功后多一步取下载地址、
    火山方舟时长必须拼进 text 尾缀、火山语音鉴权头是 `Bearer;token`、出图下载失败自动重试一次
  - 🔴 测试注入 `no_proxy()` 的底座：reqwest 默认读 `HTTP_PROXY` 且不为 127.0.0.1 绕开，
    开着本机代理时请求会绕一圈再回来、随机报 os error 10053

### 文档与工具
- `docs/providers.md` —— 由 `cargo xtask gen-docs` 生成，
  守卫测试 `providers_md_in_sync` 保证与代码一致
- `docs/versioning.md` —— 版本位判定表 + 两个高危改动的说明
- `cargo xtask probe` —— 探活各家端点（人工触发，不进 CI）

### 待办
- `packages/react` —— headless hook + 预制组件（等出现第二个想复用界面的前端再做）
- 各家模型 id 用真实密钥核实后填 `verified_at`

### 不做（2026-09-24 决定）
- `client::dry_run`：生图 / 配音直接调 `media` 的 `generate` / `synthesize` 就是试运行；
  对话的「真实测试」按职责边界归应用（要用应用自己的对话实现）
- 视频任务远程取消：各家取消接口大多没有或未经真实密钥验证；调用方停止轮询即可止损
