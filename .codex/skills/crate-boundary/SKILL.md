---
name: crate-boundary
description: |
  用于判断一段模型服务相关的逻辑该放进 ai-profile crate 还是留在下游应用，以及处理下游的重复实现。

  触发场景：
  - 想往 crate 里加一个新功能，拿不准它算不算通用知识
  - 在下游（sigil / reeve / knowledge_base…）里发现预置、协议映射、端点拼接的另一份实现
  - 下游需要一个应用特有的默认值或行为，想放进 crate 省事
  - 讨论存储、加密、对话协议适配、历史裁剪、界面这些该归谁

  触发词：职责边界、放crate还是应用、重复实现、下游副本、归属、边界判定、该不该进crate
---

# 职责边界

## 一句话判据

> **换一个应用接入时，这段逻辑要不要原样再写一遍？**
> 要 → 放 crate。不要（依赖某个应用的存储 / 加密 / 消息结构 / 界面 / 产品取舍）→ 留在应用。

第二个检验：**它会不会随服务商的变化而变？** 会（模型 id、端点、限额、协议字段）→ 放 crate，
因为这类数据变得最勤，复制 N 份必然漏改（DeepSeek 下线 `deepseek-chat` 别名后，
sigil 两个月后才发现「点开即报错」）。

## 已划定的归属

| 能力 | 归属 | 入口 |
|---|---|---|
| provider 预置、模型候选、静态限额 | crate | `preset::presets()` / `preset_by_key` / `preset::model_limits` |
| 协议拼写、默认端点 | crate | `Protocol::as_str / parse / default_base_url` |
| 端点拼接 | crate | `endpoint::join_api_path / join_chat_endpoint` |
| 模型清单清洗 | crate | `model_filter::clean_fetched_models` |
| 零成本验证 + 结构化错误 | crate | `client::Verifier` / `VerifyError` |
| 限额分层合并与输入预算 | crate | `TokenLimits::or / input_budget` |
| 历史裁剪、上下文超长识别、重试预算 | crate | `history::trim_history / is_context_overflow / retry_budget` |
| ai.profile 解析与生成 | crate | `parse_profile` / `to_profile` |
| 流式解码（SSE 字节 → 统一事件，OpenAI 兼容 + Anthropic） | crate | `stream::StreamDecoder` / `is_stream_options_rejected` |
| 配置增删改、激活态、持久化 | 应用 | 各家存储差异极大（sigil 是 SQLCipher 金库） |
| 密钥加密与解密 | 应用 | 同上；crate 只接收用完即弃的明文 |
| 对话请求体构造、消息格式互转、工具调用与 Agent 循环 | 应用 | 与各自的消息结构、Agent 循环深度耦合 |
| HTTP 客户端、取消信号接线、推给前端 | 应用 | crate 的流式解码是 sans-IO，只吃字节、吐事件 |
| 「某地址不带 `stream_options`」这类记忆状态 | 应用 | crate 只给判定函数，状态归应用存 |
| 被动重试的循环 | 应用 | 要重新发请求；crate 只判断「是不是超长」和「裁到多少」 |
| 真实对话测试（花 token） | 应用 | 要用应用存的密钥与对话实现 |
| 表单界面、文案 | 应用 | npm UI 包是后续阶段 |
| 存量数据迁移 | 应用 | 各家历史包袱，不是通用知识 |
| 「来源没给 model 时补哪个」的最终兜底 | 应用 | 产品取舍；crate 给按 host 推断的那一层 |

## 容易放错的三类

### 1. 应用的产品取舍，别塞进 crate

sigil 的「导入配置没给 model 时兜底用 deepseek-flash」是 sigil 的选择，别家可能想兜 GPT。
crate 只提供可以客观回答的部分（`infer_preset_key` 按 host 找预置的默认 model），
主观兜底留在应用。

### 2. 协议适配：只收纯解码，请求侧与循环留在应用

Anthropic ↔ OpenAI 消息格式互转、请求体构造、工具调用循环，和各自的消息结构、Agent 循环绑死。
搬进来 crate 就成了 LLM SDK，和 async-openai / genai 重叠、定位变糊，**仍然不搬**。

但「SSE 字节 → 统一事件」这一段已经按下面的判据收进来了（`stream`）：
**判据：出现第二个使用方、且事件 / 消息结构能对齐；并且这段逻辑是 sans-IO 的纯函数**
（不发 HTTP、不持有连接、不绑异步运行时），不要为一个使用方抽象。
网关怪癖（`\r\n`、keep-alive、tool_calls 缺 index、只以 `[DONE]` 收尾……）随服务商变化，
复制 N 份必然漏改，这正是第二个检验（会不会随服务商变）的典型。

对照：**历史裁剪也是按这条判据收进来的** —— reeve 接入时出现第二个使用方，且两边消息结构
完全一致（`{ role, content }` Anthropic 风格），才从 sigil 搬入 `history`。
各应用只实现 `HistoryMessage` 两行，数据结构不用改。

流式解码的边界线：crate 给「字节 → 事件 → 收尾结果」，应用负责发请求、取消、重试循环、
推给前端、执行工具、存历史。要不要把 `<think>` 标签剥离、「正文空则把思考提升为正文」这类
产品取舍，也留在应用。

### 3. 迁移代码永远不进 crate

reeve / knowledge_base 旧逻辑会自动补 `/v1`，接入时修正了存量地址（后续已发布的下游同理）。
这段修正用的是**各家自己的旧规则**，写在应用里，修完即删。crate 只有「绝不推断」一条规则。

## 发现下游有重复实现时

1. 确认 crate 已有等价能力（上表）；没有 → 先评估要不要加（走本技能判据 + `api-contract`）
2. 下游改为调用 crate，**删掉**本地副本 —— 只是「不再调用」而留着文件，下次必有人改错那份
3. 前端副本（TS 常量、TS 解析器）的解法是：后端薄 Command 暴露 crate 数据，前端只做适配
   （sigil 的 `llm_list_presets` / `llm_parse_ai_profile` 就是范例）
4. 删掉后在下游跑全量测试，并在 `docs/downstream.md` 的接入范围里记一笔

## 反例（sigil 接入时删掉的三份）

| 副本 | 问题 | 去向 |
|---|---|---|
| `fetch_models`（Rust） | 与 `verify_endpoint` 打同一个 `/models`，却是手写第二套 | 删，改用验证返回的 `models` |
| `model-filter.ts` | crate 清洗规则的 TS 副本 | 删，清洗在后端做 |
| `aiProfile.ts` | TS 版解析器，版本判断已与 crate 漂移（只认 v==1） | 删，改调 `llm_parse_ai_profile` |

## 相关

- 改公开 API → `api-contract`
- 下游升级流程 → `downstream-sync`
