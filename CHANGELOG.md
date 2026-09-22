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

### 文档与工具
- `docs/providers.md` —— 由 `cargo xtask gen-docs` 生成，
  守卫测试 `providers_md_in_sync` 保证与代码一致
- `docs/versioning.md` —— 版本位判定表 + 两个高危改动的说明
- `cargo xtask probe` —— 探活各家端点（人工触发，不进 CI）

### 待办
- `client::dry_run` —— 真实调用（产生费用），按 kind 分实现
- image / video / tts 三种 kind 的预置与 client
- `packages/react` —— headless hook + 预制组件
