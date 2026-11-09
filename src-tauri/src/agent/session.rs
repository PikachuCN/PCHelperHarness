use crate::config;
use crate::llm::Message as LlmMessage;
use crate::risk::RiskAssessment;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("无法读写会话：{0}")]
    Io(#[from] std::io::Error),
    #[error("会话格式不正确：{0}")]
    Json(#[from] serde_json::Error),
    #[error("找不到会话：{0}")]
    NotFound(String),
    #[error("会话 ID 或名称不合法")]
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub model_messages: Vec<LlmMessage>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub system_prompt_snapshots: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub tools: Vec<StoredTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTool {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
    pub explanation_zh: String,
    pub risk: RiskAssessment,
    pub status: ToolStatus,
    pub output: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    Running,
    Success,
    Error,
    Rejected,
    Cancelled,
}

pub fn summarize_title(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_lowercase();
    let has_any = |keywords: &[&str]| keywords.iter().any(|keyword| lower.contains(keyword));
    let category = if has_any(&["蓝屏", "stop code", "终止代码"]) {
        Some("蓝屏故障排查")
    } else if has_any(&[
        "上不了网",
        "无法上网",
        "不能上网",
        "断网",
        "网络连接",
        "dns",
        "wi-fi",
        "wifi",
    ]) {
        Some("网络连接异常排查")
    } else if has_any(&["c 盘", "c盘", "磁盘空间", "空间不足", "磁盘满"]) {
        Some("磁盘空间不足排查")
    } else if has_any(&["开机慢", "启动慢", "开机速度", "启动项"]) {
        Some("开机与启动项排查")
    } else if has_any(&["打印机", "打印不了", "无法打印"]) {
        Some("打印机故障排查")
    } else if has_any(&["权限", "无权访问", "拒绝访问"]) {
        Some("系统权限问题排查")
    } else if has_any(&["没声音", "没有声音", "无声"]) {
        Some("声音异常排查")
    } else if has_any(&["电脑慢", "运行缓慢", "卡顿", "明显变慢", "性能"]) {
        Some("电脑运行缓慢排查")
    } else {
        None
    };
    if let Some(title) = category {
        return title.to_string();
    }

    let mut summary = normalized
        .split(|character| {
            matches!(
                character,
                '。' | '！' | '？' | '!' | '?' | '；' | ';' | '\n'
            )
        })
        .next()
        .unwrap_or(&normalized)
        .trim()
        .to_string();
    let prefixes = [
        "请帮我",
        "请帮忙",
        "麻烦你",
        "麻烦",
        "请你",
        "请",
        "你好",
        "我的电脑",
        "我的",
        "电脑",
    ];
    while let Some(prefix) = prefixes
        .iter()
        .find(|prefix| summary.starts_with(*prefix) && summary.len() > prefix.len())
    {
        summary = summary[prefix.len()..].trim_start().to_string();
    }
    if summary.is_empty() {
        return "新对话".to_string();
    }
    if summary.chars().count() > 24 {
        format!("{}…", summary.chars().take(24).collect::<String>())
    } else {
        summary
    }
}

impl Session {
    pub fn new(system_prompt: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: "新对话".to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            model_messages: vec![LlmMessage::system(system_prompt)],
            system_prompt_snapshots: BTreeMap::new(),
        }
    }

    pub fn refresh_system_prompt(&mut self, system_prompt: String) {
        if let Some(message) = self
            .model_messages
            .first_mut()
            .filter(|message| matches!(message, LlmMessage::System { .. }))
        {
            *message = LlmMessage::system(system_prompt);
        } else {
            self.model_messages
                .insert(0, LlmMessage::system(system_prompt));
        }
    }

    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.messages.len(),
        }
    }

    pub fn push_user(&mut self, text: &str) -> ChatMessage {
        let message = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: ChatRole::User,
            content: text.to_string(),
            reasoning: String::new(),
            created_at: Utc::now(),
            tools: Vec::new(),
        };
        if self.title == "新对话" {
            self.title = summarize_title(text);
        }
        if let Some(LlmMessage::System { content }) = self.model_messages.first() {
            self.system_prompt_snapshots
                .insert(message.id.clone(), content.clone());
        }
        self.model_messages.push(LlmMessage::user(text));
        self.messages.push(message.clone());
        message
    }

    pub fn new_assistant(&mut self) -> ChatMessage {
        let message = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: ChatRole::Assistant,
            content: String::new(),
            reasoning: String::new(),
            created_at: Utc::now(),
            tools: Vec::new(),
        };
        self.messages.push(message.clone());
        message
    }

    pub fn assistant_mut(&mut self, id: &str) -> Option<&mut ChatMessage> {
        self.messages.iter_mut().find(|message| message.id == id)
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

fn validate_id(id: &str) -> Result<(), SessionError> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| SessionError::Invalid)
}

