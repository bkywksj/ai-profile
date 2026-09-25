---
name: crate-release
description: |
  用于把 ai-profile 发布一个新版本到 crates.io：定版本位、改版本号与 CHANGELOG、跑发版闸门、发布、打 tag、核对 crates.io / docs.rs、交接下游。

  触发场景：
  - 用户说「发布 / 发版 / 发一个新版本 / 发 crates.io」
  - 攒了一批新模型、新服务商或修复，要让下游用 `cargo update` 拿到
  - 公开 API 有变化，要发 minor / major 版本
  - 要轮换或排查 crates.io 发布 token

  触发词：发布、发版、release、crates.io、cargo publish、版本号、打 tag、docs.rs、发布 token、yank
---

# 发布新版本到 crates.io

## 🔴 先记住两件事

1. **crates.io 的版本永久不可撤回**（只能 yank，不能删、不能覆盖）。漏一项检查的代价是补发一个版本，
   而那个坏版本永远留在记录里 —— 0.1.0 就因为漏配 docs.rs 的 feature 只能补发 0.1.1。
2. **`cargo publish` 必须用户当次确认**。本仓库的钩子（`.claude/hooks/publish-guard.cjs`）会拦下它并弹确认；
   **不要换 shell、换写法绕开**。用户确认过一次，只对那一次发布有效。

## 完整路线

```
① 定版本位        技能 api-contract：patch / minor / major
② 改版本号        根 Cargo.toml 的 [workspace.package] version（只有这一处）
③ 写 CHANGELOG    「## [未发布]」下的内容移到新版本标题下，写面向使用者的说明；
                  逐条对照技能 change-impact，确认每条改动的连带更新（✋ 项）都已做 —— 发版是最后能补的关口
④ 发版闸门        见下方清单，全部读输出实体确认通过
⑤ 提交 + 推送     三个远程（技能 git-workflow），github 先推
⑥ 发布            用户确认后 cargo publish -p ai-profile
⑦ 打 tag          vX.Y.Z，推到三个远程
⑧ 核对            crates.io 版本与元数据、docs.rs 构建出 client / media 模块
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

## ⑥ 发布

```bash
cargo publish -p ai-profile
```

- 看到 `Published ai-profile vX.Y.Z at registry crates-io` 才算成功
- 钩子会弹确认；用户同意后执行。**不要**用 `cmd /c`、PowerShell、别的 shell 去绕钩子

## 发布 token

| 项 | 值 |
|---|---|
| 实际生效的凭据 | `~/.cargo/credentials.toml`（`cargo login` 写入，本机所有项目共用） |
| 本仓库留存 | `.secrets/crates-io.token`（`/.secrets/` 已在 `.gitignore`，不进 git、不进 crate 包） |
| 权限范围 | 只能发 `ai-profile`；只有 `publish-new` + `publish-update`；永不过期 |
| 账号 | crates.io 用 GitHub `bkywksj` 登录，邮箱已验证 |

**轮换**（怀疑泄露、或 token 在对话 / 日志里出现过）：

1. crates.io → Account Settings → API Tokens：吊销旧的，按上表范围建新的
2. 新 token 写进 `.secrets/crates-io.token`
3. `cargo login < .secrets/crates-io.token`
4. 🔴 不要在对话里贴 token 原文，不要 `cat` 它，不要写进任何会提交的文件

报 `401` / `403`：token 过期或被吊销 → 按上面轮换。报 `email not verified`：去 crates.io 验证邮箱。

## ⑦ 打 tag

```bash
git tag -a vX.Y.Z -m "ai-profile X.Y.Z：一句话说明" <发布时的提交>
```

用 Sigil `git_push`（传 `tag` 参数）推到 `github` / `gitee` / `gitcode`。tag 要指向**发布时那个提交**，
crates.io 包里的 `.cargo_vcs_info.json` 记着它。

## ⑧ 核对

- `https://crates.io/api/v1/crates/ai-profile`：`max_version`、`rust_version`、features、`repository`
- `https://docs.rs/ai-profile/X.Y.Z/ai_profile/`：通常 5–20 分钟构建完，首页要能看到 `client` 与 `media` 模块

## 发错了怎么办

- **yank** 能阻止新项目选中这个版本，但已经锁定它的项目照样能下载 —— 它不是删除
- 正确做法永远是**补发一个修正版本**，并在 CHANGELOG 里写清楚上一版的问题
- 真出了安全问题：yank + 补发 + 通知下游，三件事同时做

## 常见错误

| 错误 | 后果 | 对策 |
|---|---|---|
| 只跑 `cargo check` 就发 | 测试代码编译不过、行为回归都查不出来 | 闸门跑全量测试 |
| 没等 CI 的 msrv 任务就发 | 声明的最低版本其实编不过 | 推送后等 CI 绿 |
| 忘了 docs.rs 配置 | 文档站上整块 API 消失 | 闸门里检查包内 `Cargo.toml` |
| tag 打在发布之后的提交上 | tag 与 crates.io 包内容对不上 | tag 指向发布时的提交 |
| 发完不交接 | 下游不知道有新版本 | 走 `downstream-sync` |
