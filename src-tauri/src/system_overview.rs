use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemOverviewSectionResponse {
    pub section: String,
    pub data: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverviewSection {
    Basic,
    Activation,
    Hardware,
    Printers,
    Drivers,
    Software,
    Storage,
    Network,
    Security,
}

impl OverviewSection {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "basic" => Ok(Self::Basic),
            "activation" => Ok(Self::Activation),
            "hardware" => Ok(Self::Hardware),
            "printers" => Ok(Self::Printers),
            "drivers" => Ok(Self::Drivers),
            "software" => Ok(Self::Software),
            "storage" => Ok(Self::Storage),
            "network" => Ok(Self::Network),
            "security" => Ok(Self::Security),
            _ => Err(format!(
                "不支持的系统概况分区：{value}；仅支持 basic、activation、hardware、printers、drivers、software、storage、network、security"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Activation => "activation",
            Self::Hardware => "hardware",
            Self::Printers => "printers",
            Self::Drivers => "drivers",
            Self::Software => "software",
            Self::Storage => "storage",
            Self::Network => "network",
            Self::Security => "security",
        }
    }

    fn timeout_seconds(self) -> u64 {
        match self {
            Self::Basic => 30,
            Self::Activation | Self::Security => 90,
            Self::Hardware
            | Self::Printers
            | Self::Drivers
            | Self::Software
            | Self::Storage
            | Self::Network => 60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTarget {
    Activation,
    WindowsUpdate,
    DeviceManager,
    InstalledApps,
    Network,
    Security,
}

impl SettingsTarget {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "activation" => Ok(Self::Activation),
            "windowsUpdate" => Ok(Self::WindowsUpdate),
            "deviceManager" => Ok(Self::DeviceManager),
            "installedApps" => Ok(Self::InstalledApps),
            "network" => Ok(Self::Network),
            "security" => Ok(Self::Security),
            _ => Err(format!(
                "不支持的系统设置目标：{value}；仅支持 activation、windowsUpdate、deviceManager、installedApps、network、security"
            )),
        }
    }

    fn settings_uri(self) -> Option<&'static str> {
        match self {
            Self::Activation => Some("ms-settings:activation"),
            Self::WindowsUpdate => Some("ms-settings:windowsupdate"),
            Self::InstalledApps => Some("ms-settings:appsfeatures"),
            Self::Network => Some("ms-settings:network"),
            Self::Security => Some("ms-settings:windowsdefender"),
            Self::DeviceManager => None,
        }
    }
}

#[tauri::command]
pub async fn get_system_overview_section(
    section: String,
) -> Result<SystemOverviewSectionResponse, String> {
    let section = OverviewSection::parse(&section)?;
    let data = tauri::async_runtime::spawn_blocking(move || collect_section(section))
        .await
        .map_err(|error| format!("系统概况采集任务失败：{error}"))??;
    Ok(SystemOverviewSectionResponse {
        section: section.as_str().to_string(),
        data,
    })
}

#[tauri::command]
pub async fn scan_driver_updates() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(collect_driver_update_scan)
        .await
        .map_err(|error| format!("驱动更新扫描任务失败：{error}"))?
}

#[tauri::command]
pub async fn set_printer_share(
    name: String,
    share_name: Option<String>,
    shared: bool,
) -> Result<Value, String> {
    validate_printer_name(&name)?;
    if shared {
        validate_share_name(share_name.as_deref().unwrap_or(""))?;
    }
    let share_name = share_name.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || change_printer_share(name, share_name, shared))
        .await
        .map_err(|error| format!("打印机共享任务失败：{error}"))?
}

fn validate_printer_name(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= 220
        && !value.chars().any(|character| {
            character.is_control() || matches!(character, '\\' | '/' | ';' | '\'' | '"' | '`' | '$')
        });
    if valid {
        Ok(())
    } else {
        Err("打印机名称无效：请使用本机打印机的完整原始名称".to_string())
    }
}

