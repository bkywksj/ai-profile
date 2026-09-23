//! 模型的 token 限额 —— 上下文窗口与单次输出上限。
//!
//! # 🔴 这不是一张「全量模型能力表」
//!
//! 本模块**刻意不做** models.dev / LiteLLM 那种事。那类项目维护着上万条模型的
//! 上下文、价格、能力矩阵（models.dev 至今一万多次提交），而那份数据周级变动、
//! 且同一个模型在不同中转站的实际限额并不相同 —— 我们既没有那个维护量，
//! 也无从知道某个中转站到底给用户开了多大的窗口。
//!
//! 真要全量能力表，应用自己去拉 <https://models.dev/api.json>，那是它的专业。
//!
//! 本模块只回答一个具体问题：**「发这次请求之前，该按多大的窗口裁历史？」**
//!
//! # 分层回退，且来源可分辨
//!
//! ```text
//! 0. 用户手填       ← 用户明确说了算，压过一切（端点报错、中转站另有限制时的出口）
//! 1. 端点实时上报   ← 最准：中转站报的就是它自己的真实限额
//! 2. 预置静态兜底   ← 端点不报时用；只写「保守够用」而非追新
//! 3. 都没有 → None  ← 让用户自己填，别猜
//! ```
//!
//! 合并用 [`TokenLimits::or`]，**逐字段**回退：用户只填了窗口，输出上限照样能从下一层拿。
//!
//! 🔴 **三者必须能被调用方分辨**，这是本模块最重要的设计。
//!
//! 调研真实故障时，几乎每一条都源于「把猜的数字当成真的」：
//!
//! - 某网关丢掉元数据 → 客户端回落硬编码表 → 512K 的模型被当成 131K，提前触发压缩
//! - 某应用钉死 65536，而实测请求中位数 153395、p90 达 433535
//! - 某扩展硬编码 288K，同时无视服务器上报值**和**用户设置
//!
//! 共同点不是「数字错了」，而是**错了也看不出来**。所以 [`TokenLimits::source`]
//! 是必填字段：界面可以据此显示「端点上报 128K」还是「预估 128K，可修改」。
//!
//! # 为什么 OpenAI 兼容端点大多不报
//!
//! **OpenAI 的 `/v1/models` 规范里就没有 context 字段**。实测：
//!
//! | 端点 | `/models` 是否带上下文 |
//! |---|---|
//! | OpenRouter | ✅ `context_length` 100% 覆盖，还有 `top_provider.max_completion_tokens` |
//! | DeepSeek | ✅ `context_window` + `max_output_tokens`（2026-09-23 真实密钥实测） |
//! | LM Studio / Ollama 的兼容层 | ❌ 只有 `{id, object, owned_by}`（原生 `/api/v1/models` 才有） |
//!
//! 所以静态兜底不是可选项 —— 不做的话，多数端点上这个能力等于不存在。

use serde::{Deserialize, Serialize};

/// 限额数字的来源。
///
/// 🔴 调用方**必须**据此区别对待：端点上报的可以直接用，静态兜底的应当让用户能改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LimitSource {
    /// 用户在界面上手填的 —— 优先级最高。
    ///
    /// 端点报的也可能不对（中转站按套餐另有限制却照抄模型标称值），
    /// 这时唯一的出口是让用户改；改了就必须压过端点值，否则改了等于没改。
    User,
    /// 端点在 `/models` 响应里自己报的 —— 最可信，包含中转站的真实限制
    Endpoint,
    /// 本 crate 的预置静态值 —— 保守估计，可能过时，应允许用户覆盖
    Preset,
}

impl LimitSource {
    /// 规范字符串，与 serde 线格式一致。调用方持久化来源时用它。
    pub const fn as_str(self) -> &'static str {
        match self {
            LimitSource::User => "user",
            LimitSource::Endpoint => "endpoint",
            LimitSource::Preset => "preset",
        }
    }

    /// 从 [`Self::as_str`] 的产物解析回来；认不出返回 `None`。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "user" => Some(LimitSource::User),
            "endpoint" => Some(LimitSource::Endpoint),
            "preset" => Some(LimitSource::Preset),
            _ => None,
        }
    }
}

