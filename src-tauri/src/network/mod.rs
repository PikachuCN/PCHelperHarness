mod diagnostic_report;
mod dns;
mod export;
mod ipv6;
mod ping;
mod process;
mod public_ip;
mod speed;
mod trace;
mod types;
mod validation;
mod website;

pub use types::{NetworkDiagnosticReport, NetworkTestRequest, NetworkTestResult};

use crate::AppState;
use chrono::Utc;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;
use types::{NetworkProgress, TestStatus, ToolOutcome};
use validation::validate_task_id;

#[derive(Clone)]
pub(crate) struct NetworkContext {
    pub task_id: String,
    pub cancel: CancellationToken,
    emitter: Arc<dyn Fn(NetworkProgress) + Send + Sync>,
}

impl NetworkContext {
    pub fn emit(
        &self,
        tool: &str,
        phase: &str,
        status: &str,
        message: impl Into<String>,
        completed: u32,
        total: u32,
        bytes: u64,
        points: Vec<types::ProgressPoint>,
    ) {
        (self.emitter)(NetworkProgress {
            task_id: self.task_id.clone(),
            tool: tool.to_string(),
            phase: phase.to_string(),
            status: status.to_string(),
            message: message.into(),
            completed,
            total,
            bytes,
            points,
        });
    }
}

pub(crate) async fn execute(
    request: NetworkTestRequest,
    context: NetworkContext,
) -> NetworkTestResult {
    let started = Utc::now();
    let timer = std::time::Instant::now();
    let tool = request.tool_name().to_string();
    let target = request.target();
    let path = request.path();
    context.emit(
        &tool,
        "validate",
        "running",
        "正在验证参数并准备测试",
        0,
        1,
        0,
        Vec::new(),
    );
    if let Err(error) = validation::validate_request(&request) {
        return finish_result(
            &context,
            tool,
            target,
            path,
            started,
            timer,
            ToolOutcome::error("参数验证失败", error, Value::Null),
        );
    }
    if context.cancel.is_cancelled() {
        return finish_result(
            &context,
            tool,
            target,
            path,
            started,
            timer,
            ToolOutcome::cancelled("测试已取消"),
        );
    }
    let outcome = match request {
        NetworkTestRequest::Speed {
            node,
            node_url,
            node_name,
            profile,
            path,
        } => speed::run(node, node_url, node_name, profile, path, &context).await,
        NetworkTestRequest::Website {
            url,
            path,
            timeout_ms,
        } => website::run(url, path, timeout_ms, &context).await,
        NetworkTestRequest::PublicIp { path } => public_ip::run(path, &context).await,
        NetworkTestRequest::Ping {
            target,
            protocol,
            port,
            count,
            timeout_ms,
        } => ping::run(target, protocol, port, count, timeout_ms, &context).await,
        NetworkTestRequest::Trace {
            target,
            max_hops,
            timeout_ms,
        } => trace::run(target, max_hops, timeout_ms, &context).await,
        NetworkTestRequest::Dns {
            domain,
            record_type,
            transport,
            server,
            compare_system,
            timeout_ms,
        } => {
            dns::run(
                domain,
                record_type,
                transport,
                server,
                compare_system,
                timeout_ms,
                &context,
            )
            .await
        }
        NetworkTestRequest::Ipv6 { path } => ipv6::run(path, &context).await,
    };
    finish_result(&context, tool, target, path, started, timer, outcome)
}

fn finish_result(
    context: &NetworkContext,
    tool: String,
    target: Option<String>,
    path: Option<types::NetworkPath>,
    started: chrono::DateTime<Utc>,
    timer: std::time::Instant,
    outcome: ToolOutcome,
) -> NetworkTestResult {
    let finished = Utc::now();
    let duration_ms = timer.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let status_text = serde_json::to_value(outcome.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "error".to_string());
    context.emit(
        &tool,
        "complete",
        &status_text,
        outcome.summary.clone(),
        1,
        1,
        0,
        Vec::new(),
    );
    NetworkTestResult {
        task_id: context.task_id.clone(),
        tool,
        status: outcome.status,
        started_at: started.to_rfc3339(),
        finished_at: finished.to_rfc3339(),
        duration_ms,
        target,
        path,
        summary: outcome.summary,
        error: outcome.error,
        data: outcome.data,
    }
}

fn immediate_error(task_id: String, tool: &str, message: String) -> NetworkTestResult {
    let now = Utc::now().to_rfc3339();
    NetworkTestResult {
        task_id,
        tool: tool.to_string(),
        status: TestStatus::Error,
        started_at: now.clone(),
        finished_at: now,
        duration_ms: 0,
        target: None,
        path: None,
        summary: "无法启动网络测试".to_string(),
        error: Some(message),
        data: Value::Null,
    }
}

fn context(app: &AppHandle, task_id: String, cancel: CancellationToken) -> NetworkContext {
    let app = app.clone();
    NetworkContext {
        task_id,
        cancel,
        emitter: Arc::new(move |event| {
            let _ = app.emit("network_progress", event);
        }),
    }
}

fn register_task(state: &AppState, task_id: &str) -> Result<CancellationToken, String> {
    validate_task_id(task_id)?;
    let token = CancellationToken::new();
    let mut tasks = state
        .network_tasks
        .lock()
        .map_err(|_| "网络任务状态不可用".to_string())?;
    if tasks.contains_key(task_id) {
        return Err("相同 taskId 的任务仍在运行".to_string());
    }
    tasks.insert(task_id.to_string(), token.clone());
    Ok(token)
}

fn unregister_task(state: &AppState, task_id: &str) {
    if let Ok(mut tasks) = state.network_tasks.lock() {
        tasks.remove(task_id);
    }
}

#[tauri::command]
pub async fn run_network_test(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    request: NetworkTestRequest,
) -> Result<NetworkTestResult, String> {
    let tool = request.tool_name();
    let token = match register_task(&state, &task_id) {
        Ok(token) => token,
        Err(error) => return Ok(immediate_error(task_id, tool, error)),
    };
    let result = execute(request, context(&app, task_id.clone(), token)).await;
    unregister_task(&state, &task_id);
    Ok(result)
}

#[tauri::command]
pub async fn run_network_diagnostic(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<NetworkDiagnosticReport, String> {
    let token = match register_task(&state, &task_id) {
        Ok(token) => token,
        Err(error) => return Ok(diagnostic_report::immediate_error(task_id, error)),
    };
    let report = diagnostic_report::run(context(&app, task_id.clone(), token)).await;
    unregister_task(&state, &task_id);
    Ok(report)
}

#[tauri::command]
pub fn cancel_network_task(state: State<'_, AppState>, task_id: String) {
    if let Ok(tasks) = state.network_tasks.lock() {
        if let Some(token) = tasks.get(&task_id) {
            token.cancel();
        }
    }
}

#[tauri::command]
pub fn export_network_report(content: String, format: String) -> Result<String, String> {
    export::export_report(&content, &format)
}
