//! 本地工具集。当前只有一个子命令：
//!
//! ```bash
//! cargo xtask probe   # 打所有预置端点，验证 base_url 还通不通
//! ```
//!
//! 🔴 **绝不能进 CI** —— 每次 PR 都去打各家服务商的端点，既是对人家的滥用，
//! 也会因为网络波动让 CI 随机红。这是人工触发的季度体检，不是门禁。

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "probe" => {
            eprintln!("probe：待实现（阶段 1.12）—— 需要 preset 模块就位后才能遍历端点");
            std::process::exit(1);
        }
        _ => {
            eprintln!("用法: cargo xtask probe");
            std::process::exit(2);
        }
    }
}
