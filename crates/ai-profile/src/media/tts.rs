//! 漫剧化 V3·M2 · TTS 配音供应商抽象（TtsProvider）。
//!
//! 与图生视频（异步两段式 submit→poll）不同，TTS 多是「一次请求出音频」：
//! POST {endpoint}/audio/speech（OpenAI 兼容）body {model, input, voice, response_format}
//! → 直接返回音频字节流。首个实现 `SiliconFlowTtsProvider` 适配硅基流动 CosyVoice
//! （OpenAI /audio/speech 兼容）。trait 留扩展点（火山语音 / 阿里 CosyVoice 后续加）。

use super::truncate;
use super::MediaError;

/// 音色选项（音色目录项）。`id` = 供应商音色名 / speaker（如 alex），调用方存起来、下拉与 AI 推荐用。
///
/// 线格式与 StoryLoom 的 `models::VoiceOption` 一致（字段名不转 camelCase），前端无需改动。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VoiceOption {
    /// 音色 id（speaker 名，如 alex）
    pub id: String,
    /// 中文气质标签（如「沉稳男声」）
    pub label: String,
    /// 性别：male / female
    pub gender: String,
}

/// 音色目录（供应商感知，按 model 分发）：
/// - 火山豆包（model=volcano_tts）→ 火山 voice_type 目录（含多情感音色）；
/// - 硅基流动 CosyVoice / fish-speech，或 model 为空（缺省按 CosyVoice）→ CosyVoice 目录；
/// - 其它未知供应商 → 空（前端下拉允许手填 voice_type 兜底）。
pub fn voice_catalog(model: &str) -> Vec<VoiceOption> {
    let m = model.to_ascii_lowercase();
    // 火山豆包（火山引擎 TTS）：model 形如 volcano_tts
    if m.contains("volcano") || m.contains("volc") {
        return volc_voice_catalog();
    }
    // 智谱 glm-tts（中宇智行等 OpenAI 兼容聚合站）：固定枚举音色（tongtong 等）
    if m.contains("glm") {
        return glm_voice_catalog();
    }
    // 硅基流动 CosyVoice / fish-speech，或空（缺省按 CosyVoice，设置页/AI 推荐可先用）
    if m.is_empty() || m.contains("cosyvoice") || m.contains("fish") {
        return cosyvoice_voice_catalog();
    }
    Vec::new()
}

/// 音色「音质档案」（漫剧化 V5·选角）：年龄段 + 音质特征。
/// 喂给 AI 推荐音色的提示词，让 AI 据角色年龄/气质匹配声线
/// （如中年→低沉沉稳、少年→清亮、长者→苍老），避免「40 岁配少年音」。
/// 仅覆盖内置目录音色；未知 id 返回空串（AI 仅凭音色名+性别判断）。
/// 注：bigtts 音色音质为按官方音色名归纳的近似描述，供 AI 选角参考。
pub fn voice_timbre_hint(id: &str) -> &'static str {
    match id {
        // ── 火山大模型音色（bigtts）──
        "zh_female_meilinvyou_moon_bigtts" => "青年女声·甜美亲昵·温柔",
        "zh_male_beijingxiaoye_moon_bigtts" => "青年男声·京腔·爽朗明亮",
        "zh_female_wanwanxiaohe_moon_bigtts" => "青年女声·台湾腔·软糯活泼",
        "zh_male_M392_conversation_wvae_bigtts" => "青年男声·自然口语·亲切",
        "zh_female_shuangkuaisisi_moon_bigtts" => "青年女声·爽快利落·明快",
        "zh_male_wennuanahu_moon_bigtts" => "中年男声·低沉沉稳·温厚",
        "zh_female_tianmeixiaoyuan_moon_bigtts" => "少女声·甜美清脆·元气",
        "zh_male_jingqiangkanye_moon_bigtts" => "中年男声·京腔·豪爽侃气",
        // ── 经典 BV ──
        "BV700_streaming" => "青年女声·多情感·百搭",
        "BV701_streaming" => "中年男声·多情感·浑厚（适合旁白/长者）",
        "BV001_streaming" => "青年女声·通用",
        "BV002_streaming" => "青年男声·通用",
        // ── CosyVoice ──
        "alex" => "中年男声·沉稳",
        "benjamin" => "中年男声·低沉",
        "charles" => "青年男声·磁性",
        "david" => "青年男声·欢快明亮",
        "anna" => "中年女声·沉稳",
        "bella" => "青年女声·激情有力",
        "claire" => "青年女声·温柔",
        "diana" => "青年女声·欢快",
        // ── 智谱 glm-tts（中宇智行）音色 ──
        "tongtong" => "少女声·甜美童真·清亮",
        "jieyu" => "青年女声·知性温婉",
        "tianmeng_shaonv" => "少女声·甜萌·元气",
        "nuanyang_nvsheng" => "青年女声·温暖柔和",
        "jieshuo_nansheng" => "中年男声·解说旁白·沉稳",
        "jingdian_yueyu" => "粤语女声·经典",
        _ => "",
    }
}

