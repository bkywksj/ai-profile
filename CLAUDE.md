# CLAUDE.md — ai-profile

## 语言设置

**必须使用中文**与用户对话。

> 本文件只放**每次都要知道**的铁律与索引。具体流程（加模型、改接口、升级下游…）在
> `.claude/skills/` 里，按场景自动触发 —— 见文末「技能索引」。

---

## 🔴 这是库项目，不是应用项目

> 本仓库与同级的 sigil / reeve / story_loom 是**不同类型的项目**。
> 那些是 Tauri 桌面应用，本仓库是**公开的 Rust 库**。
> 不要套用应用项目的约定（三层架构、Tauri Command、Capabilities、窗口管理…都不适用）。

| 维度 | 本项目 |
|---|---|
| 形态 | Rust crate（npm UI 包是后续阶段） |
| 可见性 | **GitHub 主仓公开**（`bkywksj/ai-profile`，下游按提交号拉取）；Gitee / GitCode 私有镜像 |
| 远程 | `github` / `gitee` / `gitcode` 三个都要推；文档站同样三个，**上线由 Gitee 触发**（见下方「仓库与推送」） |
| 分发 | 目前下游按 **git 提交号**引用；三个下游接入、API 稳定后再发 crates.io |
| 下游 | 见 `docs/downstream.md`（**唯一权威清单**，本文件不重复列） |
| License | MIT |

### `pub` 即契约

改一个 `pub` 字段、一个枚举值、一个 serde 线格式，都可能 break 所有下游。
**任何公开 API 或序列化形状的改动，先走技能 `api-contract` 判定版本位。**

---

## 🔴 职责边界（一句话判据）

> **换一个应用接入时，这段逻辑要不要原样再写一遍？**
> 要 → 放 crate；不要（依赖某个应用的存储 / 加密 / 消息结构 / 界面）→ 留在应用。

| 本 crate | 下游应用 |
|---|---|
| provider 预置、模型候选、静态限额 | 配置的增删改与持久化 |
| 协议拼写（`Protocol::as_str/parse`）、默认端点 | 密钥加密与解密 |
| 端点拼接（不推断版本段） | 对话协议适配、SSE 解析、工具调用 |
| 模型清单清洗 | 被动重试的循环（重新发请求） |
| 零成本验证 + 结构化错误 | 真实对话测试（花 token） |
| 历史裁剪、上下文超长识别（`history`） | 各自的消息类型（实现 `HistoryMessage` 两行） |
| 限额分层合并（`TokenLimits::or`） | 表单界面、应用自己的默认值 |
| `ai.profile` 解析与生成 | 存量数据迁移（各家历史包袱） |
| 生图 / 视频 / 配音协议实现（`media`） | 视频任务编排（轮询循环、取消、落盘、进度事件） |
| 公共预置 + 定制接口（`PresetCatalog`、`ProviderPreset::new` 构造器） | **只属于本应用的预置**（合作渠道、内网网关）、按自身能力删减 / 筛选 |

🔴 **某家服务商只有个别应用要用 → 不进 crate**，那个应用用 `PresetCatalog::new().extend(&LOCAL)` 自己加。
协议是通用的，品牌不是：私有条目只换地址和模型名，需要显式协议的用 `with_default_extra`（如 `video_api=newapi`），
crate 代码里不按域名认品牌（2026-09-24 中宇AI智行就是这样撤出去的）。

判不准、或发现下游有重复实现时 → 技能 `crate-boundary`。

---

## 🔴 三条数据铁律（都是踩出来的，不要"优化"掉）

### 1. `base_url` 原样使用，绝不推断版本段

各家不统一：多数 `/v1`、智谱 `/v4`、Gemini 是 `/v1beta/openai`（版本段不在末尾）。
曾经的推断规则需要一个转义符（末尾 `#`）才能表达例外 —— **需要转义符本身就说明规则不对**。

推断错的代价是隐性的：用户照服务商文档填对了，我们悄悄加一段，他只看到 404。

> 判据：`join_api_path("https://api.deepseek.com", "chat/completions")`
> 必须返回 `https://api.deepseek.com/chat/completions`，**不补 /v1**。

### 2. 默认 model 一律选「够用档」而非「最强档」

把旗舰塞进默认值 = 替用户做了一个他没同意的花钱决定。最强档留在 `models[]` 里随时可选。
**官方档与中转档分开判断**：中转站上新滞后，默认一个它还不认的 id，用户第一次对话就报错。

### 3. 模型清单清洗用排除法，不用白名单

