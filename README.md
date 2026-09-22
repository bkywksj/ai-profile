# ai-profile

> 桌面应用的 AI 模型服务配置层 —— provider 预置清单、`ai.profile` 互通协议、端点拼接与连通性验证。
> Shared model-service layer for desktop apps: provider presets, the `ai.profile` interchange format,
> endpoint resolution and connectivity verification.

[![status](https://img.shields.io/badge/status-planning-orange)](docs/tasks/active/)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

**当前状态：规划阶段，尚未发布。** 设计文档见 [`docs/tasks/active/`](docs/tasks/active/)，
交互原型见 [`docs/prototypes/model-service.html`](docs/prototypes/model-service.html)。

---

## 这是什么

多个桌面应用都需要「让用户配置 AI 模型服务」这一整套能力：

- 20 家服务商的预置清单（base_url / 模型 id / 协议类型 / 专有字段）
- `ai.profile` 协议 —— 配置在应用之间粘贴互通
- 端点拼接（各家版本段并不统一：多数 `/v1`、智谱 `/v4`、Gemini `/v1beta/openai`）
- 连通性验证与结构化错误（让 UI 能给出「一键修正」而不只是一行红字）

这些逻辑此前在每个应用里各写一遍，约 1.5 万行做同一件事，且模型 id 变动时必然漏改
（DeepSeek 2026-07-24 下线 `deepseek-chat` 别名后，某个应用两个月后才发现「点开即报错」）。

本仓库把**变动最频繁、跨应用差异为零**的那部分抽出来，做成一份可依赖的库。

## 边界：什么进、什么不进

| | 内容 | 归属 |
|---|---|---|
| ✅ | 预置清单、`ai.profile` 协议、端点拼接、模型清单清洗、验证与结构化错误 | 本 crate |
| ❌ | **密钥存储与加密** | 留给应用 —— 各家差异极大（有的用系统密钥环、有的用 SQLCipher 金库） |
| ❌ | 数据库 / CRUD / 激活态管理 | 同上 |
| ⏸ | React 组件（headless hook + 预制件） | `packages/react`，第二阶段 |

**本 crate 不接触任何密钥明文的持久化**。它只在验证/调用时接收调用方传入的 key，用完即弃。

## 支持的能力（kind）

| kind | 状态 | 执行模型 |
|---|---|---|
| `chat` | 第一版 | 同步 / 流式 |
| `image` | 规划中 | 同步 **或** submit+poll |
| `video` | 规划中 | 必然 submit+poll（各家轮询协议不同） |
| `tts` | 规划中 | 同步，返回字节流 |

按 feature 开启，不用的不编进二进制：

```toml
ai-profile = { version = "0.1", features = ["chat"] }
```

## 设计要点

**base_url 原样使用，不做版本段推断。**
各家并不统一，推断需要转义符才能表达例外（Gemini 的 `/v1beta/openai` 版本段不在末尾），
而推断错的代价是隐性的 —— 用户照文档填对了，库悄悄加了一段，他只看到 404。

**默认 model 选「够用档」而非「最强档」。**
把旗舰塞进默认值，等于替用户做了一个他没同意的花钱决定。

**模型清单清洗用排除法而非白名单。**
厂商上新速度远快于特征词更新，白名单必然把新模型误藏 —— 而"藏起来"对用户不可见。

**错误必须结构化。**
`Err(String)` 会让调用方只能显示一行红字；`NotFound { suggested_url }` 才能做出「一键补 /v1」。

## 协议

`ai.profile` 是一个跨应用的配置交换格式，规范见 [`SPEC.md`](SPEC.md)。
任何工具都可以生成/解析它，与本 crate 无关。

## License

MIT
