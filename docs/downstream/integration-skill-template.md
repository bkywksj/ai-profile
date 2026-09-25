# 下游接入技能模板

> 每个接入 ai-profile 的应用，复制下面 `---` 之间的内容到该项目的
> `.claude/skills/ai-profile-integration/SKILL.md`（并镜像到 `.codex/skills/`），
> 把 `{…}` 占位替换成该项目的实际路径与情况。
>
> 🔴 **接入完成后再建**，不要提前建：技能固化的是已经跑通的做法。
> sigil 的成品可作参考：`E:/my/桌面软件tauri/sigil/.claude/skills/ai-profile-integration/SKILL.md`。
>
> 🔴 这是**我们自己的应用**用的模板（带本机路径与内部流程）。给外部用户的通用版本在文档站
> `ai-profile-docs/docs/public/ai/ai-profile-integration.md`（线上 `/ai/ai-profile-integration.md`），
> 规则类内容两边要保持一致。

---

```markdown
---
name: ai-profile-integration
description: |
  用于本项目中与 ai-profile crate 相关的开发：模型服务预置、ai.profile 导入导出、端点验证、token 限额。

  触发场景：
  - 要加新模型、新服务商，或改某个预置的默认 model
  - 修改模型服务配置表单、粘贴导入、测试连接
  - 升级 ai-profile 依赖版本
  - 发现本项目里有预置 / 协议 / 端点拼接的本地实现

  触发词：ai-profile、模型服务、预置、provider、新模型、ai.profile、粘贴导入、测试连接、限额、升级crate
---

# ai-profile 接入（{项目名}）

## 🔴 先判断改哪个仓库

| 要做的事 | 去哪改 |
|---|---|
| 加模型 / 加服务商 / 改默认 model / 改静态限额 | **ai-profile 仓库**（`E:/my/桌面软件tauri/ai-profile`），本项目只升依赖 |
| ai.profile 协议规则、端点拼接、模型清洗、验证错误 | **ai-profile 仓库** |
| 存储、密钥加密、对话实现、界面、本项目的默认值 | 本项目 |

**不要在本项目里写预置清单、模型列表、协议映射表** —— 那正是接入要消灭的副本。

## 本项目的接入点

| 能力 | 文件 |
|---|---|
| 依赖声明 | `{Cargo.toml 路径}` |
| 预置暴露给前端 | `{Command / 适配层文件}` |
| 验证 / 测试连接 | `{文件}` |
| 限额取值 | `{文件}` |
| ai.profile 导入导出 | `{文件}` |

## 升级 ai-profile

1. 改 `{Cargo.toml 路径}` 里的 `rev`
2. 跑全量测试：`{本项目的测试命令}`
3. 回 ai-profile 仓库更新 `docs/downstream.md` 本项目那一行

## 本项目特有的坑

- {例：移动端是独立 workspace，要一起升}
- {例：存量地址修正已于 YYYY-MM-DD 完成，旧的补 /v1 逻辑已删除}
```