/// 把 (id,label,gender) 三元组列表转成 VoiceOption 向量。
fn to_voices(rows: &[(&str, &str, &str)]) -> Vec<VoiceOption> {
    rows.iter()
        .map(|(id, label, gender)| VoiceOption {
            id: (*id).to_string(),
            label: (*label).to_string(),
            gender: (*gender).to_string(),
        })
        .collect()
}

/// 硅基流动 CosyVoice2-0.5B 系统音色。
fn cosyvoice_voice_catalog() -> Vec<VoiceOption> {
    to_voices(&[
        ("alex", "沉稳男声", "male"),
        ("benjamin", "低沉男声", "male"),
        ("charles", "磁性男声", "male"),
        ("david", "欢快男声", "male"),
        ("anna", "沉稳女声", "female"),
        ("bella", "激情女声", "female"),
        ("claire", "温柔女声", "female"),
        ("diana", "欢快女声", "female"),
    ])
}

/// 智谱 glm-tts 音色（中宇智行聚合站实测枚举，2026-07）。
/// voice 为固定枚举名（不可自定义克隆），只出 wav/pcm（见 SiliconFlowTtsProvider is_glm 分支）。
/// gender 按音色名义推断供 AI 选角匹配；jingdian_yueyu 为粤语音色，供角色设定粤语时选用。
fn glm_voice_catalog() -> Vec<VoiceOption> {
    to_voices(&[
        ("tongtong", "童童·甜美", "female"),
        ("jieyu", "婕语·知性", "female"),
        ("tianmeng_shaonv", "甜萌少女", "female"),
        ("nuanyang_nvsheng", "暖阳女声", "female"),
        ("jieshuo_nansheng", "解说男声", "male"),
        ("jingdian_yueyu", "经典粤语", "female"),
    ])
}

/// 火山豆包（火山引擎 TTS）音色 voice_type。
/// 🔴 默认用「语音合成大模型」(`*_bigtts`) 音色 —— LLM 端到端，拟人、有韵律与情感，
/// 解决老 BV 音色（拼接式）机械感重、无情感的问题。所有 bigtts 音色都支持 emotion/emotion_scale
/// （详见 `is_volc_emotion_voice`），配合分镜 AI 标注的情感「跟随语气」更自然（漫剧化 V5·拟真配音）。
/// 经典 BV 音色保留在末尾兜底（部分账号未开通大模型时可用）。
/// 注：实际可用音色取决于火山控制台「语音合成大模型」开通情况，下拉支持手填自定义 voice_type 兜底。
fn volc_voice_catalog() -> Vec<VoiceOption> {
    to_voices(&[
        // ── 大模型音色（拟真·有情感，推荐）──
        (
            "zh_female_meilinvyou_moon_bigtts",
            "魅力女友·大模型",
            "female",
        ),
        (
            "zh_male_beijingxiaoye_moon_bigtts",
            "北京小爷·大模型",
            "male",
        ),
        (
            "zh_female_wanwanxiaohe_moon_bigtts",
            "湾湾小何·大模型",
            "female",
        ),
        (
            "zh_male_M392_conversation_wvae_bigtts",
            "对话男声·大模型",
            "male",
        ),
        (
            "zh_female_shuangkuaisisi_moon_bigtts",
            "爽快思思·大模型",
            "female",
        ),
        ("zh_male_wennuanahu_moon_bigtts", "温暖阿虎·大模型", "male"),
        (
            "zh_female_tianmeixiaoyuan_moon_bigtts",
            "甜美小源·大模型",
            "female",
        ),
        (
            "zh_male_jingqiangkanye_moon_bigtts",
            "京腔侃爷·大模型",
            "male",
        ),
        // ── 经典 BV 音色（兜底；机械感较重，未开通大模型时用）──
        ("BV700_streaming", "灿灿·多情感女声（经典）", "female"),
        ("BV701_streaming", "擎苍·多情感男声（经典）", "male"),
        ("BV001_streaming", "通用女声（经典）", "female"),
        ("BV002_streaming", "通用男声（经典）", "male"),
    ])
}

/// TTS 调用配置（含解密后的 Key，仅 Rust 内部传递）
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// 端点（OpenAI 兼容 base url，如 https://api.siliconflow.cn/v1）
    pub endpoint: String,
    /// 模型 id
    pub model: String,
    /// 明文密钥（仅在内存中传递，不落盘、不写日志）
    pub api_key: String,
}

