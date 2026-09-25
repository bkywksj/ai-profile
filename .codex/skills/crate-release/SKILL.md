---
name: crate-release
description: |
  用于把 ai-profile 发布一个新版本到 crates.io：定版本位、改版本号与 CHANGELOG、跑发版闸门、经 Sigil 发布、打 tag、核对 crates.io / docs.rs、交接下游。

  触发场景：
  - 用户说「发布 / 发版 / 发一个新版本 / 发 crates.io」
  - 攒了一批新模型、新服务商或修复，要让下游用 `cargo update` 拿到
  - 公开 API 有变化，要发 minor / major 版本
  - 要轮换或排查 crates.io 发布 token（Sigil 金库里的 cargo_registry 凭据）

  触发词：发布、发版、release、crates.io、cargo publish、crates_publish、crates_info、版本号、打 tag、docs.rs、发布 token、yank
---

# 发布新版本到 crates.io

## 🔴 先记住三件事

1. **crates.io 的版本永久不可撤回**（只能 yank，不能删、不能覆盖）。漏一项检查的代价是补发一个版本，
   而那个坏版本永远留在记录里 —— 0.1.0 就因为漏配 docs.rs 的 feature 只能补发 0.1.1。
2. **发布走 Sigil 的 `mcp__sigil__crates_publish`，不直接跑 `cargo publish`**。发布 token 只在 Sigil 金库里，
   AI 只传凭据名、拿不到明文；确认在 **Sigil 桌面端的审批弹窗**里做，弹窗写着「包名 + 版本号」。
   Sigil 正式版还没有这个能力时，见下方「过渡期」。
3. **Sigil 上传时不编译**（带 token 的 cargo 进程若编译，依赖的 build.rs 就能读到 token），
   编译验证全靠你在**发布提交上**跑的 `cargo package`。Sigil 会比对两次打包的 sha256，对不上就拒绝上传 ——
   所以 `cargo package` 之后**不要再提交任何东西**再发（哪怕一行文档）。

## 完整路线

```
① 定版本位        技能 api-contract：patch / minor / major
② 改版本号        根 Cargo.toml 的 [workspace.package] version（只有这一处）
③ 写 CHANGELOG    「## [未发布]」下的内容移到新版本标题下，写面向使用者的说明；
                  逐条对照技能 change-impact，确认每条改动的连带更新（✋ 项）都已做 —— 发版是最后能补的关口
④ 发版闸门        见下方清单，全部读输出实体确认通过
⑤ 提交 + 推送     三个远程（技能 git-workflow），github 先推；等 CI 全绿
⑥ 发布            在发布提交上 cargo package → crates_publish 预检 → crates_publish 正式发布（Sigil 弹窗确认）
⑦ 打 tag          vX.Y.Z 指向发布提交，推到三个远程
⑧ 核对            crates_info 查版本与 docs.rs；docs.rs 首页要有 client / media 模块
⑨ 交接            技能 downstream-sync：文档站「更新日志」、升级下游、回填登记表
```

## ① 定版本位（0.x 阶段）

0.x 期间 Cargo 把 **minor 当作破坏性版本**：`0.1` 的下游不会自动升到 `0.2`。

| 改动 | 版本位 | 例 |
|---|---|---|
| 新模型、新服务商、改默认模型、修 bug、文档 | patch `0.1.x` | 0.1.1 |
| 新增公开 API（新函数、给 `non_exhaustive` 结构加字段） | patch 也行，**写进 CHANGELOG** | |
| 删改公开 API、改 serde 线格式、reqwest 升大版本 | minor `0.x.0` | 0.2.0 |

拿不准就走 `api-contract`，它有完整判定表。

