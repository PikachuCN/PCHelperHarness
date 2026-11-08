use super::probes::command_exists;
use crate::plugins::{PluginLaunch, PluginScript};
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PythonRuntimeSource {
    Plugin,
    Portable,
    System,
}

#[derive(Debug, Clone)]
struct PythonRuntime {
    launch: PluginLaunch,
    source: PythonRuntimeSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonRuntimeInfo {
    pub available: bool,
    pub source: Option<PythonRuntimeSource>,
    pub program: Option<String>,
    pub error: Option<String>,
}

pub(crate) fn launch() -> Result<PluginLaunch, String> {
    runtime().map(|runtime| runtime.launch)
}

pub fn status() -> PythonRuntimeInfo {
    match runtime() {
        Ok(runtime) => PythonRuntimeInfo {
            available: true,
            source: Some(runtime.source),
            program: Some(runtime.launch.program.display().to_string()),
            error: None,
        },
        Err(error) => PythonRuntimeInfo {
            available: false,
            source: None,
            program: None,
            error: Some(error),
        },
    }
}

fn runtime() -> Result<PythonRuntime, String> {
    static RUNTIME: OnceLock<Result<PythonRuntime, String>> = OnceLock::new();
    RUNTIME.get_or_init(detect).clone()
}

fn detect() -> Result<PythonRuntime, String> {
    let plugin = crate::plugins::launch("python");
    let portable = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .and_then(|directory| find_portable_python(&directory));
    let system = find_system_python();
    choose_runtime(plugin, portable, system)
}

fn choose_runtime(
    plugin: Option<PluginLaunch>,
    portable: Option<PathBuf>,
    system: Option<PluginLaunch>,
) -> Result<PythonRuntime, String> {
    if let Some(launch) = plugin {
        return Ok(PythonRuntime {
            launch,
            source: PythonRuntimeSource::Plugin,
        });
    }
    if let Some(program) = portable {
        return Ok(PythonRuntime {
            launch: python_launch(program, Vec::new()),
            source: PythonRuntimeSource::Portable,
        });
    }
    if let Some(launch) = system {
        return Ok(PythonRuntime {
            launch,
            source: PythonRuntimeSource::System,
        });
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
    fn built_in_plugin_has_priority_over_portable_and_system_python() {
        let plugin = python_launch("plugin-python", Vec::new());
        let system = python_launch("system-python", Vec::new());
        let selected = choose_runtime(
            Some(plugin),
            Some(PathBuf::from("portable-python")),
            Some(system),
        )
        .unwrap();

        assert_eq!(selected.source, PythonRuntimeSource::Plugin);
        assert_eq!(selected.launch.program, PathBuf::from("plugin-python"));
    }

    #[test]
    fn portable_python_is_used_before_system_python_without_plugin() {
        let selected = choose_runtime(
            None,
            Some(PathBuf::from("portable-python")),
            Some(python_launch("system-python", Vec::new())),
        )
        .unwrap();

        assert_eq!(selected.source, PythonRuntimeSource::Portable);
        assert_eq!(selected.launch.program, PathBuf::from("portable-python"));
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