/// TTS 合成入参
#[derive(Debug, Clone)]
pub struct TtsParams {
    /// 待合成文本（对白 / 旁白）
    pub text: String,
    /// 音色 id（如 CosyVoice 音色标识；空串则用供应商默认音色）
    pub voice: String,
    /// 输出音频格式（mp3/wav/opus…；默认 mp3）
    pub format: String,
    /// 情感语气标签（中文，漫剧化 V3·M2.6；空=普通合成；火山多情感音色据此「跟随语气」更拟人）
    pub emotion: String,
    /// 配音演绎风格（漫剧化 V5·可干预：角色卡自定义语气如「傲娇 / 气声」，空=不干预）。
    /// 仅 CosyVoice 用（拼进 `<|endofprompt|>`）；火山/GLM 忽略此自由文本。
    pub style: String,
}

impl Default for TtsParams {
    fn default() -> Self {
        Self {
            text: String::new(),
            voice: String::new(),
            format: "mp3".to_string(),
            emotion: String::new(),
            style: String::new(),
        }
    }
}

/// TTS 配音供应商抽象（一次请求出音频字节）。
#[allow(async_fn_in_trait)]
pub trait TtsProvider {
    /// 合成语音，返回音频字节（调用方据 params.format 决定落盘扩展名）
    async fn synthesize(&self, params: &TtsParams) -> Result<Vec<u8>, MediaError>;
}

/// 硅基流动 CosyVoice（OpenAI /audio/speech 兼容）TTS 实现
pub struct SiliconFlowTtsProvider {
    client: reqwest::Client,
    config: TtsConfig,
}

impl SiliconFlowTtsProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: TtsConfig) -> Self {
        Self {
            // 带超时客户端：配音请求兜底 180s，杜绝供应商挂起永久阻塞（审查 P0）
            client: super::http::default_client(),
            config,
        }
    }

    /// 拼 /audio/speech 地址（容忍端点尾斜杠 / 是否已含路径）
    fn speech_url(&self) -> String {
        let e = self.config.endpoint.trim_end_matches('/');
        if e.ends_with("/audio/speech") {
            e.to_string()
        } else {
            format!("{e}/audio/speech")
        }
    }
}

impl TtsProvider for SiliconFlowTtsProvider {
    async fn synthesize(&self, params: &TtsParams) -> Result<Vec<u8>, MediaError> {
        if params.text.trim().is_empty() {
            return Err(MediaError::InvalidInput("配音文本为空".into()));
        }
        // 智谱 glm-tts（中宇智行等聚合站）与 CosyVoice 同走 OpenAI /audio/speech，但两点不同，故先分派：
        // ① response_format 仅支持 wav/pcm（不出 mp3，发 mp3 直接 400）；
        // ② voice 为固定枚举音色名（tongtong 等），不用 CosyVoice 的 `<model>:<speaker>` 拼法。
        let is_glm = self.config.model.to_ascii_lowercase().contains("glm");
        let fmt = if is_glm {
            "wav" // glm-tts 只收 wav/pcm；统一 wav（service 层落盘扩展名同步为 .wav）
        } else if params.format.trim().is_empty() {
            "mp3"
        } else {
            params.format.as_str()
        };
        // 音色：
        let voice = if is_glm {
            // glm-tts：固定枚举音色名。空 → 缺省 tongtong（童童·甜美）；
            // 防呆：非 glm 音色（如切换供应商后残留的 CosyVoice「alex」/火山「BVxxx」）→ 回退 tongtong，
            // 避免把它当音色名发给 glm 而静默用错音色/报错（与 CosyVoice/火山防呆对称）。
            let v = params.voice.trim();
            if v.is_empty() {
                "tongtong".to_string()
            } else if is_glm_voice(v) {
                v.to_string()
            } else {
                log::warn!(
                    "glm-tts 配音收到非 glm 音色 '{v}'（疑似切换供应商后残留），已回退 tongtong；请在 设置→配音 改用 glm 音色"
                );
                "tongtong".to_string()
            }
        } else {
            // 硅基流动 CosyVoice 强制要 voice（否则 HTTP 400 code 20025
            // "voice or reference audio should be set"）。系统音色格式 <model>:<speaker>：
            // - 空 → 缺省沉稳男声 alex；
            // - 裸 speaker 名（如 anna，无 ":"）→ 拼 <model>:anna（voice_id 存音色名、换模型不失效）；
            // - 已含 ":"（完整系统/自定义音色 id）→ 原样用。
            let v = params.voice.trim();
            if is_volc_voice(v) {
                // 防呆：火山音色（BVxxx_streaming…）串味到 CosyVoice（切换供应商后残留），退回 CosyVoice 默认音色
                log::warn!(
                    "CosyVoice 配音收到火山音色 '{v}'（疑似切换供应商后旁白/角色音色残留），已回退 alex；请在 设置→配音 改用 CosyVoice 音色"
                );
                format!("{}:alex", self.config.model.trim())
            } else if v.is_empty() {
                format!("{}:alex", self.config.model.trim())
            } else if v.contains(':') {
                v.to_string()
            } else {
                format!("{}:{}", self.config.model.trim(), v)
            }
        };
        // 演绎意图（漫剧化 V5·统一演绎层）：
        // - CosyVoice 支持 `<|endofprompt|>` 指令前缀按情感演绎，语速也随情感偏移（愤怒快、悲伤慢）；
        // - glm-tts 不认该语法、且自带「按文本上下文预测情感」，故原样发文本、不加 speed，
        //   避免指令被当正文读出而穿帮。
        // 中性情感 → 不注入前缀、不下发 speed（保持默认 1.0），完全等于旧行为。
        let dir = VoiceDirection::new(&params.emotion, &params.style);
        let input = match (is_glm, dir.cosyvoice_instruction()) {
            (false, Some(instr)) => format!("{instr}<|endofprompt|>{}", params.text),
            _ => params.text.clone(),
        };
        let mut body = serde_json::json!({
            "model": self.config.model,
            "input": input,
            "voice": voice,
            "response_format": fmt,
        });
        // CosyVoice 语速仅在有明确情感时下发（中性不加，保持供应商默认）；glm 保守不加。
        if !is_glm && dir.cosyvoice_emotion_word().is_some() {
            body["speed"] = serde_json::json!(dir.cosyvoice_speed());
        }

        let resp = self
            .client
            .post(self.speech_url())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交配音合成失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "配音合成 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MediaError::Failed(format!("读取配音音频字节失败: {e}")))?;
        if bytes.is_empty() {
            return Err(MediaError::Failed("配音合成返回空音频".into()));
        }
        Ok(bytes.to_vec())
    }
}

