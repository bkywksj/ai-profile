<!-- 🔴 本文件由 `cargo xtask gen-docs` 生成，请勿手改。
     改预置请编辑 crates/ai-profile/src/preset/*.rs，再重新生成。
     守卫测试 `providers_md_in_sync` 会比对一致性。 -->

# 支持的服务商

当前共 **26** 家服务商、**26** 条预置配置。

> `base_url` 一律是**服务商文档里的原文**（含版本段、不含端点后缀）。
> 本 crate 原样使用它、不做任何推断 —— 所以各家的版本段不统一（多数 `/v1`、
> 智谱 `/v4`、Gemini 的 `/v1beta/openai` 还不在末尾）也不影响。

## 按服务商

同一家的多种能力**共用一个密钥** —— 配一次就能全部启用。

| 服务商 | 能力 | Host | 类型 |
|---|---|---|---|
| Anthropic 官方 | 对话 | `（自定义）` | 云端 |
| Claude Code 客户端（自定义接口地址） | 对话 | `（自定义）` | 自定义端点 |
| Codex 客户端（自定义接口地址） | 对话 | `（自定义）` | 自定义端点 |
| DeepSeek | 对话 | `api.deepseek.com` | 云端 |
| 智谱 GLM | 对话 | `open.bigmodel.cn` | 云端 |
| 通义千问（阿里云百炼） | 对话 | `dashscope.aliyuncs.com` | 云端 |
| 月之暗面 Kimi | 对话 | `api.moonshot.cn` | 云端 |
| 硅基流动 SiliconFlow | 对话 | `api.siliconflow.cn` | 云端 |
| 火山方舟（豆包） | 对话 | `ark.cn-beijing.volces.com` | 云端 |
| 腾讯 TokenHub | 对话 | `tokenhub.tencentmaas.com` | 云端 |
| MiniMax（稀宇科技） | 对话 | `api.minimax.chat` | 自定义端点 |
| 百度千帆 | 对话 | `qianfan.baidubce.com` | 自定义端点 |
| 阶跃星辰 | 对话 | `api.stepfun.com` | 自定义端点 |
| 百川智能 | 对话 | `api.baichuan-ai.com` | 自定义端点 |
| 小米 MiMo | 对话 | `api.xiaomimimo.com` | 自定义端点 |
| 中宇AI智行（聚合） | 对话 | `api.ipsunion.com` | 自定义端点 |
| OpenAI 官方 | 对话 | `api.openai.com` | 云端 |
| OpenRouter | 对话 | `openrouter.ai` | 云端 |
| Google Gemini（OpenAI 兼容层） | 对话 | `generativelanguage.googleapis.com` | 云端 |
| Groq | 对话 | `api.groq.com` | 云端 |
| xAI Grok | 对话 | `api.x.ai` | 云端 |
| Together AI | 对话 | `api.together.xyz` | 自定义端点 |
| Ollama（本地） | 对话 | `localhost:11434` | 本地，需先启动服务 |
| LM Studio（本地） | 对话 | `localhost:1234` | 本地，需先启动服务 |
| vLLM / 自建推理服务 | 对话 | `localhost:8000` | 本地，需先启动服务 |
| 其它 OpenAI 兼容（自定义接口地址） | 对话 | `（自定义）` | 自定义端点 |

## 对话

| 预置 key | 名称 | Base URL | 默认模型 | 协议 | 核实于 |
|---|---|---|---|---|---|
| `anthropic_official` | Anthropic 官方 | `—` | `claude-opus-5-5` | Anthropic | 未核实 |
| `claude_code` | Claude Code 客户端（自定义接口地址） | `—` | `claude-opus-5` | Anthropic | 2026-09-22 |
| `codex` | Codex 客户端（自定义接口地址） | `—` | `gpt-5.6-terra` | OpenAI 兼容 | 未核实 |
| `deepseek` | DeepSeek | `https://api.deepseek.com/v1` | `deepseek-flash` | OpenAI 兼容 | 2026-09-17 |
| `zhipu` | 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` | `glm-5.3` | OpenAI 兼容 | 未核实 |
| `qwen` | 通义千问（阿里云百炼） | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` | OpenAI 兼容 | 未核实 |
| `moonshot` | 月之暗面 Kimi | `https://api.moonshot.cn/v1` | `kimi-k3` | OpenAI 兼容 | 未核实 |
| `siliconflow` | 硅基流动 SiliconFlow | `https://api.siliconflow.cn/v1` | `deepseek-ai/DeepSeek-V3` | OpenAI 兼容 | 未核实 |
| `volcengine_ark` | 火山方舟（豆包） | `https://ark.cn-beijing.volces.com/api/v3` | `doubao-seed-1-6-251015` | OpenAI 兼容 | 未核实 |
| `tencent_tokenhub` | 腾讯 TokenHub | `https://tokenhub.tencentmaas.com/v1` | `hy3-preview` | OpenAI 兼容 | 未核实 |
| `minimax` | MiniMax（稀宇科技） | `https://api.minimax.chat/v1` | `MiniMax-M1` | OpenAI 兼容 | 未核实 |
| `qianfan` | 百度千帆 | `https://qianfan.baidubce.com/v2` | `ernie-4.5-turbo-128k` | OpenAI 兼容 | 未核实 |
| `stepfun` | 阶跃星辰 | `https://api.stepfun.com/v1` | `step-1-flash` | OpenAI 兼容 | 未核实 |
| `baichuan` | 百川智能 | `https://api.baichuan-ai.com/v1` | `Baichuan4-Turbo` | OpenAI 兼容 | 未核实 |
| `mimo` | 小米 MiMo | `https://api.xiaomimimo.com/v1` | `mimo-v2-flash` | OpenAI 兼容 | 未核实 |
| `ipsunion` | 中宇AI智行（聚合） | `https://api.ipsunion.com/v1` | `deepseek-v3.2` | OpenAI 兼容 | 未核实 |
| `openai_official` | OpenAI 官方 | `https://api.openai.com/v1` | `gpt-5.6-terra` | OpenAI 兼容 | 未核实 |
| `openrouter` | OpenRouter | `https://openrouter.ai/api/v1` | `anthropic/claude-sonnet-5` | OpenAI 兼容 | 未核实 |
| `gemini` | Google Gemini（OpenAI 兼容层） | `https://generativelanguage.googleapis.com/v1beta/openai` | `gemini-3.8-flash` | OpenAI 兼容 | 未核实 |
| `groq` | Groq | `https://api.groq.com/openai/v1` | `llama-3.3-70b-versatile` | OpenAI 兼容 | 未核实 |
| `xai` | xAI Grok | `https://api.x.ai/v1` | `grok-4.5` | OpenAI 兼容 | 未核实 |
| `together` | Together AI | `https://api.together.xyz/v1` | `meta-llama/Llama-3.3-70B-Instruct-Turbo` | OpenAI 兼容 | 未核实 |
| `ollama` | Ollama（本地） | `http://localhost:11434/v1` | `qwen3:8b` | OpenAI 兼容 | 未核实 |
| `lmstudio` | LM Studio（本地） | `http://localhost:1234/v1` | `—` | OpenAI 兼容 | 未核实 |
| `vllm` | vLLM / 自建推理服务 | `http://localhost:8000/v1` | `—` | OpenAI 兼容 | 未核实 |
| `openai_compatible_custom` | 其它 OpenAI 兼容（自定义接口地址） | `—` | `—` | OpenAI 兼容 | 未核实 |

## 加一家服务商

见 [`CLAUDE.md`](../CLAUDE.md) 的「加一家 provider 的完整流程」。要点：

1. `base_url` 照抄文档原文，含版本段
2. 默认 `model` 选**够用档**而非最强档
3. `models` 只放核对过的 id，并填 `verified_at`
4. 同一厂商复用同一个 `vendor_id`
5. `cargo test` 七个守卫测试必须全绿
6. `cargo xtask gen-docs` 重新生成本文件
