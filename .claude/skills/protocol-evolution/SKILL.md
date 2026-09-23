---
name: protocol-evolution
description: |
  用于修改 ai.profile 跨软件互通协议：加字段、兼容别家写法、多条打包等扩展。

  触发场景：
  - 给 ai.profile 的 data 加字段（如限额、hints 新键）
  - 某个软件导出的配置我们解析不了，要兼容它的写法
  - 一次分享多条配置（多条打包 `ai.profile.bundle`）
  - 调整协议推断规则（provider 映射、claude 兜底）

  触发词：ai.profile、协议、互通、粘贴导入、分享配置、parse_profile、to_profile、多条打包、bundle、宽进严出
---

# ai.profile 协议演进

## 协议是什么

跨软件复制模型配置的明文 JSON 信封：

```json
{ "kind": "ai.profile", "v": 1,
  "data": { "name": "", "provider": "", "baseURL": "", "apiKey": "", "model": "", "hints": { "toolId": "" } } }
```

生产方与消费方分布在十几个项目里（sigil、reeve、knowledge_base、story_loom、onestop，
以及 tauri-cc 等不接入 crate 的项目）。**我们改不动别人的实现，只能保证自己宽进严出。**

## 三条原则

### 1. 宽进严出

- **解析**：接受所有见过的别名（`baseURL` / `baseUrl` / `base_url`、`apiKey` / `api_key`、`toolId` / `tool_id`）
- **生成**：只产出规范写法（`baseURL`、`apiKey`），别给生态添新变体
- 守卫：`accepts_all_three_base_url_spellings`、`output_uses_canonical_spelling_only`、`tolerates_foreign_spellings`

### 2. 只拒绝更高版本

`v` 低于当前实现的一律接受（字段只增不改），**拒绝低版本会把老软件分享的配置挡在外面**。
sigil 旧的 TS 解析器只认 `v == 1`，就是这个错（守卫 `accepts_older_version_rejects_newer`）。

### 3. 加字段不升 `v`，改语义才升

- 新增可选字段：旧实现忽略它，不影响 → **不升 `v`**
- 改已有字段的含义或必填性：旧实现会误读 → 升 `v`，并保证新实现仍能读旧版本

## 推断规则（改前先读现状）

`map_protocol`：provider 含 anthropic / claude → Anthropic；否则 model 以 `claude-` 开头 → Anthropic；
否则 `hints.toolId` 提到 claude → Anthropic；其余一律 OpenAI 兼容。
两条兜底都来自真实场景（中转卖家的 model 名、智码分享的 `provider: "custom"` + `toolId: "claude-code"`），不是防御性编程。

## 边界：crate 管什么，应用管什么

| crate（`parse_profile`） | 应用 |
|---|---|
| 格式错误：`empty` / `invalid_json` / `not_ai_profile` / `unsupported_version` / `missing_data` | 建档要求（如 sigil 要求必须有密钥 → `missing_api_key`） |
| 协议推断、别名兼容 | 来源没给 model 时的最终兜底（产品取舍） |
| `model_fallback` 标记 | 名字为空时用什么兜底 |

应用把自己的错误与 crate 的 `ParseError` 合成一个扁平 `{ code }` 给前端（sigil 用 `#[serde(untagged)]`）。

## 多条打包（2026-09-23 已完成）

```json
{ "kind": "ai.profile.bundle", "v": 1, "data": { "profiles": [ { …单条的 data… } ] } }
```

- 入口 `parse_profiles`：单条、多条都收，统一返回 `ParsedProfiles { profiles, skipped, bundle }`
- 独立的 `kind`：只认单条的 `parse_profile` 遇到打包报 `not_ai_profile`，不会误读成一条
- 🔴 兼容智码**已发出去**的写法：`data.api_profiles`、snake_case 档案、条目顶层 `tool_id`
  （作用同 `hints.toolId`）、`manifest`；`auth_type = "oauth"` 跳过并计数 —— OAuth 凭据与设备绑定
- 守卫：`parses_tauri_cc_bundle`（形状取自 tauri-cc 的 `ApiProfile`）、`bundle_edge_cases`
- 生成打包暂不提供：目前没有下游需要导出多条；要加时出 `to_profiles`，只产出规范写法 `profiles`

## 改完

- [ ] `protocol_roundtrip` / `cross_app_interop` 必过；新写法加互通测试
- [ ] 同步文档站 `api/protocol.md`
- [ ] 版本位按 `api-contract`
