use super::probes::command_exists;
use super::{OutputCallback, ToolOutput, ToolRequest, ToolResult};
use crate::plugins::{PluginLaunch, PluginScript};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct Capture {
    text: String,
    truncated: bool,
}

pub async fn execute_shell(
    request: &ToolRequest,
    cancel: CancellationToken,
    on_output: OutputCallback,
) -> ToolResult {
    let shell = match request.arguments.get("shell").and_then(Value::as_str) {
        Some(shell) => shell,
        None => return ToolResult::error(request, "run_command 缺少 shell 参数"),
    };
    let command = match request.arguments.get("command").and_then(Value::as_str) {
        Some(command) if !command.trim().is_empty() => command,
        _ => return ToolResult::error(request, "run_command 缺少有效的 command 参数"),
    };
    let explanation = request
        .arguments
        .get("explanation_zh")
        .and_then(Value::as_str)
        .unwrap_or("");
    if explanation.trim().is_empty() {
        return ToolResult::error(request, "run_command 必须提供 explanation_zh");
    }
    if !super::probes::available_shells()
        .iter()
        .any(|available| available == shell)
    {
        return ToolResult::error(request, format!("本机未检测到可用 shell：{shell}"));
    }
    let timeout = request
        .arguments
        .get("timeout_sec")
        .and_then(Value::as_u64)
        .unwrap_or(120)
        .clamp(1, 1800);
    execute_fixed_shell(request, shell, command, timeout, cancel, on_output).await
}

pub(crate) async fn execute_fixed_shell(
    request: &ToolRequest,
    shell: &str,
    command: &str,
    timeout_sec: u64,
    cancel: CancellationToken,
    on_output: OutputCallback,
) -> ToolResult {
    let mut temporary_script = None;
    let mut process = match shell {
        "powershell" => {
            let executable = if command_exists("pwsh.exe") {
                "pwsh"
            } else {
                "powershell.exe"
            };
            let mut process = Command::new(executable);
            process.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
                &encode_powershell(&format!(
                    "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; $ProgressPreference = 'SilentlyContinue';\n{command}"
                )),
            ]);
            process
        }
        "cmd" => {
            let mut process = Command::new("cmd.exe");
            process.args(["/d", "/s", "/c", command]);
            process
        }
        "wsl" => {
            let mut process = Command::new("wsl.exe");
            process.args(["-e", "bash", "-lc", command]);
            process
        }
        "python" => {
            let launch = match super::python::launch() {
                Ok(launch) => launch,
                Err(error) => return ToolResult::error(request, error),
            };
            let mut process = match external_process(command, &launch, &mut temporary_script) {
                Ok(process) => process,
                Err(error) => return ToolResult::error(request, error),
            };
            process.env("PYTHONIOENCODING", "utf-8");
            process
        }
        other => {
            let launch = match crate::plugins::launch(other) {
                Some(launch) => launch,
                None => return ToolResult::error(request, format!("不支持的 shell：{other}")),
            };
            match external_process(command, &launch, &mut temporary_script) {
                Ok(process) => process,
                Err(error) => return ToolResult::error(request, error),
            }
        }
    };

    process
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    process.creation_flags(0x08000000);

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            drop(temporary_script.take());
            return ToolResult::error(request, format!("无法启动命令：{error}"));
        }
    };

    #[cfg(windows)]
    let job = match Job::assign(child.id().unwrap_or(0)) {
        Ok(job) => Some(job),
        Err(error) => {
            let _ = child.kill().await;
            drop(temporary_script.take());
            return ToolResult::error(request, format!("无法建立进程树保护：{error}"));
        }
    };

    let capture = Arc::new(Mutex::new(Capture::default()));
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout_task = tokio::spawn(pump_output(
        stdout,
        "stdout",
        request.id.clone(),
        on_output.clone(),
        capture.clone(),
    ));
    let stderr_task = tokio::spawn(pump_output(
        stderr,
        "stderr",
        request.id.clone(),
        on_output,
        capture.clone(),
    ));

    let wait_result = tokio::select! {
        status = child.wait() => WaitOutcome::Exited(status),
        _ = cancel.cancelled() => WaitOutcome::Cancelled,
        _ = tokio::time::sleep(Duration::from_secs(timeout_sec)) => WaitOutcome::TimedOut,
    };

    let (status, timed_out, cancelled) = match wait_result {
        WaitOutcome::Exited(status) => (Some(status), false, false),
        WaitOutcome::Cancelled => {
            let _ = child.kill().await;
            let status = Some(child.wait().await);
            (status, false, true)
        }
        WaitOutcome::TimedOut => {
            let _ = child.kill().await;
            let status = Some(child.wait().await);
            (status, true, false)
        }
    };

    #[cfg(windows)]
    drop(job);
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    drop(temporary_script);

    let capture = capture.lock().expect("output capture mutex poisoned");
    let mut content = capture.text.clone();
    let truncated = capture.truncated;
    drop(capture);
    if timed_out {
        content.push_str(&format!("\n[命令在 {timeout_sec} 秒后超时，进程树已结束]"));
    } else if cancelled {
        content.push_str("\n[命令已由用户停止，进程树已结束]");
    }
    if truncated {
        content.push_str("\n[输出超过 4 MB，后续输出仅实时显示，未完整保留]");
    }
    let exit_code = status
        .and_then(|result| result.ok())
        .and_then(|status| status.code());
    if let Some(code) = exit_code {
        content.push_str(&format!("\n[进程退出代码：{code}]"));
    }

    ToolResult {
        call_id: request.id.clone(),
        name: request.name.clone(),
        content,
        exit_code,
        duration_ms: 0,
        timed_out,
        cancelled,
        truncated,
        risk: request.risk.clone(),
    }
}

