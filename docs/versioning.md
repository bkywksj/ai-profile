# 版本化策略

本 crate 遵循 [语义化版本](https://semver.org/lang/zh-CN/)。下游是五个独立发版的桌面
应用，**改一个 `pub` 字段就可能 break 它们全部** —— 所以每次改动前先对照本表定版本位。

## 版本位判定

| 改动 | 版本位 | 说明 |
|---|---|---|
| 改模型 id / 加 provider / 改 base_url | **patch** | 纯数据更新，不动 API 形状 |
| 加 `Kind` 枚举值 | **minor** | `#[non_exhaustive]` 保证下游 `match` 不 break |
| 给 struct 加字段 | **minor** | 同上 |
| 加公开函数 / 方法 | **minor** | |
| 改 / 删 struct 已有字段 | **major** | 破坏性 |
| 改函数签名 | **major** | |
| 改 `preset.key` | **major** | 🔴 见下 |
| 改 `ai.profile` 的**规范**字段名 | **major** | 见下 |

> 🔴 **0.x 阶段的换算**：上表按 1.0 之后的语义写。0.x 期间 Cargo 把版本位整体右移一位 ——
> 表中 patch 与 minor（兼容的新增）都发 `0.1.x` 补丁位，下游 `cargo update` 自动拿到；
> 表中 major（破坏性）才发 `0.x.0`，下游必须改 `Cargo.toml`。0.1.2、0.1.3 的新增公开 API 都按补丁位发。

### 🔴 reqwest 的大版本在公开 API 里

`Verifier::from_builder` 收 `reqwest::ClientBuilder`，`lib.rs` 重导出了 `reqwest`。
后果是 **reqwest 的大版本升级 = 本 crate 的 major 变更**，不是 patch：

| 改动 | 版本位 |
|---|---|
| reqwest `0.12.x` → `0.12.y` | patch（补丁版本不破坏 API） |
| reqwest `0.12` → `0.13` | **major** 🔴 |

这是**刻意付的代价**。不重导出的话，下游用自己依赖树里的 reqwest 建 builder，
版本一旦不同就编译失败，报错还是「expected `ClientBuilder`, found `ClientBuilder`」
这种看不懂的形式。宁可把耦合写进版本号，也不要让下游撞上这种错误。

替代方案（未采用）：自己定义一个 `ProxyConfig` 结构体做中间层。否决理由是
代理配置各家差异极大 —— sigil 用的是 `reqwest::Proxy::custom(闭包)` 按 URL 动态路由，
任何简化的中间层都覆盖不了，最后还是要开逃生口。

## 🔴 两个特别危险的改动

### 1. `preset.key` 是存量配置的锚

用户已保存的配置靠 `key` 找回对应的预置模板（`infer_preset_key` 的返回值就是它）。
改名会让所有老配置掉进「自定义端点」—— 用户看到的是"我配好的服务商突然不认识了"。

**要改名时**：保留旧 key 作为别名，而不是直接替换。

### 2. `ai.profile` 的字段名只增不改

协议是跨应用契约，且**别人的工具也可能在生成它**。规则：

- **解析端**可以随时加别名（宽进）—— patch
- **生成端**改规范拼写 = 生态分裂 —— major，且应当有极强的理由
- 加可选字段 —— minor
- 删字段 / 改字段语义 —— major

版本号 `v` 的兼容规则：**只拒绝更高版本**。低版本能被高版本实现读懂（字段只增不改），
拒绝低版本会把老软件分享的配置挡在外面。

## `#[non_exhaustive]` 的用法

所有公开 struct / enum 一律加 —— 这是把「加字段」从 major 降到 minor 的唯一办法，
**第一版就要加**，事后补本身就是破坏性变更。

### 🔴 但入参类型必须同时提供 builder

`#[non_exhaustive]` 让外部 crate **不能用字面量构造**该类型（E0639）。

- **出参**类型（`ProviderPreset` / `Vendor` / `VerifyOk`）：下游只读，加就行
- **入参**类型（`ServiceConfig`）：下游要构造它，**必须配完整的链式 builder**，
  否则加了 `non_exhaustive` 等于让它无法被使用

这条是实测踩出来的：`ServiceConfig` 最初只有 `new()` 没有 `with_*()`，
xtask 作为外部 crate 编译直接报 E0639。单元测试抓不到 —— 它们在 crate 内部，
不受这条限制。**`tests/` 下的集成测试才是这类问题的唯一防线。**

## 发版前检查

```bash
cargo test -p ai-profile                                    # 默认 feature
cargo test -p ai-profile --no-default-features --features chat
cargo test -p ai-profile --features client
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo xtask gen-docs                                        # 确认 providers.md 无变化
cargo xtask gen-spec                                        # 改版本号后 spec/ 的 crateVersion 必变，一并提交
RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p ai-profile --all-features --no-deps
cargo package -p ai-profile                                 # 打包 + 编译验证
```

另外三条是 0.1.0 发布时漏掉、只能靠补发 0.1.1 弥补的：

- **docs.rs 按全部 feature 构建**：解出的包内 `Cargo.toml` 里要有 `[package.metadata.docs.rs] all-features = true`，
  否则 docs.rs 只用默认 feature，`client` / `media` 在文档站上整块消失
- **打包后单独跑测试**：`cargo package` 的产物解到临时目录再 `cargo test --all-features`。
  下载包里没有仓库的 `docs/`，依赖它的测试必须能跳过
- **MSRV 真编**：CI 的 `msrv` 任务用声明的 1.88 编一遍，推送后等它绿了再发布

完整发版流程（版本号、CHANGELOG、发布、tag、核对、交接）见技能 `crate-release`。

> 🔴 `cargo test --workspace` **不能**代替前三条：xtask 依赖 `client` feature，
> workspace 级命令会触发 feature unification，让 `ai-profile` 永远带上 client ——
> 「不带 client 能否编译」那条就永远测不到。这也是实测踩出来的。

`cargo xtask probe`（探活各家端点）是**人工触发的季度体检**，不进发版流程，
更不能进 CI —— 每次 PR 都打人家的接口既是滥用，也会让 CI 随网络波动随机红。
