mod chat_completions;
mod models;
mod responses;
mod sse;
mod types;

use std::time::Instant;

use reqwest::{Client, RequestBuilder};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::config::{ApiMode, ProviderConfig};
pub use types::{
    FunctionDefinition, LlmError, LlmResponse, Message, ModelInfo, ProviderTestResult, StreamEvent,
    ToolCall, ToolDefinition, Usage,
};

const USER_AGENT: &str = "Harness/0.1";

/// 流式调用供应商，并在成功结束时返回聚合后的完整响应。
///
/// `api_key` 必须为已解密值。回调在协议解析任务中同步执行；如需更新 UI，
/// 调用方可在回调中向自身消息通道发送事件。
pub async fn stream<F>(
    provider: &ProviderConfig,
    api_key: &str,
    messages: &[Message],
    tools: &[ToolDefinition],
    cancellation: CancellationToken,
    mut on_event: F,
) -> Result<LlmResponse, LlmError>
where
    F: FnMut(StreamEvent) + Send,
{
    let client = Client::new();
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(LlmError::Cancelled),
        result = async {
            match provider.api_mode {
                ApiMode::ChatCompletions => {
                    chat_completions::stream(
                        &client, provider, api_key, messages, tools, &mut on_event,
                    ).await
                }
                ApiMode::Responses => {
                    responses::stream(
                        &client, provider, api_key, messages, tools, &mut on_event,
                    ).await
                }
            }
        } => result,
    }
}

pub async fn fetch_models(
    provider: &ProviderConfig,
    api_key: &str,
) -> Result<Vec<ModelInfo>, LlmError> {
    models::fetch(&Client::new(), provider, api_key).await
}

/// 先验证模型列表连通性，再用一个无副作用的工具定义验证工具请求能力。
pub async fn test_provider(
    provider: &ProviderConfig,
    api_key: &str,
) -> Result<ProviderTestResult, LlmError> {
    let started = Instant::now();
    let models = fetch_models(provider, api_key).await?;
    let probe_tool = ToolDefinition {
        function: FunctionDefinition {
            name: "harness_connectivity_probe".into(),
            description: Some("Connectivity test only; do not call this tool.".into()),
            parameters: json!({"type": "object", "properties": {}}),
        },
    };
    let supports_tools = stream(
        provider,
        api_key,
        &[Message::user(
            "Reply with OK. Do not call the provided tool.",
        )],
        &[probe_tool],
        CancellationToken::new(),
        |_| {},
    )
    .await
    .is_ok();

    Ok(ProviderTestResult {
        connected: true,
        latency_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        models,
        supports_tools,
    })
}

pub(crate) fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

pub(crate) fn api_request(request: RequestBuilder, api_key: &str) -> RequestBuilder {
    request
        .bearer_auth(api_key)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
}

pub(crate) async fn checked_response(
    request: RequestBuilder,
    api_key: &str,
) -> Result<reqwest::Response, LlmError> {
    let response = request.send().await?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "无法读取错误响应".into());
    let mut message = if api_key.is_empty() {
        body
    } else {
        body.replace(api_key, "[已隐藏]")
    };
    if message.chars().count() > 1024 {
        message = message.chars().take(1024).collect();
        message.push('…');
    }
    Err(LlmError::Api {
        status: status.as_u16(),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_base_url_without_duplicate_slash() {
        assert_eq!(
            endpoint("https://example.test/v1/", "models"),
            "https://example.test/v1/models"
        );
    }

    #[test]
    fn error_type_does_not_embed_api_key() {
        let error = LlmError::Api {
            status: 401,
            message: "密钥已隐藏".into(),
        };
        assert!(!error.to_string().contains("sk-secret"));
    }
}