enum WaitOutcome {
    Exited(std::io::Result<std::process::ExitStatus>),
    TimedOut,
    Cancelled,
}

async fn pump_output<R>(
    mut reader: R,
    stream: &'static str,
    call_id: String,
    callback: OutputCallback,
    capture: Arc<Mutex<Capture>>,
) where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0u8; 8192];
    let mut pending = Vec::new();
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(count) => {
                pending.extend_from_slice(&chunk[..count]);
                while let Some(position) = pending.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = pending.drain(..=position).collect();
                    emit_decoded(&line, stream, &call_id, &callback, &capture);
                }
                if pending.len() > 64 * 1024 {
                    let line: Vec<u8> = std::mem::take(&mut pending);
                    emit_decoded(&line, stream, &call_id, &callback, &capture);
                }
            }
            Err(error) => {
                emit_decoded(
                    format!("\n[读取输出失败：{error}]\n").as_bytes(),
                    stream,
                    &call_id,
                    &callback,
                    &capture,
                );
                break;
            }
        }
    }
    if !pending.is_empty() {
        emit_decoded(&pending, stream, &call_id, &callback, &capture);
    }
}

fn emit_decoded(
    bytes: &[u8],
    stream: &str,
    call_id: &str,
    callback: &OutputCallback,
    capture: &Arc<Mutex<Capture>>,
) {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.to_string(),
    };
    {
        let mut capture = capture.lock().expect("output capture mutex poisoned");
        if capture.text.len().saturating_add(text.len()) <= MAX_CAPTURE_BYTES {
            capture.text.push_str(&text);
        } else {
            capture.truncated = true;
        }
    }
    callback(ToolOutput {
        call_id: call_id.to_string(),
        stream: stream.to_string(),
        text,
    });
}

fn external_process(
    command: &str,
    launch: &PluginLaunch,
    temporary_script: &mut Option<tempfile::NamedTempFile>,
) -> Result<Command, String> {
    let mut process = Command::new(&launch.program);
    process.args(&launch.base_args);
    match &launch.script {
        PluginScript::File { extension } => {
            use std::io::Write;
            let mut file = tempfile::Builder::new()
                .prefix("harness-")
                .suffix(extension)
                .tempfile()
                .map_err(|error| format!("无法创建临时脚本：{error}"))?;
            file.write_all(command.as_bytes())
                .map_err(|error| format!("无法写入临时脚本：{error}"))?;
            process.arg(file.path());
            *temporary_script = Some(file);
        }
        PluginScript::Argument => {
            process.arg(command);
        }
    }
    Ok(process)
}

fn encode_powershell(command: &str) -> String {
    let utf16: Vec<u8> = command
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    BASE64.encode(utf16)
}

