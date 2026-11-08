use futures_util::StreamExt;
use reqwest::Client;
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
    let input: Vec<Value> = messages.iter().flat_map(message_to_items).collect();
    let tool_values: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let mut value = json!({
                "type": "function",
                "name": tool.function.name,
                "parameters": tool.function.parameters
            });
            if let Some(description) = &tool.function.description {
                value["description"] = json!(description);
            }
            value
        })
        .collect();
    let mut body = Map::from_iter([
        ("model".into(), json!(provider.model)),
        ("input".into(), Value::Array(input)),
        ("stream".into(), Value::Bool(true)),
    ]);
    if let Some(temperature) = provider.temperature {
        body.insert("temperature".into(), json!(temperature));
    }
    if !tool_values.is_empty() {
        body.insert("tools".into(), Value::Array(tool_values));
    }

    let request = api_request(
        client.post(endpoint(&provider.base_url, "responses")),
        api_key,
    )
    .json(&Value::Object(body));
    let response = checked_response(request, api_key).await?;
    let mut bytes = response.bytes_stream();
    let mut parser = SseParser::new();
    let mut aggregator = ResponsesAggregator::default();

    while let Some(chunk) = bytes.next().await {
        for event in parser.feed(&chunk?)? {
            if event.data.trim() == "[DONE]" {
                continue;
            }
            let value: Value = serde_json::from_str(&event.data)?;
            aggregator.push(value, on_event)?;
        }
    }
    if let Some(event) = parser.finish()? {
        if event.data.trim() != "[DONE]" {
            aggregator.push(serde_json::from_str(&event.data)?, on_event)?;
        }
    }
    aggregator.finish(on_event)
}

fn message_to_items(message: &Message) -> Vec<Value> {
    match message {
        Message::System { content } => vec![json!({
            "role": "system",
            "content": [{"type": "input_text", "text": content}]
        })],
        Message::User { content } => vec![json!({
            "role": "user",
            "content": [{"type": "input_text", "text": content}]
        })],
        Message::Assistant {
            text, tool_calls, ..
        } => {
            let mut items = Vec::new();
            if !text.is_empty() {
                items.push(json!({
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": text}]
                }));
            }
            items.extend(tool_calls.iter().map(|call| {
                json!({
                    "type": "function_call",
                    "call_id": call.id,
                    "name": call.name,
                    "arguments": call.arguments
                })
            }));
            items
        }
        Message::Tool { call_id, content } => vec![json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": content
        })],
    }
}

#[derive(Debug, Default)]
struct PendingCall {
    item_id: String,
    index: usize,
    call: ToolCall,
    started: bool,
    completed: bool,
}

#[derive(Debug, Default)]
struct ResponsesAggregator {
    response: LlmResponse,
    calls: Vec<PendingCall>,
}