/// 一个模型的 token 限额。
///
/// 两个字段都是 `Option`：拿不到就是拿不到，**不填一个猜的数字**。
/// 调用方遇到 `None` 应当让用户手填，而不是自己兜一个默认值 ——
/// 那正是要避免的「硬编码回落」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TokenLimits {
    /// 上下文窗口（输入 + 输出的总上限）。
    ///
    /// 用途：裁历史消息。注意它是**总量**，留给输出的部分要从中扣除。
    pub context_window: Option<u32>,
    /// 单次输出上限。
    ///
    /// 用途：填请求里的 `max_tokens`。此前各应用普遍硬编码 4096 ——
    /// 对支持 64K 输出的模型来说白白浪费了大半能力。
    pub max_output: Option<u32>,
    /// 这两个数字是谁给的，见 [`LimitSource`]
    pub source: LimitSource,
}

impl TokenLimits {
    /// 端点上报的限额。
    pub const fn from_endpoint(context_window: Option<u32>, max_output: Option<u32>) -> Self {
        Self {
            context_window,
            max_output,
            source: LimitSource::Endpoint,
        }
    }

    /// 预置静态兜底。
    pub const fn from_preset(context_window: Option<u32>, max_output: Option<u32>) -> Self {
        Self {
            context_window,
            max_output,
            source: LimitSource::Preset,
        }
    }

    /// 用户手填的限额。
    pub const fn from_user(context_window: Option<u32>, max_output: Option<u32>) -> Self {
        Self {
            context_window,
            max_output,
            source: LimitSource::User,
        }
    }

    /// 按来源构造 —— 调用方从存储里读回 `(窗口, 输出, 来源)` 时用。
    pub const fn with_source(
        context_window: Option<u32>,
        max_output: Option<u32>,
        source: LimitSource,
    ) -> Self {
        Self {
            context_window,
            max_output,
            source,
        }
    }

    /// 逐字段回退：自己缺的字段从 `fallback` 补。
    ///
    /// `source` 取**自己的**（只要自己至少有一个字段）—— 它标的是优先级最高、
    /// 真正起作用的那一层。自己全空时整个换成 `fallback`。
    ///
    /// 🔴 为什么要逐字段：用户常常只知道窗口大小（文档写了），不知道输出上限。
    /// 整条替换的话，填了窗口就丢了预置里的输出上限，等于填了反而变差。
    ///
    /// ```
    /// # use ai_profile::limits::{LimitSource, TokenLimits};
    /// let user = TokenLimits::from_user(Some(64_000), None);
    /// let preset = TokenLimits::from_preset(Some(128_000), Some(8192));
    /// let l = user.or(preset);
    /// assert_eq!(l.context_window, Some(64_000)); // 用户的
    /// assert_eq!(l.max_output, Some(8192));       // 预置补的
    /// assert_eq!(l.source, LimitSource::User);
    /// ```
    pub const fn or(self, fallback: TokenLimits) -> TokenLimits {
        if self.is_empty() {
            return fallback;
        }
        TokenLimits {
            context_window: match self.context_window {
                Some(v) => Some(v),
                None => fallback.context_window,
            },
            max_output: match self.max_output {
                Some(v) => Some(v),
                None => fallback.max_output,
            },
            source: self.source,
        }
    }

    /// 两个数字都没有 —— 等于「不知道」，调用方该让用户填。
    pub const fn is_empty(&self) -> bool {
        self.context_window.is_none() && self.max_output.is_none()
    }

    /// 留给输入的预算 = 上下文窗口 − 为输出预留的量。
    ///
    /// 这是裁历史时真正要用的数字。`reserve_output` 传本次请求实际要用的
    /// `max_tokens`，而不是模型的输出上限 —— 用上限会把预算压得过小。
    ///
    /// 上下文窗口未知时返回 `None`：**不猜**。
    ///
    /// ```
    /// # use ai_profile::limits::TokenLimits;
    /// let l = TokenLimits::from_endpoint(Some(128_000), Some(8192));
    /// assert_eq!(l.input_budget(4096), Some(123_904));
    ///
    /// // 预留量比窗口还大 —— 返回 0 而不是下溢
    /// let tiny = TokenLimits::from_preset(Some(1000), None);
    /// assert_eq!(tiny.input_budget(4096), Some(0));
    ///
    /// // 不知道窗口就是不知道
    /// assert_eq!(TokenLimits::from_preset(None, None).input_budget(4096), None);
    /// ```
    pub fn input_budget(&self, reserve_output: u32) -> Option<u32> {
        self.context_window
            .map(|w| w.saturating_sub(reserve_output))
    }
}