厂商上新速度远快于我们更新特征词。白名单必然把新模型误藏 —— 而「藏起来」对用户不可见。
全被滤光时**原样返回去重后的清单**：宁可把原始清单摆给用户，也不能给空下拉。

---

## 🔴 错误必须结构化，不用 `Err(String)`

调用方要靠错误类型做出「一键修正」这类动作。返回字符串 = UI 只能显示一行红字。
`VerifyError` 六个变体各对应一个界面动作（聚焦密钥框 / 一键补地址 / 跳网络设置 /
展开可用清单 / 切协议档 / 验证前禁用按钮）。

**加错误变体前先想清楚：调用方拿到它能做什么动作？** 做不出动作的变体不该加。

---

## 仓库与推送

本 crate 与文档站（同级 `../ai-profile-docs`）各维护三个远程，推送一律走 Sigil `git_push`，逐个 remote 推：

| 仓库 | remote | 平台 / 地址 | 可见性 | 凭据 | 作用 |
|---|---|---|---|---|---|
| 本 crate | `github` | `github.com/bkywksj/ai-profile` | **公开** | `github1` | 🔴 主仓：下游 `Cargo.toml` 按提交号从这里拉，CI 也从这里拉 |
| 本 crate | `gitee` | `gitee.com/bkywksj/ai-profile` | 私有 | `gitee` | 镜像 |
| 本 crate | `gitcode` | `gitcode.com/zhuawashi/ai-profile` | 私有 | `gitcode` | 镜像 |
| 文档站 `../ai-profile-docs` | `github` | `github.com/bkywksj/ai-profile-docs` | 私有 | `github1` | 镜像 |
| 文档站 | `gitee` | `gitee.com/bkywksj/ai-profile-docs` | 私有 | `gitee` | 🔴 **上线触发源**：推到这里才会重新构建部署 |
| 文档站 | `gitcode` | `gitcode.com/zhuawashi/ai-profile-docs` | 私有 | `gitcode` | 镜像 |

- 🔴 crate **先推 `github`**：下游只认 GitHub 的提交号
- 🔴 文档站**必须推 `gitee`**：线上部署由 Gitee 触发，漏了线上不更新
- 🔴 EdgeOne Pages 项目关联的是 **`bkywksj/ai-profile-docs`**，不是本 crate 仓库：安装 `pnpm install`、
  构建 `pnpm build`、输出目录 `docs/.vitepress/dist`、根目录 `/`。关联错成 `ai-profile` 的表现是
  `ERR_PNPM_NO_PKG_MANIFEST No package.json found`（2026-09-23 踩过）
- 细节见技能 `git-workflow`

---

## 目录结构

```
crates/ai-profile/src/
├── lib.rs            模块文档 + 重导出（含 `pub use reqwest` —— 升 reqwest 大版本 = 本 crate major）
├── kind.rs           Kind（按 feature 切分）+ Protocol（as_str / parse / default_base_url）
├── error.rs          VerifyError（结构化，serde tag = code）
├── limits.rs         TokenLimits / LimitSource：User > Endpoint > Preset > 未知，逐字段 or
├── history.rs        历史裁剪（不拆 tool 配对）+ 上下文超长识别 + 被动重试预算
├── endpoint.rs       join_api_path / join_chat_endpoint（不推断版本段）
├── model_filter.rs   拉回清单的清洗（排除法）
├── protocol.rs       ai.profile 解析 / 生成（宽进严出）
├── preset/
│   ├── mod.rs        ProviderPreset / ModelOption / ExtraField + 查询函数 + model_limits
│   ├── chat.rs       🔴 对话预置（26 家）—— 加一家只碰这一个文件
│   ├── vendor.rs     vendor_id 聚合（服务商目录用）
│   └── docgen.rs     生成 docs/providers.md
└── client/mod.rs     feature = "client"：Verifier（零成本验证）+ 诊断纯函数
crates/ai-profile/tests/smoke.rs   从下游视角调用公开 API 的集成测试
xtask/                cargo xtask probe（手动探活）/ gen-docs（生成服务商清单）
docs/
├── downstream.md     🔴 下游登记表
├── providers.md      生成物，勿手改
├── versioning.md     版本位判定
├── tasks/            任务文档（跨会话续跑）
└── prototypes/       交互原型
```

⏸ `image` / `video` / `tts` 三个 feature 已预留枚举值，预置文件尚未创建（story_loom 接入时反向搬入）。

---

## 常用命令