fn session_path(root: &Path, id: &str) -> Result<PathBuf, SessionError> {
    validate_id(id)?;
    Ok(root.join(format!("{id}.json")))
}

pub fn save(session: &Session) -> Result<(), SessionError> {
    save_to(&config::sessions_dir(), session)
}

pub fn save_to(root: &Path, session: &Session) -> Result<(), SessionError> {
    fs::create_dir_all(root)?;
    let path = session_path(root, &session.id)?;
    let temporary = root.join(format!(".{}.tmp", session.id));
    let content = serde_json::to_vec_pretty(session)?;
    fs::write(&temporary, content)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn load(id: &str) -> Result<Session, SessionError> {
    load_from(&config::sessions_dir(), id)
}

pub fn load_from(root: &Path, id: &str) -> Result<Session, SessionError> {
    let path = session_path(root, id)?;
    if !path.is_file() {
        return Err(SessionError::NotFound(id.to_string()));
    }
    let mut session: Session = serde_json::from_slice(&fs::read(path)?)?;
    if session.model_messages.is_empty() {
        session
            .model_messages
            .push(LlmMessage::system(crate::agent::prompt::system_prompt()));
    }
    Ok(session)
}

pub fn prompt_through_user(
    session: &Session,
    message_id: &str,
) -> Result<Vec<LlmMessage>, SessionError> {
    let mut user_messages = session
        .messages
        .iter()
        .filter(|message| matches!(message.role, ChatRole::User));
    let target_index = user_messages
        .position(|message| message.id == message_id)
        .ok_or_else(|| SessionError::NotFound(format!("用户消息 {message_id}")))?;

    let mut remaining = target_index;
    for (index, message) in session.model_messages.iter().enumerate() {
        if matches!(message, LlmMessage::User { .. }) {
            if remaining == 0 {
                let mut prompt = session.model_messages[..=index].to_vec();
                if let (Some(snapshot), Some(LlmMessage::System { content })) = (
                    session.system_prompt_snapshots.get(message_id),
                    prompt.first_mut(),
                ) {
                    *content = snapshot.clone();
                }
                return Ok(prompt);
            }
            remaining -= 1;
        }
    }
    Err(SessionError::NotFound(format!(
        "用户消息 {message_id} 的模型请求上下文"
    )))
}

pub fn list() -> Result<Vec<SessionSummary>, SessionError> {
    let root = config::sessions_dir();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Ok(session) = serde_json::from_slice::<Session>(&fs::read(path)?) {
            sessions.push(session.summary());
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    Ok(sessions)
}

pub fn rename(id: &str, title: &str) -> Result<SessionSummary, SessionError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 80 {
        return Err(SessionError::Invalid);
    }
    let mut session = load(id)?;
    session.title = title.to_string();
    session.touch();
    save(&session)?;
    Ok(session.summary())
}

pub fn delete(id: &str) -> Result<(), SessionError> {
    let path = session_path(&config::sessions_dir(), id)?;
    if !path.is_file() {
        return Err(SessionError::NotFound(id.to_string()));
    }
    fs::remove_file(path)?;
    Ok(())
}

pub fn export(id: &str) -> Result<String, SessionError> {
    let session = load(id)?;
    let directory = config::data_dir().join("exports");
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!(
        "{}-{}.md",
        Local::now().format("%Y%m%d-%H%M%S"),
        session.id
    ));
    fs::write(&path, markdown_report(&session))?;
    Ok(path.display().to_string())
}

