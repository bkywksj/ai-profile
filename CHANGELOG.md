# Changelog

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)，版本位的判定规则见
[`docs/versioning.md`](docs/versioning.md)。

## [未发布]

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

### 文档与工具
- `docs/providers.md` —— 由 `cargo xtask gen-docs` 生成，
  守卫测试 `providers_md_in_sync` 保证与代码一致
- `docs/versioning.md` —— 版本位判定表 + 两个高危改动的说明
- `cargo xtask probe` —— 探活各家端点（人工触发，不进 CI）

### 待办
- `client::dry_run` —— 真实调用（产生费用），按 kind 分实现
- image / video / tts 三种 kind 的预置与 client
- `packages/react` —— headless hook + 预制组件
