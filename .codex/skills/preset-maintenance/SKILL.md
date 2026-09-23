---
name: preset-maintenance
description: |
  用于维护 provider 预置数据：新模型上线、加一家服务商、改默认 model、下线旧模型。

  触发场景：
  - Claude / GPT / DeepSeek 等发布新模型，要加进候选清单
  - 新增一家服务商（国内 / 国际 / 本地推理服务）
  - 调整某个预置的默认 model，或某模型已被服务商下线
  - 另一个项目（如 tauri-cc）已经接了新模型，要同步到 crate

  触发词：新模型、加模型、加服务商、加provider、预置、默认模型、模型下线、模型id、preset、chat.rs
---

# 预置维护

所有对话预置在 `crates/ai-profile/src/preset/chat.rs` 一个文件里。**数组顺序即下拉顺序，同组必须连续。**

## 一、模型 id 去哪核对（别凭记忆写）

| 来源 | 核对什么 | 备注 |
|---|---|---|
| 本机 Claude Code 二进制的机型目录 | Anthropic 的 id、`[1m]` 后缀、默认 effort | tauri-cc 接新机型时就是这么核的 |
| 本机 Codex 的模型目录（服务端下发） | OpenAI / Codex 的 id | 放量期不同账号看到的不一样 |
| 官方定价页 | 价格（决定默认值）、上下文窗口 | 静态限额只填这里写明的 |
| OpenRouter 模型页 | OpenRouter 自己的 id | 🔴 Anthropic 用**点号**：`anthropic/claude-opus-5.5`，不是 `-5-5` |
| 同级项目已落地的写法 | tauri-cc 的 `toolModelCatalog.ts` 等 | 别家核对过的可以直接引用，提交说明里注明出处 |

**只放核对过的 id。** 下拉只是建议清单，用户可以手填任意 id —— 放一个错的 id 比少放一个糟得多。

## 二、默认 model 怎么定

铁律：**选「够用档」，不选「最强档」**。在此基础上：

| 情况 | 做法 | 例子 |
|---|---|---|
| 新模型更强**且**更便宜 | 官方档直接换成它 | Opus 5.5（$4/$20）替 Opus 5（$5/$25） |
| 中转档 / 协议档 | **默认不急着换**：中转上新滞后，默认一个它不认的 id，建档后第一次对话就报 model not found | `claude_code` 档仍默认 Opus 5 |
| 新模型还在分批放量 | 默认不动，只加进候选 | GPT-6 Sol / Luna 放量期 `codex` 档仍默认 `gpt-5.6-terra` |
| 旧模型被服务商下线 | **立刻从默认和候选里去掉**，在注释里写下线日期 | DeepSeek 2026-07-24 下线 `deepseek-chat` |

守卫测试 `default_model_is_in_its_own_list`：默认 model 必须在自己的 `models[]` 里。

## 三、静态限额（context_window / max_output）

- 只填**官方文档写明**的；查不到用 `ModelOption::plain`，不要猜
- 只知道窗口 → `with_context`；两个都知道 → `with_limits`
- 🔴 **同一个 id 经不同渠道窗口不同，就不填**。例：Opus 5.5 官方 1M、经中转默认 200k，
  而 `CLAUDE_MODELS` 被官方档与中转档共用，填哪个都会对其中一档说错
- 端点会上报的（OpenRouter、DeepSeek）静态值取略保守的整数，只作兜底
- 细则见技能 `token-limits`

## 四、加一家服务商

1. 在对应分组的连续区域加一条 `ProviderPreset`
2. `base_url` **照抄服务商文档原文**：含版本段、不含端点后缀（`/chat/completions`）
3. `match_hosts` 填主机名片段，用于从已存配置反推模板
4. `vendor_id`：同一厂商的多种能力共用一个（硅基流动 chat / image 共用 `siliconflow`）
5. `hint`：一句「选择的依据」（如「需先本机跑起 ollama serve」），不是营销语
6. `verified_at`：**实际调通过**才填日期；照文档抄的留 `None`
7. `apply_url`：本地服务填 `None`，`is_local: true`
8. 同步下游的 i18n：sigil 的 `scripts/check-preset-i18n.py` 会报缺的 `label_key` / `hint_key`

## 五、改完必跑

```bash
cargo xtask gen-docs                            # 重新生成 docs/providers.md
cargo test -p ai-profile --features client      # providers_md_in_sync 会拦下没重新生成的情况
cargo fmt --all --check
```

然后：

1. `CHANGELOG.md` 的「未发布」下写一条（新模型 / 默认值变更要写**为什么**）
2. 提交说明里注明 id 的核对来源
3. 推送后走技能 `downstream-sync`：文档站同步 `reference/providers.md`、升级已接入的下游

## 版本位

加模型、加服务商、改 base_url、改默认 model → **patch**（数据更新，不动 API）。
改 `preset.key` → **major**（它是存量配置回填的依据）。详见 `api-contract`。

## 实例：2026-09-23 加 Opus 5.5 与 GPT-6 Sol / Luna（提交 428340b）

- `CLAUDE_MODELS` 头部加 `claude-opus-5-5`；官方档默认换它，中转档不换
- OpenAI 官方档加 `gpt-6-sol` / `gpt-6-luna`；Codex 档补齐 GPT-6 三款，默认不动
- OpenRouter 补 `anthropic/claude-opus-5.5`（点号）等
- Opus 5.5 不给静态限额（理由见第三节）
- 第一次推送前忘了 `gen-docs`，被 `providers_md_in_sync` 拦下 —— 守卫在起作用
