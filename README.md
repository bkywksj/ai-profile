# ai-profile

> 桌面应用的 AI 模型服务配置层 —— 服务商预置、`ai.profile` 互通协议、端点拼接、连通性验证，
> 以及生图 / 视频 / 配音的调用实现。
>
> Shared model-service layer for desktop apps: provider presets, the `ai.profile` interchange format,
> endpoint resolution, connectivity verification, and image / video / TTS calls.

[![crates.io](https://img.shields.io/crates/v/ai-profile)](https://crates.io/crates/ai-profile)
[![docs.rs](https://img.shields.io/docsrs/ai-profile)](https://docs.rs/ai-profile)
[![license](https://img.shields.io/badge/license-MIT-blue)](https://opensource.org/licenses/MIT)

📖 **完整文档：<https://ai-profile.ruoyi.plus>**

## 这是什么

多个应用都需要「让用户配置 AI 模型服务」这一整套能力。它们此前在每个应用里各写一遍，
模型 id 一变就必然漏改（DeepSeek 下线 `deepseek-chat` 别名后，有个应用两个月后才发现「点开即报错」）。

本库把**变动最频繁、跨应用差异为零**的那部分抽出来：

| 能力 | 内容 |
|---|---|
| 预置 | 对话 25 家 + 生图 / 视频 / 配音 17 条：地址、模型、协议、专有字段、密钥申请页；按厂商聚合成目录 |
| 定制目录 | 应用可以增删改筛预置、加自己的私有服务商（`PresetCatalog`） |
| `ai.profile` | 配置在应用之间粘贴互通的交换格式，支持多条打包 |
| 端点 | 拼接规则、模型清单清洗 |
| 验证 | 「获取模型」零成本验证，结构化错误（能做出「一键补 `/v1`」而不只是一行红字） |
| 限额 | 上下文窗口 / 输出上限的四层来源（用户 > 端点上报 > 预置 > 未知），历史裁剪、超长识别 |
| 多模态调用 | 生图（OpenAI 兼容 / 通义万相）、视频（六套提交+轮询协议）、配音（OpenAI 兼容 / 火山） |

**不进本库**：密钥存储与加密、数据库与 CRUD、对话请求与流式解析、视频任务编排、只属于某个应用的服务商。

## 安装

```toml
[dependencies]
# 按需三选一
ai-profile = "0.1"                                             # 只要预置与纯函数：零 HTTP 依赖，可编到移动端
ai-profile = { version = "0.1", features = ["client"] }        # + 零成本验证
ai-profile = { version = "0.1", features = ["client", "image", "video", "tts"] }  # + 多模态调用
```

最低 Rust 版本 1.88（由开启多模态时的图像解码依赖决定）。

## 30 秒上手

```rust
use ai_profile::{preset_by_key, vendors, Kind};

// 给用户列厂商：只做对话的应用不会看到「只提供视频」的厂商
for v in vendors(&[Kind::Chat]) {
    println!("{} [{}]", v.label, v.group_label);
}

// 用户选了一家，取预置填表单
let p = preset_by_key("deepseek").expect("预置存在");
println!("{:?} / {}", p.base_url, p.model);
```

验证、导入导出、限额、多模态的用法见[快速开始](https://ai-profile.ruoyi.plus/guide/quick-start)。

## 三条设计铁律

1. **`base_url` 原样使用，绝不推断版本段** —— 各家不统一（多数 `/v1`、智谱 `/v4`、Gemini `/v1beta/openai`）。
   推断错的代价是隐性的：用户照文档填对了，库悄悄加一段，他只看到 404。
2. **默认模型选「够用档」而非「最强档」** —— 把旗舰塞进默认值，等于替用户做了一个他没同意的花钱决定。
3. **模型清单清洗用排除法，不用白名单** —— 厂商上新远快于特征词更新，白名单必然把新模型误藏。

## License

MIT
