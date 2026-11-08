use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::platform::dpapi;

pub const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub version: u32,
    pub active_provider_id: String,
    pub providers: Vec<ProviderConfig>,
    pub approval: ApprovalConfig,
    pub execution: ExecutionConfig,
    pub appearance: AppearanceConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: ApiKeyConfig,
    pub api_mode: ApiMode,
    pub model: String,
    pub models: Vec<String>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ApiKeyConfig {
    pub encrypted: bool,
    pub value: String,
}

pub type ApiKey = ApiKeyConfig;

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            encrypted: true,
            value: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    #[default]
    ChatCompletions,
    Responses,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ApprovalConfig {
    pub mode: ApprovalMode,
    pub readonly_needs_approval: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    #[default]
    Manual,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ExecutionConfig {
    pub default_timeout_sec: u64,
    pub max_timeout_sec: u64,
    pub output_limit_kb: usize,
    pub max_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceConfig {
    pub theme: Theme,
    pub font_size: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct SecurityConfig {
    pub encrypt_api_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoadedConfig {
    pub config: Config,
    pub api_keys_requiring_reset: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigLocation {
    pub path: PathBuf,
    pub display_path: String,
    pub used_fallback: bool,
    pub writable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid configuration JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to encrypt API key for provider {provider_id}: {source}")]
    ApiKeyEncryption {
        provider_id: String,
        source: io::Error,
    },
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            active_provider_id: "deepseek".to_owned(),
            providers: vec![ProviderConfig::default()],
            approval: ApprovalConfig::default(),
            execution: ExecutionConfig::default(),
            appearance: AppearanceConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            id: "deepseek".to_owned(),
            name: "DeepSeek".to_owned(),
            base_url: "https://api.deepseek.com/v1".to_owned(),
            api_key: ApiKeyConfig {
                encrypted: true,
                value: String::new(),
            },
            api_mode: ApiMode::ChatCompletions,
            model: "deepseek-chat".to_owned(),
            models: vec!["deepseek-chat".to_owned(), "deepseek-reasoner".to_owned()],
            temperature: None,
        }
    }
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            mode: ApprovalMode::Manual,
            readonly_needs_approval: false,
        }
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            default_timeout_sec: 120,
            max_timeout_sec: 1800,
            output_limit_kb: 16,
            max_steps: 25,
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            font_size: 14,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            encrypt_api_key: true,
        }
    }
}

impl Config {
    pub fn active_provider(&self) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.id == self.active_provider_id)
            .or_else(|| self.providers.first())
    }

    pub fn active_provider_mut(&mut self) -> Option<&mut ProviderConfig> {
        let index = self
            .providers
            .iter()
            .position(|provider| provider.id == self.active_provider_id)
            .unwrap_or(0);
        self.providers.get_mut(index)
    }
}

pub fn decrypt_api_key(key: &ApiKeyConfig) -> io::Result<String> {
    if !key.encrypted || key.value.is_empty() {
        return Ok(key.value.clone());
    }
    dpapi::unprotect_string(&key.value).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("API key decryption failed; please enter it again: {error}"),
        )
    })
}

fn resolve_config_dir() -> (PathBuf, bool) {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            if directory_is_writable(directory) {
                return (directory.to_path_buf(), false);
            }
        }
    }
    (fallback_config_dir(), true)
}

fn fallback_config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Harness")
}

