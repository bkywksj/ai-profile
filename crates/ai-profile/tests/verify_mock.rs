//! 「获取模型」零成本验证的模拟服务端测试：守住端点拼接在真实请求里的样子。
//!
//! 起因（2026-09-24）：从别的工具粘来 Anthropic 协议、地址只填到主机名的配置，
//! 中转网关的 `/models` 不带 `/v1` 也能用，于是「获取」通过、对话却打到 `/messages` 拿回一张网页。
//! 修复后两边都按 Anthropic 约定补 `/v1`，这里断言验证请求确实发到了 `/v1/models`。
#![cfg(feature = "client")]

use ai_profile::client::{ServiceConfig, Verifier};
use ai_profile::Protocol;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 🔴 关掉代理：reqwest 默认读 HTTP_PROXY 且不为 127.0.0.1 绕开（见 media_mock.rs 同名说明）。
fn verifier() -> Verifier {
    Verifier::from_builder(ai_profile::reqwest::Client::builder().no_proxy()).expect("建客户端")
}

fn models_body() -> serde_json::Value {
    json!({"data": [{"id": "claude-opus-5", "type": "model"}]})
}

#[tokio::test]
async fn anthropic_base_without_version_hits_v1_models() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", "sk-ant"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .expect(1)
        .mount(&s)
        .await;

    let base = s.uri(); // 只有主机名 + 端口，照 Claude Code 的习惯
    let ok = verifier()
        .verify(
            ServiceConfig::new(Protocol::Anthropic, &base)
                .with_api_key("sk-ant")
                .with_model("claude-opus-5"),
        )
        .await
        .expect("补 /v1 后验证通过");
    assert!(ok.model_in_list);
}

/// 🔴 OpenAI 兼容一侧不补：不带版本段就原样拼（有的服务商就是要这样填）。
#[tokio::test]
async fn openai_base_is_used_as_is() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer sk-oa"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "m"}]})))
        .expect(1)
        .mount(&s)
        .await;

    let base = s.uri();
    verifier()
        .verify(
            ServiceConfig::new(Protocol::OpenAiCompatible, &base)
                .with_api_key("sk-oa")
                .with_model("m"),
        )
        .await
        .expect("原样拼 /models");
}
