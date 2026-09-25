//! 本地工具集。
//!
//! ```bash
//! cargo xtask gen-docs   # 重新生成 docs/providers.md
//! cargo xtask probe      # 打所有预置端点，验证 base_url 还通不通
//! cargo xtask gen-spec   # 重新生成 spec/（预置 JSON + 一致性用例，给其他语言用）
//! ```
//!
//! # 🔴 probe 绝不能进 CI
//!
//! 每次 PR 都去打各家服务商的端点，既是对人家的滥用，也会因为网络波动让 CI 随机红。
//! 它是**人工触发的季度体检**，不是门禁。
//!
//! gen-docs 则相反 —— 它是纯本地计算，CI 里由守卫测试 `providers_md_in_sync`
//! 检查结果是否已提交。

mod facts;
mod spec;

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // xtask/ 的上一级就是仓库根
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask 应当在仓库根下")
        .to_path_buf()
}

fn gen_docs() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("docs/providers.md");
    let content = ai_profile::preset::render_providers_markdown();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let changed = std::fs::read_to_string(&path)
        .map(|old| old != content)
        .unwrap_or(true);
    std::fs::write(&path, &content)?;
    println!(
        "{} {}（{} 字节）",
        if changed { "已更新" } else { "无变化" },
        path.display(),
        content.len()
    );
    Ok(())
}

fn gen_spec() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root().join("spec");
    let mut changed = 0usize;
    for (rel, content) in spec::render_all() {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::read_to_string(&path)
            .map(|old| old != content)
            .unwrap_or(true)
        {
            std::fs::write(&path, &content)?;
            changed += 1;
            println!("已更新 spec/{rel}");
        }
    }
    println!("{changed} 个文件有变化");
    Ok(())
}

/// 打所有带 base_url 的预置端点，看看还通不通。
///
/// 不带密钥 —— 目的是验**地址**是否还有效（DNS / 路由 / 路径），而不是验鉴权。
/// 所以 401/403 算「地址没问题」，404 才是真问题。
async fn probe() -> Result<(), Box<dyn std::error::Error>> {
    use ai_profile::client::{ServiceConfig, Verifier};
    use ai_profile::VerifyError;

    // 🔴 一个 Verifier 复用给所有探测：reqwest::Client 内部持连接池，
    //    每家现建一个等于每家都从 TCP + TLS 握手重来。
    let verifier = Verifier::new()?;

    // 先筛出真正要探的，跳过的单独计数
    let targets: Vec<_> = ai_profile::preset::presets()
        .iter()
        // 本地服务没跑起来时必然连不上，探它没有意义
        .filter(|p| !p.is_local)
        .filter_map(|p| p.base_url.map(|base| (p, base)))
        .collect();
    let skipped = ai_profile::preset::presets().len() - targets.len();

    // 并发探测。串行时最坏情况是 家数 × 20 秒超时 —— 19 家就是 6 分钟，
    // 而这些请求彼此无关，纯粹在等网络。
    let results = futures_util::future::join_all(targets.into_iter().map(|(p, base)| {
        let verifier = &verifier;
        async move {
            // ServiceConfig 带 #[non_exhaustive]，外部 crate 只能走 builder 构造
            let cfg = ServiceConfig::new(p.protocol, base).with_preset(p.key);
            (p.key, verifier.verify(cfg).await)
        }
    }))
    .await;

    // 🔴 输出按预置顺序，不按完成顺序 —— 并发化不该让人每次看到不同的排列，
    //    那样两次 probe 的输出没法直接 diff。
    let mut bad = 0usize;
    for (key, r) in results {
        match r {
            Ok(ok) => println!(
                "  ✅ {:<26} {:>5}ms  {} 个模型",
                key,
                ok.latency_ms,
                ok.models.len()
            ),
            // 没带密钥，鉴权失败恰恰说明地址是对的
            Err(VerifyError::AuthFailed { .. }) => {
                println!("  ✅ {key:<26} 地址可达（未带密钥，返回鉴权失败属正常）")
            }
            Err(e @ VerifyError::NotFound { .. }) => {
                bad += 1;
                println!("  ❌ {key:<26} {e}");
            }
            Err(e) => println!("  ⚠️  {key:<26} {e}"),
        }
    }

    println!("\n跳过 {skipped} 条（本地服务 / 无固定地址）");
    if bad > 0 {
        eprintln!("🔴 {bad} 条预置的 base_url 返回 404，需要核对服务商文档后更新");
        std::process::exit(1);
    }
    println!("✅ 没有发现失效的 base_url");
    Ok(())
}

#[tokio::main]
async fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let r = match cmd.as_str() {
        "gen-docs" => gen_docs(),
        "probe" => probe().await,
        "gen-spec" => gen_spec(),
        _ => {
            eprintln!("用法:\n  cargo xtask gen-docs   重新生成 docs/providers.md\n  cargo xtask probe      探活所有预置端点（人工触发，勿进 CI）\n  cargo xtask gen-spec   重新生成 spec/（预置 JSON + 一致性用例）");
            std::process::exit(2);
        }
    };
    if let Err(e) = r {
        eprintln!("失败: {e}");
        std::process::exit(1);
    }
}
