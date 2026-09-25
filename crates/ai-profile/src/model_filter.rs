//! 端点 `/models` 返回值的清洗：去重 + 滤掉不能对话的模型。
//!
//! # 为什么需要这一步
//!
//! 「获取模型」拿到的是端点的**全量**清单，不是"能聊天的模型"清单。聚合平台
//! （硅基流动、自建网关）一个端点能返回几十条，里面混着向量（bge / gte / e5 /
//! embedding）、重排（rerank）、语音（whisper / CosyVoice / TTS / ASR）、图像
//! （stable-diffusion / flux / kolors）、审核（guard / moderation）—— 真正能对话的
//! 被埋在中间，用户还可能顺手选到 `bge-reranker`，然后在第一次发消息时才报错。
//!
//! 这套特征词移植自 tauri-cc → reeve → sigil 三代实现，跨仓库共用同一套：
//! 厂商起名风格是跨平台一致的，各自维护一份只会各漏各的。
//!
//! # 🔴 采用排除法而非白名单
//!
//! 只滤掉**明确不能对话**的，未知名称一律放行。厂商上新模型的速度远快于我们更新
//! 特征词，白名单必然把新模型误藏起来 —— 而"藏起来"这个失败模式对用户是不可见的
//! （他不知道本该有这一条），比"多留几条让他自己判断"糟糕得多。

/// 非对话模型的名称特征（全部小写子串匹配）。
///
/// 公开是为了让规则**可以被照抄**：多语言规范（`spec/conformance/model_filter.json` 的 `rules`）
/// 直接导出这张表。只给用例、不给表，其他语言的实现只能对着用例凑，凑出来的必然漏词。
pub const NON_CHAT_MARKERS: &[&str] = &[
    "embedding",
    "embed",
    "bge-",
    "gte-",
    "e5-",
    "rerank",
    "whisper",
    "sensevoice",
    "cosyvoice",
    "tts",
    "asr",
    "stable-diffusion",
    "sdxl",
    "kolors",
    "flux",
    // 图像生成 / 编辑（实测漏网样本：Qwen/Qwen-Image-Edit-2509）。
    // 🔴 只能匹配 "image"，绝不能顺手加 "vl" 或 "omni" —— 那两类是**能对话**的
    // 多模态模型（Qwen3-VL-32B-Instruct 可以正常聊天），滤掉就是误伤。
    "image",
    // ── 以下每个词都来自本库生图 / 视频 / 配音预置里的真实模型名 ──
    // 守卫测试 `non_chat_presets_are_filtered` 用预置数据反向校验：
    // 新加一条非对话预置而这里漏了词，测试直接红，不必等用户在下拉里撞见
    "dall-e",    // OpenAI 生图：dall-e-3
    "seedream",  // 火山生图：doubao-seedream-4-0-…
    "seedance",  // 火山视频：doubao-seedance-1-0-pro-…
    "t2i",       // 文生图：wan2.2-t2i-flash、wanx2.1-t2i-turbo
    "i2v",       // 图生视频：Wan-AI/Wan2.2-I2V-A14B、I2V-01-Director
    "t2v",       // 文生视频：Wan2.1-T2V（与 i2v 成对出现）
    "img2video", // vidu/viduq3-pro_img2video
    "cogvideo",  // 智谱视频：cogvideox-3
    "hailuo",    // MiniMax 视频：MiniMax-Hailuo-02
    // 语音合成：fishaudio/fish-speech-1.5、MiniMax speech-02-hd。
    // 不会误伤对话模型 —— 语音对话模型叫 audio / realtime，不叫 speech
    "speech",
    // 图像描述专用（Qwen3-Omni-30B-A3B-Captioner）：只输出图注，不能对话
    "captioner",
    "ocr",
    "moderation",
    "guard",
];

/// 按**前缀**排除的模型 id（小写后比较）。与 [`NON_CHAT_MARKERS`] 分开：
/// 这类只在开头出现才有意义，当子串匹配会误伤名字中间碰巧带这几个字的模型。
pub const NON_CHAT_PREFIXES: &[&str] = &["lora/"];

/// 单个模型 id 看起来是不是对话模型。
pub fn is_chat_model_id(id: &str) -> bool {
    let s = id.trim().to_ascii_lowercase();
    if s.is_empty() {
        return false;
    }
    // `LoRA/xxx` 是微调变体（聚合平台会连带返回），不是可直接对话的基座
    if NON_CHAT_PREFIXES.iter().any(|p| s.starts_with(p)) {
        return false;
    }
    !NON_CHAT_MARKERS.iter().any(|m| s.contains(m))
}

/// 清洗结果：`models` 是可用清单，`dropped` 是被滤掉的条数（用于提示文案）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CleanedModels {
    /// 可用于对话的模型清单（已去重）
    pub models: Vec<String>,
    /// 被滤掉的条数，供「已滤掉 N 个向量 / 重排 / 语音等」这类提示文案使用
    pub dropped: usize,
    /// 被滤掉的模型 id（保持端点返回的顺序），`len() == dropped`。
    ///
    /// 给「一个配置同时挂对话与生图 / 配音 / 向量模型」的调用方用（onestop）：
    /// 它们要的不是"只留能聊天的"，而是"能聊天的排前面、其余也别丢"。
    pub dropped_models: Vec<String>,
}

