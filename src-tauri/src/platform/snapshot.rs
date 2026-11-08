use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use sysinfo::{Disks, System};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    pub windows_version: String,
    pub machine_name: String,
    pub user_name: String,
    pub is_admin: bool,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub disks: Vec<DiskSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpuSnapshot {
    pub name: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl SystemSnapshot {
    pub fn capture() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        let cpu_name = system
            .cpus()
            .first()
            .map(|cpu| {
                let brand = cpu.brand().trim();
                if brand.is_empty() {
                    cpu.name()
                } else {
                    brand
                }
            })
            .unwrap_or("Unknown")
            .to_owned();

        let mut disks: Vec<_> = Disks::new_with_refreshed_list()
            .iter()
            .map(|disk| DiskSnapshot {
                name: disk.name().to_string_lossy().into_owned(),
                mount_point: disk.mount_point().to_string_lossy().into_owned(),
                file_system: disk.file_system().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
            })
            .collect();
        disks.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));

        Self {
            windows_version: windows_version(),
            machine_name: environment_value("COMPUTERNAME"),
            user_name: environment_value("USERNAME"),
            is_admin: super::elevation::is_admin(),
            cpu: CpuSnapshot {
                name: cpu_name,
                physical_cores: System::physical_core_count(),
                logical_cores: system.cpus().len(),
                usage_percent: system.global_cpu_usage(),
            },
            memory: MemorySnapshot {
                total_bytes: system.total_memory(),
                used_bytes: system.used_memory(),
                available_bytes: system.available_memory(),
            },
            disks,
        }
    }

    pub fn to_prompt_text(&self) -> String {
        let mut text = String::from("Windows system snapshot:\n");
        let _ = writeln!(text, "- Windows: {}", self.windows_version);
        let _ = writeln!(text, "- Machine: {}", self.machine_name);
        let _ = writeln!(text, "- User: {}", self.user_name);
        let _ = writeln!(text, "- Elevated administrator: {}", self.is_admin);
        let physical = self
            .cpu
            .physical_cores
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_owned());
        let _ = writeln!(
            text,
            "- CPU: {} ({} physical cores, {} logical cores, {:.1}% used)",
            self.cpu.name, physical, self.cpu.logical_cores, self.cpu.usage_percent
        );
        let _ = writeln!(
            text,
            "- Memory: {} used / {} total ({} available)",
            format_bytes(self.memory.used_bytes),
            format_bytes(self.memory.total_bytes),
            format_bytes(self.memory.available_bytes)
        );
        for disk in &self.disks {
            let _ = writeln!(
                text,
                "- Disk {} ({}, {}): {} free / {} total",
                disk.mount_point,
                disk.name,
                disk.file_system,
                format_bytes(disk.available_bytes),
                format_bytes(disk.total_bytes)
            );
        }
        text
    }
}

pub fn snapshot() -> SystemSnapshot {
    SystemSnapshot::capture()
}

pub fn prompt_text() -> String {
    snapshot().to_prompt_text()
}

fn environment_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "Unknown".to_owned())
}

#[cfg(windows)]
fn windows_version() -> String {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let fallback =
        || System::long_os_version().unwrap_or_else(|| "Unknown Windows version".to_owned());
    let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
    else {
        return fallback();
    };

    let product = key
        .get_value::<String, _>("ProductName")
        .unwrap_or_else(|_| "Windows".to_owned());
    let display_version = key
        .get_value::<String, _>("DisplayVersion")
        .or_else(|_| key.get_value::<String, _>("ReleaseId"))
        .unwrap_or_default();
    let build = key
        .get_value::<String, _>("CurrentBuildNumber")
        .or_else(|_| key.get_value::<String, _>("CurrentBuild"))
        .unwrap_or_default();
    let revision = key.get_value::<u32, _>("UBR").unwrap_or(0);

    let mut parts = vec![product];
    if !display_version.is_empty() {
        parts.push(display_version);
    }
    if !build.is_empty() {
        let full_build = if revision > 0 {
            format!("OS build {build}.{revision}")
        } else {
            format!("OS build {build}")
        };
        parts.push(full_build);
    }
    parts.join(", ")
}

#[cfg(not(windows))]
fn windows_version() -> String {
    System::long_os_version().unwrap_or_else(|| "Unknown operating system".to_owned())
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1} GiB", bytes as f64 / GIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_can_be_serialized_and_rendered() {
        let snapshot = SystemSnapshot::capture();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("windowsVersion"));
        let text = snapshot.to_prompt_text();
        assert!(text.contains("Windows system snapshot:"));
        assert!(text.contains(&snapshot.machine_name));
        assert!(text.contains("Memory:"));
    }
}
