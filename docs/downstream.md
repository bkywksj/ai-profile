# 下游登记表

> 🔴 **本 crate 每次推送后，都要回到这里过一遍。**
> 流程见技能 `downstream-sync`；本表只记「现在是什么状态」。

单向依赖最常见的失败是「crate 发了，没人升」—— 下游引用的提交号停在几周前，
新模型、新修复都到不了用户手里，而且没有任何报错。这张表就是为了让它可见。

## 已接入

| 项目 | 仓库路径 | 引用方式 | 当前引用 | 最后同步 | 接入范围 | 本项目侧技能 |
|---|---|---|---|---|---|---|
| sigil | `E:/my/桌面软件tauri/sigil` | git rev（`src-tauri/Cargo.toml`） | `6ef686a` | 2026-09-23 | 预置 · 协议 · 端点 · 验证 · 限额 · 模型清洗 · 历史裁剪 | `ai-profile-integration` |
| knowledge_base | `E:/my/桌面软件tauri/knowledge_base` | git rev（`src-tauri/Cargo.toml`，chat + client）；桌面与 Android 同一个 crate | `2a32261` | 2026-09-23 | 预置（只用 OpenAI 兼容）· 端点 · 验证 · 限额 · 模型清洗 · ai.profile 单条与打包 · 超长识别（对话协议、RAG 预算、降档阶梯留在应用） | `ai-profile-integration` |
| reeve | `E:/my/桌面软件tauri/reeve` | git rev，**两处同值**：`src-tauri/Cargo.toml`（chat + client）与 `src-tauri/reeve-core/Cargo.toml`（只开 chat）；移动端经 `reeve_core::ai_profile` 跟随 | `6ef686a` | 2026-09-23 | 预置 · 协议 · 端点 · 验证 · 限额 · 模型清洗 · 历史裁剪（桌面）；端点 · 预置 · 清洗（移动） | `ai-profile-integration` |

> ⏳ 三家的界面改动都**尚未实机验证**；代码测试全绿。
>
> sigil / reeve 已升到 `6ef686a`（只补了 6 家新服务商的翻译，零代码改动）。它们的粘贴导入仍只用 `parse_profile`（单条）；
> 要支持导入智码的多条打包，改用 `parse_profiles` 即可，属于应用侧的功能取舍，未排期。

> 升级一个下游 = 改它 `Cargo.toml` 里的 `rev` → 在该项目跑全量测试 → 按它自己的节奏发版 →
> **回来改上表的「当前引用」和「最后同步」**。漏了最后一步，下次就不知道它落后多少。

## 计划接入

按顺序。每家接入时在 `docs/tasks/active/` 建一份任务文档，完成后归档，并把它移到上表。

| 顺序 | 项目 | 现状（2026-09-23 盘点） | 接入时必须处理 |
|---|---|---|---|
| 1 | **ai-station** | 预置写死在 `src/components/settings/ProfileEditModal.tsx`（6 项，模型停在旧代）；ai.profile TS `src/lib/aiProfile.ts`；**无验证 / 获取** | ① `providers/mod.rs::build_endpoint` 强制拼 `/v1`，智谱 `/v4` 当前就是坏的 ② reqwest 开了默认 feature + `blocking`，与 crate 的 feature 合并后要确认 TLS 后端 |
| 2 | **story_loom** | chat 预置 TS `src/lib/providerPresets.ts`（9 家）+ **生图 5 / 视频 9 / 配音 4**；ai.profile 只导入（`services/ai/provider_share.rs`） | 多模态的种子：先把它的 image / video / tts 预置**反向搬进** crate（开 `image` / `video` / `tts` feature），再接入 |

### 🔴 存量地址修正（ai-station 还要做；reeve、knowledge_base 已做完，可作范例）

这三家的旧逻辑会**自动补 `/v1`**（reeve、knowledge_base 末尾加 `#` 可以禁止补），
crate 的规则是**原样使用、绝不推断**。三家都已发布过，用户存下的配置里一定有不带 `/v1` 的地址 ——
直接换成 crate，这些配置全部 404，而且用户看不出原因。

做法：接入时写一次性数据修正 —— **用该应用自己的旧补全逻辑**把存量 `base_url` 规范成完整地址
（补上 `/v1`、去掉末尾 `#`），修正完成后再删旧逻辑。

- 修正逻辑留在应用侧，**不进 crate**：它是各家历史包袱，不是通用知识
- sigil 没做这一步，是因为它当时还没发布过（未发布期不加迁移代码）
- knowledge_base 的做法见 `src-tauri/src/services/legacy_api_url.rs`（它的旧规则把 `v1.5` 也算版本段，与 reeve 不同 ——
  **各家要冻结自己的旧规则**，不能共用一份）
- **reeve 的做法可直接照搬**（`reeve-core/src/legacy_base_url.rs`）：
  - 对照测试里冻结一份旧拼接规则，断言「旧规则(原值) == crate(修正值)」—— 以它为判据
  - 🔴 修正函数必须**幂等**：带 `#` 的地址原样保留（去掉 `#` 再修一次会被补成 `…/v1`）
  - 🔴 只对**旧来源**修正：升级迁移一次；备份恢复看备份清单的 schema 版本；
    跨端同步看载荷标记。新数据再修正会把用户刻意不带版本段的地址错补 `/v1`
  - 默认端点若是不带 `/v1` 的根地址，也要改用 `Protocol::default_base_url`
  - 应用若在「留空」时把默认地址写进库（reeve 移动端就是），迁移也要覆盖它

## 不接入

| 项目 | 原因 | 决定时间 |
|---|---|---|
| tauri-cc（智码 AICoder） | 模型服务与登录 / 账号体系耦合；**完全不接**，包括 ai.profile 协议层 | 2026-09-23 |

## 未登记的 ai.profile 实现

盘点时发现另有 9 个项目自带 ai.profile 实现：quant_helm、reka、cup_watch、db_pilot、lumafind、
hindsight、cross_pilot、shop_sage、zhongyu_comic。

🔴 **不预先登记**。哪个项目要改时，先确认它要不要接入、接到哪一层，确定了再加进上面的表。
（用户 2026-09-23 的决定：每次改前确定了再登记。）

## 发布形态

目前所有下游都按 **git 提交号**引用（仓库是公开的，CI 能直接拉）。
计划在**至少三个下游接入、公开 API 稳定后**再发 `0.1.0` 到 crates.io —— 在那之前改 API
不必背 semver 包袱。
