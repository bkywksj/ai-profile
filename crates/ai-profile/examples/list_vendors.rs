//! 设置页的「选服务商」：按厂商列目录，选中后取预置填表单。纯数据，不联网。
//!
//! ```text
//! cargo run -p ai-profile --example list_vendors
//! ```

use ai_profile::{preset_by_key, vendors, Kind};

fn main() {
    // 1. 服务商下拉：传入本应用支持的能力 —— 只做对话的应用看不到「只提供视频」的厂商。
    //    同一家的多种能力共用一个密钥，所以界面上是「一家一行」，不是「一条预置一行」。
    let list = vendors(&[Kind::Chat]);
    println!("共 {} 家服务商：", list.len());
    for v in &list {
        // is_local 必须区分：云端引导「去申请密钥」，本地服务引导「先把服务跑起来」
        let kind = if v.is_local { "本地服务" } else { "云端" };
        println!("  {:<24} [{}] {}", v.label, v.group_label, kind);
    }

    // 2. 用户选了一家：用 preset_by_key 取预置，预填表单
    let p = preset_by_key("deepseek").expect("deepseek 是内置预置");
    println!();
    println!("选中 {}：", p.label);
    // base_url 为 None 表示没有预填地址（如「OpenAI 兼容自定义」），界面要显示地址输入框
    println!("  地址：{}", p.base_url.unwrap_or("（用户自己填）"));
    println!("  默认模型：{}", p.model);
    println!("  申请密钥：{}", p.apply_url.unwrap_or("—"));
    for m in p.models {
        // 预置的静态限额只是兜底，端点上报的更准（见 verify 示例）
        let window = m
            .context_window
            .map_or("未知".to_string(), |w| format!("{w} tokens"));
        println!("  候选模型：{:<20} 窗口 {window}", m.value);
    }
}
