use super::{OutputCallback, ToolOutput, ToolRequest, ToolResult};
use crate::tools::shell;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;
use std::cmp::Ordering;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use sysinfo::{Disks, Networks, System};
use tokio_util::sync::CancellationToken;

pub fn available_shells() -> Vec<&'static str> {
    let mut shells = vec!["cmd", "powershell"];
    if command_exists("wsl.exe") {
        shells.push("wsl");
    }
    if command_exists("python.exe") || command_exists("python") || command_exists("py.exe") {
        shells.push("python");
    }
    shells
}

#[cfg(windows)]
pub fn command_exists(command: &str) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("where.exe")
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x08000000)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn command_exists(command: &str) -> bool {
    std::process::Command::new("which")
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn success(request: &ToolRequest, content: String) -> ToolResult {
    ToolResult {
        call_id: request.id.clone(),
        name: request.name.clone(),
        content,
        exit_code: Some(0),
        duration_ms: 0,
        timed_out: false,
        cancelled: false,
        truncated: false,
        risk: request.risk.clone(),
    }
}

fn emit(request: &ToolRequest, callback: &OutputCallback, text: &str) {
    callback(ToolOutput {
        call_id: request.id.clone(),
        stream: "stdout".to_string(),
        text: text.to_string(),
    });
}

fn arg_string(request: &ToolRequest, key: &str) -> Result<String, String> {
    request
        .arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("缺少参数 {key}"))
}

fn arg_u64(request: &ToolRequest, key: &str, default: u64, max: u64) -> u64 {
    request
        .arguments
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(1, max)
}

pub async fn execute_probe(
    request: &ToolRequest,
    cancel: CancellationToken,
    on_output: OutputCallback,
) -> ToolResult {
    if cancel.is_cancelled() {
        let mut result = success(request, "操作已取消".to_string());
        result.cancelled = true;
        result.exit_code = None;
        return result;
    }

    let result = match request.name.as_str() {
        "get_system_info" => system_info().map(|text| success(request, text)),
        "list_processes" => processes(request).map(|text| success(request, text)),
        "read_file" => read_file(request).map(|text| success(request, text)),
        "list_directory" => list_directory(request).map(|text| success(request, text)),
        "read_registry" => read_registry(request).map(|text| success(request, text)),
        "write_file" => write_file(request).map(|text| success(request, text)),
        "load_skill" => {
            let name = arg_string(request, "name");
            name.and_then(|name| {
                crate::skills::load(&name)
                    .map_err(|error| error.to_string())
                    .and_then(|skill| {
                        serde_json::to_string_pretty(&skill).map_err(|error| error.to_string())
                    })
            })
            .map(|text| success(request, text))
        }
        "read_skill_file" => {
            let name = arg_string(request, "name");
            let path = arg_string(request, "path");
            name.and_then(|name| {
                path.and_then(|path| {
                    crate::skills::read_file(&name, &path, 256 * 1024)
                        .map_err(|error| error.to_string())
                })
            })
            .map(|text| success(request, text))
        }
        "list_services" => {
            return fixed_powershell(request, services_script(request), 60, cancel, on_output)
                .await;
        }
        "query_event_log" => {
            return fixed_powershell(request, event_log_script(request), 120, cancel, on_output)
                .await;
        }
        other => Err(format!("未知工具：{other}")),
    };

    match result {
        Ok(tool_result) => {
            emit(request, &on_output, &tool_result.content);
            tool_result
        }
        Err(error) => {
            emit(request, &on_output, &format!("错误：{error}\n"));
            ToolResult::error(request, error)
        }
    }
}

async fn fixed_powershell(
    request: &ToolRequest,
    script: Result<String, String>,
    timeout: u64,
    cancel: CancellationToken,
    on_output: OutputCallback,
) -> ToolResult {
    match script {
        Ok(script) => {
            shell::execute_fixed_shell(request, "powershell", &script, timeout, cancel, on_output)
                .await
        }
        Err(error) => ToolResult::error(request, error),
    }
}