/// 演绎意图（provider 无关 · 漫剧化 V5·统一配音演绎层）。
/// 把分镜 AI 标注的中文情感（`shot.emotion`）抽象成各家 TTS 都能翻译的中间层，
/// 避免「每接一家配音就重抄一遍情感逻辑」。各 provider 各自 render：
/// - 火山：`volc_emotion()` 枚举 + `prosody()` 的 (speed_ratio, loudness_ratio, emotion_scale) 结构化参数；
/// - CosyVoice（硅基流动）：`cosyvoice_emotion_word()` 拼 `<|endofprompt|>` 指令前缀 + `cosyvoice_speed()` 语速；
/// - GLM-TTS（智谱）：模型自带「按文本上下文预测情感」，本层**不**为它注入指令
///   —— 它不认 CosyVoice 的 `<|endofprompt|>` 语法，硬注入会被当正文读出而穿帮。
#[derive(Debug, Clone)]
pub struct VoiceDirection {
    /// 原始中文情感标签（来自分镜 AI 标注；空串 / 中性 = 无特定情感，不下发任何情感控制）。
    emotion: String,
    /// 角色自定义演绎风格（漫剧化 V5·可干预；来自角色卡，自由文本；仅 CosyVoice 拼进指令，火山/GLM 忽略）。
    style: String,
}

impl VoiceDirection {
    /// 由情感标签与演绎风格构造（均可为空）。
    pub fn new(emotion: &str, style: &str) -> Self {
        Self {
            emotion: emotion.trim().to_string(),
            style: style.trim().to_string(),
        }
    }

