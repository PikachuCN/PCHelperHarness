use libloading::Library;
use serde::Deserialize;
use std::collections::HashSet;
use std::ffi::{c_char, CStr, CString, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const API_VERSION: u32 = 1;

type ApiVersionFn = unsafe extern "C" fn() -> u32;
type ManifestFn = unsafe extern "C" fn() -> *const c_char;
type PrepareFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type FreeStringFn = unsafe extern "C" fn(*mut c_char);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub(crate) enum PluginScript {
    File { extension: String },
    Argument,
}

#[derive(Clone, Debug)]
pub(crate) struct PluginLaunch {
    pub program: PathBuf,
    pub base_args: Vec<OsString>,
    pub script: PluginScript,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    id: String,
    name: String,
    version: String,
    shell: String,
    script: PluginScript,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrepareResult {
    program: Option<PathBuf>,
    #[serde(default)]
    base_args: Vec<String>,
    error: Option<String>,
}

struct LoadedPlugin {
    manifest: PluginManifest,
    launch: PluginLaunch,
    _library: Library,
}

pub(crate) fn shell_names() -> Vec<String> {
    let mut seen = HashSet::new();
    plugins()
        .iter()
        .filter_map(|plugin| {
            let shell = plugin.manifest.shell.clone();
            seen.insert(shell.clone()).then_some(shell)
        })
        .collect()
}

pub(crate) fn prompt_catalog() -> String {
    let descriptions = plugins()
        .iter()
        .map(|plugin| {
            format!(
                "{} {}（shell={}）",
                plugin.manifest.name, plugin.manifest.version, plugin.manifest.shell
            )
        })
        .collect::<Vec<_>>();
    if descriptions.is_empty() {
        "无".to_string()
    } else {
        descriptions.join("；")
    }
}

pub(crate) fn launch(shell: &str) -> Option<PluginLaunch> {
    plugins()
        .iter()
        .find(|plugin| plugin.manifest.shell == shell)
        .map(|plugin| plugin.launch.clone())
}

fn plugins() -> &'static [LoadedPlugin] {
    static PLUGINS: OnceLock<Vec<LoadedPlugin>> = OnceLock::new();
    PLUGINS.get_or_init(load_plugins)
}

fn load_plugins() -> Vec<LoadedPlugin> {
    let mut paths = match plugin_paths() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("插件目录读取失败：{error}");
            return Vec::new();
        }
    };
    paths.sort();
    paths
        .iter()
        .filter_map(|path| match load_plugin(path) {
            Ok(Some(plugin)) => {
                eprintln!("已加载运行时插件：{}", path.display());
                Some(plugin)
            }
            Ok(None) => None,
            Err(error) => {
                eprintln!("插件 {} 加载失败：{error}", path.display());
                None
            }
        })
        .collect()
}

fn plugin_paths() -> Result<Vec<PathBuf>, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable
        .parent()
        .ok_or_else(|| "无法确定应用目录".to_string())?;
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn load_plugin(path: &Path) -> Result<Option<LoadedPlugin>, String> {
    let library =
        unsafe { Library::new(path) }.map_err(|error| format!("无法加载 DLL：{error}"))?;
    let api_version = match unsafe { library.get::<ApiVersionFn>(b"harness_plugin_api_version\0") }
    {
        Ok(symbol) => *symbol,
        Err(_) => return Ok(None),
    };
    let actual_version = unsafe { api_version() };
    if actual_version != API_VERSION {
        return Err(format!(
            "插件 ABI 版本为 {actual_version}，主程序要求 {API_VERSION}"
        ));
    }

    let manifest_fn = unsafe { library.get::<ManifestFn>(b"harness_plugin_manifest_json\0") }
        .map_err(|error| format!("缺少 manifest 导出：{error}"))?;
    let prepare_fn = unsafe { library.get::<PrepareFn>(b"harness_plugin_prepare_json\0") }
        .map_err(|error| format!("缺少 prepare 导出：{error}"))?;
    let free_string_fn = unsafe { library.get::<FreeStringFn>(b"harness_plugin_free_string\0") }
        .map_err(|error| format!("缺少 free_string 导出：{error}"))?;

    let manifest_text = unsafe { read_static_json(manifest_fn()) }?;
    let manifest: PluginManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("manifest JSON 无效：{error}"))?;
    validate_manifest(&manifest)?;

    let context = serde_json::json!({
        "dataDir": crate::config::data_dir().join("plugins").join(&manifest.id),
    });
    let context = CString::new(context.to_string()).map_err(|error| error.to_string())?;
    let result_pointer = unsafe { prepare_fn(context.as_ptr()) };
    let result_text = unsafe { read_owned_json(result_pointer, *free_string_fn) }?;
    let result: PrepareResult = serde_json::from_str(&result_text)
        .map_err(|error| format!("prepare JSON 无效：{error}"))?;
    if let Some(error) = result.error.filter(|error| !error.trim().is_empty()) {
        return Err(format!("插件初始化失败：{error}"));
    }

    let program = result
        .program
        .ok_or_else(|| "插件未返回 program".to_string())?;
    if !program.is_absolute() || !program.is_file() {
        return Err(format!(
            "插件 program 必须是存在的绝对路径：{}",
            program.display()
        ));
    }

    Ok(Some(LoadedPlugin {
        launch: PluginLaunch {
            program,
            base_args: result.base_args.into_iter().map(OsString::from).collect(),
            script: manifest.script.clone(),
        },
        manifest,
        _library: library,
    }))
}

