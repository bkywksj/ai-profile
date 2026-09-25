//! 「获取模型」：零成本验证地址与密钥，失败时把结构化错误翻成界面动作。
//!
//! 只打端点的 `/models`，不产生任何生成费用。需要 `client` feature：
//!
//! ```text
//! # 密钥只从环境变量读，绝不写进代码或提交
//! AI_PROFILE_KEY=sk-... cargo run -p ai-profile --features client --example verify -- deepseek
//! ```
//!
//! 不给密钥也能跑：会走到「密钥不对」那条分支，正好演示错误处理。

use ai_profile::client::{ServiceConfig, Verifier};
use ai_profile::{preset_by_key, VerifyError};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let key_name = std::env::args().nth(1).unwrap_or_else(|| "deepseek".into());
    let p = preset_by_key(&key_name).unwrap_or_else(|| panic!("没有预置 {key_name}"));
    let api_key = std::env::var("AI_PROFILE_KEY").unwrap_or_default();

    // 🔴 Verifier 内部持有连接池：应用启动时建一次、存进全局状态反复用。
    //    每次现建会让连接池永远是空的，每次都从 TLS 握手重来。
    //    需要代理时用 Verifier::from_builder(ai_profile::reqwest::Client::builder().proxy(…))
    let verifier = Verifier::new().expect("建 HTTP 客户端");

    // ServiceConfig 只能用 builder 构造（non_exhaustive）
    let cfg = ServiceConfig::new(p.protocol, p.base_url.unwrap_or(""))
        .with_preset(p.key) // 校验该预置的必填专有字段；地址为空时用预置地址兜底
        .with_api_key(&api_key)
        .with_model(p.model);

    match verifier.verify(cfg).await {
        Ok(ok) => {
            println!(
                "连通 · {}ms · 端点提供 {} 个对话模型",
                ok.latency_ms,
                ok.models.len()
            );
            if ok.dropped > 0 {
                // 清单被清洗过，要告诉用户，否则会以为端点就这么几个模型
                println!("（已滤掉 {} 个向量 / 重排 / 语音等非对话模型）", ok.dropped);
            }
            if !ok.model_in_list {
                println!("⚠️ 当前模型 {} 不在端点清单里，让用户确认", p.model);
            }
            if let Some(l) = ok.limits {
                println!(
                    "端点上报的限额：窗口 {:?} / 输出 {:?}",
                    l.context_window, l.max_output
                );
            }
        }
        // 🔴 每个变体对应一个界面动作，不要只显示一行红字
        Err(e) => {
            let action = match &e {
                VerifyError::AuthFailed { .. } => "高亮密钥输入框：密钥不对或已过期",
                VerifyError::NotFound {
                    suggested_url: Some(_),
                    ..
                } => "显示「一键改用建议地址」按钮",
                VerifyError::NotFound { .. } => "提示检查地址",
                VerifyError::Unreachable { proxy_hint: true } => "提示「这家可能需要代理」",
                VerifyError::Unreachable { .. } => "提示检查网络与地址",
                VerifyError::ModelNotFound { .. } => "把端点给的可用模型列成下拉让用户选",
                VerifyError::ProtocolMismatch { .. } => "提示「协议选错了」并给出正确协议",
                VerifyError::MissingExtraField { .. } => "高亮缺少的专有字段（发请求前就能判出来）",
                // 🔴 公开枚举都是 non_exhaustive：必须留 `_` 分支，本库加变体只算 minor 版本
                _ => "显示原始错误信息",
            };
            println!("失败：{e}");
            println!("界面应该：{action}");
        }
    }
}
