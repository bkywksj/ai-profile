---
name: git-workflow
description: |
  用于 ai-profile 库仓库的提交与推送规范（与应用项目的发版流程不同）。

  触发场景：
  - 提交代码、写提交说明
  - 推送到 GitHub
  - 同一次工作同时改了代码、数据、文档，要决定怎么拆提交

  触发词：Git、提交、commit、推送、push、提交说明、拆分提交、CHANGELOG
---

# Git 工作流（库项目）

## 与应用项目的区别

本仓库**没有**打 tag 触发 CI 构建安装包那一套（那是 sigil 的 `release-publish`）。
本 crate 已发布到 crates.io，下游按**版本号**引用（`ai-profile = "0.1"`）。

- 「推送」≠「发布」：推送只是把提交放到三个远程；下游要等**发了新版本**才能 `cargo update` 拿到
- 发新版本走技能 `crate-release`（版本号、CHANGELOG、发版闸门、`cargo publish`、打 tag）

## 提交说明

Conventional Commits，中文正文：

| type | 用于 | scope 示例 |
|---|---|---|
| `feat` | 新能力、新预置 | `preset` / `limits` / `client` / `protocol` |
| `fix` | 修错（含纠正错误断言） | 同上 |
| `refactor` | 不改行为的重构 | |
| `docs` | 文档、CLAUDE.md、技能 | |
| `test` | 只加测试 | |
| `build` / `ci` | 依赖、CI、xtask | |

正文写**为什么**：新模型写 id 的核对来源；改默认值写理由；破坏性变更写迁移方法。
末尾附会话要求的 `Co-Authored-By` 行。

多行说明用 heredoc 传给 `git commit -F -`，别用 PowerShell here-string 进 Bash。

## 拆分

- 功能改动与纯注释 / 文档改动分开提交
- 预置数据变更与 API 变更分开 —— 前者是 patch，后者可能是 minor / major，分开便于判定与回溯
- 逐个 `git add <文件>`，不用 `git add -A` / `git add .`

## 推送：三个远程都要推

本 crate 与文档站各维护 GitHub / Gitee / GitCode 三个远程（2026-09-23 起）：

| 仓库 | remote | 平台 / 地址 | 可见性 | 凭据 | 作用 |
|---|---|---|---|---|---|
| 本 crate | `github` | `github.com/bkywksj/ai-profile` | **公开** | `github1` | 🔴 主仓：crates.io 的 `repository` 指向它，CI 跑在这里 |
| 本 crate | `gitee` | `gitee.com/bkywksj/ai-profile` | 私有 | `gitee` | 镜像 |
| 本 crate | `gitcode` | `gitcode.com/zhuawashi/ai-profile` | 私有 | `gitcode` | 镜像 |
| 文档站 `../ai-profile-docs` | `github` | `github.com/bkywksj/ai-profile-docs` | 私有 | `github1` | 镜像 |
| 文档站 | `gitee` | `gitee.com/bkywksj/ai-profile-docs` | 私有 | `gitee` | 🔴 **上线触发源**：推到这里才会重新构建部署 |
| 文档站 | `gitcode` | `gitcode.com/zhuawashi/ai-profile-docs` | 私有 | `gitcode` | 镜像 |

- 一律走 Sigil `git_push`，**逐个 remote 推**，凭据按上表（`username` 不用传）
- 🔴 **crate 先推 `github`**：CI（含 MSRV 检查）跑在 GitHub，发布前要等它绿；Gitee / GitCode 是镜像，随后补上
- 🔴 **文档站必须推 `gitee`**：上线部署由 Gitee 触发，只推 GitHub 的话线上文档不会更新
- 部署平台（EdgeOne Pages）关联的是 `bkywksj/ai-profile-docs`，**不是**本 crate：
  安装 `pnpm install`、构建 `pnpm build`、输出 `docs/.vitepress/dist`、根目录 `/`。
  构建日志报 `ERR_PNPM_NO_PKG_MANIFEST No package.json found` = 关联错成了 crate 仓库
- 不跑本地 `git push` / `fetch` / `pull`（会弹凭据窗卡死）
- 推送后看返回的 `pushed` 字段，区分「真推上去」与「远端已是最新」
- 新建镜像仓库用 `git_repo_create`，只能建私有；GitHub 主仓要保持**公开**（crates.io / docs.rs 页面链到它）
- 推送完走技能 `downstream-sync`

## 提交前

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ai-profile --features client
cargo test -p ai-profile            # 不带 client
```

改过注释的人工过一遍中文可读性；抽查文件头没有 BOM（`EF BB BF`）。
