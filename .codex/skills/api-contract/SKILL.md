---
name: api-contract
description: |
  用于判定 ai-profile 公开 API 与序列化线格式的改动是否破坏下游，以及该升哪一位版本号。

  触发场景：
  - 给公开 struct 加 / 改 / 删字段，或给公开 enum 加变体
  - 改 serde 的 rename / tag / 字段拼写（前端直接消费这些 JSON）
  - 新增公开类型、函数，或改函数签名
  - 升级 reqwest 等被重导出的依赖

  触发词：公开API、pub、breaking、版本号、semver、non_exhaustive、serde、线格式、序列化、builder、重导出
---

# 公开 API 与线格式契约

## 为什么严格

下游不只是 Rust 代码：sigil 前端直接消费 crate 序列化出来的 JSON（预置、验证结果、导入预览）。
**serde 的一个拼写变化，Rust 编译照样过，前端静默拿到 undefined。**

## 版本位判定

| 改动 | 版本位 | 理由 |
|---|---|---|
| 改模型 id / 加 provider / 改 base_url / 改默认 model | patch | 数据更新 |
| 加 `Kind` / `LimitSource` / `VerifyError` 变体 | minor | `#[non_exhaustive]` 让下游 match 必带 `_` |
| 给 `#[non_exhaustive]` struct 加字段 | minor | 下游不能用字面量构造它 |
| 新增公开函数 / 类型 | minor | |
| 改 / 删已有字段、改函数签名 | **major** | |
| 改 `preset.key` | **major** | 🔴 存量配置靠它回填，改名 = 用户配置全掉进「自定义」 |
| 改 serde 线格式（rename、tag、字段名） | **major** | 前端静默错配 |
| 升级被重导出的 `reqwest` 大版本 | **major** | `pub use reqwest`，下游用它建 `ClientBuilder` |

🔴 **上表按 1.0 之后的语义写；0.x 阶段 Cargo 把版本位整体右移一位**，实际发版时这样换算：

| 表中的判定 | 含义 | 0.x 阶段实际改的位 | 下游 |
|---|---|---|---|
| patch | 数据 / 修 bug | `0.1.x` | `cargo update` 自动拿到 |
| minor | **兼容**的新增（加字段、加函数、加变体） | `0.1.x`（同样是补丁位） | `cargo update` 自动拿到 |
| major | 破坏性变更 | `0.x.0` | 必须改 `Cargo.toml` 的版本号 |

所以 0.x 阶段「只增不破坏」一律发补丁位，写进 CHANGELOG「新增」即可（0.1.2 加公开常量、0.1.3 加
`TokenLimits` 逐字段来源都是这样）；只有真正的破坏性变更才动 `0.x.0`。

1.0 之前 semver 允许 minor 带破坏性变更，但**仍要写进 CHANGELOG 并标 ⚠️**，
且要同步改所有已接入下游（本仓库改过一次：`Protocol` 线格式 `open_ai_compatible` → `openai_compatible`）。

## 四条硬规则

### 1. 所有公开 struct / enum 加 `#[non_exhaustive]`

把「加字段」从 major 降到 minor 的唯一办法。**第一版就要加**，事后补本身就是破坏性变更。

### 2. `non_exhaustive` 的入参类型必须配完整 builder

外部 crate 写不了 `ServiceConfig { .. }` 字面量（E0639）。没有 builder 下游根本用不了 ——
而**单元测试抓不到**（crate 内部可以写字面量）。守卫是集成测试
`service_config_builder_is_usable_from_outside`（`tests/smoke.rs`，从外部视角调用）。

新增入参类型时，同步在 `tests/smoke.rs` 加一条从外部构造它的测试。

### 3. 同一个值只能有一套拼写

serde 线格式、`as_str()`、`parse()` 必须一致（守卫 `protocol_spellings_agree`、`limit_source_roundtrip`）。
两套拼写并存会逼下游写转换层，迟早有一处直接透传而静默错配。

当前约定：

| 类型 | 线格式 |
|---|---|
| `ProviderPreset` / `ModelOption` / `VerifyOk` / `TokenLimits` / `ParsedProfile` | camelCase 字段 |
| `Protocol` | `"anthropic"` / `"openai_compatible"` |
| `LimitSource` | `"user"` / `"endpoint"` / `"preset"` |
| `VerifyError` / `ParseError` | `tag = "code"`，snake_case |

### 4. 共享的有状态对象要能跨线程

`Verifier` 必须 `Send + Sync + 'static` 且 `verify(&self)`，否则批量并发验证与放进全局状态都做不到
（守卫 `verifier_is_shareable_and_concurrent`）。

## ⚠️ 本技能不管「行为变更」

接口一个字没变、结果变了（拼地址规则、模型清洗、错误判定），本技能判不出来，却最容易让下游出事 ——
Anthropic 自动补 `/v1` 就是这样逼出了 onestop 的数据迁移。这类改动走技能 `change-impact` 第三节。

## 改完的检查清单

- [ ] 判定版本位，写进 `CHANGELOG.md`（破坏性的标 ⚠️，写迁移方法）
- [ ] `cargo test -p ai-profile --features client` 与 `cargo test -p ai-profile`（不带 client）都过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] 线格式变了 → 同步文档站 `guide/frontend.md` 里的 TS 类型
- [ ] 查 `docs/downstream.md`，已接入的下游逐个改并跑测试（走 `downstream-sync`）