fn system_info() -> Result<String, String> {
    let mut system = System::new_all();
    system.refresh_all();
    let mut output = String::new();
    output.push_str("# 系统\n");
    output.push_str(&format!(
        "操作系统：{}\n版本：{}\n内核：{}\n机器名：{}\n当前用户：{}\n",
        System::long_os_version().unwrap_or_else(|| "未知".to_string()),
        System::os_version().unwrap_or_else(|| "未知".to_string()),
        System::kernel_version().unwrap_or_else(|| "未知".to_string()),
        System::host_name().unwrap_or_else(|| "未知".to_string()),
        std::env::var("USERNAME").unwrap_or_else(|_| "未知".to_string()),
    ));
    output.push_str(&format!(
        "开机时间：{}\n已运行：{} 小时 {} 分钟\n\n",
        DateTime::<Utc>::from_timestamp(System::boot_time() as i64, 0)
            .map(|time| time
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string())
            .unwrap_or_else(|| "未知".to_string()),
        System::uptime() / 3600,
        (System::uptime() % 3600) / 60,
    ));

    output.push_str("# CPU\n");
    if let Some(cpu) = system.cpus().first() {
        output.push_str(&format!(
            "型号：{}\n物理/逻辑处理器：{}\n",
            cpu.brand(),
            system.cpus().len()
        ));
    }

    output.push_str("\n# 内存\n");
    output.push_str(&format!(
        "总量：{:.2} GB\n可用：{:.2} GB\n已用：{:.2} GB\n\n",
        system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
        system.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
        system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
    ));

    output.push_str("# 磁盘\n");
    for disk in Disks::new_with_refreshed_list().iter() {
        output.push_str(&format!(
            "{} {}：可用 {:.2} GB / 总计 {:.2} GB（{}）\n",
            disk.mount_point().display(),
            disk.name().to_string_lossy(),
            disk.available_space() as f64 / 1024.0 / 1024.0 / 1024.0,
            disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0,
            disk.file_system().to_string_lossy(),
        ));
    }

    output.push_str("\n# 网络接口（累计流量）\n");
    let networks = Networks::new_with_refreshed_list();
    for (name, data) in networks.iter() {
        output.push_str(&format!(
            "{}：接收 {:.2} MB，发送 {:.2} MB\n",
            name,
            data.total_received() as f64 / 1024.0 / 1024.0,
            data.total_transmitted() as f64 / 1024.0 / 1024.0,
        ));
    }
    Ok(output)
}

fn processes(request: &ToolRequest) -> Result<String, String> {
    let sort_by = request
        .arguments
        .get("sort_by")
        .and_then(Value::as_str)
        .unwrap_or("memory");
    let limit = arg_u64(request, "limit", 20, 100) as usize;
    let mut system = System::new_all();
    system.refresh_all();
    let mut processes: Vec<_> = system.processes().values().collect();
    processes.sort_by(|a, b| {
        if sort_by == "cpu" {
            b.cpu_usage()
                .partial_cmp(&a.cpu_usage())
                .unwrap_or(Ordering::Equal)
        } else {
            b.memory().cmp(&a.memory())
        }
    });
    let mut output = format!(
        "进程数：{}，以下按{}排序\n",
        processes.len(),
        if sort_by == "cpu" { "CPU" } else { "内存" }
    );
    output.push_str("PID\tCPU %\t内存 MB\t名称\n");
    for process in processes.into_iter().take(limit) {
        output.push_str(&format!(
            "{}\t{:.1}\t{:.1}\t{}\n",
            process.pid(),
            process.cpu_usage(),
            process.memory() as f64 / 1024.0 / 1024.0,
            process.name().to_string_lossy(),
        ));
    }
    Ok(output)
}

fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.to_string(),
    }
}

fn read_file(request: &ToolRequest) -> Result<String, String> {
    let path = arg_string(request, "path")?;
    let max_bytes = arg_u64(request, "max_bytes", 256 * 1024, 1024 * 1024);
    let tail_lines = request.arguments.get("tail_lines").and_then(Value::as_u64);
    let metadata = fs::metadata(&path).map_err(|error| format!("无法读取 {path}：{error}"))?;
    if !metadata.is_file() {
        return Err(format!("{path} 不是文件"));
    }
    let mut file = fs::File::open(&path).map_err(|error| error.to_string())?;
    let length = metadata.len();
    let read_length = length.min(max_bytes);
    if tail_lines.is_some() && length > read_length {
        file.seek(SeekFrom::End(-(read_length as i64)))
            .map_err(|error| error.to_string())?;
    }
    let mut buffer = vec![0; read_length as usize];
    file.read_exact(&mut buffer)
        .map_err(|error| error.to_string())?;
    let mut text = decode_text(&buffer);
    if let Some(lines) = tail_lines {
        let split: Vec<&str> = text.lines().collect();
        let skip = split.len().saturating_sub(lines.clamp(1, 5000) as usize);
        text = split.into_iter().skip(skip).collect::<Vec<_>>().join("\n");
    }
    if length > read_length {
        text.push_str(&format!(
            "\n\n[文件共 {} 字节，以上内容已按限制截取]",
            length
        ));
    }
    Ok(text)
}