/// 去重 + 过滤。中转站偶尔返回重复 id，先去重再过滤。
///
/// 🔴 全被滤光时**原样返回去重后的清单**（并把 `dropped` 记 0）：那说明这套特征词
/// 在这个端点上判错了，此时宁可把原始清单摆给用户看，也不能给他一个空下拉 ——
/// 「增强而非依赖」，任何一步出错都不该让用户卡在"选不了模型"。
pub fn clean_fetched_models<I, S>(ids: I) -> CleanedModels
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        let t = id.as_ref().trim();
        if t.is_empty() {
            continue;
        }
        if !unique.iter().any(|x| x == t) {
            unique.push(t.to_string());
        }
    }
    let kept: Vec<String> = unique
        .iter()
        .filter(|s| is_chat_model_id(s))
        .cloned()
        .collect();
    if kept.is_empty() {
        return CleanedModels {
            models: unique,
            dropped: 0,
            dropped_models: Vec::new(),
        };
    }
    let dropped_models: Vec<String> = unique
        .into_iter()
        .filter(|s| !is_chat_model_id(s))
        .collect();
    CleanedModels {
        models: kept,
        dropped: dropped_models.len(),
        dropped_models,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_chat_models_and_drops_others() {
        let r = clean_fetched_models([
            "deepseek-flash",
            "BAAI/bge-large-zh-v1.5",
            "Qwen/Qwen3-VL-32B-Instruct", // 多模态但能对话 —— 必须保留
            "FunAudioLLM/CosyVoice2-0.5B",
            "Kwai-Kolors/Kolors",
        ]);
        assert_eq!(
            r.models,
            vec!["deepseek-flash", "Qwen/Qwen3-VL-32B-Instruct"]
        );
        assert_eq!(r.dropped, 3);
        assert_eq!(
            r.dropped_models,
            vec![
                "BAAI/bge-large-zh-v1.5",
                "FunAudioLLM/CosyVoice2-0.5B",
                "Kwai-Kolors/Kolors"
            ],
            "被滤掉的 id 要原样带回，顺序同端点"
        );
    }

    #[test]
    fn dedups_before_filtering() {
        let r = clean_fetched_models(["gpt-4o", " gpt-4o ", "gpt-4o"]);
        assert_eq!(r.models, vec!["gpt-4o"]);
        assert_eq!(r.dropped, 0);
    }

    /// 🔴 全被滤光 = 特征词在这个端点上判错了，原样返回而非给空下拉。
    #[test]
    fn returns_original_when_everything_filtered() {
        let r = clean_fetched_models(["bge-m3", "text-embedding-3-large"]);
        assert_eq!(r.models.len(), 2, "宁可摆出原始清单，也不能给空下拉");
        assert_eq!(r.dropped, 0);
        assert!(r.dropped_models.is_empty(), "已放回 models，不能再算一遍");
    }

    /// 多模态模型不能误伤 —— 这是加特征词时最容易犯的错。
    #[test]
    fn multimodal_chat_models_are_not_dropped() {
        assert!(is_chat_model_id("Qwen/Qwen3-VL-32B-Instruct"));
        assert!(is_chat_model_id("Qwen3-Omni-30B-A3B-Instruct"));
        assert!(!is_chat_model_id("Qwen3-Omni-30B-A3B-Captioner"));
        assert!(!is_chat_model_id("Qwen/Qwen-Image-Edit-2509"));
    }

    /// 🔴 用本库自己的预置当标准答案，双向校验特征词：
    /// - 生图 / 视频 / 配音预置里的每个模型都必须被滤掉 —— 否则用户对同一家端点点「获取模型」，
    ///   我们自己预置的生图模型会出现在对话下拉里（dall-e-3 就是这么被发现的）
    /// - 对话预置里的每个模型都必须放行 —— 加特征词最容易误伤的就是这边
    ///
    /// 非对话预置在 image / video / tts feature 后面，默认 feature 下这一半是空集；
    /// CI 的 `cargo test --workspace --all-features` 会覆盖到。
    #[test]
    fn non_chat_presets_are_filtered() {
        use crate::kind::Kind;
        let mut leaked = Vec::new();
        let mut hurt = Vec::new();
        for p in crate::preset::presets() {
            let ids = p.models.iter().map(|m| m.value).chain([p.model]);
            for id in ids.filter(|id| !id.is_empty()) {
                match (p.kind, is_chat_model_id(id)) {
                    (Kind::Chat, false) => hurt.push(format!("{}: {id}", p.key)),
                    (Kind::Chat, true) => {}
                    (_, true) => leaked.push(format!("{}: {id}", p.key)),
                    (_, false) => {}
                }
            }
        }
        assert!(
            leaked.is_empty(),
            "非对话模型漏进了对话清单，补特征词：{leaked:#?}"
        );
        assert!(hurt.is_empty(), "对话模型被误滤，特征词太宽：{hurt:#?}");
    }

    /// 新特征词不能误伤的对话模型：名字里碰巧带着相近片段
    #[test]
    fn new_markers_do_not_hurt_chat_models() {
        for id in [
            "doubao-seed-1-6-250615", // seed ≠ seedream / seedance
            "gpt-4o-audio-preview",   // 语音对话叫 audio，不叫 speech
            "gpt-4o-realtime-preview",
            "Qwen/Qwen2.5-VL-72B-Instruct",
            "glm-4.5v",
        ] {
            assert!(is_chat_model_id(id), "{id} 是对话模型，不能被滤掉");
        }
    }
}
