pub mod approval;
pub mod prompt;
pub mod session;

use crate::config::{ApiKey, ApprovalMode, Config};
use crate::llm::{Message, StreamEvent};
use crate::risk::{self, RiskLevel};
use crate::tools::{self, ToolOutput, ToolRequest};
use approval::{ApprovalGate, ApprovalRequest};
use serde::Serialize;
use serde_json::Value;
use session::{Session, StoredTool, ToolStatus};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcome {
    pub session_id: String,
    pub turn_id: String,
    pub status: String,
}

pub struct TurnRequest {
    pub config: Config,
    pub session_id: Option<String>,
    pub text: String,
    pub model: Option<String>,
    pub approval_mode: Option<String>,
    pub cancel: CancellationToken,
}

pub async fn run_turn(
    app: AppHandle,
    gate: &ApprovalGate,
    request: TurnRequest,
) -> Result<RunOutcome, String> {
    let TurnRequest {
        config,
        session_id,
        text,
        model,
        approval_mode,
        cancel,
    } = request;
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("不能发送空消息".to_string());
    }
    if text.len() > 64 * 1024 {
        return Err("单条消息不能超过 64 KB".to_string());
    }

    let mut session = match session_id {
        Some(id) => session::load(&id).map_err(|error| error.to_string())?,
        None => Session::new(prompt::system_prompt()),
    };
    session.refresh_system_prompt(prompt::system_prompt());
    let session_id = session.id.clone();
    let turn_id = Uuid::new_v4().to_string();
    let mut provider = config
        .providers
        .iter()
        .find(|provider| provider.id == config.active_provider_id)
        .or_else(|| config.providers.first())
        .cloned()
        .ok_or_else(|| "找不到所选接口配置，请在设置中检查".to_string())?;
    if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
        provider.model = model;
    }
    let api_key = decrypt_key(&provider.api_key)?;
    let mode = match approval_mode.as_deref() {
        Some("manual") => "manual".to_string(),
        Some("auto") => "auto".to_string(),
        Some(_) => return Err("不支持的审批模式".to_string()),
        None => match config.approval.mode {
            ApprovalMode::Manual => "manual".to_string(),
            ApprovalMode::Auto => "auto".to_string(),
        },
    };
    let max_steps = config.execution.max_steps.clamp(1, 100);
    let output_limit_kb = config.execution.output_limit_kb.clamp(2, 1024);

    let user_message = session.push_user(&text);
    session.touch();
    session::save(&session).map_err(|error| error.to_string())?;
    let _ = app.emit(
        "user_message",
        serde_json::json!({
            "sessionId": session_id,
            "turnId": turn_id,
            "message": user_message,
        }),
    );
    let _ = app.emit("session_updated", session.summary());

    for _step in 0..max_steps {
        if cancel.is_cancelled() {
            let outcome = RunOutcome {
                session_id,
                turn_id,
                status: "cancelled".to_string(),
            };
            let _ = app.emit("turn_done", &outcome);
            return Ok(outcome);
        }

        let assistant = session.new_assistant();
        let message_id = assistant.id.clone();
        let _ = app.emit(
            "assistant_started",
            serde_json::json!({
                "sessionId": session_id,
                "turnId": turn_id,
                "message": assistant,
            }),
        );

        let event_app = app.clone();
        let event_session_id = session_id.clone();
        let event_turn_id = turn_id.clone();
        let event_message_id = message_id.clone();
        let model_messages = session.model_messages.clone();
        let response = crate::llm::stream(
            &provider,
            &api_key,
            &model_messages,
            &tools::tool_definitions(),
            cancel.clone(),
            move |event| match event {
                StreamEvent::TextDelta { delta } => {
                    let _ = event_app.emit(
                        "assistant_delta",
                        serde_json::json!({
                            "sessionId": event_session_id,
                            "turnId": event_turn_id,
                            "messageId": event_message_id,
                            "delta": delta,
                        }),
                    );
                }
                StreamEvent::ReasoningDelta { delta } => {
                    let _ = event_app.emit(
                        "reasoning_delta",
                        serde_json::json!({
                            "sessionId": event_session_id,
                            "turnId": event_turn_id,
                            "messageId": event_message_id,
                            "delta": delta,
                        }),
                    );
                }
                _ => {}
            },
        )
        .await;

        let response = match response {
            Ok(response) => response,
            Err(error) => {
                session.touch();
                let _ = session::save(&session);
                let message = error.to_string();
                let cancelled = matches!(error, crate::llm::LlmError::Cancelled);
                let status = if cancelled { "cancelled" } else { "error" };
                let outcome = RunOutcome {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    status: status.to_string(),
                };
                let _ = app.emit(
                    "turn_done",
                    serde_json::json!({
                        "sessionId": outcome.session_id.clone(),
                        "turnId": outcome.turn_id.clone(),
                        "status": outcome.status.clone(),
                        "error": if cancelled { None } else { Some(message.clone()) },
                    }),
                );
                if cancelled {
                    return Ok(outcome);
                }
                return Err(message);
            }
        };

        let completed_message = session.assistant_mut(&message_id).map(|assistant| {
            assistant.content = response.text.clone();
            assistant.reasoning = response.reasoning.clone();
            assistant.clone()
        });
        if let Some(message) = completed_message {
            let _ = app.emit(
                "assistant_sync",
                serde_json::json!({
                    "sessionId": session_id,
                    "turnId": turn_id,
                    "message": message,
                }),
            );
        }
        let has_tool_calls = !response.tool_calls.is_empty();
        session.model_messages.push(response.clone().into_message());

        if !has_tool_calls {
            session.touch();
            session::save(&session).map_err(|error| error.to_string())?;
            let outcome = RunOutcome {
                session_id,
                turn_id,
                status: "done".to_string(),
            };
            let _ = app.emit("turn_done", &outcome);
            return Ok(outcome);
        }

        for call in response.tool_calls {
            let mut arguments = match serde_json::from_str::<Value>(&call.arguments) {
                Ok(value @ Value::Object(_)) => value,
                Ok(_) => {
                    let content = "工具参数必须是 JSON 对象";
                    session
                        .model_messages
                        .push(Message::tool(&call.id, content));
                    continue;
                }
                Err(error) => {
                    let content = format!("工具参数不是有效 JSON：{error}");
                    session
                        .model_messages
                        .push(Message::tool(&call.id, &content));
                    continue;
                }
            };
            if call.name == "run_command" {
                if let Some(object) = arguments.as_object_mut() {
                    object.entry("timeout_sec".to_string()).or_insert_with(|| {
                        Value::from(
                            config
                                .execution
                                .default_timeout_sec
                                .clamp(1, config.execution.max_timeout_sec),
                        )
                    });
                }
            }
            let assessment = assess_tool(&call.name, &arguments);
            let request = ToolRequest {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: arguments.clone(),
                risk: assessment.clone(),
            };
            let explanation = arguments
                .get("explanation_zh")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut stored = StoredTool {
                call_id: call.id.clone(),
                name: call.name.clone(),
                arguments: arguments.clone(),
                explanation_zh: explanation.clone(),
                risk: assessment.clone(),
                status: ToolStatus::Pending,
                output: String::new(),
                duration_ms: 0,
                exit_code: None,
            };

            if (call.name == "run_command" || call.name == "write_file")
                && explanation.trim().is_empty()
            {
                stored.status = ToolStatus::Error;
                stored.output = "模型未提供 explanation_zh，命令未执行".to_string();
                push_tool(&mut session, &message_id, stored.clone());
                emit_tool(
                    &app,
                    &session_id,
                    &turn_id,
                    &message_id,
                    "tool_done",
                    &stored,
                );
                session
                    .model_messages
                    .push(Message::tool(&call.id, &stored.output));
                continue;
            }

            push_tool(&mut session, &message_id, stored.clone());
            emit_tool(
                &app,
                &session_id,
                &turn_id,
                &message_id,
                "tool_started",
                &stored,
            );

            let needs_approval = match assessment.level {
                RiskLevel::L0 => mode == "manual" && config.approval.readonly_needs_approval,
                RiskLevel::L1 => mode != "auto",
                RiskLevel::L2 | RiskLevel::L3 => true,
            };
            if needs_approval {
                let approval =
                    ApprovalRequest::from_tool(&session_id, &turn_id, &message_id, &request, &mode);
                let decision = gate.wait(&app, approval, cancel.clone()).await;
                if !decision.allowed {
                    stored.status = if cancel.is_cancelled() {
                        ToolStatus::Cancelled
                    } else {
                        ToolStatus::Rejected
                    };
                    stored.output = format!(
                        "用户未批准此操作{}",
                        decision
                            .reason
                            .as_deref()
                            .filter(|reason| !reason.trim().is_empty())
                            .map(|reason| format!("：{}", reason.trim()))
                            .unwrap_or_default()
                    );
                    update_tool(&mut session, &message_id, stored.clone());
                    emit_tool(
                        &app,
                        &session_id,
                        &turn_id,
                        &message_id,
                        "tool_done",
                        &stored,
                    );
                    session
                        .model_messages
                        .push(Message::tool(&call.id, &stored.output));
                    session::save(&session).map_err(|error| error.to_string())?;
                    continue;
                }
            }

            stored.status = ToolStatus::Running;
            update_tool(&mut session, &message_id, stored.clone());
            emit_tool(
                &app,
                &session_id,
                &turn_id,
                &message_id,
                "tool_started",
                &stored,
            );
            let output_app = app.clone();
            let output_session = session_id.clone();
            let output_turn = turn_id.clone();
            let callback = Arc::new(move |output: ToolOutput| {
                let _ = output_app.emit(
                    "tool_output",
                    serde_json::json!({
                        "sessionId": output_session,
                        "turnId": output_turn,
                        "callId": output.call_id,
                        "stream": output.stream,
                        "text": output.text,
                    }),
                );
            });
            let result = tools::execute(request, cancel.clone(), callback).await;
            let (model_output, _) = tools::truncate_for_model(&result.content, output_limit_kb);
            stored.output = model_output.clone();
            stored.duration_ms = result.duration_ms;
            stored.exit_code = result.exit_code;
            stored.status = if result.cancelled {
                ToolStatus::Cancelled
            } else if result.timed_out || result.exit_code != Some(0) {
                ToolStatus::Error
            } else {
                ToolStatus::Success
            };
            update_tool(&mut session, &message_id, stored.clone());
            emit_tool(
                &app,
                &session_id,
                &turn_id,
                &message_id,
                "tool_done",
                &stored,
            );
            session
                .model_messages
                .push(Message::tool(&call.id, model_output));
            session.touch();
            session::save(&session).map_err(|error| error.to_string())?;
        }
    }

    let warning = format!(
        "已达到本轮最大步数（{}），请根据已有结果继续指导用户",
        max_steps
    );
    session
        .model_messages
        .push(Message::system(warning.clone()));
    if let Some(message) = session.messages.last_mut() {
        let delta = if message.content.is_empty() {
            warning.clone()
        } else {
            format!("\n\n{warning}")
        };
        message.content.push_str(&delta);
        let _ = app.emit(
            "assistant_delta",
            serde_json::json!({
                "sessionId": session_id.clone(),
                "turnId": turn_id.clone(),
                "messageId": message.id.clone(),
                "delta": delta,
            }),
        );
    }
    session.touch();
    session::save(&session).map_err(|error| error.to_string())?;
    let outcome = RunOutcome {
        session_id,
        turn_id,
        status: "max_steps".to_string(),
    };
    let _ = app.emit("turn_done", &outcome);
    Ok(outcome)
}

