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

### 待办（见 `docs/tasks/active/`）
- `preset` 模块：20 家服务商预置 + `vendor_id` 聚合
- `protocol` 模块：`ai.profile` 解析 / 生成
- `client` 模块：`verify()` / `dry_run()` / `fetch_models()`
- `docs/providers.md`、`docs/versioning.md`
- `cargo xtask probe` 实现
