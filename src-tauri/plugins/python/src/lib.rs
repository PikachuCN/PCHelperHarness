use serde::{Deserialize, Serialize};
use std::ffi::{c_char, CStr, CString};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ARCHIVE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/python-3.13.17-embed-amd64.zip"
));
// PyPI pip 26.2.1 wheel, 1,816,632 bytes.
// SHA-256: 71138adf1f4ca900cdb7d289c21b7494329f2332b6d85f0e1c42108c0384ed3e.
const PIP_ARCHIVE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/pip-26.2.1-py3-none-any.whl"
));
const PIP_WHEEL_NAME: &str = "pip-26.2.1-py3-none-any.whl";
const LEGACY_RUNTIME_DIRECTORY: &str = "python-3.13.17-embed-amd64";
const TEMP_PREFIX: &str = "harness-python-";
const OWNER_FILE: &str = "owner.pid";
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
const MANIFEST_JSON: &str = concat!(
    r#"{"id":"python","name":"Python","version":"3.13.17 + pip 26.2.1","shell":"python","script":{"mode":"file","extension":".py"}}"#,
    "\0"
);

static RUNTIME: Mutex<Option<PathBuf>> = Mutex::new(None);

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

#[no_mangle]
pub extern "C" fn harness_plugin_cleanup() {
    let _ = std::panic::catch_unwind(cleanup_runtime);
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
    let mut session = RUNTIME
        .lock()
        .map_err(|_| "Python 临时运行时状态不可用".to_string())?;
    if let Some(directory) = session.as_ref() {
        let python = directory.join("python.exe");
        if python.is_file() {
            return Ok(python);
        }
    }
    session.take();

    cleanup_stale_runtimes();
    let temporary = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .tempdir()
        .map_err(|error| format!("无法创建 Python 临时运行时：{error}"))?;
    fs::write(
        temporary.path().join(OWNER_FILE),
        std::process::id().to_string(),
    )
    .map_err(|error| format!("无法记录 Python 运行时进程归属：{error}"))?;

    extract_archive(ARCHIVE, temporary.path())?;
    if !temporary.path().join("python.exe").is_file() {
        return Err("Python 压缩包中缺少 python.exe".to_string());
    }
    configure_runtime(temporary.path())?;

    let directory = temporary.keep();
    let python = directory.join("python.exe");
    *session = Some(directory);
    drop(session);

    let legacy = context.data_dir.join(LEGACY_RUNTIME_DIRECTORY);
    if legacy.is_dir() {
        if let Err(error) = fs::remove_dir_all(&legacy) {
            eprintln!("旧版 Python 持久运行时清理失败 {}：{error}", legacy.display());
        }
    }
    Ok(python)
}

fn configure_runtime(directory: &Path) -> Result<(), String> {
    fs::write(directory.join(PIP_WHEEL_NAME), PIP_ARCHIVE)
        .map_err(|error| format!("无法释放 pip wheel：{error}"))?;
    fs::create_dir_all(directory.join("Lib").join("site-packages"))
        .map_err(|error| format!("无法创建 Python site-packages：{error}"))?;

    let path_config = fs::read_dir(directory)
        .map_err(|error| format!("无法读取 Python 临时目录：{error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "_pth")
        })
        .ok_or_else(|| "Python 运行时缺少 ._pth 路径配置".to_string())?;
    let mut content = fs::read_to_string(&path_config)
        .map_err(|error| format!("无法读取 Python 路径配置：{error}"))?
        .replace("#import site", "import site");
    ensure_line(&mut content, "import site");
    ensure_line(&mut content, "Lib\\site-packages");
    ensure_line(&mut content, PIP_WHEEL_NAME);
    fs::write(&path_config, content)
        .map_err(|error| format!("无法写入 Python 路径配置：{error}"))?;
    Ok(())
}

fn ensure_line(content: &mut String, line: &str) {
    if content.lines().any(|existing| existing.trim() == line) {
        return;
    }
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(line);
    content.push('\n');
}

fn cleanup_runtime() {
    let directory = match RUNTIME.lock() {
        Ok(mut runtime) => runtime.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    if let Some(directory) = directory {
        remove_runtime_directory(&directory);
    }
}

fn remove_runtime_directory(directory: &Path) {
    for attempt in 0..3 {
        match fs::remove_dir_all(directory) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) if attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(error) => {
                eprintln!(
                    "Python 临时运行时清理失败 {}：{error}",
                    directory.display()
                );
            }
        }
    }
}

fn cleanup_stale_runtimes() {
    cleanup_stale_runtimes_in(&std::env::temp_dir(), process_is_running);
}

