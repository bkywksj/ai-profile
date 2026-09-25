//! 手写文档里「从代码可以算出来」的事实，与代码核对。
//!
//! 生成物（providers.md、spec/）有各自的守卫；这里管的是**散落在手写文字里**的数字 ——
//! 它们没人会去重新生成，改了预置也不会报错，只会悄悄过时。
//! 曾经的实例：CLAUDE.md 写「对话预置 26 家」，README 写「25 家」，实际是 25。
//!
//! 能不写死的数字就别写；写了的，就在这里加一条核对。

#[cfg(test)]
mod tests {
    use ai_profile::preset::presets;
    use ai_profile::Kind;

    fn counts() -> (usize, usize) {
        let chat = presets().iter().filter(|p| p.kind == Kind::Chat).count();
        (chat, presets().len() - chat)
    }

    /// README 首页表格里的「对话 N 家 + 生图 / 视频 / 配音 M 条」必须与预置一致。
    /// 它同时是 crates.io 的包首页 —— 数字错了是对外的错。
    #[test]
    fn readme_preset_counts_match() {
        let (chat, media) = counts();
        let readme = std::fs::read_to_string(crate::repo_root().join("README.md")).unwrap();
        let expect = format!("对话 {chat} 家 + 生图 / 视频 / 配音 {media} 条");
        assert!(
            readme.contains(&expect),
            "README 的预置数量与代码不一致，应为「{expect}」—— 改了预置记得同步 README"
        );
    }
}