```bash
cargo test -p ai-profile --features client   # 全部测试（开 client 才编译验证相关测试）
cargo test -p ai-profile                     # 不带 client：确认纯数据形态能编
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo xtask gen-docs    # 重新生成 docs/providers.md（改预置后必跑，守卫测试会拦）
cargo xtask probe       # 🔴 手动探活，打所有预置端点；绝不进 CI
```

> 🔴 测试用 `-p ai-profile` 而不是 `--workspace`：xtask 依赖 client feature，
> workspace 级命令会触发 feature 合并，让「不带 client 能否编译」永远测不到。CI 也是这么分的。
>
> 🔴 **`xtask probe` 绝不能进 CI** —— 每次 PR 都打各家端点，既是滥用，也会让 CI 随机红。

---

## 🔴 守卫测试（不能删）

测试共约 60 个，下面这些是**防回归的守卫**，删了等于拆掉一道防线：

| 测试 | 防什么 |
|---|---|
| `preset_groups_are_contiguous` | 同组不连续会切出重复的分组标题 |
| `preset_base_urls_are_well_formed` | base_url 漏版本段 = 下游 404 |
| `preset_keys_are_unique` | key 重复会让回填逻辑静默错乱 |
| `vendor_ids_consistent` | 同 vendor_id 的 host 必须一致 |
| `default_model_is_in_its_own_list` | 默认 model 不在下拉里 = 表单一打开就是「自定义」 |
| `join_api_path_never_infers_version_segment` | 防止有人「好心」把版本段推断加回来 |
| `protocol_spellings_agree` | Protocol 的 serde / as_str / parse 三处拼写必须一致 |
| `default_base_url_has_version_segment` | 默认端点缺版本段 = 404 |
| `protocol_roundtrip` / `cross_app_interop` | ai.profile 解析→生成→解析不丢字段；与别家的写法互通 |
| `accepts_older_version_rejects_newer` | 只拒绝更高版本，老软件的配置必须能导 |
| `parses_tauri_cc_bundle` / `bundle_edge_cases` | 多条打包兼容智码已发出去的写法；OAuth 档案跳过；单条入口不误读打包 |
| `nested_top_provider_wins` / `deepseek_shape_is_parsed` | 真实端点返回形状，防止限额解析退化 |
| `or_merges_field_by_field` | 限额逐字段合并；空的用户设置不能冒充 User |
| `never_starts_with_orphan_tool_result` / `extreme_budget_still_respects_tool_pairing` | 裁剪绝不留下残缺的 tool 配对（发出去必被拒） |
| `oversized_first_user_is_not_forced_back` | 首条消息超大时不强行补回，否则会话永远降不下来 |
| `detects_real_overflow_errors` / `does_not_misfire` | 超长识别：真实报错必须命中；限流、输出上限太大绝不能误判 |
| `providers_md_in_sync` | 防止文档变成又一份会漂移的副本 |
| `service_config_builder_is_usable_from_outside` | 🔴 集成测试：`non_exhaustive` 入参缺 builder 时下游报 E0639 |
| `verifier_is_shareable_and_concurrent` | 🔴 集成测试：`Verifier` 必须 `Send + Sync + 'static` |

---

## 技能索引

| 技能 | 什么时候用 |
|---|---|
| `crate-boundary` | 「这段该放 crate 还是应用」；发现下游有重复实现 |
| `preset-maintenance` | 新模型上线 / 加服务商 / 改默认 model / 填静态限额 |
| `token-limits` | 改限额相关代码；给模型填 context / max_output |
| `api-contract` | 改任何 `pub` 类型、字段、枚举值、serde 线格式；判定版本位 |
| `protocol-evolution` | 改 ai.profile 协议（加字段、多条打包…） |
| `downstream-sync` | crate 推送后升级下游、维护登记表、同步文档站、真实密钥联调 |
| `task-tracker` | 跨会话的多步骤任务（每个下游接入各开一份） |
| `git-workflow` | 提交规范与推送 |

---

## 文件编码

- 源码 / 配置统一 **UTF-8 无 BOM**；中文注释、文档不允许乱码
- 提交前跑一次测试；改过注释的人工过一遍中文可读性

---

## 相关文档

- 下游登记表：`docs/downstream.md`
- 规划与设计决策：`docs/tasks/archive/2026-09/task-20260922-070704-模型服务公共库规划.md`（已归档）
- 文档站：同级 `../ai-profile-docs`（同步记录见本仓库根 `.docs-meta.json`）
- 交互原型：`docs/prototypes/model-service.html`
