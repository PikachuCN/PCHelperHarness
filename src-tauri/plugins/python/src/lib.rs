use serde::{Deserialize, Serialize};
use std::ffi::{c_char, CStr, CString};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

const ARCHIVE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/python-3.13.17-embed-amd64.zip"
));
const RUNTIME_DIRECTORY: &str = "python-3.13.17-embed-amd64";
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
const MANIFEST_JSON: &str = concat!(
    r#"{"id":"python","name":"Python","version":"3.13.17","shell":"python","script":{"mode":"file","extension":".py"}}"#,
    "\0"
);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginContext {
    data_dir: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareResult {
    program: Option<PathBuf>,
    base_args: Vec<String>,
    error: Option<String>,
}

impl PrepareResult {
    fn success(program: PathBuf) -> Self {
        Self {
            program: Some(program),
            base_args: Vec::new(),
            error: None,
        }
    }

    fn error(error: String) -> Self {
        Self {
            program: None,
            base_args: Vec::new(),
            error: Some(error),
        }
    }
}

#[no_mangle]
pub extern "C" fn harness_plugin_api_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn harness_plugin_manifest_json() -> *const c_char {
    MANIFEST_JSON.as_ptr().cast()
}

/// # Safety
/// `context` 必须指向有效的 NUL 结尾字符串，并在调用期间保持有效。
#[no_mangle]
pub unsafe extern "C" fn harness_plugin_prepare_json(context: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| unsafe { prepare(context) })
        .map_err(|_| "Python 插件初始化发生内部错误".to_string())
        .and_then(|result| result);
    into_raw(result)
}

/// # Safety
/// `pointer` 必须来自本插件的 prepare 调用，且每个非空指针只能释放一次。
#[no_mangle]
pub unsafe extern "C" fn harness_plugin_free_string(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(unsafe { CString::from_raw(pointer) });
    }
}

unsafe fn prepare(context: *const c_char) -> Result<PathBuf, String> {
    if context.is_null() {
        return Err("主程序未提供插件上下文".to_string());
    }
    let context = unsafe { CStr::from_ptr(context) }
        .to_str()
        .map_err(|error| format!("插件上下文不是 UTF-8：{error}"))?;
    let context: PluginContext =
        serde_json::from_str(context).map_err(|error| format!("插件上下文 JSON 无效：{error}"))?;
    prepare_runtime(&context)
}

fn prepare_runtime(context: &PluginContext) -> Result<PathBuf, String> {
    ensure_archive(ARCHIVE, &context.data_dir.join(RUNTIME_DIRECTORY))
}

fn into_raw(result: Result<PathBuf, String>) -> *mut c_char {
    let result = match result {
        Ok(program) => PrepareResult::success(program),
        Err(error) => PrepareResult::error(error),
    };
    let json = serde_json::to_string(&result).unwrap_or_else(|error| {
        format!(
            "{{\"program\":null,\"baseArgs\":[],\"error\":\"JSON 序列化失败：{}\"}}",
            error
        )
    });
    match CString::new(json) {
        Ok(text) => text.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn ensure_archive(archive_bytes: &[u8], directory: &Path) -> Result<PathBuf, String> {
    let python = directory.join("python.exe");
    if python.is_file() {
        return Ok(python);
    }

    let parent = directory
        .parent()
        .ok_or_else(|| "无法确定 Python 运行时缓存目录".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("无法创建 {}：{error}", parent.display()))?;
    let temporary = tempfile::Builder::new()
        .prefix(".python-extract-")
        .tempdir_in(parent)
        .map_err(|error| format!("无法创建 Python 临时目录：{error}"))?;

    extract_archive(archive_bytes, temporary.path())?;
    if !temporary.path().join("python.exe").is_file() {
        return Err("Python 压缩包中缺少 python.exe".to_string());
    }

    match fs::rename(temporary.path(), directory) {
        Ok(()) => Ok(python),
        Err(_) if python.is_file() => Ok(python),
        Err(error) => Err(format!(
            "无法启用 Python 运行时 {}：{error}",
            directory.display()
        )),
    }
}

fn extract_archive(archive_bytes: &[u8], destination: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))
        .map_err(|error| format!("Python 压缩包格式无效：{error}"))?;
    if archive.len() > 4096 {
        return Err("Python 压缩包文件数量异常".to_string());
    }

    let mut total_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 Python 压缩包：{error}"))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "Python 压缩包包含不安全路径".to_string())?;
        let output_path = destination.join(relative);

        if entry.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(|error| format!("无法创建 {}：{error}", output_path.display()))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("无法创建 {}：{error}", parent.display()))?;
        }

        total_bytes = total_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "Python 压缩包大小异常".to_string())?;
        if total_bytes > MAX_EXTRACTED_BYTES {
            return Err("Python 压缩包解压后超过 512 MB 限制".to_string());
        }

        let mut output = fs::File::create(&output_path)
            .map_err(|error| format!("无法写入 {}：{error}", output_path.display()))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("无法解压 {}：{error}", output_path.display()))?;
        output
            .flush()
            .map_err(|error| format!("无法保存 {}：{error}", output_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_exposes_python_file_runner() {
        let manifest = CStr::from_bytes_until_nul(MANIFEST_JSON.as_bytes())
            .unwrap()
            .to_str()
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(manifest).unwrap();
        assert_eq!(value["shell"], "python");
        assert_eq!(value["script"]["mode"], "file");
        assert_eq!(value["script"]["extension"], ".py");
    }

    #[cfg(windows)]
    #[test]
    fn embedded_runtime_executes_python_code() {
        let temporary = tempfile::tempdir().unwrap();
        let context = PluginContext {
            data_dir: temporary.path().to_path_buf(),
        };
        let python = prepare_runtime(&context).unwrap();
        let output = std::process::Command::new(python)
            .args([
                "-c",
                "import sys; print(sys.version); print('DLL 内嵌运行时正常')",
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("3.13.17"), "{stdout}");
        assert!(stdout.contains("DLL 内嵌运行时正常"), "{stdout}");
    }
}