fn directory_is_writable(directory: &Path) -> bool {
    if !directory.is_dir() {
        return false;
    }
    let probe = directory.join(format!(
        ".harness-write-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

pub fn config_dir() -> PathBuf {
    resolve_config_dir().0
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

pub fn data_dir() -> PathBuf {
    config_dir().join("data")
}

pub fn sessions_dir() -> PathBuf {
    data_dir().join("sessions")
}

pub fn skills_dir() -> PathBuf {
    config_dir().join("skills")
}

pub fn config_location() -> ConfigLocation {
    let (directory, used_fallback) = resolve_config_dir();
    let writable = if directory.is_dir() {
        directory_is_writable(&directory)
    } else {
        directory
            .parent()
            .map(directory_is_writable)
            .unwrap_or(false)
    };
    let path = directory.join(CONFIG_FILE_NAME);
    let display_path = if used_fallback {
        format!("{}（程序目录不可写，已回退）", path.display())
    } else {
        path.display().to_string()
    };
    ConfigLocation {
        path,
        display_path,
        used_fallback,
        writable,
    }
}

pub fn load() -> Result<Config, ConfigError> {
    Ok(load_with_status()?.config)
}

pub fn load_with_status() -> Result<LoadedConfig, ConfigError> {
    load_from_with_status(config_path())
}

pub fn load_from(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
    Ok(load_from_with_status(path)?.config)
}

pub fn load_from_with_status(path: impl AsRef<Path>) -> Result<LoadedConfig, ConfigError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(LoadedConfig {
            config: Config::default(),
            api_keys_requiring_reset: Vec::new(),
        });
    }
    let bytes = fs::read(path)?;
    let mut config: Config = serde_json::from_slice(&bytes)?;
    let mut api_keys_requiring_reset = Vec::new();

    for provider in &mut config.providers {
        if !provider.api_key.encrypted || provider.api_key.value.is_empty() {
            continue;
        }
        match dpapi::unprotect_string(&provider.api_key.value) {
            Ok(value) => {
                provider.api_key.value = value;
                provider.api_key.encrypted = false;
            }
            Err(_) => {
                provider.api_key.value.clear();
                provider.api_key.encrypted = false;
                api_keys_requiring_reset.push(provider.id.clone());
            }
        }
    }

    Ok(LoadedConfig {
        config,
        api_keys_requiring_reset,
    })
}

pub fn save(config: &Config) -> Result<(), ConfigError> {
    save_to(config_path(), config)
}

pub fn save_to(path: impl AsRef<Path>, config: &Config) -> Result<(), ConfigError> {
    let path = path.as_ref();
    let parent = path.parent().ok_or_else(|| {
        ConfigError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "configuration path has no parent directory",
        ))
    })?;
    fs::create_dir_all(parent)?;

    let mut stored = config.clone();
    for provider in &mut stored.providers {
        if provider.api_key.value.is_empty() {
            provider.api_key.encrypted = stored.security.encrypt_api_key;
        } else if stored.security.encrypt_api_key && !provider.api_key.encrypted {
            provider.api_key.value =
                dpapi::protect_string(&provider.api_key.value).map_err(|source| {
                    ConfigError::ApiKeyEncryption {
                        provider_id: provider.id.clone(),
                        source,
                    }
                })?;
            provider.api_key.encrypted = true;
        }
    }

    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &stored)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    let temporary_path = temporary.into_temp_path();
    replace_file(&temporary_path, path)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both paths are NUL-terminated and remain alive for the call. The
    // temporary file is in the destination directory, so replacement stays atomic.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_application_contract() {
        let config = Config::default();
        assert_eq!(config.version, 1);
        assert_eq!(config.active_provider_id, "deepseek");
        let provider = config.active_provider().unwrap();
        assert_eq!(provider.base_url, "https://api.deepseek.com/v1");
        assert_eq!(provider.model, "deepseek-chat");
        assert_eq!(provider.models, ["deepseek-chat", "deepseek-reasoner"]);
        assert_eq!(provider.api_mode, ApiMode::ChatCompletions);
        assert_eq!(provider.temperature, None);
        assert!(provider.api_key.encrypted);
        assert_eq!(config.approval.mode, ApprovalMode::Manual);
        assert!(!config.approval.readonly_needs_approval);
        assert_eq!(config.execution.default_timeout_sec, 120);
        assert_eq!(config.execution.max_timeout_sec, 1800);
        assert_eq!(config.execution.output_limit_kb, 16);
        assert_eq!(config.execution.max_steps, 25);
        assert_eq!(config.appearance.theme, Theme::System);
        assert_eq!(config.appearance.font_size, 14);
        assert!(config.security.encrypt_api_key);
    }

    #[test]
    fn serde_uses_camel_case_and_round_trips() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"activeProviderId\""));
        assert!(json.contains("\"baseUrl\""));
        assert!(json.contains("\"chat_completions\""));
        assert!(json.contains("\"readonlyNeedsApproval\""));
        assert!(json.contains("\"outputLimitKb\""));
        assert!(json.contains("\"encryptApiKey\""));
        assert!(json.contains("\"temperature\":null"));
        assert!(!json.contains("active_provider_id"));
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
    }

    #[test]
    fn save_and_load_round_trip_with_atomic_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join(CONFIG_FILE_NAME);
        let mut config = Config::default();
        config.security.encrypt_api_key = false;
        config.providers[0].api_key.encrypted = false;
        config.providers[0].api_key.value = "plaintext-test-key".to_owned();

        save_to(&path, &config).unwrap();
        save_to(&path, &config).unwrap();
        let loaded = load_from_with_status(&path).unwrap();
        assert_eq!(loaded.config, config);
        assert!(loaded.api_keys_requiring_reset.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn api_key_is_encrypted_on_disk_and_decrypted_on_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILE_NAME);
        let mut config = Config::default();
        config.providers[0].api_key.encrypted = false;
        config.providers[0].api_key.value = "secret-test-key".to_owned();

        save_to(&path, &config).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("secret-test-key"));
        let stored: Config = serde_json::from_str(&raw).unwrap();
        assert!(stored.providers[0].api_key.encrypted);

        let loaded = load_from_with_status(&path).unwrap();
        assert_eq!(loaded.config.providers[0].api_key.value, "secret-test-key");
        assert!(!loaded.config.providers[0].api_key.encrypted);
        assert!(loaded.api_keys_requiring_reset.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn decryption_failure_requests_key_reset_without_crashing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILE_NAME);
        let mut config = Config::default();
        config.providers[0].api_key.value = "invalid base64!".to_owned();
        config.providers[0].api_key.encrypted = true;
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();

        let loaded = load_from_with_status(&path).unwrap();
        assert_eq!(loaded.api_keys_requiring_reset, ["deepseek"]);
        assert!(loaded.config.providers[0].api_key.value.is_empty());
        assert!(!loaded.config.providers[0].api_key.encrypted);
    }
}
