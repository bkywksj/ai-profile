# ai-profile 规范（与语言无关）

这个目录让**别的语言**也能实现 ai-profile：数据直接拿，规则照着用例写，跑通用例即与 Rust 版行为一致。

| 文件 | 内容 |
|------|------|
| `presets.json` | 全部服务商预置 + 按厂商聚合的目录，与 crate 同源 |
| `conformance/endpoint.json` | 端点拼接：`join_api_path` / `join_chat_endpoint` / `anthropic_base_url` / `ends_with_version_segment` |
| `conformance/model_filter.json` | 模型清单清洗：`is_chat_model_id` / `clean_fetched_models` |
| `conformance/models_response.json` | `/models` 响应解析：模型 id 与限额 |
| `conformance/diagnose.json` | 验证失败的错误判定：`diagnose` / `suggest_url` / `check_required_fields` |
| `conformance/ai_profile.json` | `ai.profile` 解析（宽进）与生成（严出） |
| `conformance/limits.json` | token 限额三层合并 |
| `conformance/preset_lookup.json` | 从已存配置反推预置：`infer_preset_key` / `model_limits`（预置登记的静态限额） |
| `conformance/history.json` | 超长报错识别：`is_context_overflow`（历史裁剪本身暂无用例） |

🔴 **全部由 `cargo xtask gen-spec` 生成，请勿手改。** 期望值是 Rust 参考实现现场算出来的，
守卫测试 `spec_files_in_sync` 保证它们与代码同步 —— 改了规则忘了重新生成，CI 会红。

## 文件格式

每个文件都带同样的头：

```json
{
  "specVersion": 1,
  "crateVersion": "0.1.1",
  "title": "端点拼接",
  "description": "规则摘要",
  "generatedBy": "cargo xtask gen-spec …",
  "cases": [ … ]
}
```

- `specVersion`：**格式**版本。只在字段改名、结构调整时加 1；加用例、改期望值不算。
  你的实现遇到不认识的 `specVersion` 应当拒绝运行，而不是按旧格式硬读。
- `crateVersion`：生成这批用例的 Rust crate 版本。用例的期望值跟着它走。

每条用例：

```json
{ "fn": "join_api_path", "name": "可选的说明", "input": { … }, "expected": … }
```

- `fn` 对应一个要实现的函数，`input` 是它的参数（键名用 camelCase），`expected` 是返回值。
- `expected` 的键名照 Rust 版的线格式原样给出：数据结构是 camelCase，**错误对象是 snake_case**
  （如 `suggested_url`、`requested_url`）。这是已发布的格式，实现时照抄，别统一成一种。
- 返回 `Result` 的函数，`expected` 是 `{"ok": …}` 或 `{"error": …}` 二选一。
- 比较时按 **JSON 值**比较（对象键无序），不要比字符串。`to_profile` 的输出也先解析再比。
- 键按字母序排列只是生成器的输出习惯，没有语义。

## 怎么接进你的测试

以 Python 为例（其他语言同理）：

```python
import json, pathlib, pytest
from my_ai_profile import join_api_path, join_chat_endpoint, anthropic_base_url

FNS = {
    "join_api_path": lambda i: join_api_path(i["base"], i["path"]),
    "join_chat_endpoint": lambda i: join_chat_endpoint(i["base"], i["path"]),
    "anthropic_base_url": lambda i: anthropic_base_url(i["base"]),
}
spec = json.loads(pathlib.Path("spec/conformance/endpoint.json").read_text("utf-8"))
assert spec["specVersion"] == 1

@pytest.mark.parametrize("case", spec["cases"], ids=lambda c: f'{c["fn"]}:{c["input"]}')
def test_endpoint(case):
    assert FNS[case["fn"]](case["input"]) == case["expected"]
```

建议把 `spec/` 整个目录复制进你的仓库并记下来源的 `crateVersion`，升级时整体替换，再看哪些用例红了。

## 规则摘要

用例是权威，这里只是帮你读懂用例在测什么。

### 端点拼接

- **OpenAI 兼容地址原样使用，不替用户补 `/v1`。** 智谱是 `/v4`、Gemini 是 `/v1beta/openai`，
  猜错了反而把对的地址改坏。`join_api_path` 只做两处容错：剥掉误填的对话端点后缀
  （`chat/completions`、`messages`）和旧契约遗留的末尾 `#`。
- **Anthropic 协议是唯一例外**：官方 SDK 约定 base 不带版本段、由 SDK 补 `/v1`，用户照着服务商文档
  填主机名是对的。`anthropic_base_url`：末段不是 `v<纯数字>` 就补 `/v1`；末尾带 `#` 表示「别替我补」。
- `join_chat_endpoint`：用户把**完整对话端点**粘进来时原样使用；`path` 为 `messages` 时先过
  `anthropic_base_url`。这条捷径只对对话端点成立 —— 获取模型走 `join_api_path(base, "models")`。

### 模型清单清洗

排除法：只滤掉名字里带明确非对话特征的（向量 / 重排 / 语音 / 生图 / OCR / 审核…，大小写不敏感；
`LoRA/` 前缀的微调变体也滤），**未知名称一律放行** —— 白名单会把新模型悄悄藏起来，而用户不会知道
本该有那一条。清洗同时去空白、去重、保持原顺序；全部被滤光时原样返回（`dropped` 为 0），
不给用户一个空下拉。特征词表见 crate 源码 `model_filter.rs`。

### `/models` 响应解析

`{"data":[{"id":…}]}` 与裸数组都接受。限额只收录**报了**的模型；OpenRouter 优先取
`top_provider`（当前实际服务商的限制比模型标称的更准）；写成字符串的数字也接受。

### 验证错误判定

`code` 是判别字段：401/403 → `auth_failed`；404 → `not_found`，看不出版本段时带 `suggested_url`；
其余 → `malformed`。`detail` 优先取响应体里的 `error.message` / `message`。
`check_required_fields` 在发请求前按预置的 `extraFields` 查必填项，缺了或只有空白 → `missing_extra_field`。
`unreachable` 由网络层产生（连不上、超时）。`model_not_found`、`protocol_mismatch` 目前 Rust 版并不产生，
是为后续预留的；「模型不在清单里」用验证成功结果里的 `modelInList: false` 表示，而不是报错。

### ai.profile

格式定义见 <https://ai-profile.ruoyi.plus/api/protocol>。解析**宽进**：`baseURL` / `baseUrl` /
`base_url` 等拼写都认，单条与打包统一返回列表，OAuth 条目跳过并计入 `skipped`，没给 model 用调用方的
兜底值并标 `modelFallback`。生成**严出**：只产出规范写法。

### 限额合并

优先级 用户手填 > 端点上报 > 预置，从高到低两两合并。高层两个字段都空时整条换成低层（连 `source`
一起 —— 空的用户设置不能冒充来源）；否则逐字段补空，`source` 保留高层。

## 为其他语言实现时的边界

- **只实现你需要的部分。** 只想要预置清单，读 `presets.json` 就够了，不必实现全部函数。
- **不要改用例去迁就实现。** 觉得某条期望值不合理，请到 crate 仓库提 issue —— 规则改在 Rust 版，
  重新生成后所有语言一起跟上。
- 目前没有官方维护的其他语言实现。你写了一个，欢迎告诉我们，会列进文档站。