## ④ 发版闸门（每条都要读输出实体，不看退出码）

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p ai-profile                                    # 默认 feature
cargo test -p ai-profile --no-default-features --features chat
cargo test -p ai-profile --features client
cargo test --workspace --all-features                       # 含 wiremock 模拟服务端测试
RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p ai-profile --all-features --no-deps
RUSTDOCFLAGS="-D warnings" cargo doc -p ai-profile --no-deps   # 默认 feature：指向 client / media 的文档链接在这里会断
cargo xtask gen-docs && git diff --exit-code docs/providers.md
cargo xtask gen-spec                                        # 🔴 改版本号后必跑，见下
cargo package -p ai-profile                                 # 打包 + 编译验证
```

🔴 `spec/` 每个文件头都带 `crateVersion`，**改了版本号就一定会变**：先改版本号、再 `gen-spec`、
把 `spec/` 的变化和版本号放进同一个 release 提交。漏了的话守卫测试 `spec_files_in_sync` 会红 ——
这是故意的，保证公开出去的用例永远标着产出它的版本。发布后还要把 `spec/` 同步到文档站（技能 `downstream-sync`）：
`pnpm sync-spec` 会同时更新 `/spec/`（最新）并新建 `/spec/v<新版本>/` 存档，旧版本目录不动 ——
其他语言的实现锁的就是这些存档地址，**已发布版本的目录绝不能手改或删除**。
同步后在文档站跑 `pnpm check`：`check-facts` 会核对版本策略页「当前版本」、规范页里的示例版本号 ——
发了新版这几处必须跟着改，它会逐条点名。

再做两项（0.1.0 就是漏了它们）：

- **打包后单独跑测试**：`cargo package` 的产物解到临时目录，在里面 `cargo test --all-features`。
  下载包里没有仓库的 `docs/`，依赖它的测试必须能跳过而不是报错
- **确认包内元数据**：解出的 `Cargo.toml` 里有 `[package.metadata.docs.rs] all-features = true`、
  `rust-version`、`repository`；`cargo package --list` 里有 `LICENSE`、`README.md`，**没有** `.secrets/`

MSRV 由 CI 的 `msrv` 任务用 1.88 真编（本机通常没装 1.88）—— **推送后等 CI 绿了再发布**。

## ⑥ 发布（走 Sigil）

前提：⑤ 已推送、CI 全绿、**工作树干净**、HEAD 就是要发布的那个提交。

**1. 在发布提交上重跑一次打包**（带编译验证，不需要 token）：

```bash
cargo package -p ai-profile
```

它留下的 `target/package/ai-profile-X.Y.Z.crate` 就是 Sigil 要比对的「闸门产物」。
包里的 `.cargo_vcs_info.json` 记着提交号，所以**必须在发布提交上跑**：④ 里那次如果是在提交之前 / 之后又有新提交，
哈希就对不上。实例：0.1.3 的闸门产物是 17:45 打的，之后仓库又提交了 README，预检立刻报「与闸门产物不一致」。

**2. 预检**（不上传、不用凭据）：

```
mcp__sigil__crates_publish(
  manifest_dir = "E:\\my\\桌面软件tauri\\ai-profile",   # workspace 根目录
  package      = "ai-profile",
  version      = "X.Y.Z",                               # 必须等于 Cargo.toml 里的版本，否则拒绝
  dry_run      = true
)
```

- 返回里要看到「✓ 与闸门验证过的包一致」；文件清单里有 `LICENSE` / `README.md`，**没有** `.secrets/`
- 预检也会在 Sigil 桌面端弹一次确认（显示「预检 crate 发布（不上传）」），提醒用户点放行

**3. 正式发布**：同样的参数，去掉 `dry_run`，加 `credential_name`：

- 凭据名用 `mcp__sigil__list_credentials` 查 `cargo_registry` 类的那条，**别猜**（不同金库里名字不一样）
- Sigil 桌面端弹窗：动作「发布 crate 到 crates.io」、目标「crates.io · ai-profile X.Y.Z」，附目录与凭据名，
  **由用户在弹窗里确认**
- 返回「✓ 已发布 ai-profile vX.Y.Z」才算成功；返回里还带着这次发布的 HEAD 提交号，⑦ 打 tag 用它

**Sigil 拒绝时怎么办**（它的报错都是人话，照着做）：

| 报错里说的 | 原因 | 处理 |
|---|---|---|
| 版本对不上 | `version` 参数与 Cargo.toml 不符 | 核对 ② 是否已改、参数是否写对 |
| 没找到发版闸门的打包产物 | 没跑第 1 步 | 跑第 1 步 |
| 与闸门验证过的包不一致 | 第 1 步之后又有提交或改动 | 在当前提交上重跑第 1 步 |
| 工作区有未提交的改动 | 工作树不干净 | 提交或还原后再来 |
| 不是 rustup 官方渠道 / 项目级 cargo 配置里有可能劫持发布的设置 | `rust-toolchain.toml` 指向自定义路径，或 `.cargo/config.toml` 里有 `http` / `registry` / `source` / `credential-provider` 等 | 移走这些设置（本仓库的 `.cargo/config.toml` 只有 `[alias]`，不受影响） |
| 该版本已经在 crates.io 上了 | 版本号复用 | 递增版本号 |
| token 无效 / 权限不够 / 邮箱未验证 | 凭据或账号问题 | 见下方「发布 token」 |
| 已有一个 crate 发布在进行中 | 同一时间只许一个发布 | 等上一个结束 |

### 钩子会拦什么

| 你做的事 | 会发生什么 |
|---|---|
| 调 `mcp__sigil__crates_publish` / `crates_info` | MCP 工具调用，**不经过任何 shell 钩子**；确认在 Sigil 弹窗里做 |
| 在本仓库会话里直接跑 `cargo publish`（不带 `--dry-run`） | `.claude/hooks/publish-guard.cjs` 弹确认（过渡期仍是「询问」；迁移完成后改为拒绝） |
| 在 sigil 仓库会话里直接跑 `cargo publish` | sigil 的 `pre-tool-use.cjs` 直接拦，Bash / PowerShell 都查 |
| 在 sigil 会话里 grep「cargo publish」、或提交信息里写了这几个字 | 同样被 sigil 钩子拦 —— 它按命令文本匹配，分不清是不是真发布。搜索用 Grep 工具，提交信息写进文件用 `git commit -F` |

没有明确授权时，**不要**换 `cmd /c`、换 shell、写脚本去绕钩子。

### 过渡期（ToolSearch 查不到 `mcp__sigil__crates_publish` 时）

查不到有两种原因，先分清再动手，别反复重试：

1. **正在运行的 Sigil 还没有这个能力**（2.0.0 及之前的正式版都没有）→ 等 Sigil 新版
2. **金库里没有 `cargo_registry` 类凭据** → 工具按凭据自动隐藏（连 `crates_info` 也看不到）；
   请用户在 Sigil「新建凭据」里选「crates.io 发布（cargo_registry）」，填 token 后点「测试」

急着发、又确实用不了 Sigil 时，只剩两条路（**都依赖 `~/.cargo/credentials.toml` 里的明文 token**，
迁移完成、明文删掉之后就走不通了 —— 到那时删掉本节）：

- **用户自己在输入框执行** `! cd <本仓库> && cargo publish -p ai-profile`：用户亲手执行的命令不走 AI 钩子
- **用户当次明确授权后由 AI 执行**（0.1.3 就是这样发的）：必须是用户**这一次**说了「你绕过就行 / 授权」这类话，
  并在回复里明说「钩子没改，下次照样拦」

注意：走这两条时 cargo 会先**带着 token 编译一遍**，依赖的 build.rs 能读到它 —— 这正是改走 Sigil 的原因。

## 发布 token

| 项 | 值 |
|---|---|
| 实际生效的凭据 | Sigil 金库里的 `cargo_registry` 类凭据：token + 可选的「允许发布」名单（留空 = 以 token 在 crates.io 上的权限为准） |
| 本机明文 | 迁移完成后应**不存在**：`~/.cargo/credentials.toml` 里的 token 与 `.secrets/crates-io.token` 都要删（过渡期可能还在，见上） |
| 权限范围 | 只能发 `ai-profile`；只有 `publish-new` + `publish-update`（刻意不给 yank / change-owners） |
| 账号 | crates.io 用 GitHub `bkywksj` 登录，邮箱已验证 |

**轮换**（怀疑泄露、或 token 在对话 / 日志里出现过）：

1. crates.io → Account Settings → API Tokens：吊销旧的，按上表范围建新的（建议设过期时间）
2. Sigil → 编辑这条凭据 → 重新填 token（token 加密不可回填；编辑时 token 与名单都要重填）→ 点「测试」
3. 🔴 不要在对话里贴 token 原文；**不再**写 `.secrets/`、**不再** `cargo login`

「测试」的判定：「token 有效」= 通过；「token 无效、已吊销或已过期」= 回到第 1 步重建。
它只能判 token 有没有效，**判不出 scope 够不够** —— scope 不足会在真正发布时由 crates.io 报出来。

## ⑦ 打 tag

```bash
git tag -a vX.Y.Z -m "ai-profile X.Y.Z：一句话说明" <发布时的提交>
```

用 Sigil `git_push`（传 `tag` 参数）推到 `github` / `gitee` / `gitcode`。tag 要指向**发布时那个提交**
（`crates_publish` 返回里的 HEAD），crates.io 包里的 `.cargo_vcs_info.json` 记着它。

## ⑧ 核对

- **`mcp__sigil__crates_info(name = "ai-profile", version = "X.Y.Z")`**：一次拿到最新版本、`rust_version`、
  `repository`、这个版本是否已发布 / 是否 yank、docs.rs 是否构建成功。
  刚发布时 docs.rs 显示「尚无记录」属正常（通常 5–20 分钟），过会儿再查
- `crates_info` 不返回 features；要核对就看 `https://crates.io/api/v1/crates/ai-profile` 的 `features`
- docs.rs 首页要能看到 `client` 与 `media` 模块：`https://docs.rs/ai-profile/X.Y.Z/ai_profile/`
  （`crates_info` 只报构建成败，模块齐不齐还得看页面）