fn list_directory(request: &ToolRequest) -> Result<String, String> {
    let path = arg_string(request, "path")?;
    let limit = arg_u64(request, "limit", 200, 500) as usize;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&path).map_err(|error| format!("无法列出 {path}：{error}"))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        entries.push(format!(
            "{}\t{}\t{}",
            if metadata.is_dir() {
                "目录"
            } else {
                "文件"
            },
            if metadata.is_file() {
                format!("{} 字节", metadata.len())
            } else {
                "-".to_string()
            },
            entry.file_name().to_string_lossy(),
        ));
    }
    entries.sort();
    let total = entries.len();
    entries.truncate(limit);
    Ok(format!(
        "{}（共 {} 项，显示 {} 项）\n{}",
        path,
        total,
        entries.len(),
        entries.join("\n")
    ))
}

fn write_file(request: &ToolRequest) -> Result<String, String> {
    let path = arg_string(request, "path")?;
    let content = arg_string(request, "content")?;
    let explanation = arg_string(request, "explanation_zh")?;
    if explanation.trim().is_empty() {
        return Err("write_file 必须提供 explanation_zh".to_string());
    }
    if content.len() > 4 * 1024 * 1024 {
        return Err("单次写入内容不能超过 4 MB".to_string());
    }
    let append = request
        .arguments
        .get("append")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(parent) = Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!("目标目录不存在：{}", parent.display()));
        }
    }
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&path)
        .map_err(|error| format!("无法写入 {path}：{error}"))?;
    file.write_all(content.as_bytes())
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(format!(
        "已{}写入 {}（{} 字节）",
        if append { "追加" } else { "覆盖" },
        path,
        content.len()
    ))
}

fn validate_filter(value: &str) -> bool {
    Regex::new(r"^[\p{L}\p{N}\s_./*-]+$")
        .map(|regex| regex.is_match(value))
        .unwrap_or(false)
}

fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn services_script(request: &ToolRequest) -> Result<String, String> {
    let status = request
        .arguments
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let name = request
        .arguments
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let limit = arg_u64(request, "limit", 100, 200);
    if !name.is_empty() && !validate_filter(name) {
        return Err("服务名称筛选包含不支持的字符".to_string());
    }
    let mut filters = Vec::new();
    match status {
        "running" => filters.push("$_.Status -eq 'Running'".to_string()),
        "stopped" => filters.push("$_.Status -eq 'Stopped'".to_string()),
        _ => {}
    }
    if !name.is_empty() {
        filters.push(format!(
            "$_.Name -like '{}' -or $_.DisplayName -like '{}'",
            ps_quote(name),
            ps_quote(name)
        ));
    }
    let filter = if filters.is_empty() {
        "$true".to_string()
    } else {
        format!("({})", filters.join(") -and ("))
    };
    Ok(format!(
        "$ErrorActionPreference='Stop'; Get-Service | Where-Object {{ {filter} }} | Sort-Object Status,Name | Select-Object -First {limit} Name,DisplayName,Status,StartType | ConvertTo-Json -Depth 2"
    ))
}