fn validate_share_name(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 80
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        });
    if valid {
        Ok(())
    } else {
        Err("共享名须为 1–80 个字母、数字、点、下划线或连字符".to_string())
    }
}

#[tauri::command]
pub fn open_system_settings(target: String) -> Result<(), String> {
    open_settings_target(SettingsTarget::parse(&target)?)
}

#[tauri::command]
pub fn export_system_overview_report(content: String) -> Result<String, String> {
    let directory = crate::config::data_dir().join("exports");
    export_to(&directory, &content)
        .map(|path| path.display().to_string())
        .map_err(|error| format!("无法导出系统概况报告：{error}"))
}

fn export_to(directory: &Path, content: &str) -> io::Result<PathBuf> {
    use std::io::Write as _;

    fs::create_dir_all(directory)?;
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    for suffix in 0..10_000_u32 {
        let filename = if suffix == 0 {
            format!("system-overview-{timestamp}.md")
        } else {
            format!("system-overview-{timestamp}-{suffix}.md")
        };
        let path = directory.join(filename);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(content.as_bytes())?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "无法生成不重复的报告文件名",
    ))
}

#[cfg(windows)]
fn collect_section(section: OverviewSection) -> Result<Value, String> {
    let section_name = section.as_str();
    let script = crate::system_overview_scripts::get(section_name)
        .ok_or_else(|| format!("未找到系统概况分区脚本：{section_name}"))?;
    run_powershell_json(section_name, script, section.timeout_seconds())
}

#[cfg(windows)]
fn collect_driver_update_scan() -> Result<Value, String> {
    run_powershell_json(
        "driverUpdateScan",
        crate::system_overview_scripts::DRIVER_UPDATE_SCAN,
        120,
    )
}

#[cfg(windows)]
fn change_printer_share(name: String, share_name: String, shared: bool) -> Result<Value, String> {
    let shared_value = if shared { "true" } else { "false" };
    run_powershell_json_with_environment(
        "printerShare",
        crate::system_overview_scripts::SET_PRINTER_SHARE,
        30,
        &[
            ("HARNESS_PRINTER_NAME", name.as_str()),
            ("HARNESS_SHARE_NAME", share_name.as_str()),
            ("HARNESS_PRINTER_SHARED", shared_value),
        ],
    )
}

#[cfg(windows)]
fn run_powershell_json(
    section_name: &str,
    script: &str,
    timeout_seconds: u64,
) -> Result<Value, String> {
    run_powershell_json_with_environment(section_name, script, timeout_seconds, &[])
}

#[cfg(windows)]
fn run_powershell_json_with_environment(
    section_name: &str,
    script: &str,
    timeout_seconds: u64,
    environment: &[(&str, &str)],
) -> Result<Value, String> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use std::io::Read as _;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let command = format!("{}\n{script}", crate::system_overview_scripts::PREAMBLE);
    let encoded = BASE64.encode(
        command
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    let mut process = Command::new("powershell.exe");
    process
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    for (key, value) in environment {
        process.env(key, value);
    }
    let mut child = process
        .spawn()
        .map_err(|error| format!("无法启动 PowerShell 采集 {section_name}：{error}"))?;
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });

    let started = Instant::now();
    let wait_result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() >= Duration::from_secs(timeout_seconds) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "系统概况分区 {section_name} 采集超过 {timeout_seconds} 秒，已终止 PowerShell"
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("等待 PowerShell 采集结束失败：{error}"));
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "读取 PowerShell 标准输出的线程异常结束".to_string())?
        .map_err(|error| format!("读取 PowerShell 标准输出失败：{error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "读取 PowerShell 错误输出的线程异常结束".to_string())?
        .map_err(|error| format!("读取 PowerShell 错误输出失败：{error}"))?;
    let status = wait_result?;
    let stderr = decode_windows_output(&stderr);
    if !status.success() {
        let code = status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "进程被终止".to_string());
        let detail = stderr.trim();
        return Err(format!(
            "PowerShell 采集 {section_name} 失败（{code}）：{}",
            if detail.is_empty() {
                "无错误输出"
            } else {
                detail
            }
        ));
    }
    let output = decode_windows_output(&stdout);
    let json = output.trim().trim_start_matches('\u{feff}').trim();
    if json.is_empty() {
        return Err(format!("PowerShell 采集 {section_name} 未返回 JSON"));
    }
    serde_json::from_str(json).map_err(|error| {
        let detail = stderr.trim();
        if detail.is_empty() {
            format!("系统概况分区 {section_name} 返回了无效 JSON：{error}")
        } else {
            format!("系统概况分区 {section_name} 返回了无效 JSON：{error}；{detail}")
        }
    })
}

