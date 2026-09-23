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

### 文档与工具
- `docs/providers.md` —— 由 `cargo xtask gen-docs` 生成，
  守卫测试 `providers_md_in_sync` 保证与代码一致
- `docs/versioning.md` —— 版本位判定表 + 两个高危改动的说明
- `cargo xtask probe` —— 探活各家端点（人工触发，不进 CI）

### 待办
- `client::dry_run` —— 真实调用（产生费用），按 kind 分实现
- image / video / tts 三种 kind 的预置与 client
- `packages/react` —— headless hook + 预制组件
