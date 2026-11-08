use harness_lib::config::{ApiKey, ApiMode, ProviderConfig};
use harness_lib::llm::{self, Message};
use harness_lib::risk;
use harness_lib::tools::{self, ToolOutput, ToolRequest};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn sse(value: Value) -> String {
    format!("data: {}\n\n", serde_json::to_string(&value).unwrap())
}

fn mock_server(bodies: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            let header_end = loop {
                let count = stream.read(&mut chunk).unwrap();
                if count == 0 {
                    panic!("client closed before sending headers");
                }
                request.extend_from_slice(&chunk[..count]);
                if let Some(position) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .unwrap_or(0);
            while request.len() - header_end < content_length {
                let count = stream.read(&mut chunk).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
            }
            let json: Value =
                serde_json::from_slice(&request[header_end..header_end + content_length]).unwrap();
            captured.lock().unwrap().push(json);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (format!("http://{address}/v1"), requests, handle)
}

fn provider(base_url: String, api_mode: ApiMode) -> ProviderConfig {
    ProviderConfig {
        id: "mock".to_string(),
        name: "本地模拟服务".to_string(),
        base_url,
        api_key: ApiKey {
            encrypted: false,
            value: "test-key".to_string(),
        },
        api_mode,
        model: "repair-test".to_string(),
        models: vec!["repair-test".to_string()],
        temperature: None,
    }
}

fn chat_tool_body(arguments: &str) -> String {
    let first = sse(
        json!({"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call-file","type":"function","function":{"name":"read_file","arguments":""}}]}}]}),
    );
    let middle = sse(
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":arguments}}]}}]}),
    );
    let done = sse(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}));
    format!("{first}{middle}{done}data: [DONE]\n\n")
}

fn responses_tool_body(arguments: &str) -> String {
    let added = sse(
        json!({"type":"response.output_item.added","item":{"type":"function_call","id":"item-file","call_id":"call-file","name":"read_file","arguments":""}}),
    );
    let delta = sse(
        json!({"type":"response.function_call_arguments.delta","item_id":"item-file","delta":arguments}),
    );
    let done = sse(
        json!({"type":"response.function_call_arguments.done","item_id":"item-file","arguments":arguments}),
    );
    let item_done = sse(
        json!({"type":"response.output_item.done","item":{"type":"function_call","id":"item-file","call_id":"call-file","name":"read_file","arguments":arguments}}),
    );
    let completed = sse(json!({"type":"response.completed","response":{"output":[]}}));
    format!("{added}{delta}{done}{item_done}{completed}")
}

fn chat_text_body(text: &str) -> String {
    format!(
        "{}data: [DONE]\n\n",
        sse(
            json!({"choices":[{"index":0,"delta":{"role":"assistant","content":text},"finish_reason":"stop"}]})
        )
    )
}

fn responses_text_body(text: &str) -> String {
    format!(
        "{}{}",
        sse(json!({"type":"response.output_text.delta","delta":text})),
        sse(json!({"type":"response.completed","response":{"output":[]}}))
    )
}

async fn exercise_round_trip(mode: ApiMode) {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "诊断内容：只读工具执行成功").unwrap();
    let arguments = serde_json::to_string(&json!({"path":file.path()})).unwrap();
    let (first_body, second_body) = match mode {
        ApiMode::ChatCompletions => (chat_tool_body(&arguments), chat_text_body("检查完成")),
        ApiMode::Responses => (
            responses_tool_body(&arguments),
            responses_text_body("检查完成"),
        ),
    };
    let (base_url, requests, server) = mock_server(vec![first_body, second_body]);
    let provider = provider(base_url, mode);
    let tools = tools::tool_definitions();
    let mut messages = vec![Message::user("读取测试文件")];

    let first = llm::stream(
        &provider,
        "test-key",
        &messages,
        &tools,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(first.tool_calls.len(), 1);
    let call = first.tool_calls[0].clone();
    assert_eq!(call.name, "read_file");

    let request = ToolRequest {
        id: call.id.clone(),
        name: call.name.clone(),
        arguments: serde_json::from_str(&call.arguments).unwrap(),
        risk: risk::classify("cmd", "systeminfo", None),
    };
    let output: Arc<dyn Fn(ToolOutput) + Send + Sync> = Arc::new(|_| {});
    let result = tools::execute(request, CancellationToken::new(), output).await;
    assert_eq!(result.exit_code, Some(0));
    assert!(result.content.contains("诊断内容"));

    messages.push(first.into_message());
    messages.push(Message::tool(call.id.clone(), result.content));
    let second = llm::stream(
        &provider,
        "test-key",
        &messages,
        &tools,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(second.text, "检查完成");

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    match mode {
        ApiMode::ChatCompletions => {
            assert!(requests[0]["tools"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty()));
            let messages = requests[1]["messages"].as_array().unwrap();
            let tool_message = messages
                .iter()
                .find(|message| message["role"] == "tool")
                .unwrap();
            assert!(tool_message["content"]
                .as_str()
                .unwrap()
                .contains("只读工具执行成功"));
        }
        ApiMode::Responses => {
            assert!(requests[0]["tools"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty()));
            let input = requests[1]["input"].as_array().unwrap();
            let output = input
                .iter()
                .find(|item| item["type"] == "function_call_output")
                .unwrap();
            assert!(output["output"]
                .as_str()
                .unwrap()
                .contains("只读工具执行成功"));
        }
    }
    drop(requests);
    server.join().unwrap();
}

#[tokio::test]
async fn chat_completions_executes_tool_and_returns_result() {
    exercise_round_trip(ApiMode::ChatCompletions).await;
}

#[tokio::test]
async fn responses_executes_tool_and_returns_result() {
    exercise_round_trip(ApiMode::Responses).await;
}