fn markdown_report(session: &Session) -> String {
    let mut report = format!(
        "# 灵犀电脑管家诊断报告\n\n- 会话：{}\n- 创建时间：{}\n- 导出时间：{}\n\n",
        session.title,
        session
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S"),
        Local::now().format("%Y-%m-%d %H:%M:%S"),
    );
    for message in &session.messages {
        match message.role {
            ChatRole::User => {
                report.push_str(&format!("## 问题\n\n{}\n\n", message.content));
            }
            ChatRole::Assistant => {
                if !message.content.trim().is_empty() {
                    report.push_str(&format!("## 分析与结论\n\n{}\n\n", message.content));
                }
                for tool in &message.tools {
                    report.push_str(&format!(
                        "### 工具：{}（{:?}）\n\n{}\n\n```json\n{}\n```\n\n",
                        tool.name,
                        tool.status,
                        tool.explanation_zh,
                        serde_json::to_string_pretty(&tool.arguments).unwrap_or_default(),
                    ));
                    if !tool.output.trim().is_empty() {
                        let (output, _) = crate::tools::truncate_for_model(&tool.output, 16);
                        report.push_str(&format!("```text\n{}\n```\n\n", output));
                    }
                }
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trip_and_summary() {
        let root = tempfile::tempdir().unwrap();
        let mut session = Session::new("系统提示".to_string());
        session.push_user("我的电脑上不了网，请检查");
        let assistant = session.new_assistant();
        session.assistant_mut(&assistant.id).unwrap().content = "先检查网卡".to_string();
        save_to(root.path(), &session).unwrap();
        let loaded = load_from(root.path(), &session.id).unwrap();
        assert_eq!(loaded.title, "网络连接异常排查");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.summary().message_count, 2);
        assert!(markdown_report(&loaded).contains("先检查网卡"));
    }

    #[test]
    fn summarizes_first_message_without_truncating_problem_context() {
        assert_eq!(
            summarize_title("我的电脑上不了网，请检查"),
            "网络连接异常排查"
        );
        assert_eq!(
            summarize_title("我的 C 盘满了，请分析并安全清理"),
            "磁盘空间不足排查"
        );
        assert_eq!(
            summarize_title("请帮我排查打印机无法工作的问题"),
            "打印机故障排查"
        );
        assert_eq!(summarize_title("你好"), "你好");
        let long_title = summarize_title("请帮我检查一个不属于预分类但内容特别长的自定义故障问题");
        assert!(long_title.chars().count() <= 25);
    }

    #[test]
    fn refreshes_system_prompt_without_losing_conversation() {
        let mut session = Session::new("旧提示词".to_string());
        let first = session.push_user("第一个问题");
        session.refresh_system_prompt("包含 Python 插件的新提示词".to_string());
        let second = session.push_user("第二个问题");

        assert_eq!(session.model_messages.len(), 3);
        assert!(matches!(
            &session.model_messages[0],
            LlmMessage::System { content } if content == "包含 Python 插件的新提示词"
        ));
        let first_prompt = prompt_through_user(&session, &first.id).unwrap();
        assert!(matches!(
            &first_prompt[0],
            LlmMessage::System { content } if content == "旧提示词"
        ));
        let second_prompt = prompt_through_user(&session, &second.id).unwrap();
        assert!(matches!(
            &second_prompt[0],
            LlmMessage::System { content } if content == "包含 Python 插件的新提示词"
        ));
    }

    #[test]
    fn restores_the_exact_request_context_for_each_user_message() {
        let mut session = Session::new("系统提示".to_string());
        let first = session.push_user("第一个问题");
        session
            .model_messages
            .push(LlmMessage::assistant("第一条回复"));
        let second = session.push_user("第二个问题");

        let first_prompt = prompt_through_user(&session, &first.id).unwrap();
        assert_eq!(first_prompt.len(), 2);
        assert!(
            matches!(&first_prompt[0], LlmMessage::System { content } if content == "系统提示")
        );
        assert!(
            matches!(&first_prompt[1], LlmMessage::User { content } if content == "第一个问题")
        );

        let second_prompt = prompt_through_user(&session, &second.id).unwrap();
        assert_eq!(second_prompt.len(), 4);
        assert!(
            matches!(&second_prompt[2], LlmMessage::Assistant { text, .. } if text == "第一条回复")
        );
        assert!(
            matches!(&second_prompt[3], LlmMessage::User { content } if content == "第二个问题")
        );
        assert!(prompt_through_user(&session, "missing").is_err());
    }

    #[test]
    fn rejects_non_uuid_session_ids() {
        assert!(session_path(Path::new("sessions"), "../config").is_err());
    }
}
