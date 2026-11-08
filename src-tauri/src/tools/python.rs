use super::probes::command_exists;
use std::collections::hash_map::DefaultHasher;
use std::ffi::OsString;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const EMBEDDED_ARCHIVE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/python-3.13.17-embed-amd64.zip"
));
const EMBEDDED_CACHE_NAME: &str = "python-3.13.17-embed-amd64";
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PythonLaunch {
    pub program: PathBuf,
    pub base_args: Vec<OsString>,
}

impl PythonLaunch {
    fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            base_args: Vec::new(),
        }
    }
}

pub(crate) fn launch() -> Result<PythonLaunch, String> {
    static LAUNCH: OnceLock<Result<PythonLaunch, String>> = OnceLock::new();
    LAUNCH.get_or_init(detect).clone()
}

fn detect() -> Result<PythonLaunch, String> {
    let mut errors = Vec::new();

    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            if let Some(program) = find_portable_python(directory) {
                return Ok(PythonLaunch::new(program));
            }

            let archive_path = directory.join("python.zip");
            if archive_path.is_file() {
                match fs::read(&archive_path)
                    .map_err(|error| format!("无法读取 {}：{error}", archive_path.display()))
                    .and_then(|archive| {
                        let cache_name = external_cache_name(&archive);
                        ensure_archive(&archive, &cache_name)
                    }) {
                    Ok(program) => return Ok(PythonLaunch::new(program)),
                    Err(error) => errors.push(error),
                }
            }
        }
    }

    match ensure_archive(EMBEDDED_ARCHIVE, EMBEDDED_CACHE_NAME) {
        Ok(program) => return Ok(PythonLaunch::new(program)),
        Err(error) => errors.push(format!("内嵌 Python 不可用：{error}")),
    }

    if let Some(launch) = find_system_python() {
        return Ok(launch);
    }

    if errors.is_empty() {
        Err("未检测到可用 Python。请安装 Python 并加入 PATH，或将便携 Python 放到应用同级 python 目录。".to_string())
    } else {
        Err(format!(
            "未检测到可用 Python（{}）。请修复便携运行时，或安装 Python 并加入 PATH。",
            errors.join("；")
        ))
    }
}

fn find_portable_python(directory: &Path) -> Option<PathBuf> {
    [
        directory.join("python").join("python.exe"),
        directory.join("python.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn external_cache_name(archive: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    archive.hash(&mut hasher);
    format!("python-external-{:016x}", hasher.finish())
}

fn ensure_archive(archive_bytes: &[u8], cache_name: &str) -> Result<PathBuf, String> {
    let directory = crate::config::data_dir().join("runtime").join(cache_name);
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

    match fs::rename(temporary.path(), &directory) {
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

fn find_system_python() -> Option<PythonLaunch> {
    find_system_python_with(command_exists)
}

fn find_system_python_with(mut exists: impl FnMut(&str) -> bool) -> Option<PythonLaunch> {
    if exists("python.exe") || exists("python") {
        return Some(PythonLaunch::new("python"));
    }
    if exists("py.exe") {
        return Some(PythonLaunch {
            program: PathBuf::from("py.exe"),
            base_args: vec![OsString::from("-3")],
        });
    }
    if exists("python3") {
        return Some(PythonLaunch::new("python3"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_directory_has_priority_over_direct_executable() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("python")).unwrap();
        fs::write(temporary.path().join("python.exe"), b"").unwrap();
        let expected = temporary.path().join("python").join("python.exe");
        fs::write(&expected, b"").unwrap();

        assert_eq!(find_portable_python(temporary.path()), Some(expected));
    }

    #[test]
    fn direct_portable_executable_is_supported() {
        let temporary = tempfile::tempdir().unwrap();
        let expected = temporary.path().join("python.exe");
        fs::write(&expected, b"").unwrap();

        assert_eq!(find_portable_python(temporary.path()), Some(expected));
    }

    #[test]
    fn system_launcher_falls_back_to_py() {
        let launch = find_system_python_with(|command| command == "py.exe").unwrap();
        assert_eq!(launch.program, PathBuf::from("py.exe"));
        assert_eq!(launch.base_args, vec![OsString::from("-3")]);
    }

    #[cfg(windows)]
    #[test]
    fn embedded_runtime_executes_python_code() {
        let python = ensure_archive(EMBEDDED_ARCHIVE, EMBEDDED_CACHE_NAME).unwrap();
        let output = std::process::Command::new(python)
            .args([
                "-c",
                "import sys; print(sys.version); print('内嵌运行时正常')",
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
        assert!(stdout.contains("内嵌运行时正常"), "{stdout}");
    }

    #[test]
    fn embedded_archive_contains_runtime_and_standard_library() {
        let mut archive = zip::ZipArchive::new(Cursor::new(EMBEDDED_ARCHIVE)).unwrap();
        let mut has_python = false;
        let mut has_standard_library = false;
        for index in 0..archive.len() {
            let entry = archive.by_index(index).unwrap();
            has_python |= entry.name() == "python.exe";
            has_standard_library |= entry.name().ends_with(".zip");
        }
        assert!(has_python);
        assert!(has_standard_library);
    }
}
