use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_REPORT_BYTES: usize = 10 * 1024 * 1024;

pub fn export_report(content: &str, format: &str) -> Result<String, String> {
    export_to(crate::config::data_dir().join("exports"), content, format)
}

fn export_to(directory: PathBuf, content: &str, format: &str) -> Result<String, String> {
    let extension = match format {
        "markdown" => "md",
        "json" => "json",
        _ => return Err("format 仅支持 markdown 或 json".to_string()),
    };
    if content.trim().is_empty() {
        return Err("报告内容不能为空".to_string());
    }
    if content.len() > MAX_REPORT_BYTES {
        return Err("报告内容不能超过 10 MB".to_string());
    }
    if format == "json" {
        serde_json::from_str::<serde_json::Value>(content)
            .map_err(|error| format!("JSON 报告内容无效：{error}"))?;
    }
    std::fs::create_dir_all(&directory).map_err(|error| format!("无法创建报告目录：{error}"))?;
    let filename = format!(
        "network-report-{}.{}",
        Utc::now().format("%Y%m%d-%H%M%S-%3f"),
        extension
    );
    let path = directory.join(filename);
    write_new(&path, content)?;
    Ok(path.to_string_lossy().into_owned())
}

fn write_new(path: &Path, content: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "报告文件已存在，未覆盖原文件".to_string()
            } else {
                format!("无法创建报告文件：{error}")
            }
        })?;
    if let Err(error) = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(format!("无法写入报告文件：{error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_validates_format_and_json() {
        let directory = tempfile::tempdir().unwrap();
        assert!(export_to(directory.path().to_path_buf(), "{}", "html").is_err());
        assert!(export_to(directory.path().to_path_buf(), "not json", "json").is_err());
        let path = export_to(directory.path().to_path_buf(), r#"{"ok":true}"#, "json").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"ok\":true"));
    }

    #[test]
    fn create_new_never_overwrites_existing_report() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("network-report-test.md");
        std::fs::write(&path, "original").unwrap();
        assert!(write_new(&path, "replacement").is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    }
}