fn event_log_script(request: &ToolRequest) -> Result<String, String> {
    let log_name = arg_string(request, "log_name")?;
    let source = request
        .arguments
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("");
    let level = request
        .arguments
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let hours = arg_u64(request, "hours", 24, 720);
    let limit = arg_u64(request, "limit", 50, 200);
    if !validate_filter(&log_name) || (!source.is_empty() && !validate_filter(source)) {
        return Err("日志名或来源包含不支持的字符".to_string());
    }
    let level_number = match level {
        "critical" => Some(1),
        "error" => Some(2),
        "warning" => Some(3),
        "information" => Some(4),
        "all" => None,
        _ => return Err("不支持的事件级别".to_string()),
    };
    let mut map = format!(
        "@{{LogName='{}';StartTime=(Get-Date).AddHours(-{})",
        ps_quote(&log_name),
        hours
    );
    if let Some(level) = level_number {
        map.push_str(&format!(";Level={level}"));
    }
    map.push('}');
    let source_filter = if source.is_empty() {
        String::new()
    } else {
        format!(
            " | Where-Object {{ $_.ProviderName -like '{}' }}",
            ps_quote(source)
        )
    };
    Ok(format!(
        "$ErrorActionPreference='Stop'; Get-WinEvent -FilterHashtable {map} -MaxEvents {limit}{source_filter} | Select-Object TimeCreated,Id,LevelDisplayName,ProviderName,Message | ConvertTo-Json -Depth 3"
    ))
}

#[cfg(windows)]
fn read_registry(request: &ToolRequest) -> Result<String, String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let path = arg_string(request, "path")?;
    let value_name = request.arguments.get("value_name").and_then(Value::as_str);
    let (root_name, subkey) = path.split_once('\\').unwrap_or((path.as_str(), ""));
    let root = match root_name.to_ascii_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => HKEY_LOCAL_MACHINE,
        "HKCU" | "HKEY_CURRENT_USER" => HKEY_CURRENT_USER,
        "HKCR" | "HKEY_CLASSES_ROOT" => HKEY_CLASSES_ROOT,
        "HKU" | "HKEY_USERS" => HKEY_USERS,
        "HKCC" | "HKEY_CURRENT_CONFIG" => HKEY_CURRENT_CONFIG,
        _ => return Err("注册表路径必须以 HKLM、HKCU、HKCR、HKU 或 HKCC 开头".to_string()),
    };
    let key = RegKey::predef(root)
        .open_subkey_with_flags(subkey, KEY_READ)
        .map_err(|error| format!("无法打开注册表键：{error}"))?;
    if let Some(name) = value_name {
        let value = key
            .get_raw_value(name)
            .map_err(|error| format!("无法读取值：{error}"))?;
        return Ok(format!("{path}\\{name}\n{}", format_reg_value(&value)));
    }
    let mut output = format!("{path}\n# 值\n");
    for item in key.enum_values() {
        let (name, value) = item.map_err(|error| error.to_string())?;
        output.push_str(&format!("{}\t{}\n", name, format_reg_value(&value)));
    }
    output.push_str("\n# 子键\n");
    for item in key.enum_keys() {
        output.push_str(&format!("{}\n", item.map_err(|error| error.to_string())?));
    }
    Ok(output)
}

#[cfg(windows)]
fn format_reg_value(value: &winreg::RegValue) -> String {
    use winreg::enums::RegType;
    match &value.vtype {
        RegType::REG_SZ | RegType::REG_EXPAND_SZ | RegType::REG_MULTI_SZ => {
            String::from_utf16_lossy(
                &value
                    .bytes
                    .chunks_exact(2)
                    .map(|part| u16::from_le_bytes([part[0], part[1]]))
                    .collect::<Vec<_>>(),
            )
            .trim_end_matches('\0')
            .to_string()
        }
        RegType::REG_DWORD if value.bytes.len() >= 4 => {
            u32::from_le_bytes(value.bytes[..4].try_into().unwrap()).to_string()
        }
        RegType::REG_QWORD if value.bytes.len() >= 8 => {
            u64::from_le_bytes(value.bytes[..8].try_into().unwrap()).to_string()
        }
        _ => format!(
            "0x{}",
            value
                .bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join("")
        ),
    }
}

#[cfg(not(windows))]
fn read_registry(_request: &ToolRequest) -> Result<String, String> {
    Err("注册表读取仅支持 Windows".to_string())
}

#[allow(dead_code)]
fn _normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_fixed_template_filters() {
        assert!(validate_filter("Windows Update"));
        assert!(validate_filter("Microsoft-Windows-PrintService/Admin"));
        assert!(!validate_filter("x'; Remove-Item C:\\"));
    }

    #[test]
    fn powershell_quotes_are_escaped() {
        assert_eq!(ps_quote("O'Brien"), "O''Brien");
    }

    #[test]
    fn decodes_utf8_without_replacement() {
        assert_eq!(decode_text("中文输出".as_bytes()), "中文输出");
    }
}