    /// 中文情感标签 → 火山多情感 emotion 枚举（无映射/中性 → None，不下发情感参数）。
    pub fn volc_emotion(&self) -> Option<&'static str> {
        let l = self.emotion.as_str();
        if l.is_empty() {
            return None;
        }
        if l.contains("开心") || l.contains("高兴") || l.contains("喜") || l.contains("快乐")
        {
            Some("happy")
        } else if l.contains("悲") || l.contains("难过") || l.contains("伤心") || l.contains("哭")
        {
            Some("sad")
        } else if l.contains("怒") || l.contains("愤") || l.contains("生气") {
            Some("angry")
        } else if l.contains("惊讶") || l.contains("震惊") || l.contains("吃惊") {
            Some("surprise")
        } else if l.contains("恐") || l.contains("惧") || l.contains("害怕") || l.contains("紧张")
        {
            Some("fear")
        } else if l.contains("厌") || l.contains("恶") || l.contains("嫌") {
            Some("hate")
        } else {
            None // 平静/中性/未知 → 不下发，走音色默认语气
        }
    }

    /// 情感 → 语气曲线（漫剧化 V5·演绎）：映射成 (speed_ratio, loudness_ratio, emotion_scale)，
    /// 让愤怒又快又响、悲伤又慢又轻——情绪立体，不再「平读」。
    /// 取值在火山合法区间内：speed_ratio[0.1,2]、loudness_ratio[0.5,2]、emotion_scale[1,5]。
    /// 平静/中性/未知 → (1.0, 1.0, 4) 自然值。speed/loudness 对所有音色生效；emotion_scale 仅情感音色用。
    pub fn prosody(&self) -> (f64, f64, i64) {
        let l = self.emotion.as_str();
        if l.contains("怒") || l.contains("愤") || l.contains("生气") {
            (1.1, 1.3, 5) // 愤怒：快、响、强
        } else if l.contains("悲") || l.contains("难过") || l.contains("伤") || l.contains("哭")
        {
            (0.9, 0.85, 4) // 悲伤：慢、轻
        } else if l.contains("恐") || l.contains("惧") || l.contains("害怕") || l.contains("紧张")
        {
            (1.1, 0.9, 4) // 恐惧：急、偏轻
        } else if l.contains("惊讶") || l.contains("震惊") || l.contains("吃惊") {
            (1.1, 1.15, 4) // 惊讶：快、略响
        } else if l.contains("开心") || l.contains("高兴") || l.contains("喜") || l.contains("快乐")
        {
            (1.05, 1.1, 4) // 开心：略快、略响
        } else {
            (1.0, 1.0, 4) // 平静/中性
        }
    }

    /// CosyVoice `<|endofprompt|>` 指令用的中文情感词（中性 → None，不注入指令）。
    /// 复用 `volc_emotion()` 的分类归一到 CosyVoice 可理解的基本情感，避免另立一套关键词边界。
    pub fn cosyvoice_emotion_word(&self) -> Option<&'static str> {
        match self.volc_emotion() {
            Some("happy") => Some("高兴"),
            Some("sad") => Some("悲伤"),
            Some("angry") => Some("愤怒"),
            Some("surprise") => Some("惊讶"),
            Some("fear") => Some("恐惧"),
            Some("hate") => Some("厌恶"),
            _ => None,
        }
    }

    /// CosyVoice 语速：复用 `prosody()` 的 speed 分量（0.9~1.1，落在 CosyVoice 合法区间 0.25~4.0 内）。
    pub fn cosyvoice_speed(&self) -> f64 {
        self.prosody().0
    }

    /// CosyVoice `<|endofprompt|>` 指令前缀：把「演绎风格（角色卡）+ 情感（分镜）」组合成一句自然语言指令。
    /// 二者都无 → None（不注入，等于旧行为）。风格与情感可叠加，如「请用傲娇、愤怒的语气说。」。
    pub fn cosyvoice_instruction(&self) -> Option<String> {
        let style = self.style.trim();
        match (style.is_empty(), self.cosyvoice_emotion_word()) {
            (true, None) => None,
            (false, None) => Some(format!("请用{style}的语气说。")),
            (true, Some(emo)) => Some(format!("请用{emo}的语气说。")),
            (false, Some(emo)) => Some(format!("请用{style}、{emo}的语气说。")),
        }
    }
}

/// 是否火山支持 emotion 参数的音色：
/// - 所有「语音合成大模型」音色（`*_bigtts`）—— 拟真、原生支持 emotion/emotion_scale；
/// - 经典多情感音色 BV700/BV701。
///
/// 其余经典 BV 音色不支持情感（发了也无效，故不发，避免无谓参数）。
fn is_volc_emotion_voice(voice: &str) -> bool {
    let v = voice.to_ascii_lowercase();
    v.ends_with("_bigtts") || v.contains("bv700") || v.contains("bv701")
}

/// 是否 glm-tts（智谱）固定枚举音色 id。glm 配音防呆用：非 glm 音色（切供应商后残留的
/// CosyVoice/火山音色）串味到 glm 时回退默认音色，避免静默用错音色/报错。glm 音色都是小写下划线枚举名。
fn is_glm_voice(voice: &str) -> bool {
    matches!(
        voice.trim(),
        "tongtong"
            | "jieyu"
            | "tianmeng_shaonv"
            | "nuanyang_nvsheng"
            | "jieshuo_nansheng"
            | "jingdian_yueyu"
    )
}

/// 是否 CosyVoice（硅基流动）系统音色 id。火山配音防呆用：这些 id 串味到火山（如切换供应商后
/// 旁白/角色音色残留）会让火山报 `[resource_id=] requested resource not granted`（火山没有这些音色名），
/// 检测到则退回火山通用音色，避免天书般的 3001。CosyVoice 音色都是小写单词，与火山 BVxxx 无重叠，无误判。
fn is_cosyvoice_voice(voice: &str) -> bool {
    matches!(
        voice.trim().to_ascii_lowercase().as_str(),
        "alex" | "benjamin" | "charles" | "david" | "anna" | "bella" | "claire" | "diana"
    )
}

/// 是否火山 voice_type（BVxxx_streaming / *_bigtts）。CosyVoice 配音防呆用：火山音色串味到
/// CosyVoice 时退回 CosyVoice 默认音色（CosyVoice 音色不以 bv 开头、不带 _streaming/_bigtts 后缀，无误判）。
fn is_volc_voice(voice: &str) -> bool {
    let v = voice.trim().to_ascii_lowercase();
    v.starts_with("bv") || v.ends_with("_streaming") || v.ends_with("_bigtts")
}