## 发错了怎么办

- **yank** 能阻止新项目选中这个版本，但已经锁定它的项目照样能下载 —— 它不是删除
- yank 去 crates.io 网页上操作：发布 token 刻意没给 `yank` scope，Sigil 也不提供 yank 能力
- 正确做法永远是**补发一个修正版本**，并在 CHANGELOG 里写清楚上一版的问题
- 真出了安全问题：yank + 补发 + 通知下游，三件事同时做

## 常见错误

| 错误 | 后果 | 对策 |
|---|---|---|
| 只跑 `cargo check` 就发 | 测试代码编译不过、行为回归都查不出来 | 闸门跑全量测试 |
| 没等 CI 的 msrv 任务就发 | 声明的最低版本其实编不过 | 推送后等 CI 绿 |
| 忘了 docs.rs 配置 | 文档站上整块 API 消失 | 闸门里检查包内 `Cargo.toml` |
| `cargo package` 之后又提交了东西才发 | Sigil 拒绝上传「与闸门验证过的包不一致」 | 在当前提交上重跑 `cargo package` |
| 直接跑 `cargo publish` | 被钩子拦；迁移完成后本机也没有 token | 走 `crates_publish` |
| 凭据名靠猜 | 报「凭据 'xxx' 不存在」 | 用 `list_credentials` 查 `cargo_registry` 类的那条 |
| tag 打在发布之后的提交上 | tag 与 crates.io 包内容对不上 | tag 指向 `crates_publish` 返回里的 HEAD |
| 发完不交接 | 下游不知道有新版本 | 走 `downstream-sync` |