pub fn truncate_for_model(input: &str, limit_kb: usize) -> (String, bool) {
    let limit = limit_kb.max(2) * 1024;
    if input.len() <= limit {
        return (input.to_string(), false);
    }
    let head = floor_char_boundary(input, limit / 2);
    let tail = ceil_char_boundary(input, input.len() - limit / 2);
    (
        format!(
            "{}\n\n[输出过长，中间省略 {} 字节]\n\n{}",
            &input[..head],
            tail.saturating_sub(head),
            &input[tail..]
        ),
        true,
    )
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(windows)]
struct Job(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

#[cfg(windows)]
impl Job {
    fn assign(pid: u32) -> Result<Self, String> {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        unsafe {
            let job = CreateJobObjectW(None, None).map_err(|error| error.to_string())?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if let Err(error) = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) {
                let _ = CloseHandle(job);
                return Err(error.to_string());
            }
            let process = match OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) {
                Ok(process) if process != HANDLE::default() => process,
                Ok(_) => {
                    let _ = CloseHandle(job);
                    return Err("OpenProcess 返回空句柄".to_string());
                }
                Err(error) => {
                    let _ = CloseHandle(job);
                    return Err(error.to_string());
                }
            };
            let assigned = AssignProcessToJobObject(job, process);
            let _ = CloseHandle(process);
            if let Err(error) = assigned {
                let _ = CloseHandle(job);
                return Err(error.to_string());
            }
            Ok(Self(job))
        }
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_encoding_is_utf16le_base64() {
        assert_eq!(encode_powershell("A"), "QQA=");
    }

    #[test]
    fn truncation_preserves_head_and_tail() {
        let input = format!(
            "{}{}{}",
            "开".repeat(2000),
            "中".repeat(2000),
            "尾".repeat(2000)
        );
        let (output, truncated) = truncate_for_model(&input, 16);
        assert!(truncated);
        assert!(output.starts_with('开'));
        assert!(output.ends_with('尾'));
        assert!(output.contains("中间省略"));
    }

    #[test]
    fn short_output_is_not_truncated() {
        let input = "中文命令输出";
        assert_eq!(truncate_for_model(input, 16), (input.to_string(), false));
    }

    #[cfg(windows)]
    fn request(shell: &str, command: &str, timeout_sec: u64) -> ToolRequest {
        ToolRequest {
            id: format!("test-{shell}"),
            name: "run_command".to_string(),
            arguments: serde_json::json!({
                "shell": shell,
                "command": command,
                "explanation_zh": "验证命令执行器",
                "risk_self": "l1",
                "timeout_sec": timeout_sec,
            }),
            risk: crate::risk::classify(shell, command, None),
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cmd_chinese_output_is_decoded_and_streamed() {
        let streamed = Arc::new(Mutex::new(String::new()));
        let streamed_clone = streamed.clone();
        let callback: OutputCallback = Arc::new(move |output| {
            streamed_clone.lock().unwrap().push_str(&output.text);
        });
        let result = execute_shell(
            &request("cmd", "echo Harness 中文诊断输出", 10),
            CancellationToken::new(),
            callback,
        )
        .await;
        assert_eq!(result.exit_code, Some(0), "{}", result.content);
        assert!(
            result.content.contains("中文诊断输出"),
            "{}",
            result.content
        );
        assert!(streamed.lock().unwrap().contains("中文诊断输出"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn python_executes_utf8_with_selected_runtime() {
        let result = execute_shell(
            &request(
                "python",
                "import sys; print(sys.executable); print('Python 中文输出')",
                30,
            ),
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await;
        assert_eq!(result.exit_code, Some(0), "{}", result.content);
        assert!(
            result.content.contains("Python 中文输出"),
            "{}",
            result.content
        );
        assert!(result.content.contains("python"), "{}", result.content);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn powershell_timeout_ends_process() {
        let result = execute_shell(
            &request("powershell", "Start-Sleep -Seconds 30", 1),
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await;
        assert!(result.timed_out, "{}", result.content);
        assert!(result.content.contains("进程树已结束"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cancellation_stops_running_command() {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let canceller = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            cancel_clone.cancel();
        });
        let result = execute_shell(
            &request("powershell", "Start-Sleep -Seconds 30", 30),
            cancel,
            Arc::new(|_| {}),
        )
        .await;
        canceller.await.unwrap();
        assert!(result.cancelled, "{}", result.content);
        assert!(result.content.contains("用户停止"));
    }
}
