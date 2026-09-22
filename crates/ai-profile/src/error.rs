//! 结构化错误 —— 调用方「一键修正」的唯一前提。
//!
//! # 🔴 为什么不用 `Err(String)`
//!
//! 调用方要靠错误**类型**决定给用户什么动作。返回字符串 = 界面只能显示一行红字，
//! 「一键补 /v1」「展开可用模型」这类动作根本做不出来。
//!
//! 每个变体都对应一个具体的 UI 动作：
//!
//! | 变体                | 调用方应当                     |
//! |---------------------|--------------------------------|
//! | `AuthFailed`        | 聚焦到密钥输入框               |
//! | `NotFound`          | 给「一键改用 suggested_url」   |
//! | `Unreachable`       | 跳到网络 / 代理设置            |
//! | `ModelNotFound`     | 展开 `available` 让用户选      |
//! | `ProtocolMismatch`  | 提示切到 `expect` 对应的模板   |
//! | `MissingExtraField` | 验证前就禁用按钮，指出缺哪个   |
//!
//! **加新变体前先问：调用方拿到它能做什么动作？** 做不出动作的变体不该加。

use serde::Serialize;

use crate::kind::Protocol;

/// 验证 / 调用失败的结构化原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
#[non_exhaustive]
pub enum VerifyError {
    /// 401 / 403 —— 密钥无效或已过期。
    #[error("密钥无效或已过期：{detail}")]
    AuthFailed {
        /// 端点返回的原始说明（已脱敏，不含密钥）
        detail: String,
    },

    /// 404 —— 端点不存在。多半是 base_url 缺版本段。
    ///
    /// `suggested_url` 非空时，调用方应提供「一键改用」按钮。
    #[error("端点不存在：{requested_url}")]
    NotFound {
        /// 实际请求的完整 URL —— 直接展示给用户，省去他猜"到底打了哪个地址"
        requested_url: String,
        /// 推断出的正确地址；非空时调用方应给「一键改用」按钮
        suggested_url: Option<String>,
    },

    /// 连不上（超时 / DNS / TLS）。`proxy_hint` 为真时建议提示用户配代理。
    #[error("无法连接到端点{}", if *.proxy_hint { "（国内访问该站点可能需要代理）" } else { "" })]
    Unreachable {
        /// 该 host 在国内通常需要代理，调用方可据此提示去网络设置
        proxy_hint: bool,
    },

    /// 端点可达，但它不认当前填的模型名。`available` 是端点返回的真实清单。
    #[error("端点不认识该模型，它提供了 {} 个可选模型", available.len())]
    ModelNotFound {
        /// 端点返回的真实可用清单，调用方展开成下拉让用户改选
        available: Vec<String>,
    },

    /// 密钥只接受另一种协议（典型：中转站的 key 限定 `/v1/messages`）。
    #[error("密钥要求 {expect:?} 协议，当前配置不匹配")]
    ProtocolMismatch {
        /// 密钥实际要求的协议，调用方据此提示切到对应模板
        expect: Protocol,
    },

    /// 服务商要求的专有字段没填（如豆包 TTS 的 `appid`）。
    ///
    /// 调用方应在**发起验证之前**就用它禁用按钮 —— 禁用态优于「点了才报错」。
    #[error("缺少该服务商要求的配置项：{key}")]
    MissingExtraField {
        /// 缺失字段的 key（对应预置里的 `extra_fields`）
        key: String,
    },

    /// 端点返回了预期之外的内容（解析失败等）。兜底变体。
    #[error("端点返回了无法解析的内容：{detail}")]
    Malformed {
        /// 解析失败的简述（不含响应体原文，避免把密钥带进日志）
        detail: String,
    },
}

impl VerifyError {
    /// 这个错误是否可能通过「改配置」解决（而非重试）。
    ///
    /// 调用方据此决定给「重试」还是给「去修改」按钮。
    pub fn is_actionable(&self) -> bool {
        !matches!(self, VerifyError::Unreachable { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误要能原样序列化给前端 —— 带 `code` 判别字段，前端按 code 分支。
    #[test]
    fn serializes_with_code_tag() {
        let e = VerifyError::NotFound {
            requested_url: "https://api.deepseek.com/chat/completions".into(),
            suggested_url: Some("https://api.deepseek.com/v1".into()),
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(
            j.contains(r#""code":"not_found""#),
            "前端要按 code 分支：{j}"
        );
        assert!(j.contains("suggested_url"), "一键修正靠这个字段：{j}");
    }

    /// 网络不通只能重试，其余都是「改配置」能解决的。
    #[test]
    fn unreachable_is_not_actionable() {
        assert!(!VerifyError::Unreachable { proxy_hint: true }.is_actionable());
        assert!(VerifyError::AuthFailed { detail: "x".into() }.is_actionable());
    }
}
