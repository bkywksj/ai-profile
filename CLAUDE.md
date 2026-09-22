# CLAUDE.md — ai-profile

## 语言设置

**必须使用中文**与用户对话。

---

## 🔴 这是库项目，不是应用项目

> 本仓库与同级的 sigil / reeve / story_loom 是**不同类型的项目**。
> 那些是 Tauri 桌面应用，本仓库是**发布到 crates.io / npm 的公开库**。
> 不要套用应用项目的约定（三层架构、Tauri Command、Capabilities、窗口管理…都不适用）。

| 维度 | 本项目 |
|---|---|
| 形态 | Rust crate（+ 第二阶段的 npm 包） |
| 消费方 | sigil / reeve / knowledge_base / tauri-cc / StoryLoom |
| 可见性 | **公开开源**（github1 / bkywksj） |
| 分发 | crates.io + npm，不打安装包、无 CI 签名构建 |
| License | MIT |

### 与应用项目最大的差别：`pub` 即契约

改一个 `pub` 字段就可能 break 五个下游。**任何公开 API 的改动都要先问：这是 patch 还是 major？**

---

## 🔴 版本化策略（每次改动前必查）

| 改动 | 版本位 | 理由 |
|---|---|---|
| 改模型 id / 加 provider / 改 base_url | **patch** | 数据更新，不动 API |
| 加 `Kind` 枚举值 / 加 struct 字段 | **minor** | `#[non_exhaustive]` 保证下游不 break |
| 改 / 删 `ProviderPreset` 已有字段 | **major** | 破坏性 |
| 改 `preset.key` | **major** | 🔴 它是存量配置回填的依据，改名会让用户已存的配置失效 |

**所有公开 struct / enum 一律加 `#[non_exhaustive]`** —— 这是把「加字段」从 major 降到
minor 的唯一办法，第一版就要加，事后补本身就是破坏性变更。

---

## 🔴 三条数据铁律（都是踩出来的，不要"优化"掉）

### 1. `base_url` 原样使用，绝不推断版本段

各家不统一：多数 `/v1`、智谱 `/v4`、Gemini 是 `/v1beta/openai`（版本段不在末尾）。
曾经的推断规则需要一个转义符（末尾 `#`）才能表达例外 —— **需要转义符本身就说明规则不对**。

推断错的代价是隐性的：用户照服务商文档填对了，我们悄悄加一段，他只看到 404，
也不知道真正请求的是哪个地址。

> 判据：`join_api_path("https://api.deepseek.com", "chat/completions")`
> 必须返回 `https://api.deepseek.com/chat/completions`，**不补 /v1**。
> 有测试 `join_api_path_never_infers_version_segment` 守着。

### 2. 默认 model 一律选「够用档」而非「最强档」

把旗舰塞进默认值 = 替用户做了一个他没同意的花钱决定。最强档留在 `models[]` 里随时可选。

### 3. 模型清单清洗用排除法，不用白名单

厂商上新速度远快于我们更新特征词。白名单必然把新模型误藏 —— 而「藏起来」这个失败模式
对用户不可见（他不知道本该有这一条），比「多留几条让他自己判断」糟糕得多。

全被滤光时**原样返回去重后的清单**：说明特征词在这个端点上判错了，
宁可把原始清单摆给用户，也不能给空下拉。

---

## 🔴 错误必须结构化，不用 `Err(String)`

调用方要靠错误类型做出「一键修正」这类动作。返回字符串 = UI 只能显示一行红字。

```rust
pub enum VerifyError {
    AuthFailed { detail: String },                                  // → 聚焦密钥框
    NotFound { requested_url: String, suggested_url: Option<String> }, // → 一键补 /v1
    Unreachable { proxy_hint: bool },                               // → 跳网络设置
    ModelNotFound { available: Vec<String> },                       // → 展开可用清单
    ProtocolMismatch { expect: Protocol },                          // → 切 Claude Code 档
    MissingExtraField { key: String },                              // → 验证前就禁用按钮
}
```

**加错误变体前先想清楚：调用方拿到它能做什么动作？** 做不出动作的变体不该加。

---

## 目录结构

```
crates/ai-profile/src/
├── kind.rs           Kind 枚举 + feature gate
├── error.rs          结构化错误
├── preset/
│   ├── mod.rs        ProviderPreset / ModelOption / ExtraField
│   ├── vendor.rs     vendor_id 聚合（服务商目录用）
│   ├── chat.rs       🔴 按 kind 分文件：加一家只碰一个文件
│   ├── image.rs      ⏸ feature = "image"
│   ├── video.rs      ⏸ feature = "video"
│   └── tts.rs        ⏸ feature = "tts"
├── protocol.rs       ai.profile 解析 / 生成
├── endpoint.rs       join_api_path / join_chat_endpoint
├── model_filter.rs   拉回清单的清洗
└── client/           feature = "client"
    ├── verify.rs     零成本验证（所有 kind 共用）
    └── chat.rs       dry_run + stream
docs/
├── providers.md      人读的服务商清单（由测试校验与代码一致）
├── versioning.md     版本化策略
└── prototypes/       交互原型
xtask/                cargo xtask probe —— 手动探活，不进 CI
```

---

## 加一家 provider 的完整流程

1. 在 `preset/<kind>.rs` 加一条 `ProviderPreset`
2. `base_url` **照抄服务商文档原文**，含版本段、不含端点后缀
3. `model` 选够用档；`models[]` 只放**核对过**的 id
4. 填 `verified_at`（YYYY-MM-DD）—— 没实际调通过就留 `None`
5. `vendor_id`：已有厂商用同一个（如硅基流动的 chat/image 共用 `siliconflow`）
6. 跑 `cargo test`，七个守卫测试必须全绿
7. 更新 `docs/providers.md`（或跑生成命令）
8. 版本按上方策略 bump —— 加 provider 是 **patch**

---

## 常用命令

```bash
cargo test --workspace          # 含七个守卫测试
cargo clippy --workspace --all-targets
cargo fmt --check
cargo xtask probe               # 🔴 手动探活，打所有预置端点；不进 CI
```

> 🔴 **`xtask probe` 绝不能进 CI** —— 每次 PR 都去打各家服务商的端点，
> 既是对人家的滥用，也会因为网络波动让 CI 随机红。

---

## 🔴 守卫测试（七个，不能删）

| 测试 | 防什么 |
|---|---|
| `preset_groups_are_contiguous` | 同组不连续会切出重复的分组标题 |
| `preset_base_urls_are_well_formed` | base_url 漏版本段 = 下游 404 |
| `preset_keys_are_unique` | key 重复会让回填逻辑静默错乱 |
| `vendor_ids_consistent` | 同 vendor_id 的 host 必须一致，否则目录会把两家并成一张卡 |
| `endpoint_never_infers_version` | 防止有人「好心」把版本段推断加回来 |
| `protocol_roundtrip` | ai.profile 解析→生成→解析 不丢字段 |
| `providers_md_in_sync` | 防止文档变成又一份会漂移的副本 |

---

## 文件编码

- 源码 / 配置统一 **UTF-8 无 BOM**
- 中文注释、文档不允许乱码
- 提交前跑一次 `cargo test`；改过注释的人工过一遍中文可读性

---

## 相关文档

- 规划与设计决策：`docs/tasks/active/task-20260922-070704-模型服务公共库规划.md`
- 交互原型：`docs/prototypes/model-service.html`（6 场景，浏览器打开即看）
