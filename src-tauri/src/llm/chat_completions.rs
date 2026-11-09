use std::collections::BTreeMap;

use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::sse::SseParser;
use super::types::{LlmResponse, Message, StreamEvent, ToolCall, ToolDefinition, Usage};
use super::{api_request, checked_response, endpoint};
use crate::config::ProviderConfig;

pub(crate) async fn stream<F>(
    client: &Client,
    provider: &ProviderConfig,
    api_key: &str,
    messages: &[Message],
    tools: &[ToolDefinition],
    on_event: &mut F,
) -> Result<LlmResponse, super::LlmError>
where
    F: FnMut(StreamEvent),
{
    let input: Vec<Value> = messages.iter().map(message_to_json).collect();
    let tool_values: Vec<Value> = tools
        .iter()
        .map(|tool| json!({"type": "function", "function": tool.function}))
        .collect();
    let mut body = Map::from_iter([
        ("model".into(), json!(provider.model)),
        ("messages".into(), Value::Array(input)),
        ("stream".into(), Value::Bool(true)),
        ("stream_options".into(), json!({"include_usage": true})),
    ]);
    if let Some(temperature) = provider.temperature {
        body.insert("temperature".into(), json!(temperature));
    }
    if !tool_values.is_empty() {
        body.insert("tools".into(), Value::Array(tool_values));
    }

    let request = api_request(
        client.post(endpoint(&provider.base_url, "chat/completions")),
        provider,
        api_key,
    )
    .json(&Value::Object(body));
    let response = checked_response(request, api_key).await?;
    let mut bytes = response.bytes_stream();
    let mut parser = SseParser::new();
    let mut aggregator = ChatAggregator::default();

    while let Some(chunk) = bytes.next().await {
        for event in parser.feed(&chunk?)? {
            if event.data.trim() == "[DONE]" {
                continue;
            }
            let chunk: ChatChunk = serde_json::from_str(&event.data)?;
            aggregator.push(chunk, on_event)?;
        }
    }
    if let Some(event) = parser.finish()? {
        if event.data.trim() != "[DONE]" {
            aggregator.push(serde_json::from_str(&event.data)?, on_event)?;
        }
    }
    aggregator.finish(on_event)
}

fn message_to_json(message: &Message) -> Value {
    match message {
        Message::System { content } => json!({"role": "system", "content": content}),
        Message::User { content } => json!({"role": "user", "content": content}),
        Message::Assistant {
            text, tool_calls, ..
        } => {
            let calls: Vec<Value> = tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {"name": call.name, "arguments": call.arguments}
                    })
                })
                .collect();
            let mut value = json!({"role": "assistant", "content": text});
            if !calls.is_empty() {
                value["tool_calls"] = Value::Array(calls);
            }
            value
        }
        Message::Tool { call_id, content } => json!({
            "role": "tool",
            "tool_call_id": call_id,
            "content": content
        }),
    }
}

#[derive(Debug, Default, Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default, alias = "reasoning")]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

#[derive(Debug, Default, Deserialize)]
struct ToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: FunctionDelta,
}

#[derive(Debug, Default, Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

#[derive(Debug, Default)]
struct PendingCall {
    call: ToolCall,
    started: bool,
}

#[derive(Debug, Default)]
struct ChatAggregator {
    response: LlmResponse,
    calls: BTreeMap<usize, PendingCall>,
}

impl ChatAggregator {
    fn push<F>(&mut self, chunk: ChatChunk, on_event: &mut F) -> Result<(), super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        for choice in chunk.choices {
            if let Some(delta) = choice.delta.content.filter(|delta| !delta.is_empty()) {
                self.response.text.push_str(&delta);
                on_event(StreamEvent::TextDelta { delta });
            }
            if let Some(delta) = choice
                .delta
                .reasoning_content
                .filter(|delta| !delta.is_empty())
            {
                self.response.reasoning.push_str(&delta);
                on_event(StreamEvent::ReasoningDelta { delta });
            }
            for delta in choice.delta.tool_calls {
                self.push_tool_delta(delta, on_event)?;
            }
            if choice.finish_reason.is_some() {
                self.response.finish_reason = choice.finish_reason;
            }
        }
        if let Some(usage) = chunk.usage {
            self.response.usage = Some(Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            });
        }
        Ok(())
    }

    fn push_tool_delta<F>(
        &mut self,
        delta: ToolCallDelta,
        on_event: &mut F,
    ) -> Result<(), super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        let pending = self.calls.entry(delta.index).or_default();
        if let Some(id) = delta.id {
            if !pending.call.id.is_empty() && pending.call.id != id {
                return Err(super::LlmError::InvalidStream(format!(
                    "工具调用 {} 的 id 发生变化",
                    delta.index
                )));
            }
            pending.call.id = id;
        }
        if let Some(name) = delta.function.name {
            pending.call.name.push_str(&name);
        }
        if !pending.started && (!pending.call.id.is_empty() || !pending.call.name.is_empty()) {
            pending.started = true;
            on_event(StreamEvent::ToolCallStarted {
                index: delta.index,
                id: pending.call.id.clone(),
                name: pending.call.name.clone(),
            });
        }
        if let Some(arguments) = delta.function.arguments.filter(|delta| !delta.is_empty()) {
            pending.call.arguments.push_str(&arguments);
            on_event(StreamEvent::ToolCallArgumentsDelta {
                index: delta.index,
                delta: arguments,
            });
        }
        Ok(())
    }

    fn finish<F>(mut self, on_event: &mut F) -> Result<LlmResponse, super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        for (index, pending) in self.calls {
            on_event(StreamEvent::ToolCallCompleted {
                index,
                call: pending.call.clone(),
            });
            self.response.tool_calls.push(pending.call);
        }
        Ok(self.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_recorded_text_reasoning_and_indexed_tool_calls() {
        let recorded = [
            r#"{"choices":[{"delta":{"reasoning_content":"先查"}}]}"#,
            r#"{"choices":[{"delta":{"content":"好的"}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"shell","arguments":"{\"comm"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","function":{"name":"read","arguments":"{\"path\":\"a\"}"}},{"index":0,"function":{"arguments":"and\"}"}}]}}]}"#,
            r#"{"choices":[],"usage":{"prompt_tokens":8,"completion_tokens":3,"total_tokens":11}}"#,
        ];
        let mut aggregator = ChatAggregator::default();
        let mut events = Vec::new();
        for data in recorded {
            aggregator
                .push(serde_json::from_str(data).unwrap(), &mut |event| {
                    events.push(event)
                })
                .unwrap();
        }
        let response = aggregator.finish(&mut |_| {}).unwrap();

        assert_eq!(response.text, "好的");
        assert_eq!(response.reasoning, "先查");
        assert_eq!(response.tool_calls[0].arguments, "{\"command\"}");
        assert_eq!(response.tool_calls[1].name, "read");
        assert_eq!(response.usage.unwrap().total_tokens, 11);
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::ToolCallArgumentsDelta { index: 0, delta } if delta == "and\"}"
        )));
    }
}
