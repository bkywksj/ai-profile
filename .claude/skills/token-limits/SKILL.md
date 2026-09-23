---
name: token-limits
description: |
  用于处理模型的 token 限额：上下文窗口、单次输出上限、来源分层与合并、输入预算。

  触发场景：
  - 修改 limits.rs、parse_model_limits 或 TokenLimits 相关代码
  - 给预置模型填 context_window / max_output 静态值
  - 下游实现历史裁剪或 max_tokens 封顶，需要知道限额从哪取
  - 某端点的 /models 返回了新的限额字段形状

  触发词：限额、上下文窗口、context_window、max_output、max_tokens、TokenLimits、LimitSource、input_budget、历史裁剪
---

# token 限额

## 这不是全量能力表

本模块只回答一个问题：**发这次请求之前，该按多大的窗口裁历史、max_tokens 封顶多少。**
全量能力表（价格、能力矩阵）是 models.dev / LiteLLM 的专业，数据周级变动，我们不做。

## 分层与优先级

```
0. User      用户手填         ← 压过一切（端点也可能报错，中转站可能另有套餐限制）
1. Endpoint  端点 /models 上报 ← 最准，含中转站的真实限制
2. Preset    crate 静态值      ← 端点不报时兜底，保守
3. None      都没有            ← 不猜；调用方据此「不裁」
```

合并用 `TokenLimits::or(fallback)`，**逐字段**：用户只填了窗口，输出上限照样从下一层补。
自己全空时整条换成 fallback —— **空的用户设置不能把来源冒充成 User**（守卫 `or_merges_field_by_field`）。

## 🔴 来源必须可分辨

调研到的真实故障几乎都源于「把猜的数字当成真的」：网关丢元数据 → 回落硬编码表 →
512K 被当成 131K；某扩展硬编码 288K、无视服务器上报值**和**用户设置。
共同点是**错了也看不出来**。所以 `source` 是必填字段，界面据此显示「端点上报 / 手动 / 预置（可能过时）/ 未知」。

## 下游必须做的两件事（sigil 踩过）

1. **把端点上报值存下来。** 端点值只在验证时拿得到；对话路径不会为它再打一次 `/models`。
   sigil 此前验证完就丢，结果中转站 / 自定义端点的窗口恒为未知，历史裁剪从不触发。
2. **`preset` 不落库。** 存下来就冻结成旧值，crate 以后修正了数字，这条配置永远用着当初那份。
   落库只存 `user` / `endpoint`，预置值每次现查 `preset::model_limits`。

参考实现：sigil `services/vault/llm.rs::effective_limits`。

## 解析端点返回（`parse_model_limits`）

| 我们要的 | 依次尝试 |
|---|---|
| 上下文窗口 | `top_provider.context_length` → `context_length` → `context_window` → `max_context_length` → `max_input_tokens` |
| 输出上限 | `top_provider.max_completion_tokens` → `max_completion_tokens` → `max_output_tokens` → `max_tokens` |

- 🔴 `top_provider` 优先：OpenRouter 顶层是**模型本体**标称值，嵌套的是**这家实际提供**的，两者会不一致
- `0` 当作「未知」，不是「上限为 0」（否则预算算成 0，历史全被裁光）
- 数字字符串、浮点也接受
- 新遇到一种返回形状 → **拿真实响应加一条测试**（如 `deepseek_shape_is_parsed`），别只改代码

实测覆盖（2026-09）：

| 端点 | 报不报 |
|---|---|
| OpenRouter | ✅ `context_length` + `top_provider.*` |
| DeepSeek | ✅ `context_window` + `max_output_tokens` |
| LM Studio / Ollama 兼容层 | ❌ 只有 `{id, object, owned_by}` |

曾经误判过 DeepSeek「不报」—— 结论必须来自真实响应（`scripts/inspect-models-payload.py`），不能靠文档推测。

## 输入预算

`input_budget(reserve_output)` = 窗口 − 本次要预留的输出。
`reserve_output` 传**本次请求实际的 max_tokens**，不是模型的输出上限 —— 用上限会把预算压得过小。
窗口未知返回 `None`：调用方**不裁**，不要自己兜一个默认值。

## 历史裁剪（`history` 模块）

两道防线：

| 场景 | 做法 |
|---|---|
| 窗口已知 | 发送前主动裁：`history_budget(limits, max_tokens)` → `trim_history` |
| 窗口未知 | 🔴 不猜，照常发；`is_context_overflow(status, body)` 为真 → `retry_budget` 裁一半重试 |

- 重试循环写在应用里，**上限 3 次**，`dropped == 0` 时停（裁无可裁）
- `dropped` 要显示给用户（「已裁掉 N 条」），否则用户只会觉得模型变笨
- 超长识别宁可漏判不可误判：`429` 限流、「max_tokens 太大」（输出上限）都不算
- 首条 user 消息（任务描述）先预留位置，但只在不超过预算一半时 —— 超大首条不强行保留

## 推理模型的坑

推理模型（deepseek-flash 等）的思考 token 也计入 max_tokens。实测 max_tokens=16 时正文为空、
256 才有正文。测试请求、默认值都别设得太小。

## 填静态值的规则

见 `preset-maintenance` 第三节：只填官方文档写明的；同一 id 经不同渠道窗口不同就不填。