/// 从 `/models` 响应的**单条**模型对象里抽限额。
///
/// 各家字段名不统一，按下表依次尝试（前者优先）：
///
/// | 我们要的 | 依次尝试的字段 |
/// |---|---|
/// | 上下文窗口 | `context_length`、`context_window`、`max_context_length`、`max_input_tokens`、`top_provider.context_length` |
/// | 输出上限 | `max_completion_tokens`、`max_output_tokens`、`max_tokens`、`top_provider.max_completion_tokens` |
///
/// 🔴 `top_provider` 那两条不是凑数：OpenRouter 的顶层 `context_length` 是**模型本体**的，
/// 而 `top_provider.context_length` 是**当前这家服务商实际提供**的，后者才是用户
/// 真正能用到的。实测两者会不一致，所以嵌套那份优先级更高。
///
/// 返回 `None` 表示这条模型一个限额字段都没有 —— 对 OpenAI 规范的兼容端点是常态。
pub fn parse_model_limits(item: &serde_json::Value) -> Option<TokenLimits> {
    /// 读一个非负整数；浮点与数字字符串也接受（有些网关把它序列化成字符串）
    fn num(v: Option<&serde_json::Value>) -> Option<u32> {
        let v = v?;
        let n = v
            .as_u64()
            .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
            .or_else(|| v.as_str()?.trim().parse::<u64>().ok())?;
        // 0 当作「没有」：某些端点用 0 表示未知，当成真实上限会让预算算成 0
        if n == 0 {
            return None;
        }
        u32::try_from(n).ok()
    }

    let top = item.get("top_provider");
    let pick = |keys: &[&str]| -> Option<u32> {
        // 🔴 先看 top_provider：它是「这家实际给你的」，比模型本体的标称值更贴近现实
        for k in keys {
            if let Some(n) = num(top.and_then(|t| t.get(*k))) {
                return Some(n);
            }
        }
        keys.iter().find_map(|k| num(item.get(*k)))
    };

    let context_window = pick(&[
        "context_length",
        "context_window",
        "max_context_length",
        "max_input_tokens",
    ]);
    let max_output = pick(&["max_completion_tokens", "max_output_tokens", "max_tokens"]);

    if context_window.is_none() && max_output.is_none() {
        return None;
    }
    Some(TokenLimits::from_endpoint(context_window, max_output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 🔴 `top_provider` 的优先级高于顶层 —— 它才是用户真能用到的额度。
    ///
    /// 这条用的是 OpenRouter 的真实结构：顶层 `context_length` 是模型本体标称值，
    /// 而某家服务商可能只提供其中一部分。取错了会让裁历史按一个用不到的大数字算，
    /// 表现为「明明裁过了还是超限」。
    #[test]
    fn nested_top_provider_wins() {
        let item = json!({
            "id": "some/model",
            "context_length": 1_048_576,
            "top_provider": { "context_length": 131_072, "max_completion_tokens": 32_768 }
        });
        let l = parse_model_limits(&item).unwrap();
        assert_eq!(l.context_window, Some(131_072), "该取服务商实际提供的");
        assert_eq!(l.max_output, Some(32_768));
        assert_eq!(l.source, LimitSource::Endpoint);
    }

    /// 没有 top_provider 时回落到顶层字段。
    #[test]
    fn falls_back_to_top_level() {
        let item = json!({ "id": "m", "context_length": 200_000 });
        let l = parse_model_limits(&item).unwrap();
        assert_eq!(l.context_window, Some(200_000));
        assert_eq!(l.max_output, None, "没报就是没报，不猜");
    }

    /// 🔴 OpenAI 规范的裸响应 —— LM Studio / Ollama 的兼容层都是这样。
    ///
    /// 这是**最常见**的情况，不是边角料：OpenAI 的 /v1/models 规范里根本没有
    /// context 字段。返回 None 才能让调用方走静态兜底那一层。
    #[test]
    fn bare_openai_shape_yields_none() {
        let item = json!({ "id": "qwen3-8b", "object": "model", "owned_by": "organization_owner" });
        assert!(parse_model_limits(&item).is_none());
    }

    /// DeepSeek `/models` 的真实结构（2026-09-23 真实密钥实测）：
    /// 顶层直接带 `context_window` + `max_output_tokens`，没有 top_provider。
    ///
    /// 此前曾误以为 DeepSeek 不报限额，这条用实测形状钉住，防止再次误判。
    #[test]
    fn deepseek_shape_is_parsed() {
        let item = json!({
            "id": "deepseek-flash",
            "object": "model",
            "owned_by": "deepseek",
            "context_window": 1_048_576,
            "max_output_tokens": 393_216
        });
        let l = parse_model_limits(&item).unwrap();
        assert_eq!(l.context_window, Some(1_048_576));
        assert_eq!(l.max_output, Some(393_216));
        assert_eq!(l.source, LimitSource::Endpoint);
    }

    /// 逐字段回退 + 来源取高优先级那层；自己全空时整条换成 fallback。
    #[test]
    fn or_merges_field_by_field() {
        let endpoint = TokenLimits::from_endpoint(None, Some(32_000));
        let preset = TokenLimits::from_preset(Some(128_000), Some(8192));
        let l = endpoint.or(preset);
        assert_eq!(l.context_window, Some(128_000), "端点没报窗口，由预置补");
        assert_eq!(l.max_output, Some(32_000), "端点报了的不被覆盖");
        assert_eq!(l.source, LimitSource::Endpoint);

        let empty = TokenLimits::from_user(None, None);
        assert_eq!(
            empty.or(preset),
            preset,
            "空的用户设置不能把来源冒充成 User"
        );
    }

    /// 来源的持久化字符串必须能原样读回，且与 serde 线格式一致。
    #[test]
    fn limit_source_roundtrip() {
        for s in [
            LimitSource::User,
            LimitSource::Endpoint,
            LimitSource::Preset,
        ] {
            assert_eq!(LimitSource::parse(s.as_str()), Some(s));
            assert_eq!(serde_json::to_value(s).unwrap(), s.as_str());
        }
        assert_eq!(LimitSource::parse("guess"), None);
    }

    /// 0 不是「上限为 0」，是「未知」。
    ///
    /// 当成真值会让 input_budget 算出 0，表现为「历史全被裁光，模型失忆」。
    #[test]
    fn zero_means_unknown() {
        let item = json!({ "id": "m", "context_length": 0, "max_output_tokens": 0 });
        assert!(parse_model_limits(&item).is_none());
    }

    /// 数字被序列化成字符串时也要认（部分自建网关如此）。
    #[test]
    fn accepts_stringified_numbers() {
        let item = json!({ "id": "m", "context_length": "32768" });
        assert_eq!(
            parse_model_limits(&item).unwrap().context_window,
            Some(32_768)
        );
    }

    /// 🔴 来源必须能被分辨 —— 这是本模块存在的核心理由。
    #[test]
    fn source_is_distinguishable() {
        assert_eq!(
            parse_model_limits(&json!({ "context_length": 1 }))
                .unwrap()
                .source,
            LimitSource::Endpoint
        );
        assert_eq!(
            TokenLimits::from_preset(Some(1), None).source,
            LimitSource::Preset
        );
    }

    /// 线格式是前端契约 —— 字段名变了下游会静默读到 undefined。
    #[test]
    fn serializes_camel_case_with_snake_source() {
        let l = TokenLimits::from_endpoint(Some(128_000), Some(8192));
        let j = serde_json::to_string(&l).unwrap();
        assert!(j.contains(r#""contextWindow":128000"#), "{j}");
        assert!(j.contains(r#""maxOutput":8192"#), "{j}");
        // 来源枚举走 snake_case，与 VerifyError 的 code 口径一致
        assert!(j.contains(r#""source":"endpoint""#), "{j}");
    }

    /// 预算计算不能下溢 —— 预留量大于窗口时返回 0。
    #[test]
    fn budget_saturates_instead_of_underflowing() {
        let l = TokenLimits::from_preset(Some(4096), None);
        assert_eq!(l.input_budget(8192), Some(0));
    }
}