/// 字节豆包 TTS 配置（火山 openspeech 专有协议；漫剧化 V3·M5 W3）
#[derive(Debug, Clone)]
pub struct VolcTtsConfig {
    /// 端点（默认 https://openspeech.bytedance.com/api/v1/tts）
    pub endpoint: String,
    /// 应用 appid（火山语音控制台）
    pub appid: String,
    /// 业务集群（默认 volcano_tts）
    pub cluster: String,
    /// 访问令牌（access_token）
    pub access_token: String,
}

/// 字节豆包 TTS（火山 openspeech）实现 —— 专有协议（非 OpenAI 兼容）：
///   POST {endpoint} 头 Authorization: Bearer;{token}
///   body {app{appid,token,cluster}, user{uid}, audio{voice_type,encoding}, request{reqid,text,operation:"query"}}
///   响应 {code:3000, data:"<base64 音频>"}
pub struct VolcTtsProvider {
    client: reqwest::Client,
    config: VolcTtsConfig,
}

impl VolcTtsProvider {
    /// 用给定配置构造（内部自带带超时的 HTTP 客户端）。
    pub fn new(config: VolcTtsConfig) -> Self {
        // 带超时客户端：杜绝供应商挂起永久阻塞（审查 P0）
        Self {
            client: super::http::default_client(),
            config,
        }
    }
}

#[derive(serde::Deserialize)]
struct VolcTtsResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<String>,
}

