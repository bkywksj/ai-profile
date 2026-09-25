---
name: downstream-sync
description: |
  用于 ai-profile 推送之后把改动送到下游：同步文档站、升级已接入项目、维护下游登记表、跑真实密钥联调。

  触发场景：
  - crate 刚发布了新版本（新模型、修复、API 变更），要让 sigil / reeve 等用上
  - 开始接入一个新的下游项目，或查某个下游落后了多少
  - 同步文档站 ai-profile-docs（providers 清单、API 页）
  - 需要用真实 API 密钥验证端点行为

  触发词：下游、升级下游、同步下游、接入、版本号、cargo update、downstream、登记表、文档站同步、联调、真实密钥
---

# 下游同步

## 唯一出发点

**跨项目的模型服务更新一律从本仓库出发**，下游只做「升级依赖 + 应用侧适配」。
下游状态的唯一权威清单是 `docs/downstream.md` —— 本技能讲流程，那张表记状态。

## 一次改动的完整路线

```
① 在本仓库改            测试 + 连带更新（技能 change-impact）+ CHANGELOG → 提交 → 推送 → 发新版本（技能 crate-release）
② 同步文档站            ai-profile-docs（见下，含「更新日志」页）→ 回写 .docs-meta.json
③ 读 docs/downstream.md  「已接入」表：哪些项目、各引用哪个版本
④ 逐个升级已接入的下游    改 Cargo.toml 的 version + cargo update -p → 全量测试 → 只提交依赖文件
⑤ 回填登记表            改「当前引用」「最后同步」→ 提交本仓库
```

未接入的下游不动；但如果这次改动会影响它将来的接入（例如改了协议），在「计划接入」表的
「接入时必须处理」一栏补一句。

## ① 推送本仓库

凭据走 Sigil `git_push`，**三个远程都推**：`github`（`github1`，公开主仓，下游与 CI 从这里拉，必须先推）、
`gitee`（`gitee`）、`gitcode`（`gitcode`）。远程清单见技能 `git-workflow`。

## ② 同步文档站

```bash
# 在 ../ai-profile-docs
pnpm sync-providers        # 把 docs/providers.md 同步到 reference/providers.md
pnpm sync-spec             # 🔴 只在发版升版本号之后跑：把 spec/ 同步到 public/spec/ 并新建 v<版本>/ 存档
pnpm build                 # 必须构建通过
pnpm check                 # 站内链接 + 手写文档里的数量 / 当前版本号与代码一致（check-links + check-facts）
```

🔴 `sync-spec` 的时机：crate 改了规则但还没升版本号时，`spec/` 头里仍是旧版本号；
这时同步会改写已发布版本的冻结存档，脚本会拒绝并报错。**别删存档目录绕过** ——
先发版（技能 `crate-release`），再来同步。未发布期间，文档站里引用新用例的改动先在本地提交、随发版一起推。

提交后文档仓库同样**三个远程都推**（`github` / `gitee` / `gitcode`）。
🔴 **上线由 Gitee 触发**：只推 GitHub 的话线上文档不会更新。

API / 限额 / 协议有变 → 同步改对应页（`api/*.md`、`guide/frontend.md` 的 TS 类型）。
发了新版本 → 在 `reference/changelog.md` 加一节（面向使用者：带来了什么、升级要不要改代码），导航栏的版本号一起改。
**同步完回到本仓库更新 `.docs-meta.json`**：`lastSyncCommit` + 追加一条 `updateHistory`。
漏了这步，下次增量更新会从错误的起点算 diff（曾经停在建仓那次一整天）。

## ④ 升级一个已接入的下游

### 1. 改版本号：`Cargo.toml` 与 `Cargo.lock` 两处都动

```bash
# 把 Cargo.toml 里 ai-profile 的 version 改成新版本（补丁版本也改），再只更新这一个包
cargo update -p ai-profile        # 在该项目 Cargo.lock 所在目录跑
git diff Cargo.lock               # 🔴 确认只动了 ai-profile 一个包（version + checksum 两行）
```

🔴 **补丁版本也要改 `Cargo.toml` 的最低版本**，不能只跑 `cargo update`：

- 下游一旦用上新版本才有的接口（如 0.1.3 的 `ProviderPreset::endpoint()`），最低版本还写着旧值，
  而某个分支 / 另一台机器的 `Cargo.lock` 仍锁在旧版本 —— cargo 认为旧版本满足要求、不会自动升级，
  直接编译失败。最低版本写成新值，cargo 会发现锁文件不满足而自己更新它
- 登记表「当前引用」记的是 `Cargo.toml` 的值 —— 只动锁文件的话，表和实际对不上，下次判断谁落后会看错

