use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InteractionFieldKind {
    Text,
    Textarea,
    Number,
    Select,
    MultiSelect,
    Toggle,
    File,
    Files,
    Folder,
    SaveFile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionField {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: InteractionFieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<InteractionOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<FileFilter>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionRequest {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<InteractionField>,
}

#[derive(Debug)]
struct PendingInteraction {
    request: InteractionRequest,
    sender: Option<oneshot::Sender<InteractionResolution>>,
}

#[derive(Debug, Clone)]
pub struct InteractionResolution {
    pub values: Value,
    pub cancelled: bool,
    pub reason: Option<String>,
}

#[derive(Default)]
pub struct InteractionGate {
    pending: Mutex<HashMap<String, PendingInteraction>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestUserInputArguments {
    title: String,
    description: String,
    fields: Vec<InteractionField>,
}

impl InteractionRequest {
    pub fn from_arguments(
        arguments: &Value,
        session_id: &str,
        turn_id: &str,
        call_id: &str,
    ) -> Result<Self, String> {
        let arguments: RequestUserInputArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| format!("request_user_input 参数无效：{error}"))?;
        validate_text("标题", &arguments.title, 1, 120)?;
        validate_text("说明", &arguments.description, 1, 1000)?;
        validate_fields(&arguments.fields, 6)?;
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            call_id: call_id.to_string(),
            title: arguments.title.trim().to_string(),
            description: arguments.description.trim().to_string(),
            fields: arguments.fields,
        })
    }
}

impl InteractionGate {
    pub async fn wait(
        &self,
        app: &AppHandle,
        request: InteractionRequest,
        cancel: CancellationToken,
    ) -> InteractionResolution {
        let id = request.id.clone();
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("interaction mutex poisoned")
            .insert(
                id.clone(),
                PendingInteraction {
                    request: request.clone(),
                    sender: Some(sender),
                },
            );
        if let Err(error) = app.emit("interaction_request", request) {
            self.pending
                .lock()
                .expect("interaction mutex poisoned")
                .remove(&id);
            return InteractionResolution {
                values: Value::Null,
                cancelled: true,
                reason: Some(format!("无法显示用户输入卡片：{error}")),
            };
        }

        tokio::select! {
            resolution = receiver => {
                resolution.unwrap_or(InteractionResolution {
                    values: Value::Null,
                    cancelled: true,
                    reason: Some("用户输入通道已关闭".to_string()),
                })
            }
            _ = cancel.cancelled() => {
                let request = self.pending
                    .lock()
                    .expect("interaction mutex poisoned")
                    .remove(&id)
                    .map(|entry| entry.request);
                if let Some(request) = request {
                    emit_resolved(app, &request, true);
                }
                InteractionResolution {
                    values: Value::Null,
                    cancelled: true,
                    reason: Some("本轮对话已停止".to_string()),
                }
            }
        }
    }

    pub fn resolve(
        &self,
        app: &AppHandle,
        interaction_id: &str,
        values: Value,
        cancelled: bool,
    ) -> Result<(), String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "用户输入状态不可用".to_string())?;
        let entry = pending
            .get(interaction_id)
            .ok_or_else(|| "用户输入请求已处理或不存在".to_string())?;
        if !cancelled {
            validate_values(&entry.request.fields, &values)?;
        }
        let entry = pending
            .remove(interaction_id)
            .expect("pending interaction disappeared");
        let request = entry.request;
        if let Some(sender) = entry.sender {
            let _ = sender.send(InteractionResolution {
                values: if cancelled { Value::Null } else { values },
                cancelled,
                reason: cancelled.then(|| "用户取消了本次输入".to_string()),
            });
        }
        emit_resolved(app, &request, cancelled);
        Ok(())
    }

    pub fn cancel_session(&self, app: &AppHandle, session_id: &str, reason: &str) {
        let mut pending = self.pending.lock().expect("interaction mutex poisoned");
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, entry)| entry.request.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(entry) = pending.remove(&id) {
                if let Some(sender) = entry.sender {
                    let _ = sender.send(InteractionResolution {
                        values: Value::Null,
                        cancelled: true,
                        reason: Some(reason.to_string()),
                    });
                }
                emit_resolved(app, &entry.request, true);
            }
        }
    }
}

fn emit_resolved(app: &AppHandle, request: &InteractionRequest, cancelled: bool) {
    let _ = app.emit(
        "interaction_resolved",
        serde_json::json!({
            "interactionId": request.id,
            "sessionId": request.session_id,
            "callId": request.call_id,
            "cancelled": cancelled,
        }),
    );
}

