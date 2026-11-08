use super::probes::command_exists;
use crate::plugins::{PluginLaunch, PluginScript};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) fn launch() -> Result<PluginLaunch, String> {
    static LAUNCH: OnceLock<Result<PluginLaunch, String>> = OnceLock::new();
    LAUNCH.get_or_init(detect).clone()
}

fn detect() -> Result<PluginLaunch, String> {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            if let Some(program) = find_portable_python(directory) {
                return Ok(python_launch(program, Vec::new()));
            }
        }
    }

    if let Some(launch) = crate::plugins::launch("python") {
        return Ok(launch);
    }
    if let Some(launch) = find_system_python() {
        return Ok(launch);
    }

    Err("未检测到可用 Python。请将 python.dll 插件放到应用同目录并重启，或安装 Python 并加入 PATH。".to_string())
}

fn python_launch(program: impl Into<PathBuf>, base_args: Vec<OsString>) -> PluginLaunch {
    PluginLaunch {
        program: program.into(),
        base_args,
        script: PluginScript::File {
            extension: ".py".to_string(),
        },
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

fn find_system_python() -> Option<PluginLaunch> {
    find_system_python_with(command_exists)
}

fn find_system_python_with(mut exists: impl FnMut(&str) -> bool) -> Option<PluginLaunch> {
    if exists("python.exe") || exists("python") {
        return Some(python_launch("python", Vec::new()));
    }
    if exists("py.exe") {
        return Some(python_launch("py.exe", vec![OsString::from("-3")]));
    }
    if exists("python3") {
        return Some(python_launch("python3", Vec::new()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_directory_has_priority_over_direct_executable() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("python")).unwrap();
        std::fs::write(temporary.path().join("python.exe"), b"").unwrap();
        let expected = temporary.path().join("python").join("python.exe");
        std::fs::write(&expected, b"").unwrap();

        assert_eq!(find_portable_python(temporary.path()), Some(expected));
    }

    #[test]
    fn direct_portable_executable_is_supported() {
        let temporary = tempfile::tempdir().unwrap();
        let expected = temporary.path().join("python.exe");
        std::fs::write(&expected, b"").unwrap();

        assert_eq!(find_portable_python(temporary.path()), Some(expected));
    }

    #[test]
    fn system_launcher_falls_back_to_py() {
        let launch = find_system_python_with(|command| command == "py.exe").unwrap();
        assert_eq!(launch.program, PathBuf::from("py.exe"));
        assert_eq!(launch.base_args, vec![OsString::from("-3")]);
        assert_eq!(
            launch.script,
            PluginScript::File {
                extension: ".py".to_string()
            }
        );
    }
}
