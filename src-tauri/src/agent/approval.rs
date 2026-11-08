use crate::risk::{RiskAssessment, RiskLevel};
use crate::tools::ToolRequest;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub message_id: String,
    pub tool_name: String,
    pub shell: String,
    pub command: String,
    pub arguments: Value,
    pub explanation_zh: String,
    pub risk: RiskAssessment,
    pub level: RiskLevel,
    pub mode: String,
    pub requires_typed_confirmation: bool,
}

impl ApprovalRequest {
    pub fn from_tool(
        session_id: &str,
        turn_id: &str,
        message_id: &str,
        tool: &ToolRequest,
        mode: &str,
    ) -> Self {
        let arguments = tool.arguments.clone();
        let explanation_zh = arguments
            .get("explanation_zh")
            .and_then(Value::as_str)
            .unwrap_or("执行本机检查或修改")
            .to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            call_id: tool.id.clone(),
            message_id: message_id.to_string(),
            tool_name: tool.name.clone(),
            shell: arguments
                .get("shell")
                .and_then(Value::as_str)
                .unwrap_or(&tool.name)
                .to_string(),
            command: arguments
                .get("command")
                .and_then(Value::as_str)
                .or_else(|| arguments.get("path").and_then(Value::as_str))
                .unwrap_or("")
                .to_string(),
            arguments,
            explanation_zh,
            level: tool.risk.level,
            requires_typed_confirmation: tool.risk.requires_typed_confirmation,
            risk: tool.risk.clone(),
            mode: mode.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecision {
    pub allowed: bool,
    pub reason: Option<String>,
    pub confirmation_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResolution {
    pub status: String,
    pub ready_at: Option<i64>,
    pub requires_typed_confirmation: bool,
}

#[derive(Debug)]
struct PendingApproval {
    request: ApprovalRequest,
    confirmation_started: bool,
    ready_instant: Option<Instant>,
    sender: Option<oneshot::Sender<ApprovalDecision>>,
}

#[derive(Default)]
pub struct ApprovalGate {
    pending: Mutex<HashMap<String, PendingApproval>>,
}

impl ApprovalGate {
    pub async fn wait(
        &self,
        app: &AppHandle,
        request: ApprovalRequest,
        cancel: CancellationToken,
    ) -> ApprovalDecision {
        let id = request.id.clone();
        let (sender, receiver) = oneshot::channel();
        {
            let mut pending = self.pending.lock().expect("approval mutex poisoned");
            pending.insert(
                id.clone(),
                PendingApproval {
                    request: request.clone(),
                    confirmation_started: false,
                    ready_instant: None,
                    sender: Some(sender),
                },
            );
        }
        if let Err(error) = app.emit("approval_request", request) {
            let mut pending = self.pending.lock().expect("approval mutex poisoned");
            pending.remove(&id);
            return ApprovalDecision {
                allowed: false,
                reason: Some(format!("无法显示审批卡片：{error}")),
                confirmation_text: None,
            };
        }

        tokio::select! {
            decision = receiver => {
                decision.unwrap_or(ApprovalDecision {
                    allowed: false,
                    reason: Some("审批通道已关闭".to_string()),
                    confirmation_text: None,
                })
            }
            _ = cancel.cancelled() => {
                let mut pending = self.pending.lock().expect("approval mutex poisoned");
                pending.remove(&id);
                ApprovalDecision {
                    allowed: false,
                    reason: Some("本轮对话已停止".to_string()),
                    confirmation_text: None,
                }
            }
        }
    }

    pub fn resolve(
        &self,
        app: &AppHandle,
        id: &str,
        decision: ApprovalDecision,
    ) -> Result<ApprovalResolution, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "审批状态不可用".to_string())?;
        let entry = pending
            .get_mut(id)
            .ok_or_else(|| "审批已处理或不存在".to_string())?;

        if !decision.allowed {
            let request = entry.request.clone();
            let sender = entry.sender.take();
            pending.remove(id);
            if let Some(sender) = sender {
                let _ = sender.send(decision);
            }
            let _ = app.emit(
                "approval_resolved",
                serde_json::json!({
                    "approvalId": id,
                    "sessionId": request.session_id,
                    "callId": request.call_id,
                    "allowed": false,
                }),
            );
            return Ok(ApprovalResolution {
                status: "rejected".to_string(),
                ready_at: None,
                requires_typed_confirmation: false,
            });
        }

        if entry.request.level == RiskLevel::L3 {
            let now = Instant::now();
            if !entry.confirmation_started {
                entry.confirmation_started = true;
                entry.ready_instant = Some(now + Duration::from_secs(3));
                let ready_at = Utc::now().timestamp_millis() + 3000;
                let payload = serde_json::json!({
                    "approvalId": id,
                    "sessionId": entry.request.session_id,
                    "callId": entry.request.call_id,
                    "readyAt": ready_at,
                    "requiresTypedConfirmation": entry.request.requires_typed_confirmation,
                });
                app.emit("approval_confirmation_required", payload)
                    .map_err(|error| error.to_string())?;
                return Ok(ApprovalResolution {
                    status: "confirmation_required".to_string(),
                    ready_at: Some(ready_at),
                    requires_typed_confirmation: entry.request.requires_typed_confirmation,
                });
            }

            if entry.ready_instant.is_some_and(|ready| now < ready) {
                let ready_at = Utc::now().timestamp_millis()
                    + entry.ready_instant.unwrap().duration_since(now).as_millis() as i64;
                return Ok(ApprovalResolution {
                    status: "confirmation_required".to_string(),
                    ready_at: Some(ready_at),
                    requires_typed_confirmation: entry.request.requires_typed_confirmation,
                });
            }
            if entry.request.requires_typed_confirmation
                && decision.confirmation_text.as_deref().map(str::trim) != Some("确认")
            {
                return Err("磁盘级操作需要手动输入“确认”两个字".to_string());
            }
        }

        let request = entry.request.clone();
        let sender = entry.sender.take();
        pending.remove(id);
        if let Some(sender) = sender {
            let _ = sender.send(decision);
        }
        app.emit(
            "approval_resolved",
            serde_json::json!({
                "approvalId": id,
                "sessionId": request.session_id,
                "callId": request.call_id,
                "allowed": true,
            }),
        )
        .map_err(|error| error.to_string())?;
        Ok(ApprovalResolution {
            status: "approved".to_string(),
            ready_at: None,
            requires_typed_confirmation: false,
        })
    }

    pub fn reject_session(&self, session_id: &str, reason: &str) {
        let mut pending = self.pending.lock().expect("approval mutex poisoned");
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, approval)| approval.request.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(mut entry) = pending.remove(&id) {
                if let Some(sender) = entry.sender.take() {
                    let _ = sender.send(ApprovalDecision {
                        allowed: false,
                        reason: Some(reason.to_string()),
                        confirmation_text: None,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_request_binds_the_exact_message_and_tool() {
        let tool = ToolRequest {
            id: "call-check".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({
                "shell": "powershell",
                "command": "Remove-Item .\\temp.txt",
                "explanation_zh": "删除指定临时文件"
            }),
            risk: crate::risk::classify("powershell", "Remove-Item .\\temp.txt", None),
        };
        let request =
            ApprovalRequest::from_tool("session-1", "turn-1", "message-1", &tool, "manual");
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["messageId"], "message-1");
        assert_eq!(value["callId"], "call-check");
        assert_eq!(value["explanationZh"], "删除指定临时文件");
        assert_eq!(value["level"], "l3");
        assert!(value["risk"]["summaryZh"].as_str().is_some());
    }
}
