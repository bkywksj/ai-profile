---
name: change-impact
description: |
  用于改完 ai-profile 的代码或预置之后，查清楚还有哪些地方必须跟着改：生成物、文档站、多语言规范、公开 AI 文件、下游项目；并对「接口没变、结果变了」的行为变更逐个评估下游影响。

  触发场景：
  - 改了 crates/ai-profile/src 下的任何代码，准备提交前
  - 改了预置（加服务商、加模型、改默认模型、改限额）之后
  - 改了拼地址、模型清洗、错误判定、ai.profile 解析这类规则，结果会变但接口没变
  - 准备发版前，清点「未发布」里攒下的改动还欠哪些连带更新
  - 发现文档、README、规范页里的数字或说法和代码对不上

  触发词：连带更新、影响范围、改完还要改什么、同步哪些、行为变更、下游影响、漏改、文档过时、数字不对、change impact、checklist
---

# 改动影响清单

## 为什么需要这张表

本库的一处改动会扩散到六个地方：生成物、文档站、多语言规范（`spec/`）、公开的 AI 接入文件、
Context7 规则、五个下游项目。其中**一部分有测试守着，另一部分只能靠人记得** ——
后者不会报错，只会悄悄过时。

实例：
- `CLAUDE.md` 写「对话预置 26 家」，README 写「25 家」，实际 25 家（已加守卫）
- 模型清洗漏了 `dall-e-3` 等 10 余个我们**自己预置**的生图 / 视频 / 配音模型，没人发现，
  直到做多语言用例时顺手看见（已加守卫 `non_chat_presets_are_filtered`）
- Anthropic 自动补 `/v1` 接口一个字没变，却逼得 onestop 补写了一次数据迁移

**用法**：改完先按下面第一张表找到你的改动类型，逐项过；标 🛡️ 的有守卫，漏了会红，标 ✋ 的只能靠你。

## 一、改动类型 → 连带更新

### A. 预置数据（`preset/*.rs`：加服务商、加模型、改默认模型、改限额）

| 连带项 | 怎么做 | 守卫 |
|---|---|---|
| 服务商清单文档 | `cargo xtask gen-docs` | 🛡️ `providers_md_in_sync` |
| 多语言规范的预置数据 | `cargo xtask gen-spec` | 🛡️ `spec_files_in_sync` |
| 生图 / 视频 / 配音模型不能漏进对话下拉 | 新模型名没被识别就补 `model_filter.rs` 的特征词 | 🛡️ `non_chat_presets_are_filtered` |
| README 的「对话 N 家 + 生图 / 视频 / 配音 M 条」 | 手改 | 🛡️ `readme_preset_counts_match`（xtask） |
| 文档站首页、介绍页的数量 | 手改，发版同步后跑 `pnpm check` | 🛡️ `check-facts`（文档站仓库） |
| 下游 sigil 的界面翻译 | 新 `label_key` / `hint_key` → sigil 跑 `scripts/check-preset-i18n.py` | ✋ 升级下游时做 |

细节见技能 `preset-maintenance`。

### B. 纯规则函数（结果会变、接口不变）

`endpoint` / `model_filter` / `client::{diagnose, suggest_url, check_required_fields, parse_model_*}` /
`protocol::parse_*` / `limits` / `history::is_context_overflow` / `preset::{infer_preset_key, model_limits}`

| 连带项 | 怎么做 | 守卫 |
|---|---|---|
| 单元测试 | 先写一条能复现旧行为问题的测试 | ✋ |
| 多语言用例 | `cargo xtask gen-spec`，看 diff 里哪些期望值变了 —— **那就是本次行为变化的全部清单** | 🛡️ `spec_files_in_sync` |
| 用例的说明文字 | `xtask/src/spec.rs` 里该文件的 `header(...)` 描述；`spec/README.md` 的规则摘要 | ✋ |
| 文档站对应页的正文 | 见下方「源码 → 文档页」 | ✋ |
| 文档站规范页的规则摘要表 | `reference/spec.md`「规则摘要」 | ✋ |
| **下游影响评估** | 见第三节，**必做** | ✋ |
| CHANGELOG「未发布」 | 写清旧行为、新行为、谁会受影响 | ✋ |

### C. 公开 API（签名、类型、字段、枚举变体、serde 拼写）

先走技能 `api-contract` 定版本位，再过：

| 连带项 | 怎么做 | 守卫 |
|---|---|---|
| 从外部调用的集成测试 | `tests/smoke.rs` 加一条 | ✋（缺 builder 时下游直接编译失败） |
| 示例程序 | `crates/ai-profile/examples/*.rs` | 🛡️ CI 编译 examples |
| 文档站 API 页、`guide/frontend.md` 的 TS 类型 | 手改 | ✋ |
| 公开 AI 接入文件 | 文档站 `docs/public/ai/ai-profile-integration.md` —— 外部 AI 照它写代码，API 变了它必须跟上 | ✋ |
| Context7 规则 | 文档站 `context7.json` 的 `rules` | ✋ |
| 新增的纯函数要不要进多语言规范 | 能写成「输入 → 输出」的就在 `xtask/src/spec.rs` 加用例，并登记到 `spec/README.md` 与 `reference/spec.md` 的实现范围表 | ✋ |
| 下游代码 | 逐个改、同一提交里改（`downstream-sync`） | ✋ |