pub(crate) fn field_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "key": {"type": "string", "description": "稳定的英文字段键，用于 values 对象"},
            "label": {"type": "string"},
            "description": {"type": "string"},
            "kind": {"type": "string", "enum": ["text", "textarea", "number", "select", "multiSelect", "toggle", "file", "files", "folder", "saveFile"]},
            "required": {"type": "boolean"},
            "placeholder": {"type": "string"},
            "defaultValue": {},
            "options": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "value": {"type": "string"},
                        "label": {"type": "string"},
                        "description": {"type": "string"}
                    },
                    "required": ["value", "label"]
                }
            },
            "filters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "extensions": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["name", "extensions"]
                }
            }
        },
        "required": ["key", "label", "kind"]
    })
}

pub fn validate_fields(fields: &[InteractionField], maximum: usize) -> Result<(), String> {
    if fields.is_empty() || fields.len() > maximum {
        return Err(format!("字段数量应在 1 到 {maximum} 个之间"));
    }
    let mut keys = HashSet::new();
    for field in fields {
        validate_text("字段 key", &field.key, 1, 40)?;
        if !field
            .key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
            || !field
                .key
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        {
            return Err(format!(
                "字段 key“{}”只能包含字母、数字、下划线和连字符，且须以字母或下划线开头",
                field.key
            ));
        }
        if !keys.insert(field.key.clone()) {
            return Err(format!("字段 key“{}”重复", field.key));
        }
        validate_text("字段标签", &field.label, 1, 80)?;
        if let Some(description) = &field.description {
            validate_text("字段描述", description, 0, 500)?;
        }
        if let Some(placeholder) = &field.placeholder {
            validate_text("占位提示", placeholder, 0, 200)?;
        }
        if matches!(
            field.kind,
            InteractionFieldKind::Select | InteractionFieldKind::MultiSelect
        ) {
            let options = field
                .options
                .as_ref()
                .filter(|options| !options.is_empty())
                .ok_or_else(|| format!("字段“{}”必须提供选项", field.label))?;
            if options.len() > 30 {
                return Err(format!("字段“{}”最多允许 30 个选项", field.label));
            }
            let mut values = HashSet::new();
            for option in options {
                validate_text("选项值", &option.value, 1, 200)?;
                validate_text("选项标签", &option.label, 1, 120)?;
                if let Some(description) = &option.description {
                    validate_text("选项描述", description, 0, 500)?;
                }
                if !values.insert(&option.value) {
                    return Err(format!("字段“{}”存在重复选项值", field.label));
                }
            }
        } else if field
            .options
            .as_ref()
            .is_some_and(|options| !options.is_empty())
        {
            return Err(format!("字段“{}”不是选择类型，不能提供选项", field.label));
        }
        if let Some(filters) = &field.filters {
            if !matches!(
                field.kind,
                InteractionFieldKind::File
                    | InteractionFieldKind::Files
                    | InteractionFieldKind::Folder
                    | InteractionFieldKind::SaveFile
            ) {
                return Err(format!(
                    "字段“{}”不是路径类型，不能提供文件过滤器",
                    field.label
                ));
            }
            if filters.len() > 12 {
                return Err(format!("字段“{}”最多允许 12 个文件过滤器", field.label));
            }
            for filter in filters {
                validate_text("过滤器名称", &filter.name, 1, 80)?;
                if filter.extensions.is_empty() || filter.extensions.len() > 32 {
                    return Err("每个文件过滤器应包含 1 到 32 个扩展名".to_string());
                }
                for extension in &filter.extensions {
                    validate_text("文件扩展名", extension, 1, 32)?;
                }
            }
        }
        if let Some(default) = &field.default_value {
            validate_field_value(field, default).map_err(|error| format!("默认值无效：{error}"))?;
        }
    }
    Ok(())
}

pub fn validate_values(fields: &[InteractionField], values: &Value) -> Result<(), String> {
    let values = values
        .as_object()
        .ok_or_else(|| "用户输入 values 必须是 JSON 对象".to_string())?;
    for field in fields {
        let value = values.get(&field.key);
        if field.required.unwrap_or(false)
            && value.is_none_or(|value| {
                value.is_null()
                    || value.as_str().is_some_and(|text| text.trim().is_empty())
                    || value.as_array().is_some_and(|items| items.is_empty())
            })
        {
            return Err(format!("必填字段“{}”尚未填写", field.label));
        }
        if let Some(value) = value.filter(|value| !value.is_null()) {
            validate_field_value(field, value)
                .map_err(|error| format!("字段“{}”：{error}", field.label))?;
        }
    }
    Ok(())
}