fn validate_manifest(manifest: &PluginManifest) -> Result<(), String> {
    if !valid_identifier(&manifest.id) || !valid_identifier(&manifest.shell) {
        return Err("插件 id 和 shell 只能包含小写字母、数字、_ 或 -".to_string());
    }
    if matches!(
        manifest.shell.as_str(),
        "cmd"
            | "command"
            | "bat"
            | "powershell"
            | "pwsh"
            | "ps"
            | "wsl"
            | "bash"
            | "sh"
            | "linux"
            | "py"
            | "python3"
    ) {
        return Err(format!(
            "插件不能覆盖内置 shell 或其别名：{}",
            manifest.shell
        ));
    }
    if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err("插件 name 和 version 不能为空".to_string());
    }
    if let PluginScript::File { extension } = &manifest.script {
        let valid = extension.len() >= 2
            && extension.len() <= 12
            && extension.starts_with('.')
            && extension[1..]
                .chars()
                .all(|character| character.is_ascii_alphanumeric());
        if !valid {
            return Err("文件脚本扩展名格式无效".to_string());
        }
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}

unsafe fn read_static_json(pointer: *const c_char) -> Result<String, String> {
    if pointer.is_null() {
        return Err("插件返回了空字符串指针".to_string());
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map(str::to_string)
        .map_err(|error| format!("插件字符串不是 UTF-8：{error}"))
}

unsafe fn read_owned_json(
    pointer: *mut c_char,
    free_string: FreeStringFn,
) -> Result<String, String> {
    if pointer.is_null() {
        return Err("插件 prepare 返回了空指针".to_string());
    }
    let result = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map(str::to_string)
        .map_err(|error| format!("插件 prepare 字符串不是 UTF-8：{error}"));
    unsafe { free_string(pointer) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_script_manifest() {
        let manifest: PluginManifest = serde_json::from_str(
            r#"{"id":"python","name":"Python","version":"3.13.17","shell":"python","script":{"mode":"file","extension":".py"}}"#,
        )
        .unwrap();
        assert_eq!(manifest.shell, "python");
        assert_eq!(
            manifest.script,
            PluginScript::File {
                extension: ".py".to_string()
            }
        );
    }

    #[test]
    fn plugins_cannot_override_builtin_shells() {
        let mut manifest: PluginManifest = serde_json::from_str(
            r#"{"id":"bad","name":"Bad","version":"1","shell":"cmd","script":{"mode":"argument"}}"#,
        )
        .unwrap();
        for shell in ["cmd", "pwsh", "bash", "py"] {
            manifest.shell = shell.to_string();
            assert!(validate_manifest(&manifest).is_err(), "shell={shell}");
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires the release python.dll plugin"]
    fn loads_compiled_python_plugin_and_executes_code() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("release")
            .join("python.dll");
        let loaded = load_plugin(&path)
            .unwrap()
            .expect("python.dll must be a plugin");
        assert_eq!(loaded.manifest.shell, "python");

        let test_directory = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        std::fs::copy(&path, test_directory.join("python.dll")).unwrap();
        let prompt = crate::agent::prompt::system_prompt();
        assert!(
            prompt.contains("Python 3.13.17（shell=python）"),
            "{prompt}"
        );
        assert!(crate::tools::available_shells()
            .iter()
            .any(|shell| shell == "python"));

        let output = std::process::Command::new(&loaded.launch.program)
            .args(&loaded.launch.base_args)
            .args(["-c", "print('DLL 插件运行正常')"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("DLL 插件运行正常"));
    }
}