#[cfg(not(windows))]
fn collect_section(_section: OverviewSection) -> Result<Value, String> {
    Err("系统概况采集仅支持 Windows".to_string())
}

#[cfg(not(windows))]
fn collect_driver_update_scan() -> Result<Value, String> {
    Err("驱动更新扫描仅支持 Windows".to_string())
}

#[cfg(not(windows))]
fn change_printer_share(
    _name: String,
    _share_name: String,
    _shared: bool,
) -> Result<Value, String> {
    Err("打印机共享仅支持 Windows".to_string())
}

#[cfg(windows)]
fn open_settings_target(target: SettingsTarget) -> Result<(), String> {
    if let Some(uri) = target.settings_uri() {
        return tauri_plugin_opener::open_url(uri, None::<&str>)
            .map_err(|error| format!("无法打开 Windows 设置：{error}"));
    }

    use std::os::windows::process::CommandExt;
    let console = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("devmgmt.msc");
    std::process::Command::new("mmc.exe")
        .arg(console)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开设备管理器：{error}"))
}

#[cfg(not(windows))]
fn open_settings_target(_target: SettingsTarget) -> Result<(), String> {
    Err("打开系统设置仅支持 Windows".to_string())
}

#[cfg(windows)]
fn decode_windows_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_whitelist_is_exact_and_complete() {
        for name in [
            "basic",
            "activation",
            "hardware",
            "printers",
            "drivers",
            "software",
            "storage",
            "network",
            "security",
        ] {
            let section = OverviewSection::parse(name).unwrap();
            assert_eq!(section.as_str(), name);
            assert!(crate::system_overview_scripts::get(name).is_some());
        }
        for invalid in ["", "Basic", " basic", "basic ", "all", "basic;exit"] {
            assert!(OverviewSection::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn settings_target_whitelist_is_exact() {
        for (name, uri) in [
            ("activation", Some("ms-settings:activation")),
            ("windowsUpdate", Some("ms-settings:windowsupdate")),
            ("deviceManager", None),
            ("installedApps", Some("ms-settings:appsfeatures")),
            ("network", Some("ms-settings:network")),
            ("security", Some("ms-settings:windowsdefender")),
        ] {
            assert_eq!(SettingsTarget::parse(name).unwrap().settings_uri(), uri);
        }
        for invalid in [
            "",
            "windowsupdate",
            "WindowsUpdate",
            "device-manager",
            "ms-settings:activation",
            "activation;calc",
        ] {
            assert!(SettingsTarget::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn report_export_is_utf8_and_never_overwrites() {
        let directory = tempfile::tempdir().unwrap();
        let first = export_to(directory.path(), "# 系统概况\n中文内容").unwrap();
        let second = export_to(directory.path(), "# 第二份报告").unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(first).unwrap(), "# 系统概况\n中文内容");
        assert_eq!(fs::read_to_string(second).unwrap(), "# 第二份报告");
    }

    #[test]
    fn response_uses_camel_case_contract() {
        let response = SystemOverviewSectionResponse {
            section: "basic".to_string(),
            data: serde_json::json!({"osName": "Windows"}),
        };
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["section"], "basic");
        assert_eq!(value["data"]["osName"], "Windows");
    }

    #[test]
    fn activation_script_does_not_select_product_keys() {
        let script = crate::system_overview_scripts::get("activation").unwrap();
        assert!(!script.contains("PartialProductKey"));
        assert!(!script.contains("OA3xOriginalProductKey"));
    }

    #[test]
    fn hardware_script_collects_memory_usage() {
        let script = crate::system_overview_scripts::get("hardware").unwrap();
        assert!(script.contains("Win32_OperatingSystem"));
        assert!(script.contains("FreePhysicalMemory"));
        assert!(script.contains("memoryUsedBytes = $memoryUsedBytes"));
    }

    #[test]
    fn network_script_uses_internet_profile_and_ipv4() {
        let script = crate::system_overview_scripts::get("network").unwrap();
        assert!(script.contains("IPv4Connectivity -eq 'Internet'"));
        assert!(!script.contains("IPv4Connectivity -in @('LocalNetwork', 'Internet')"));
        assert!(script.contains("-AddressFamily IPv4"));
    }

    #[test]
    fn printer_share_arguments_are_validated() {
        for name in [
            "HP LaserJet",
            "办公室打印机 (2F)",
            "Microsoft Print to PDF",
            "Printer [Office] *",
        ] {
            assert!(validate_printer_name(name).is_ok(), "{name}");
        }
        for name in ["", " HP", "HP ", "HP;calc", "HP$HOME", "HP\nCMD"] {
            assert!(validate_printer_name(name).is_err(), "{name:?}");
        }
        for name in ["OfficePrinter", "HP-LaserJet_2", "printer.local"] {
            assert!(validate_share_name(name).is_ok(), "{name}");
        }
        for name in ["", "two words", "printer;calc", "printer/name", "打印机"] {
            assert!(validate_share_name(name).is_err(), "{name}");
        }
        assert!(validate_share_name(&"a".repeat(81)).is_err());
    }

    #[test]
    fn printer_scripts_use_fixed_commands_and_environment_values() {
        let read_script = crate::system_overview_scripts::get("printers").unwrap();
        assert!(read_script.contains("Get-Printer"));
        assert!(read_script.contains("isLocal = $typeCode -eq 0"));
        assert!(!read_script.contains("Set-Printer"));
        let share_script = crate::system_overview_scripts::SET_PRINTER_SHARE;
        assert!(share_script.contains("$env:HARNESS_PRINTER_NAME"));
        assert!(share_script.contains("$env:HARNESS_SHARE_NAME"));
        assert!(share_script.contains("Set-Printer -InputObject $printer"));
        assert!(share_script.contains("Where-Object { [string]$_.Name -eq [string]$Name }"));
        assert!(!share_script.contains("Get-Printer -Name $printerName"));
        assert!(!share_script.contains("Set-Printer -Name $printerName"));
        assert!(!share_script.contains("Invoke-Expression"));
    }

    #[cfg(windows)]
    #[test]
    fn printers_script_returns_json_on_windows() {
        let data = collect_section(OverviewSection::Printers).unwrap();
        assert!(data["printers"].is_array());
        assert!(data["warnings"].is_array());
        for printer in data["printers"].as_array().unwrap() {
            assert!(printer["name"].is_string());
            assert!(printer["isLocal"].is_boolean());
            assert!(printer["shared"].is_boolean());
        }
    }

    #[test]
    fn driver_scan_searches_without_downloading_or_installing() {
        let script = crate::system_overview_scripts::DRIVER_UPDATE_SCAN;
        assert!(script.contains("Type='Driver'"));
        assert!(script.contains("IsInstalled=0"));
        for forbidden in [
            "CreateUpdateDownloader",
            "CreateUpdateInstaller",
            ".Download()",
            ".Install()",
            "AcceptEula",
        ] {
            assert!(!script.contains(forbidden), "扫描脚本不得包含 {forbidden}");
        }
    }
}