### D. ai.profile 协议

走技能 `protocol-evolution`；另外 `gen-spec` 会刷新 `ai_profile.json`，文档站改 `api/protocol.md`。

### E. 发版

走技能 `crate-release`，发完走 `downstream-sync`。本表在发版时的额外要求：
「未发布」里每一条改动，A–D 的 ✋ 项都要确认已做 —— 发版是最后一道能补的关口。

## 二、源码 → 文档页

| 源码 | 文档站页面 |
|---|---|
| `preset/*.rs` | `api/preset.md`、`api/catalog.md`、`reference/providers.md`（生成）、`reference/add-provider.md` |
| `endpoint.rs`、`model_filter.rs` | `api/endpoint.md` |
| `client/mod.rs` | `api/verify.md`、`reference/errors.md`、`reference/spec.md`「测试连接的网络层」 |
| `error.rs` | `reference/errors.md`、`api/verify.md` |
| `limits.rs` | `api/limits.md` |
| `history.rs` | `api/history.md` |
| `protocol.rs` | `api/protocol.md`、`guide/frontend.md` |
| `media/*` | `api/media.md` |
| 公开 API 整体 | `guide/quick-start.md`、`guide/cookbook.md`、`public/ai/ai-profile-integration.md` |
| `spec/` | `reference/spec.md`、`public/spec/`（`pnpm sync-spec`，**只在发版后**） |

增量同步的机制见 sigil 的技能 `docs-management`：`.docs-meta.json` 的 `lastSyncCommit` 起算 diff，按本表路由。

## 三、行为变更评估（B 类必做）

`api-contract` 只判断**接口**有没有破坏。行为变更更危险：编译照过、测试照过，下游的结果变了。

1. **列出变了什么**：`cargo xtask gen-spec` 后看 `git diff spec/`，变了的期望值就是完整清单；
   没有用例覆盖的函数，自己写出「旧输入 → 旧输出 / 新输出」
2. **查谁在用**：对 `docs/downstream.md`「已接入」的每个项目，搜被改函数的调用点
   （如 `rg "clean_fetched_models|is_chat_model_id" <下游>/src-tauri/src`）
3. **逐个判断**：结果变化对它是修复、无感、还是需要应用侧配合（数据迁移、界面文案、存量配置）
4. **写下结论**：CHANGELOG 里写「谁会受影响、要不要改代码」；需要应用侧配合的，
   在 `docs/downstream.md` 的备注里登记，升级该下游时照做

先例：

| 改动 | 影响 | 应用侧配合 |
|---|---|---|
| Anthropic 协议自动补 `/v1`（0.1.1） | 带路径、无版本段的 Anthropic 地址请求目标变了 | onestop 写 v16 迁移，给这类存量地址钉上 `#` |
| 模型清洗补生图 / 视频 / 配音特征词（0.1.2） | `models` 变少、`dropped_models` 变多 | 无，全是改进：sigil / reeve / knowledge_base 对话下拉变干净；onestop 把 `dropped_models` 接在对话模型后面；story_loom 在生图 / 视频 / 配音场景**把 `dropped_models` 排最前**，这些模型从埋在对话模型里变成排到顶部 |

## 四、时序：未发布期间 vs 发版时

- crate 仓库的 `spec/` 随代码走，**头里的 `crateVersion` 在升版本号之前仍是旧值**
- 文档站的 `public/spec/v<版本>/` 是已发布版本的冻结存档，`sync-spec` 发现内容不同会拒绝 ——
  所以 **`pnpm sync-spec` 只在发版升版本号之后跑**
- 文档站里引用新用例文件（下载链接、实现范围表）的改动，要等发版同步后 `pnpm check` 才会全绿；
  这类文档提交可以先做好放在本地，随发版一起推

## 五、守卫一览

| 守卫 | 位置 | 管什么 |
|---|---|---|
| `providers_md_in_sync` | crate | 服务商清单文档与预置一致 |
| `spec_files_in_sync` | xtask | `spec/` 与代码一致（含版本号） |
| `non_chat_presets_are_filtered` | crate（`--all-features`） | 预置与模型清洗特征词双向一致 |
| `readme_preset_counts_match` | xtask | README 的预置数量 |
| `service_config_builder_is_usable_from_outside` 等 | `tests/smoke.rs` | 从外部能否调用公开 API |
| `pnpm check`（`check-links` + `check-facts`） | 文档站仓库 | 站内链接；首页 / 介绍页数量、各处写死的当前版本号 |
| `sync-spec` 冻结保护 | 文档站仓库 | 已发布版本的规范存档不被改写 |

新发现一处「靠人记得」的漂移时，优先**消灭它**（别写死数字），其次**加守卫**，最后才是往上面的 ✋ 表里加一行。