impl ResponsesAggregator {
    fn push<F>(&mut self, value: Value, on_event: &mut F) -> Result<(), super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        match event_type {
            "response.output_text.delta" => {
                if let Some(delta) = nonempty_delta(&value) {
                    self.response.text.push_str(delta);
                    on_event(StreamEvent::TextDelta {
                        delta: delta.to_owned(),
                    });
                }
            }
            "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta"
            | "response.reasoning.delta" => {
                if let Some(delta) = nonempty_delta(&value) {
                    self.response.reasoning.push_str(delta);
                    on_event(StreamEvent::ReasoningDelta {
                        delta: delta.to_owned(),
                    });
                }
            }
            "response.output_item.added" => {
                if let Some(item) = value.get("item").filter(|item| is_function_call(item)) {
                    self.upsert_item(item, on_event)?;
                }
            }
            "response.function_call_arguments.delta" => {
                let item_id = required_item_id(&value)?;
                let delta = value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let index = self.ensure_call(item_id);
                self.start_call(index, on_event);
                if !delta.is_empty() {
                    let pending = &mut self.calls[index];
                    pending.call.arguments.push_str(delta);
                    on_event(StreamEvent::ToolCallArgumentsDelta {
                        index: pending.index,
                        delta: delta.to_owned(),
                    });
                }
            }
            "response.function_call_arguments.done" => {
                let item_id = required_item_id(&value)?;
                let index = self.ensure_call(item_id);
                self.start_call(index, on_event);
                if let Some(arguments) = value.get("arguments").and_then(Value::as_str) {
                    self.calls[index].call.arguments = arguments.to_owned();
                }
            }
            "response.output_item.done" => {
                if let Some(item) = value.get("item").filter(|item| is_function_call(item)) {
                    let index = self.upsert_item(item, on_event)?;
                    self.complete_call(index, on_event);
                }
            }
            "response.completed" => {
                if let Some(response) = value.get("response") {
                    self.apply_completed_response(response, on_event)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn ensure_call(&mut self, item_id: &str) -> usize {
        if let Some(index) = self.calls.iter().position(|call| call.item_id == item_id) {
            return index;
        }
        let index = self.calls.len();
        self.calls.push(PendingCall {
            item_id: item_id.to_owned(),
            index,
            ..PendingCall::default()
        });
        index
    }

    fn upsert_item<F>(&mut self, item: &Value, on_event: &mut F) -> Result<usize, super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        let item_id = item
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| item.get("call_id").and_then(Value::as_str))
            .ok_or_else(|| {
                super::LlmError::InvalidStream("function_call 缺少 id/call_id".into())
            })?;
        let index = self.ensure_call(item_id);
        let call = &mut self.calls[index].call;
        if let Some(call_id) = item.get("call_id").and_then(Value::as_str) {
            call.id = call_id.to_owned();
        } else if call.id.is_empty() {
            call.id = item_id.to_owned();
        }
        if let Some(name) = item.get("name").and_then(Value::as_str) {
            call.name = name.to_owned();
        }
        if let Some(arguments) = item.get("arguments").and_then(Value::as_str) {
            if !arguments.is_empty() {
                call.arguments = arguments.to_owned();
            }
        }
        self.start_call(index, on_event);
        Ok(index)
    }

    fn start_call<F>(&mut self, index: usize, on_event: &mut F)
    where
        F: FnMut(StreamEvent),
    {
        let pending = &mut self.calls[index];
        if pending.started {
            return;
        }
        pending.started = true;
        on_event(StreamEvent::ToolCallStarted {
            index: pending.index,
            id: pending.call.id.clone(),
            name: pending.call.name.clone(),
        });
    }

    fn complete_call<F>(&mut self, index: usize, on_event: &mut F)
    where
        F: FnMut(StreamEvent),
    {
        let pending = &mut self.calls[index];
        if pending.completed {
            return;
        }
        pending.completed = true;
        on_event(StreamEvent::ToolCallCompleted {
            index: pending.index,
            call: pending.call.clone(),
        });
    }

    fn apply_completed_response<F>(
        &mut self,
        response: &Value,
        on_event: &mut F,
    ) -> Result<(), super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        if let Some(usage) = response.get("usage") {
            self.response.usage = Some(Usage {
                prompt_tokens: usage
                    .get("input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                completion_tokens: usage
                    .get("output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                total_tokens: usage
                    .get("total_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            });
        }
        self.response.finish_reason = response
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned);

        if let Some(output) = response.get("output").and_then(Value::as_array) {
            for item in output {
                if is_function_call(item) {
                    self.upsert_item(item, on_event)?;
                } else if item.get("type").and_then(Value::as_str) == Some("message")
                    && self.response.text.is_empty()
                {
                    if let Some(content) = item.get("content").and_then(Value::as_array) {
                        for part in content {
                            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                                if let Some(text) = part.get("text").and_then(Value::as_str) {
                                    self.response.text.push_str(text);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn finish<F>(mut self, on_event: &mut F) -> Result<LlmResponse, super::LlmError>
    where
        F: FnMut(StreamEvent),
    {
        for index in 0..self.calls.len() {
            self.complete_call(index, on_event);
        }
        self.response.tool_calls = self.calls.into_iter().map(|call| call.call).collect();
        Ok(self.response)
    }
}

fn is_function_call(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("function_call")
}

fn nonempty_delta(value: &Value) -> Option<&str> {
    value
        .get("delta")
        .and_then(Value::as_str)
        .filter(|delta| !delta.is_empty())
}

fn required_item_id(value: &Value) -> Result<&str, super::LlmError> {
    value.get("item_id").and_then(Value::as_str).ok_or_else(|| {
        super::LlmError::InvalidStream("function_call_arguments 事件缺少 item_id".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_recorded_output_and_function_call_deltas() {
        let recorded = [
            r#"{"type":"response.output_item.added","item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"shell","arguments":""}}"#,
            r#"{"type":"response.reasoning_summary_text.delta","item_id":"rs_1","delta":"先分析"}"#,
            r#"{"type":"response.output_text.delta","item_id":"msg_1","delta":"请稍候"}"#,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"comm"}"#,
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"and\":\"dir\"}"}"#,
            r#"{"type":"response.function_call_arguments.done","item_id":"fc_1","arguments":"{\"command\":\"dir\"}"}"#,
            r#"{"type":"response.output_item.done","item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"shell","arguments":"{\"command\":\"dir\"}"}}"#,
            r#"{"type":"response.completed","response":{"status":"completed","output":[],"usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}}}"#,
        ];
        let mut aggregator = ResponsesAggregator::default();
        let mut events = Vec::new();
        for data in recorded {
            aggregator
                .push(serde_json::from_str(data).unwrap(), &mut |event| {
                    events.push(event)
                })
                .unwrap();
        }
        let response = aggregator.finish(&mut |event| events.push(event)).unwrap();

        assert_eq!(response.text, "请稍候");
        assert_eq!(response.reasoning, "先分析");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_1");
        assert_eq!(response.tool_calls[0].arguments, "{\"command\":\"dir\"}");
        assert_eq!(response.usage.unwrap().total_tokens, 16);
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::ToolCallStarted { id, name, .. } if id == "call_1" && name == "shell"
        )));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, StreamEvent::ToolCallCompleted { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn completed_response_recovers_unstreamed_tool_call() {
        let value = json!({
            "type": "response.completed",
            "response": {
                "output": [{
                    "id": "fc_2",
                    "type": "function_call",
                    "call_id": "call_2",
                    "name": "read_file",
                    "arguments": "{\"path\":\"a.txt\"}"
                }]
            }
        });
        let mut aggregator = ResponsesAggregator::default();
        aggregator.push(value, &mut |_| {}).unwrap();
        let response = aggregator.finish(&mut |_| {}).unwrap();
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(response.tool_calls[0].arguments, "{\"path\":\"a.txt\"}");
    }

    #[test]
    fn converts_tool_result_to_function_call_output() {
        let items = message_to_items(&Message::tool("call_9", "done"));
        assert_eq!(items[0]["type"], "function_call_output");
        assert_eq!(items[0]["call_id"], "call_9");
        assert_eq!(items[0]["output"], "done");
    }
}
