use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_CAPTURE_BYTES: usize = 1024 * 1024;
const MAX_PENDING_LINE: usize = 256 * 1024;
pub type LineCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug)]
pub struct ProcessOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub truncated: bool,
}

#[derive(Default)]
struct Capture {
    text: String,
    truncated: bool,
}

pub async fn run_powershell(
    script: &str,
    params: &Value,
    timeout: Duration,
    cancel: CancellationToken,
    on_line: Option<LineCallback>,
) -> Result<ProcessOutput, String> {
    if cancel.is_cancelled() {
        return Ok(ProcessOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            timed_out: false,
            cancelled: true,
            truncated: false,
        });
    }
    let encoded_params =
        BASE64.encode(serde_json::to_vec(params).map_err(|error| error.to_string())?);
    let preamble = r#"[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; $ProgressPreference = 'SilentlyContinue'; $ErrorActionPreference = 'Stop'; $params = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String($env:HARNESS_NETWORK_PARAMS)) | ConvertFrom-Json;
"#;
    let executable = if command_available("pwsh.exe") {
        "pwsh.exe"
    } else {
        "powershell.exe"
    };
    let mut command = Command::new(executable);
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encode_powershell(&format!("{preamble}{script}")),
        ])
        .env("HARNESS_NETWORK_PARAMS", encoded_params)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);

    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 PowerShell：{error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法捕获进程输出".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法捕获进程错误".to_string())?;
    let stdout_capture = Arc::new(Mutex::new(Capture::default()));
    let stderr_capture = Arc::new(Mutex::new(Capture::default()));
    let stdout_task = tokio::spawn(pump_output(stdout, stdout_capture.clone(), on_line.clone()));
    let stderr_task = tokio::spawn(pump_output(stderr, stderr_capture.clone(), on_line));

    enum Wait {
        Done(std::io::Result<std::process::ExitStatus>),
        Cancelled,
        TimedOut,
    }
    let wait = tokio::select! {
        status = child.wait() => Wait::Done(status),
        _ = cancel.cancelled() => Wait::Cancelled,
        _ = tokio::time::sleep(timeout) => Wait::TimedOut,
    };
    let (status, cancelled, timed_out) = match wait {
        Wait::Done(status) => (Some(status), false, false),
        Wait::Cancelled => {
            let _ = child.kill().await;
            (Some(child.wait().await), true, false)
        }
        Wait::TimedOut => {
            let _ = child.kill().await;
            (Some(child.wait().await), false, true)
        }
    };
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    let stdout = take_capture(stdout_capture);
    let stderr = take_capture(stderr_capture);
    let exit_code = status
        .and_then(|result| result.ok())
        .and_then(|status| status.code());
    Ok(ProcessOutput {
        stdout: stdout.0,
        stderr: stderr.0,
        exit_code,
        timed_out,
        cancelled,
        truncated: stdout.1 || stderr.1,
    })
}

fn take_capture(capture: Arc<Mutex<Capture>>) -> (String, bool) {
    let capture = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (capture.text.clone(), capture.truncated)
}

async fn pump_output<R>(mut reader: R, capture: Arc<Mutex<Capture>>, callback: Option<LineCallback>)
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0u8; 8192];
    let mut pending = Vec::new();
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(count) => {
                append_capture(&capture, &chunk[..count]);
                pending.extend_from_slice(&chunk[..count]);
                while let Some(position) = pending.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = pending.drain(..=position).collect();
                    emit_line(&line, &callback);
                }
                if pending.len() > MAX_PENDING_LINE {
                    emit_line(&std::mem::take(&mut pending), &callback);
                }
            }
            Err(_) => break,
        }
    }
    if !pending.is_empty() {
        emit_line(&pending, &callback);
    }
}

fn append_capture(capture: &Arc<Mutex<Capture>>, bytes: &[u8]) {
    let mut capture = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let remaining = MAX_CAPTURE_BYTES.saturating_sub(capture.text.len());
    if remaining > 0 {
        let text = decode_output(bytes);
        if text.len() <= remaining {
            capture.text.push_str(&text);
        } else {
            capture
                .text
                .push_str(&text[..floor_char_boundary(&text, remaining)]);
            capture.truncated = true;
        }
    } else {
        capture.truncated = true;
    }
}

fn emit_line(bytes: &[u8], callback: &Option<LineCallback>) {
    if let Some(callback) = callback {
        let line = decode_output(bytes).trim().to_string();
        if !line.is_empty() {
            callback(line);
        }
    }
}

fn decode_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.to_string(),
    }
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn command_available(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|path| path.join(command).is_file()))
        .unwrap_or(false)
}

fn encode_powershell(command: &str) -> String {
    let utf16: Vec<u8> = command
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    BASE64.encode(utf16)
}

pub fn result_json(stdout: &str) -> Result<Value, String> {
    let line = stdout
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("HARNESS_RESULT:"))
        .ok_or_else(|| "进程未返回结构化结果".to_string())?;
    serde_json::from_str(line).map_err(|error| format!("无法解析进程结果：{error}"))
}

pub fn process_error(output: &ProcessOutput) -> Option<String> {
    if output.cancelled {
        return None;
    }
    if output.timed_out {
        return Some("命令执行超时".to_string());
    }
    if output.truncated {
        return Some("进程输出超过 1 MB，结果已截断".to_string());
    }
    if output.exit_code != Some(0) {
        let detail = if output.stderr.trim().is_empty() {
            output.stdout.trim()
        } else {
            output.stderr.trim()
        };
        let detail = detail.chars().take(1000).collect::<String>();
        if is_permission_error(&format!("{} {}", output.stdout, output.stderr)) {
            return Some(format!(
                "PERMISSION_DENIED: 操作需要管理员权限：{detail}。可使用“以管理员身份重启”后重试"
            ));
        }
        return Some(format!(
            "PowerShell 执行失败（退出码 {:?}）：{detail}",
            output.exit_code
        ));
    }
    if is_permission_error(&format!("{} {}", output.stdout, output.stderr)) {
        return Some(
            "PERMISSION_DENIED: 操作被系统拒绝，可使用“以管理员身份重启”后重试".to_string(),
        );
    }
    None
}

pub fn is_permission_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "access is denied",
        "permission denied",
        "requires elevation",
        "administrator privilege",
        "unauthorizedaccessexception",
        "拒绝访问",
        "需要提升",
        "管理员权限",
        "权限不足",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_encoding_matches_utf16le() {
        assert_eq!(encode_powershell("A"), "QQA=");
    }

    #[test]
    fn structured_result_ignores_progress_lines() {
        let value =
            result_json("HARNESS_SAMPLE:{\"sequence\":1}\nHARNESS_RESULT:{\"ok\":true}\n").unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn permission_errors_are_machine_recognizable() {
        let output = ProcessOutput {
            stdout: String::new(),
            stderr: "Access is denied".to_string(),
            exit_code: Some(1),
            timed_out: false,
            cancelled: false,
            truncated: false,
        };
        assert!(process_error(&output)
            .unwrap()
            .starts_with("PERMISSION_DENIED:"));
    }
}
