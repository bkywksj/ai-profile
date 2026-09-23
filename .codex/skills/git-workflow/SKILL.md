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
目前也不发 crates.io（等至少三个下游接入后再发，见 `docs/downstream.md`）。
「发布」= 推送到 GitHub，下游按提交号引用。

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

## 推送

- 一律走 Sigil：`git_push`，凭据 `github1`，remote `github`
- 不跑本地 `git push` / `fetch` / `pull`（会弹凭据窗卡死）
- 推送后看返回的 `pushed` 字段，区分「真推上去」与「远端已是最新」
- 推送完走技能 `downstream-sync`

## 提交前

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ai-profile --features client
cargo test -p ai-profile            # 不带 client
```

改过注释的人工过一遍中文可读性；抽查文件头没有 BOM（`EF BB BF`）。