fn validate_field_value(field: &InteractionField, value: &Value) -> Result<(), String> {
    match field.kind {
        InteractionFieldKind::Text
        | InteractionFieldKind::Textarea
        | InteractionFieldKind::File
        | InteractionFieldKind::Folder
        | InteractionFieldKind::SaveFile => {
            if !value.is_string() {
                return Err("必须是字符串".to_string());
            }
        }
        InteractionFieldKind::Number => {
            if !value.is_number() {
                return Err("必须是数字".to_string());
            }
        }
        InteractionFieldKind::Toggle => {
            if !value.is_boolean() {
                return Err("必须是布尔值".to_string());
            }
        }
        InteractionFieldKind::Select => {
            let selected = value
                .as_str()
                .ok_or_else(|| "必须是字符串选项值".to_string())?;
            if !field
                .options
                .as_ref()
                .is_some_and(|options| options.iter().any(|option| option.value == selected))
            {
                return Err("不在允许的选项中".to_string());
            }
        }
        InteractionFieldKind::MultiSelect => {
            let selected = string_array(value)?;
            let options = field
                .options
                .as_ref()
                .ok_or_else(|| "缺少选项定义".to_string())?;
            if selected
                .iter()
                .any(|selected| !options.iter().any(|option| &option.value == selected))
            {
                return Err("包含不在允许范围内的选项".to_string());
            }
        }
        InteractionFieldKind::Files => {
            string_array(value)?;
        }
    }
    Ok(())
}

fn string_array(value: &Value) -> Result<Vec<&str>, String> {
    value
        .as_array()
        .ok_or_else(|| "必须是字符串数组".to_string())?
        .iter()
        .map(|item| {
            item.as_str()
                .ok_or_else(|| "数组中的每一项都必须是字符串".to_string())
        })
        .collect()
}

fn validate_text(name: &str, value: &str, minimum: usize, maximum: usize) -> Result<(), String> {
    let length = value.trim().chars().count();
    if length < minimum || length > maximum {
        return Err(format!("{name}长度应在 {minimum} 到 {maximum} 个字符之间"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: InteractionFieldKind) -> InteractionField {
        InteractionField {
            key: "choice".to_string(),
            label: "选择".to_string(),
            description: None,
            kind,
            required: Some(true),
            placeholder: None,
            default_value: None,
            options: Some(vec![InteractionOption {
                value: "a".to_string(),
                label: "A".to_string(),
                description: None,
            }]),
            filters: None,
        }
    }

    #[test]
    fn field_kind_uses_frontend_camel_case() {
        let value = serde_json::to_value(field(InteractionFieldKind::MultiSelect)).unwrap();
        assert_eq!(value["kind"], "multiSelect");
        let value = serde_json::to_value(field(InteractionFieldKind::SaveFile)).unwrap();
        assert_eq!(value["kind"], "saveFile");
    }

    #[test]
    fn validates_required_types_and_options() {
        let select = field(InteractionFieldKind::Select);
        assert!(validate_values(
            std::slice::from_ref(&select),
            &serde_json::json!({"choice":"a"})
        )
        .is_ok());
        assert!(validate_values(
            std::slice::from_ref(&select),
            &serde_json::json!({"choice":"b"})
        )
        .is_err());
        assert!(validate_values(&[select], &serde_json::json!({"choice":1})).is_err());

        let multiple = field(InteractionFieldKind::MultiSelect);
        assert!(validate_values(
            std::slice::from_ref(&multiple),
            &serde_json::json!({"choice":["a"]})
        )
        .is_ok());
        assert!(validate_values(&[multiple], &serde_json::json!({"choice":"a"})).is_err());
    }

    #[test]
    fn save_file_only_requires_a_string_not_an_existing_path() {
        let save = field(InteractionFieldKind::SaveFile);
        assert!(validate_values(
            &[save],
            &serde_json::json!({"choice":"D:/not-created-yet/result.txt"})
        )
        .is_ok());
    }

    #[test]
    fn cancelled_resolution_does_not_need_values() {
        let fields = [field(InteractionFieldKind::Text)];
        let values = serde_json::Map::new();
        assert!(validate_values(&fields, &Value::Object(values)).is_err());
    }
}
