//! 「粘贴导入」与「分享」：`ai.profile` 是跨应用的配置交换格式。纯数据，不联网。
//!
//! ```text
//! cargo run -p ai-profile --example import_export
//! ```

use ai_profile::{parse_profiles, to_profile, ParseError, Protocol};

fn main() {
    // 1. 分享：生成一段别的应用能粘的 JSON
    let shared = to_profile(
        "我的 DeepSeek",
        Protocol::OpenAiCompatible,
        "https://api.deepseek.com/v1",
        "sk-示例密钥",
        "deepseek-flash",
    );
    println!("分享出去的内容：\n{shared}\n");

    // 2. 粘贴导入：单条与多条打包统一走 parse_profiles，返回列表
    //    第二个参数是来源没带 model 时的兜底值
    import(&shared);

    // 从 Claude Code 等工具粘来的 Anthropic 配置常常只填到主机名 —— 原样保存即可，
    // 本库拼对话地址时会按 Anthropic 约定补 /v1（见 endpoint::join_chat_endpoint）
    import(
        r#"{"kind":"ai.profile","v":1,"data":{"name":"中转","provider":"anthropic","baseURL":"https://relay.example.com","apiKey":"sk-x","model":"claude-opus-5"}}"#,
    );

    // 3. 粘了别的东西：给出准确提示，而不是「解析失败」
    import(r#"{"hello":"world"}"#);
    import("");
}

fn import(text: &str) {
    match parse_profiles(text, "deepseek-flash") {
        Ok(r) => {
            for p in &r.profiles {
                println!(
                    "导入：{} [{}] {} / {}{}",
                    p.name,
                    p.protocol.as_str(),
                    p.base_url,
                    p.model,
                    // 来源没给 model、用了兜底值时要让用户知道
                    if p.model_fallback {
                        "（模型为兜底值）"
                    } else {
                        ""
                    }
                );
            }
            if r.skipped > 0 {
                // 设备绑定的 OAuth 档案不能导入 —— 要告诉用户，否则会以为漏导了
                println!("跳过 {} 条（设备绑定的登录凭据）", r.skipped);
            }
        }
        Err(e) => {
            let hint = match &e {
                ParseError::Empty => "剪贴板是空的",
                ParseError::NotAiProfile { .. } => "这不是 ai.profile 配置",
                ParseError::EmptyBundle { .. } => "打包里没有能导入的配置",
                _ => "配置格式不对",
            };
            println!("导入失败：{hint}（{e}）");
        }
    }
    println!();
}
