# 任务：knowledge_base 接入 ai-profile

**状态**: 🔵 已推送；数据迁移与对话链路已用真实数据验证，待界面实机验证后归档
**创建时间**: 2026-09-23 20:00:00
**更新时间**: 2026-09-23 22:30:00
**下游仓库**: `E:/my/桌面软件tauri/knowledge_base`（master，v1.64.0，**已发布，有真实用户**）
**参照**: reeve 接入（`task-20260923-140000-reeve接入.md`，存量地址修正的范例在 `reeve-core/src/legacy_base_url.rs`）

---

## 📋 需求描述

knowledge_base 的模型服务（预置、端点拼接、获取模型、ai.profile、限额来源）改为依赖 ai-profile，
删除本地副本。它自带的上下文预算 / RAG 配额 / 历史降档逻辑保留在应用侧（登记表已定）。

---

## 🔍 现状地图（2026-09-23 只读盘点，路径相对 knowledge_base 仓库）

| 项 | 位置 | 备注 |
|---|---|---|
| 存储 | 表 `ai_models`（`src-tauri/src/database/schema.rs:246`） | 多条 + `is_default`；地址列叫 **`api_url`**；`provider` 存的是**厂商 id**（24 种），不是协议 |
| 限额列 | `max_context INTEGER NOT NULL DEFAULT 32000`（v25）→ v51 把 32000 抬到 128000；`max_tokens` 可空（v61，Ollama 置 -1） | 没有来源列；128000 是**默认值冒充的用户值** |
| schema 版本 | `SCHEMA_VERSION = 61`，`PRAGMA user_version` | 新迁移 = v62 |
| 密钥 | `enc:v1:` AES-256-GCM（`crypto.rs`），DAO 读写时加解密 | 不动 |
| 预置 | **只在前端** `src/lib/aiProviderPresets.ts`（24 家，5 张平行表），桌面与手机共用 | Rust 侧无预置 |
| 端点拼接 | `services/ai.rs:209 build_openai_api_url` / `:252 build_openai_chat_url` | 末段非 `vN` / `vN.M` 就**自动补 /v1**；末尾 `#` 锁定不补；剥 `chat/completions`；`v1beta` 不算版本段 |
| Ollama | 原生 `/api/chat`（NDJSON）、`/api/tags`，地址存根 `http://localhost:11434` | 🔴 5 个非流式功能（plan_today 等）没有 Ollama 分支，**靠自动补 /v1 才走得通** OpenAI 兼容层 |
| 协议 | 只有 Ollama 原生 + OpenAI chat/completions；**没有 Anthropic 原生** | `claude` 走 Anthropic 的 OpenAI 兼容端点 |
| 获取 / 测试 | `list_remote_models`（`:1267`，`claude` 用 x-api-key）/ `test_model_connection`（`:1166`，真发对话） | 错误是中文字符串，不结构化 |
| 限额 / 预算 | 无模型窗口表；`max_context` 用户填（默认 128000）；`ContextBudget` 按字符估算分配附件与 RAG | 历史降档 `[20,10,4,0]`，只认 `convert_request_failed` / `context_length_exceeded` 两个串；`chat_stream_with_skills` 不裁不降档 |
| ai.profile | `src/lib/configShare.ts`：解析单条 + **`ai.profile.bundle`**（snake_case、跳过 oauth）；只生成单条 | 另有应用自己的 `kbConfig` 信封（webdav / 同步 / ASR 等），**不归 crate** |
| HTTP | reqwest 0.12，`default-features=false` + rustls（Android NDK 要求）；OpenAI 兼容走系统代理，Ollama 走 no_proxy | crate `client` 同为 rustls，兼容 |
| 手机 | 同一前端 + 同一 Rust crate 打 Android；`MobileAiModelModal.tsx`（无测试 / 获取） | 不是独立 workspace |
| 写入 `api_url` 的入口 | ① 设置表单 ② 手机弹窗 ③ 配置导入（`kbConfig` 信封 / ai.profile 单条与 bundle，可 PIN 加密）④ 整库恢复（ZIP / WebDAV，恢复后跑迁移）⑤ `app.db.bak-*` 恢复 | ④⑤ 由迁移覆盖；③ 需按来源判断 |
| i18n | **无**，全中文硬编码 | 用 crate 预置的纯文本 `label` / `hint` / `group_label` |
| 测试 | `cargo test --manifest-path src-tauri/Cargo.toml`（不 cd）、`pnpm test`、`npx tsc --noEmit` | URL 规则 4 个测试（`ai.rs:4657`）是冻结旧规则的现成素材 |

