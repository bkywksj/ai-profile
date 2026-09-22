//! # ai-profile
//!
//! 桌面应用的 AI 模型服务配置层：provider 预置清单、`ai.profile` 互通协议、
//! 端点拼接与连通性验证。
//!
//! ## 边界：什么进、什么不进
//!
//! | | 内容 | 归属 |
//! |---|---|---|
//! | ✅ | 预置清单、协议、端点拼接、模型清洗、验证与结构化错误 | 本 crate |
//! | ❌ | **密钥存储与加密** | 留给应用（各家差异极大） |
//! | ❌ | 数据库 / CRUD / 激活态管理 | 同上 |
//!
//! **本 crate 不持久化任何密钥明文**。验证/调用时接收调用方传入的 key，用完即弃。
//!
//! ## 三条数据铁律（都是踩出来的）
//!
//! 1. **`base_url` 原样使用，绝不推断版本段** —— 各家不统一（多数 `/v1`、智谱 `/v4`、
//!    Gemini `/v1beta/openai`）。推断错的代价是隐性的：用户照文档填对了，
//!    库悄悄加一段，他只看到 404。详见 [`endpoint`]。
//! 2. **默认 model 选「够用档」而非「最强档」** —— 把旗舰塞进默认值，
//!    等于替用户做了一个他没同意的花钱决定。
//! 3. **模型清单清洗用排除法，不用白名单** —— 厂商上新远快于特征词更新，
//!    白名单必然把新模型误藏，而"藏起来"对用户不可见。详见 [`model_filter`]。
//!
//! ## feature
//!
//! ```toml
//! ai-profile = { version = "0.1", features = ["chat"] }              # 仅对话
//! ai-profile = { version = "0.1", features = ["chat", "client"] }    # + 真实调用
//! ```
//!
//! 不开 `client` 时零 HTTP 依赖，纯数据 + 纯函数，可编到 Android / iOS。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod endpoint;
pub mod error;
pub mod kind;
pub mod model_filter;
pub mod preset;
pub mod protocol;

#[cfg(feature = "client")]
pub mod client;

pub use error::VerifyError;
pub use kind::{Kind, Protocol};
pub use preset::{preset_by_key, presets, presets_for, vendors, ProviderPreset, Vendor};
pub use protocol::{parse_profile, to_profile, ParseError, ParsedProfile};

#[cfg(feature = "client")]
pub use client::{verify, ServiceConfig, VerifyOk};

// ⏸ 后续阶段（见 docs/tasks/active/ 的规划）：
// - client::dry_run —— 真实调用（产生费用），按 kind 分实现