impl TtsProvider for VolcTtsProvider {
    async fn synthesize(&self, params: &TtsParams) -> Result<Vec<u8>, MediaError> {
        if params.text.trim().is_empty() {
            return Err(MediaError::InvalidInput("配音文本为空".into()));
        }
        if self.config.appid.trim().is_empty() {
            return Err(MediaError::InvalidInput(
                "字节豆包配音需配置 App ID（设置 → 配音 → 添加时填写）".into(),
            ));
        }
        // 音色（voice_type）：空则给大模型拟真音色兜底（替代老 BV001，默认即拟真有情感）
        let voice = if params.voice.trim().is_empty() {
            "zh_female_meilinvyou_moon_bigtts".to_string()
        } else if is_cosyvoice_voice(params.voice.trim()) {
            // 防呆：CosyVoice 音色（alex/anna…）串味到火山——火山不认这些名字，原样发会报
            // "[resource_id=] requested resource not granted"。退回火山大模型拟真音色，避免天书错误。
            log::warn!(
                "火山配音收到 CosyVoice 音色 '{}'（疑似切换供应商后旁白/角色音色残留），已回退大模型拟真音色；请在 设置→配音 改用火山音色",
                params.voice.trim()
            );
            "zh_female_meilinvyou_moon_bigtts".to_string()
        } else {
            params.voice.trim().to_string()
        };
        let cluster = if self.config.cluster.trim().is_empty() {
            "volcano_tts"
        } else {
            self.config.cluster.trim()
        };
        let reqid = format!(
            "sl-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        // 语气曲线（漫剧化 V5·演绎）：按分镜情感映射语速/响度/情感强度，让情绪立体不再平读。
        // speed/loudness 对所有音色生效；emotion 枚举 + emotion_scale 仅情感音色（bigtts/BV700-701）下发。
        let dir = VoiceDirection::new(&params.emotion, &params.style);
        let (speed, loudness, scale) = dir.prosody();
        let mut audio = serde_json::json!({
            "voice_type": voice,
            "encoding": "mp3",
            "speed_ratio": speed,
            "loudness_ratio": loudness,
        });
        if is_volc_emotion_voice(&voice) {
            if let Some(emo) = dir.volc_emotion() {
                audio["enable_emotion"] = serde_json::Value::Bool(true);
                audio["emotion"] = serde_json::Value::String(emo.to_string());
                audio["emotion_scale"] = serde_json::json!(scale);
            }
        }
        let body = serde_json::json!({
            "app": { "appid": self.config.appid, "token": self.config.access_token, "cluster": cluster },
            "user": { "uid": "storyloom" },
            "audio": audio,
            "request": { "reqid": reqid, "text": params.text, "operation": "query" },
        });

        let resp = self
            .client
            .post(self.config.endpoint.trim())
            // 火山要求形如 "Bearer;{token}"（分号是其特有写法）
            .header(
                "Authorization",
                format!("Bearer;{}", self.config.access_token),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                MediaError::Failed(format!(
                    "提交配音合成失败: {}",
                    super::http::describe_reqwest_error(&e)
                ))
            })?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(MediaError::Failed(format!(
                "配音合成 HTTP {}: {}",
                status.as_u16(),
                truncate(&text, 300)
            )));
        }
        let parsed: VolcTtsResponse = resp
            .json()
            .await
            .map_err(|e| MediaError::Failed(format!("解析配音响应失败: {e}")))?;
        // 火山成功码 3000
        if parsed.code != 3000 {
            return Err(MediaError::Failed(format!(
                "配音合成失败(code {}): {}",
                parsed.code,
                parsed.message.unwrap_or_default()
            )));
        }
        let b64 = parsed
            .data
            .filter(|s| !s.is_empty())
            .ok_or_else(|| MediaError::Failed("配音响应未返回音频数据".into()))?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|e| MediaError::Failed(format!("配音音频 base64 解码失败: {e}")))?;
        if bytes.is_empty() {
            return Err(MediaError::Failed("配音合成返回空音频".into()));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::VoiceDirection;

    /// 中性/空情感 → 各家都不下发情感控制，等于改造前的旧行为（CosyVoice 不会把指令读出来穿帮）。
    #[test]
    fn neutral_emotion_emits_no_control() {
        for label in ["", "  ", "平静", "中性", "未知情绪"] {
            let d = VoiceDirection::new(label, "");
            assert_eq!(d.volc_emotion(), None, "{label}: 火山不应下发情感枚举");
            assert_eq!(
                d.cosyvoice_emotion_word(),
                None,
                "{label}: CosyVoice 不应有情感词"
            );
            assert_eq!(
                d.cosyvoice_instruction(),
                None,
                "{label}: 无风格无情感不应注入指令"
            );
            assert_eq!(d.prosody(), (1.0, 1.0, 4), "{label}: 语气曲线应为自然值");
            assert_eq!(d.cosyvoice_speed(), 1.0, "{label}: 语速应保持默认 1.0");
        }
    }

    /// 情感映射：火山英文枚举与 CosyVoice 中文指令词同源一致（cosyvoice_emotion_word 复用 volc_emotion 分类）。
    #[test]
    fn emotion_maps_to_volc_enum_and_cosyvoice_word() {
        let cases = [
            ("愤怒", "angry", "愤怒"),
            ("角色很生气", "angry", "愤怒"),
            ("悲伤", "sad", "悲伤"),
            ("开心", "happy", "高兴"),
            ("惊讶", "surprise", "惊讶"),
            ("恐惧", "fear", "恐惧"),
        ];
        for (label, volc, cosy) in cases {
            let d = VoiceDirection::new(label, "");
            assert_eq!(d.volc_emotion(), Some(volc), "{label}: 火山枚举不符");
            assert_eq!(
                d.cosyvoice_emotion_word(),
                Some(cosy),
                "{label}: CosyVoice 指令词不符"
            );
        }
    }

    /// 语速跟随情感：愤怒快、悲伤慢（CosyVoice speed 与火山 speed_ratio 同源）。
    #[test]
    fn speed_follows_emotion() {
        assert_eq!(VoiceDirection::new("愤怒", "").cosyvoice_speed(), 1.1);
        assert_eq!(VoiceDirection::new("悲伤", "").cosyvoice_speed(), 0.9);
    }

    /// 演绎风格（角色卡）与情感（分镜）组合成 CosyVoice 指令：可单独、可叠加、都无则不注入。
    #[test]
    fn cosyvoice_instruction_combines_style_and_emotion() {
        assert_eq!(
            VoiceDirection::new("", "傲娇").cosyvoice_instruction(),
            Some("请用傲娇的语气说。".to_string())
        );
        assert_eq!(
            VoiceDirection::new("愤怒", "").cosyvoice_instruction(),
            Some("请用愤怒的语气说。".to_string())
        );
        assert_eq!(
            VoiceDirection::new("愤怒", "傲娇").cosyvoice_instruction(),
            Some("请用傲娇、愤怒的语气说。".to_string())
        );
        assert_eq!(VoiceDirection::new("", "").cosyvoice_instruction(), None);
    }
}

// ─────────────────────────── 协议识别与统一入口 ───────────────────────────
//
// 搬自 StoryLoom `services/tts.rs`。

/// 配音协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TtsProtocol {
    /// 字节豆包（火山语音 openspeech）专有协议：直接 POST 完整地址，需 `appid` / `cluster`
    Volc,
    /// 🔴 填成了火山**方舟**（ark）端点：方舟没有语音合成接口，发出去只会拿到裸 404 —— 调用前拦下
    ArkMisconfigured,
    /// OpenAI `/audio/speech` 兼容（硅基流动 CosyVoice、GLM-TTS、OpenAI 等）
    OpenAi,
}

impl TtsProtocol {
    /// 按端点识别：含 `openspeech` → 火山语音；火山方舟地址 → 配错了；其余 → OpenAI 兼容。
    pub fn detect(endpoint: &str) -> Self {
        let e = endpoint.to_ascii_lowercase();
        if e.contains("openspeech") {
            Self::Volc
        } else if e.contains("volces.com") || e.contains("/ark") {
            Self::ArkMisconfigured
        } else {
            Self::OpenAi
        }
    }
}