### 厂商 id 对照（knowledge_base 24 家 ↔ crate 19 家）

| 对得上（改名） | 对得上（同名） | crate 没有 |
|---|---|---|
| `kimi`→`moonshot`、`doubao`→`volcengine_ark`、`openai`→`openai_official`、`custom`→`openai_compatible_custom` | deepseek、zhipu、qwen、siliconflow、openrouter、gemini、groq、xai、ollama、lmstudio、vllm | minimax、qianfan、hunyuan、stepfun、baichuan、lingyi、mimo、together、**claude**（走 OpenAI 兼容） |

### 盘点顺带发现的两个 bug（与接入无关，建议单独提交修）

1. `update_ai_model` 注释说「不传 max_context 保留原值」，代码却 `unwrap_or(128000)` 重置（`database/ai.rs:578`）
2. `provider == "claude"` 时 system prompt 被整个丢掉（`services/ai.rs:1800`）—— Anthropic 的 OpenAI 兼容端点收 system 消息，丢掉没有理由

另有一处过时注释：`src/types/index.ts:499` 说不传 `max_context` 存 32000，实际是 128000（K6 改限额时一并更正）。

---

## 🎨 方案

### 分步

| 步 | 内容 | 仓库 |
|---|---|---|
| C0 | crate 支持解析 `ai.profile.bundle`（技能 `protocol-evolution`），`parse_profile` 返回单条或多条 | ai-profile |
| C1 | （待定 ①）crate 补 knowledge_base 独有的服务商预置 | ai-profile + sigil / reeve 补翻译 |
| K1 | 依赖（`chat` + `client`）；`legacy_api_url.rs` 冻结旧规则 + 对照测试；**v62 迁移**：修正 `api_url`、厂商 id 改用 crate 键、Ollama 地址补成 `…/v1`、`max_context` 改可空 + 加 `limits_source` | knowledge_base |
| K2 | 端点拼接换 crate，删 `build_openai_api_url` / `build_openai_chat_url`；Ollama 原生路径由 `…/v1` 反推根地址（应用侧适配） | knowledge_base |
| K3 | 预置换 crate：新 Command 暴露（只给知识库能说的协议）；删 `aiProviderPresets.ts`；桌面表单 + 手机弹窗改用它，服务商下拉双行 | knowledge_base |
| K4 | 「获取」换 `Verifier`（零成本 + 结构化原因 + 一键改用建议地址）；「测试连接」保留真发对话 | knowledge_base |
| K5 | ai.profile 解析 / 生成走后端 crate；`configShare.ts` 只留 `kbConfig` 信封；旧 `kbConfig` 导入按来源修正地址 | knowledge_base |
| K6 | 限额：`effective_limits`（用户 > 端点 > 预置）喂给现有 `ContextBudget`；`max_tokens` 封顶；降档触发改用 `is_context_overflow` | knowledge_base |
| K7 | 技能 `ai-profile-integration` + CLAUDE.md；登记表移入「已接入」 | 两边 |

### 存量地址修正（照搬 reeve 做法）

- 冻结知识库自己的旧规则（与 reeve 不同：**`v1.5` 这类带小数的也算版本段**）
- 判据：`旧规则(原值) == crate(修正值)` 对照测试，样例取 `ai.rs:4657` 那 4 组测试 + Ollama 根地址
- 幂等：带 `#` 的原样保留
- 只修旧来源：v62 迁移一次（整库恢复 / `.bak` 恢复也会跑到）；`kbConfig` 导入按信封里有无新标记判断；ai.profile 按 crate 语义原样使用

### 不做

- 不加 Anthropic 原生对话（待定 ②）
- 不改 RAG / 附件配额 / 历史降档阶梯本身
- `chat_stream_with_skills` 不接 crate 的 `trim_history`：它的消息是 OpenAI 结构（`tool_calls`），crate 裁剪按 Anthropic 结构配对 tool —— 硬接会拆坏配对

---

## ✅ 用户已确认（2026-09-23，全部按建议）

1. crate 没有的服务商补进 crate —— 实际补 6 家（见变更记录）；混元、零一万物不补，存量行归「OpenAI 兼容自定义」
2. `claude`：存量行归「OpenAI 兼容自定义」、地址不变；下拉不提供 Anthropic 官方档
3. Ollama：保留原生 `/api/chat`，地址统一存成 `…/v1`，原生路径在应用侧去掉 `/v1`
4. `max_context == 128000` 的存量行视为「未设置」
5. 两个 bug 本轮顺手修，各自单独提交