跨小版本（0.1 → 0.2，有破坏性变更）同样改 `version`，再按 CHANGELOG 改代码。

有的项目**不止一个 `Cargo.toml`** 引用 ai-profile（reeve 两处、必须同值），以登记表「引用方式」一列为准。

### 2. 全量测试

| 项目 | 工作目录 | 命令 | 备注 |
|---|---|---|---|
| sigil | `src-tauri` | `cargo test --workspace` + 仓库根 `npx tsc --noEmit` | 🔴 必须 PowerShell（SQLCipher 要原生 Perl）；`--workspace` 不能省 |
| reeve | `src-tauri` | `cargo test --workspace` | 两处 `Cargo.toml` 同值 |
| knowledge_base | `src-tauri` | `cargo test --workspace` | |
| onestop | `src-tauri` | `cargo test --workspace` | 仓库在 `E:/my/backend_tauri/onestop` |
| story_loom | `src-tauri` | `cargo test --workspace --no-run` | 本机 lib 单测有 WebView2 入口崩溃（环境问题），只能验证测试代码编译通过 |

各项目的特有细节以它自己的 `ai-profile-integration` 技能为准。几家可以并行跑（各自独立的 target 目录）。
判定通过要读 `test result:` 行的实体，别只看退出码。

### 3. 提交：只提交这几个依赖文件

🔴 下游工作区里常有**别的会话的未提交改动**（升 0.1.3 时 sigil 有 46 个）。提交前先 `git status -s` 看一眼，
然后**按路径**只 add `Cargo.toml`（可能不止一个）和 `Cargo.lock`，`git diff --cached --name-only` 确认后再提交。
禁止 `git add -A` / `git add .`，也不要 stash 别人的改动。

提交说明写三件事：

- 「只升依赖，零代码改动」（如果是）—— 这正是 crate 该达到的效果
- 🔴 **用户能感知的变化**：这次升级让用户在界面上看到什么不同（如 0.1.3「Anthropic 官方档不填地址也能获取模型」
  「地址填到网站根目录时报地址不对，不再假成功」）。行为变更靠测试证明不了，下游有人要照这句去实机验证；
  同一句话也写进登记表的备注
- 版本号范围（`0.1.2 → 0.1.3`）

- 线格式 / API 有破坏性变更 → 下游要跟着改代码，**同一个提交里改**，别留中间态
- 下游仓库的推送、发版遵循**该项目自己的**规则（sigil 推 Gitee + GitHub，发版是另一套流程），
  并先征得用户同意

## 接入一个新下游

1. 在本仓库 `docs/tasks/active/` 建任务文档（技能 `task-tracker`），写清接入范围
2. 读 `docs/downstream.md` 该项目的「接入时必须处理」—— 尤其 **存量地址修正**：
   reeve / knowledge_base 旧逻辑自动补 `/v1`，已发布过，必须先用它们自己的
   旧规则把存量 `base_url` 规范好，再切换到 crate
3. 按技能 `crate-boundary` 删掉该项目的本地副本（不是「不再调用」，是删掉）
4. 给该项目加 `ai-profile-integration` 技能：模板在 `docs/downstream/integration-skill-template.md`
5. 完成后把它从「计划接入」移到「已接入」，任务文档归档

## 真实密钥联调

有些行为只有真实端点能证明（`/models` 报不报限额、裁剪后的 tool 历史服务端收不收）。

- 🔴 密钥**只从环境变量读**（如 `SIGIL_TEST_DEEPSEEK_KEY`），绝不写进文件、仓库、提交说明
- 测试一律 `#[ignore]`，CI 永远不跑（滥用对方接口 + 花钱 + 随网络波动随机红）
- 跑法：设环境变量 → `cargo test <过滤> -- --ignored --nocapture` → 立刻清掉环境变量
- 用户给过的明文密钥：用完提醒对方作废
- 本仓库的 `cargo xtask probe` 只探地址可达性，**不需要密钥**，也不进 CI

## 常见坑

| 坑 | 表现 | 对策 |
|---|---|---|
| 下游 `tauri dev` 正在跑 | 下游 cargo test 报文件占用（os error 32） | 那是别的会话 / 用户的进程，**不要结束它**；等它关掉或只跑不需重编的检查 |
| 忘了回填登记表 | 下次不知道谁落后 | 流程第 ⑤ 步是必做项 |
| 只改了 crate 没同步文档站 | 文档站的 providers 清单过时 | 流程第 ② 步 |
| 下游用 `--manifest-path` 从根目录调 cargo | sigil 会漏掉 `src-tauri/.cargo/config.toml` 的环境变量，构建中止 | 以 `src-tauri` 为工作目录 |