/// 按配音模型决定输出格式与落盘扩展名：智谱 glm-tts 只出 wav / pcm → `wav`；其余 → `mp3`。
///
/// 🔴 与 [`SiliconFlowTtsProvider`] 里的 glm 分支同一口径 ——「合成格式 = 落盘扩展名 = 播放 mime」三者要对齐。
pub fn audio_format_for(model: &str) -> &'static str {
    if model.to_ascii_lowercase().contains("glm") {
        "wav"
    } else {
        "mp3"
    }
}

/// 从供应商 `extra`（JSON）解析火山语音配置：`(appid, cluster)`，cluster 缺省 `volcano_tts`。
pub fn parse_volc_extra(extra: &str) -> (String, String) {
    let v: serde_json::Value = serde_json::from_str(extra).unwrap_or(serde_json::Value::Null);
    let appid = v
        .get("appid")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let cluster = v
        .get("cluster")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("volcano_tts")
        .to_string();
    (appid, cluster)
}

/// 合成配音（按 [`TtsProtocol::detect`] 分发），返回音频字节。
///
/// `extra` 是供应商的额外配置 JSON（火山语音从中取 `appid` / `cluster`，其余忽略）。
pub async fn synthesize(
    endpoint: &str,
    model: &str,
    extra: &str,
    api_key: String,
    params: &TtsParams,
) -> Result<Vec<u8>, MediaError> {
    match TtsProtocol::detect(endpoint) {
        TtsProtocol::Volc => {
            let (appid, cluster) = parse_volc_extra(extra);
            VolcTtsProvider::new(VolcTtsConfig {
                endpoint: endpoint.to_string(),
                appid,
                cluster,
                access_token: api_key,
            })
            .synthesize(params)
            .await
        }
        TtsProtocol::ArkMisconfigured => Err(MediaError::InvalidInput(
            "配音不能用火山方舟（ark）端点——方舟没有语音合成接口（TTS 属「火山语音」另一产品线）。请改用：\
① 字节豆包配音（火山语音）：端点 https://openspeech.bytedance.com/api/v1/tts，需 appid/cluster/access_token（用「字节豆包配音」预置重新添加才有这些字段）；\
② 或 硅基流动 CosyVoice：端点 https://api.siliconflow.cn/v1，模型 FunAudioLLM/CosyVoice2-0.5B（编辑当前供应商改这两项 + 填硅基流动 Key 即可）。"
                .into(),
        )),
        TtsProtocol::OpenAi => {
            SiliconFlowTtsProvider::new(TtsConfig {
                endpoint: endpoint.to_string(),
                model: model.to_string(),
                api_key,
            })
            .synthesize(params)
            .await
        }
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    #[test]
    fn detect_by_endpoint() {
        assert_eq!(
            TtsProtocol::detect("https://openspeech.bytedance.com/api/v1/tts"),
            TtsProtocol::Volc
        );
        assert_eq!(
            TtsProtocol::detect("https://ark.cn-beijing.volces.com/api/v3"),
            TtsProtocol::ArkMisconfigured
        );
        assert_eq!(
            TtsProtocol::detect("https://api.siliconflow.cn/v1"),
            TtsProtocol::OpenAi
        );
        assert_eq!(
            TtsProtocol::detect("https://api.ipsunion.com/v1"),
            TtsProtocol::OpenAi
        );
    }

    /// 🔴 每条配音预置都不能识别成「配错了」。
    #[test]
    fn no_tts_preset_is_misconfigured() {
        for p in crate::preset::presets_for(crate::Kind::Tts) {
            let Some(url) = p.base_url else { continue };
            assert_ne!(
                TtsProtocol::detect(url),
                TtsProtocol::ArkMisconfigured,
                "{}",
                p.key
            );
        }
        let volc = crate::preset::preset_by_key("volc_tts").unwrap();
        assert_eq!(
            TtsProtocol::detect(volc.base_url.unwrap()),
            TtsProtocol::Volc
        );
    }

    #[test]
    fn volc_extra_defaults_cluster() {
        assert_eq!(
            parse_volc_extra(r#"{"appid":"123"}"#),
            ("123".into(), "volcano_tts".into())
        );
        assert_eq!(
            parse_volc_extra(r#"{"appid":"1","cluster":" c "}"#),
            ("1".into(), "c".into())
        );
        assert_eq!(
            parse_volc_extra("not json"),
            (String::new(), "volcano_tts".into())
        );
    }

    #[test]
    fn glm_outputs_wav() {
        assert_eq!(audio_format_for("glm-tts"), "wav");
        assert_eq!(audio_format_for("FunAudioLLM/CosyVoice2-0.5B"), "mp3");
    }

    #[tokio::test]
    async fn ark_endpoint_is_rejected_before_any_request() {
        let e = synthesize(
            "https://ark.cn-beijing.volces.com/api/v3",
            "m",
            "",
            "k".into(),
            &TtsParams::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(e, MediaError::InvalidInput(_)));
    }
}