fn decrypt_key(key: &ApiKey) -> Result<String, String> {
    crate::config::decrypt_api_key(key).map_err(|error| error.to_string())
}

fn assess_tool(name: &str, arguments: &Value) -> crate::risk::RiskAssessment {
    match name {
        "run_command" => {
            let shell = arguments.get("shell").and_then(Value::as_str).unwrap_or("");
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("");
            let self_report = match arguments.get("risk_self").and_then(Value::as_str) {
                Some("l0") => Some(RiskLevel::L0),
                Some("l1") => Some(RiskLevel::L1),
                Some("l2") => Some(RiskLevel::L2),
                Some("l3") => Some(RiskLevel::L3),
                _ => None,
            };
            risk::classify(shell, command, self_report)
        }
        "write_file" => {
            let path = arguments.get("path").and_then(Value::as_str).unwrap_or("");
            risk::classify_write_file(path)
        }
        _ => risk::classify("cmd", "systeminfo", None),
    }
}

fn push_tool(session: &mut Session, message_id: &str, tool: StoredTool) {
    if let Some(message) = session.assistant_mut(message_id) {
        message.tools.push(tool);
    }
}

fn update_tool(session: &mut Session, message_id: &str, tool: StoredTool) {
    if let Some(message) = session.assistant_mut(message_id) {
        if let Some(existing) = message
            .tools
            .iter_mut()
            .find(|item| item.call_id == tool.call_id)
        {
            *existing = tool;
        } else {
            message.tools.push(tool);
        }
    }
}

fn emit_tool(
    app: &AppHandle,
    session_id: &str,
    turn_id: &str,
    message_id: &str,
    event: &str,
    tool: &StoredTool,
) {
    let _ = app.emit(
        event,
        serde_json::json!({
            "sessionId": session_id,
            "turnId": turn_id,
            "messageId": message_id,
            "tool": tool,
        }),
    );
}
