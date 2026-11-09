pub mod agent;
pub mod config;
pub mod llm;
pub mod platform;
pub mod plugins;
pub mod risk;
pub mod skills;
mod system_overview;
mod system_overview_scripts;
pub mod tools;

use agent::approval::{ApprovalDecision, ApprovalGate, ApprovalResolution};
use agent::session::Session;
use agent::RunOutcome;
use config::{Config, ProviderConfig};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    config: RwLock<Config>,
    approvals: ApprovalGate,
    turns: Mutex<HashMap<String, CancellationToken>>,
    resume_session: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    pub is_admin: bool,
    pub os_version: String,
    pub computer_name: String,
    pub user_name: String,
    pub config_path: String,
    pub config_used_fallback: bool,
    pub webview2_version: String,
    pub resume_session_id: Option<String>,
    pub available_shells: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalog {
    pub plugins: Vec<plugins::PluginInfo>,
    pub python: tools::PythonRuntimeInfo,
    pub directory: String,
}

#[tauri::command]
fn create_session() -> Result<agent::session::SessionSummary, String> {
    let session = Session::new(agent::prompt::system_prompt());
    agent::session::save(&session).map_err(|error| error.to_string())?;
    Ok(session.summary())
}

#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Option<String>,
    content: String,
    model: Option<String>,
    approval_mode: Option<String>,
) -> Result<RunOutcome, String> {
    if content.trim().is_empty() {
        return Err("不能发送空消息".to_string());
    }
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = Session::new(agent::prompt::system_prompt());
            agent::session::save(&session).map_err(|error| error.to_string())?;
            session.id
        }
    };
    let token = CancellationToken::new();
    {
        let mut turns = state
            .turns
            .lock()
            .map_err(|_| "执行状态不可用".to_string())?;
        if turns.contains_key(&session_id) {
            return Err("当前对话仍在生成，请先停止再发送".to_string());
        }
        turns.insert(session_id.clone(), token.clone());
    }

    let config = state
        .config
        .read()
        .map_err(|_| "配置状态不可用".to_string())?
        .clone();
    let result = agent::run_turn(
        app,
        &state.approvals,
        agent::TurnRequest {
            config,
            session_id: Some(session_id.clone()),
            text: content,
            model,
            approval_mode,
            cancel: token,
        },
    )
    .await;
    let mut turns = state
        .turns
        .lock()
        .map_err(|_| "执行状态不可用".to_string())?;
    turns.remove(&session_id);
    result
}

#[tauri::command]
fn cancel_turn(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let token = state
        .turns
        .lock()
        .map_err(|_| "执行状态不可用".to_string())?
        .get(&session_id)
        .cloned();
    if let Some(token) = token {
        token.cancel();
    }
    state
        .approvals
        .reject_session(&session_id, "用户停止了本轮对话");
    Ok(())
}

#[tauri::command]
fn resolve_approval(
    app: AppHandle,
    state: State<'_, AppState>,
    approval_id: String,
    approved: bool,
    reason: Option<String>,
    confirmation_text: Option<String>,
) -> Result<ApprovalResolution, String> {
    state.approvals.resolve(
        &app,
        &approval_id,
        ApprovalDecision {
            allowed: approved,
            reason,
            confirmation_text,
        },
    )
}

#[tauri::command]
fn list_sessions() -> Result<Vec<agent::session::SessionSummary>, String> {
    agent::session::list().map_err(|error| error.to_string())
}