### 厂商 id 迁移表（v62 用）

| 旧 id | 新 key | 旧 id | 新 key |
|---|---|---|---|
| kimi | moonshot | minimax / qianfan / stepfun / baichuan / mimo / together | 同名 |
| doubao | volcengine_ark | deepseek / zhipu / qwen / siliconflow / openrouter / gemini / groq / xai / ollama / lmstudio / vllm | 同名 |
| openai | openai_official | claude / hunyuan / lingyi / custom / 其它未知 | openai_compatible_custom |

## 🔄 当前进度

- [x] C0 crate 多条打包 `parse_profiles`（`fc13b02`）
- [x] C1 crate 新增 6 家服务商（`93711bc`）
- [x] 推送 crate（`2a32261`）
- [x] K1 + K2 存量修正（schema v62）+ 端点拼接换 crate（`5e2ec4f`）
- [x] bug 1 编辑模型不带窗口时被重置（`f400c44`）；bug 2 选 Claude 时系统提示词丢失（`a9377ab`）
- [x] JSON 模式白名单改用 crate key（`029d473`）—— 迁移改了 provider 取值，漏改会静默关掉两家的 JSON 模式
- [x] K3–K6 预置 / 获取 / 限额 / ai.profile / 旧配置导入（`39d39c1`）
- [x] K7 技能 + CLAUDE.md（`d2a11b1`）
- [x] 真实数据验证（`7361097`）：本机库（v61，一条远程 Ollama `http://100.100.100.100:11434`）的只读备份副本跑完整迁移 →
      地址补成 `…/v1`、窗口 32000 保留为 user；按修正后地址真实联调：「获取」拿到 qwen2.5:7b 且滤掉向量模型 bge-m3，
      OpenAI 兼容层 `/v1/chat/completions` 与原生 `/api/chat` 均 200
- [x] 推送 knowledge_base（GitHub `16ec44c..7361097`）
- [ ] 界面实机验证：设置页双行下拉、「获取」（含 404 一键改地址）、窗口留空与自动填入、导入旧导出的配置与智码打包
- [ ] sigil / reeve 升到 `2a32261`（补 6 家翻译）

**验证**：Rust 874 个测试中 872 通过，2 个失败与本次无关（`tags::tag_path_segments_independent_namespace`
项目已记录的老问题；`dataview_recent_notes_orders_by_updated_at` 时间戳精确到秒导致的不稳定）；
tsc 通过；vitest 436 通过；新增测试 23 个（knowledge_base Rust 15、前端 4，crate 4）全部实际跑到

## ⚠️ 待核实

- `cargo xtask probe` 报 **gemini 预置 404**（`…/v1beta/openai/models`）：可能只是未带密钥时 Google 返回 404，
  也可能地址有问题。crate 原有预置，与本次接入无关；需真实密钥确认

## 💬 变更记录

### 2026-09-23 22:30
**变更类型**: 进度更新（代码完成）

- 发现 JSON 模式白名单按旧厂商 id 写死，迁移后 `openai` / `kimi` 两家会被静默关掉 JSON 模式 —— 单独修
- `max_context` 保持 NOT NULL，用 0 表示未设置（避免重建表；代码里 `<= 0` 本来就按未知处理）
- 「获取」改走 crate `Verifier` 后，Ollama 也走它的 OpenAI 兼容层 `/v1/models`，不再用原生 `/api/tags`；本机服务绕开系统代理
- `chat_stream_with_skills` 仍不接 `trim_history`（OpenAI 消息结构，配对规则对不上），记在技能里

### 2026-09-23 21:30
**变更类型**: 决策确认 + 进度

- 用户确认五项全部按建议
- C0：`parse_profiles` 兼容智码的 `data.api_profiles` + 顶层 `tool_id` + `auth_type=oauth` 跳过
- C1：probe 发现零一万物 API 已停服（「Model service has been discontinued」），不补；
  混元不补（crate 已有 TokenHub 档引导迁移）。实际补 MiniMax / 千帆 / 阶跃 / 百川 / MiMo / Together

### 2026-09-23 20:00
**变更类型**: 创建

- 只读盘点 knowledge_base（存储、预置、端点、Ollama、获取 / 测试、限额 / 预算、ai.profile、HTTP、手机、导入与恢复入口、界面、测试）
- 发现与 reeve 不同的三处：厂商 id 粒度（24 家厂商而非 2 种协议）、Ollama 原生协议、没有 Anthropic 原生对话