fn cleanup_stale_runtimes_in(root: &Path, mut is_running: impl FnMut(u32) -> bool) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("无法检查旧 Python 临时运行时 {}：{error}", root.display());
            return;
        }
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|kind| kind.is_dir())
            || !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(TEMP_PREFIX))
        {
            continue;
        }
        let Ok(owner) = fs::read_to_string(path.join(OWNER_FILE)) else {
            continue;
        };
        let Ok(pid) = owner.trim().parse::<u32>() else {
            continue;
        };
        if !is_running(pid) {
            if let Err(error) = fs::remove_dir_all(&path) {
                eprintln!("旧 Python 临时运行时清理失败 {}：{error}", path.display());
            }
        }
    }
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    use windows::core::HRESULT;
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: the returned process handle is closed immediately. Access-denied and
    // other ambiguous errors are treated as running so another instance is never
    // accidentally removed.
    match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            true
        }
        Err(error) => error.code() != HRESULT::from_win32(ERROR_INVALID_PARAMETER.0),
    }
}

#[cfg(not(windows))]
fn process_is_running(_pid: u32) -> bool {
    true
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

    struct CleanupGuard;

    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            cleanup_runtime();
        }
    }

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
        assert!(value["version"].as_str().unwrap().contains("pip 26.2.1"));
    }

    #[test]
    fn configures_pip_wheel_and_site_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let path_config = temporary.path().join("python313._pth");
        fs::write(&path_config, "python313.zip\r\n.\r\n#import site\r\n").unwrap();

        configure_runtime(temporary.path()).unwrap();

        let configured = fs::read_to_string(path_config).unwrap();
        assert!(configured.lines().any(|line| line == "import site"));
        assert!(configured.lines().any(|line| line == "Lib\\site-packages"));
        assert!(configured.lines().any(|line| line == PIP_WHEEL_NAME));
        assert_eq!(
            fs::read(temporary.path().join(PIP_WHEEL_NAME)).unwrap(),
            PIP_ARCHIVE
        );
    }

    #[test]
    fn stale_cleanup_preserves_directories_owned_by_running_processes() {
        let temporary = tempfile::tempdir().unwrap();
        let active = temporary.path().join("harness-python-active");
        let stale = temporary.path().join("harness-python-stale");
        let unrelated = temporary.path().join("other-directory");
        for (directory, pid) in [(&active, 123), (&stale, 456)] {
            fs::create_dir(directory).unwrap();
            fs::write(directory.join(OWNER_FILE), pid.to_string()).unwrap();
        }
        fs::create_dir(&unrelated).unwrap();

        cleanup_stale_runtimes_in(temporary.path(), |pid| pid == 123);

        assert!(active.exists());
        assert!(!stale.exists());
        assert!(unrelated.exists());
    }

    #[cfg(windows)]
    #[test]
    fn embedded_runtime_executes_python_and_pip_then_cleans_up() {
        let temporary = tempfile::tempdir().unwrap();
        let context = PluginContext {
            data_dir: temporary.path().to_path_buf(),
        };
        let python = prepare_runtime(&context).unwrap();
        let _cleanup = CleanupGuard;
        let runtime = python.parent().unwrap().to_path_buf();
        assert!(runtime.starts_with(std::env::temp_dir()));

        let output = std::process::Command::new(&python)
            .args([
                "-c",
                "import sys; print(sys.version); print('DLL 内嵌运行时正常')",
            ])
            .env("PYTHONIOENCODING", "utf-8")
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

        let pip = std::process::Command::new(&python)
            .args(["-m", "pip", "--version"])
            .output()
            .unwrap();
        assert!(
            pip.status.success(),
            "{}",
            String::from_utf8_lossy(&pip.stderr)
        );
        assert!(String::from_utf8_lossy(&pip.stdout).contains("pip 26.2.1"));

        let installed = std::process::Command::new(&python)
            .args([
                "-m",
                "pip",
                "install",
                "--no-index",
                "--no-deps",
                "--ignore-installed",
                "--no-warn-script-location",
            ])
            .arg(runtime.join(PIP_WHEEL_NAME))
            .env("PIP_CACHE_DIR", runtime.join("pip-cache"))
            .env("PYTHONUSERBASE", runtime.join("user"))
            .output()
            .unwrap();
        assert!(
            installed.status.success(),
            "stdout={}\nstderr={}",
            String::from_utf8_lossy(&installed.stdout),
            String::from_utf8_lossy(&installed.stderr)
        );
        assert!(runtime.join("Lib").join("site-packages").join("pip").is_dir());

        cleanup_runtime();
        assert!(!runtime.exists());
    }
}