#[tauri::command]
fn load_session(session_id: String) -> Result<Vec<agent::session::ChatMessage>, String> {
    agent::session::load(&session_id)
        .map(|session| session.messages)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_message_prompt(
    session_id: String,
    message_id: String,
) -> Result<Vec<crate::llm::Message>, String> {
    agent::session::load(&session_id)
        .and_then(|session| agent::session::prompt_through_user(&session, &message_id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rename_session(
    session_id: String,
    title: String,
) -> Result<agent::session::SessionSummary, String> {
    agent::session::rename(&session_id, &title).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    if let Some(token) = state
        .turns
        .lock()
        .map_err(|_| "执行状态不可用".to_string())?
        .get(&session_id)
        .cloned()
    {
        token.cancel();
    }
    state.approvals.reject_session(&session_id, "会话已删除");
    agent::session::delete(&session_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn export_session(session_id: String) -> Result<String, String> {
    agent::session::export(&session_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    let mut config = state
        .config
        .read()
        .map_err(|_| "配置状态不可用".to_string())?
        .clone();
    for provider in &mut config.providers {
        provider.api_key.value.clear();
    }
    Ok(config)
}

#[tauri::command]
fn save_config(state: State<'_, AppState>, mut config: Config) -> Result<(), String> {
    validate_config(&config)?;
    let previous = state
        .config
        .read()
        .map_err(|_| "配置状态不可用".to_string())?
        .clone();
    for provider in &mut config.providers {
        if provider.api_key.value.is_empty() {
            if let Some(old) = previous.providers.iter().find(|old| old.id == provider.id) {
                provider.api_key = old.api_key.clone();
            }
        } else {
            provider.api_key.encrypted = false;
        }
    }
    config::save(&config).map_err(|error| format!("保存配置失败：{error}"))?;
    *state
        .config
        .write()
        .map_err(|_| "配置状态不可用".to_string())? = config;
    Ok(())
}

fn validate_config(config: &Config) -> Result<(), String> {
    if config.providers.is_empty() {
        return Err("至少需要保留一个接口配置".to_string());
    }
    let mut ids = std::collections::HashSet::new();
    for provider in &config.providers {
        if provider.id.trim().is_empty() || !ids.insert(provider.id.clone()) {
            return Err("接口 ID 不能为空或重复".to_string());
        }
        if provider.name.trim().is_empty() || provider.model.trim().is_empty() {
            return Err("接口名称和模型不能为空".to_string());
        }
        let url = url::Url::parse(&provider.base_url)
            .map_err(|_| format!("{} 的接口地址无效", provider.name))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("接口地址仅支持 HTTP 或 HTTPS".to_string());
        }
    }
    if !config
        .providers
        .iter()
        .any(|provider| provider.id == config.active_provider_id)
    {
        return Err("默认接口配置不存在".to_string());
    }
    if config.execution.default_timeout_sec == 0
        || config.execution.default_timeout_sec > config.execution.max_timeout_sec
        || config.execution.max_timeout_sec > 1800
    {
        return Err("超时时间应在 1 到 1800 秒之间，且默认值不能超过最大值".to_string());
    }
    if !(2..=1024).contains(&config.execution.output_limit_kb) {
        return Err("模型输出上限应在 2 到 1024 KB 之间".to_string());
    }
    if !(1..=100).contains(&config.execution.max_steps) {
        return Err("最大步数应在 1 到 100 之间".to_string());
    }
    if !(12..=20).contains(&config.appearance.font_size) {
        return Err("字号应在 12 到 20 px 之间".to_string());
    }
    Ok(())
}

fn provider_with_key(state: &AppState, mut provider: ProviderConfig) -> ProviderConfig {
    if provider.api_key.value.is_empty() {
        if let Ok(config) = state.config.read() {
            if let Some(saved) = config
                .providers
                .iter()
                .find(|saved| saved.id == provider.id)
            {
                provider.api_key = saved.api_key.clone();
            }
        }
    } else {
        provider.api_key.encrypted = false;
    }
    provider
}

#[tauri::command]
async fn fetch_models(
    state: State<'_, AppState>,
    provider: ProviderConfig,
) -> Result<Vec<String>, String> {
    let provider = provider_with_key(&state, provider);
    let key = config::decrypt_api_key(&provider.api_key).map_err(|error| error.to_string())?;
    llm::fetch_models(&provider, &key)
        .await
        .map(|models| models.into_iter().map(|model| model.id).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn test_provider(
    state: State<'_, AppState>,
    provider: ProviderConfig,
) -> Result<llm::ProviderTestResult, String> {
    let provider = provider_with_key(&state, provider);
    let key = config::decrypt_api_key(&provider.api_key).map_err(|error| error.to_string())?;
    llm::test_provider(&provider, &key)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_skills() -> Result<Vec<skills::SkillMeta>, String> {
    skills::list().map_err(|error| error.to_string())
}

#[tauri::command]
fn reload_skills() -> Result<Vec<skills::SkillMeta>, String> {
    skills::list().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_plugin_catalog() -> Result<PluginCatalog, String> {
    let directory = plugins::directory()?;
    Ok(PluginCatalog {
        plugins: plugins::list(),
        python: tools::python_status(),
        directory: directory.display().to_string(),
    })
}

#[tauri::command]
fn open_plugin_dir() -> Result<(), String> {
    let path = plugins::directory()?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_skills_dir() -> Result<(), String> {
    let path = skills::ensure_installed().map_err(|error| error.to_string())?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_config_dir() -> Result<(), String> {
    let path = config::config_dir();
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_env_info(state: State<'_, AppState>) -> Result<EnvInfo, String> {
    let snapshot = platform::snapshot();
    let location = config::config_location();
    Ok(EnvInfo {
        is_admin: snapshot.is_admin,
        os_version: snapshot.windows_version,
        computer_name: snapshot.machine_name,
        user_name: snapshot.user_name,
        config_path: location.path.display().to_string(),
        config_used_fallback: location.used_fallback,
        webview2_version: webview2_version(),
        resume_session_id: state.resume_session.clone(),
        available_shells: tools::available_shells(),
    })
}

#[tauri::command]
fn restart_as_admin(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<(), String> {
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = Session::new(agent::prompt::system_prompt());
            agent::session::save(&session).map_err(|error| error.to_string())?;
            session.id
        }
    };
    if let Some(token) = state
        .turns
        .lock()
        .map_err(|_| "执行状态不可用".to_string())?
        .get(&session_id)
        .cloned()
    {
        token.cancel();
    }
    platform::elevation::restart_as_admin(&session_id)
        .map_err(|error| format!("无法请求管理员权限：{error}"))?;
    app.exit(0);
    Ok(())
}

#[cfg(windows)]
fn webview2_version() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    const CLIENT: &str =
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    const CLIENT_WOW: &str =
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    for (root, path) in [
        (HKEY_CURRENT_USER, CLIENT),
        (HKEY_LOCAL_MACHINE, CLIENT),
        (HKEY_LOCAL_MACHINE, CLIENT_WOW),
    ] {
        if let Ok(value) = RegKey::predef(root)
            .open_subkey(path)
            .and_then(|key| key.get_value::<String, _>("pv"))
        {
            return value;
        }
    }
    "未检测到".to_string()
}

#[cfg(not(windows))]
fn webview2_version() -> String {
    "非 Windows 系统".to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let loaded = config::load_with_status().unwrap_or_else(|error| {
        eprintln!("配置读取失败：{error}");
        config::LoadedConfig {
            config: Config::default(),
            api_keys_requiring_reset: Vec::new(),
        }
    });
    if !config::config_path().exists() {
        let _ = config::save(&loaded.config);
    }
    let _ = skills::ensure_installed();
    let resume_session = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|args| (args[0] == "--resume").then(|| args[1].clone()));
    let location = config::config_location();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState {
            config: RwLock::new(loaded.config),
            approvals: ApprovalGate::default(),
            turns: Mutex::new(HashMap::new()),
            resume_session,
        })
        .setup(move |app| {
            if location.used_fallback {
                app.emit(
                    "config_path_fallback",
                    serde_json::json!({
                        "path": location.path.display().to_string(),
                        "message": "程序目录不可写，配置已回退到用户数据目录",
                    }),
                )?;
            }
            if !loaded.api_keys_requiring_reset.is_empty() {
                app.emit(
                    "config_warning",
                    serde_json::json!({
                        "message": "部分 API Key 无法在本机解密，请在设置中重新填写",
                        "providerIds": loaded.api_keys_requiring_reset,
                    }),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_session,
            send_message,
            cancel_turn,
            resolve_approval,
            list_sessions,
            load_session,
            get_message_prompt,
            rename_session,
            delete_session,
            export_session,
            get_config,
            save_config,
            fetch_models,
            test_provider,
            list_skills,
            reload_skills,
            get_plugin_catalog,
            open_plugin_dir,
            open_skills_dir,
            open_config_dir,
            get_env_info,
            restart_as_admin,
            system_overview::get_system_overview_section,
            system_overview::open_system_settings,
            system_overview::export_system_overview_report,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Harness");

    app.run(|_, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            plugins::cleanup();
        }
    });
}
