use crate::agent::interaction::{validate_fields, InteractionField};
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

const MAX_CUSTOM_TOOLS: usize = 50;
const MAX_TOOL_FIELDS: usize = 12;
static STORAGE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomTool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    #[serde(alias = "prompt")]
    pub prompt_template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    pub fields: Vec<InteractionField>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCustomToolInput {
    pub tool: SaveCustomToolDefinition,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SaveCustomToolDefinition {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub prompt_template: String,
    pub rules: Option<String>,
    #[serde(default)]
    pub fields: Vec<InteractionField>,
}

#[derive(Debug, Clone)]
pub struct SaveCustomToolOutcome {
    pub tool: CustomTool,
    pub tools: Vec<CustomTool>,
    pub updated: bool,
}

pub fn path() -> PathBuf {
    crate::config::config_dir().join("custom-tools.json")
}

pub fn list() -> Result<Vec<CustomTool>, String> {
    let _guard = STORAGE_LOCK
        .lock()
        .map_err(|_| "个性工具存储状态不可用".to_string())?;
    load_from(path())
}

pub fn save(input: SaveCustomToolInput) -> Result<SaveCustomToolOutcome, String> {
    let _guard = STORAGE_LOCK
        .lock()
        .map_err(|_| "个性工具存储状态不可用".to_string())?;
    save_to(path(), input)
}

pub fn delete(tool_id: &str) -> Result<Vec<CustomTool>, String> {
    let _guard = STORAGE_LOCK
        .lock()
        .map_err(|_| "个性工具存储状态不可用".to_string())?;
    delete_from(path(), tool_id)
}

fn load_from(path: impl AsRef<Path>) -> Result<Vec<CustomTool>, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(path).map_err(|error| format!("读取个性工具失败：{error}"))?;
    let tools: Vec<CustomTool> =
        serde_json::from_slice(&data).map_err(|error| format!("个性工具文件格式无效：{error}"))?;
    if tools.len() > MAX_CUSTOM_TOOLS {
        return Err(format!("个性工具最多保存 {MAX_CUSTOM_TOOLS} 个"));
    }
    Ok(tools)
}

fn save_to(
    path: impl AsRef<Path>,
    input: SaveCustomToolInput,
) -> Result<SaveCustomToolOutcome, String> {
    let mut input = input.tool;
    input.name = input.name.trim().to_string();
    input.description = input.description.trim().to_string();
    input.prompt_template = input.prompt_template.trim().to_string();
    input.rules = input
        .rules
        .as_deref()
        .map(str::trim)
        .filter(|rules| !rules.is_empty())
        .map(str::to_string);
    validate_length("工具名称", &input.name, 1, 60)?;
    validate_length("工具描述", &input.description, 1, 240)?;
    if let Some(category) = &input.category {
        validate_length("工具分类", category, 0, 40)?;
    }
    validate_length("提示词模板", &input.prompt_template, 1, 8000)?;
    if let Some(rules) = &input.rules {
        validate_length("固定规则", rules, 1, 8000)?;
    }
    if !input.fields.is_empty() {
        validate_fields(&input.fields, MAX_TOOL_FIELDS)?;
    }

    let path = path.as_ref();
    let mut tools = load_from(path)?;
    let now = Utc::now().timestamp_millis();
    let existing = tools
        .iter()
        .position(|tool| tool.name.eq_ignore_ascii_case(&input.name));
    let (tool, updated) = if let Some(index) = existing {
        let previous = &tools[index];
        let tool = CustomTool {
            id: previous.id.clone(),
            name: input.name,
            description: input.description,
            category: "我的工具".to_string(),
            prompt_template: input.prompt_template,
            rules: input.rules,
            fields: input.fields,
            created_at: previous.created_at,
            updated_at: now,
        };
        tools[index] = tool.clone();
        (tool, true)
    } else {
        if tools.len() >= MAX_CUSTOM_TOOLS {
            return Err(format!(
                "个性工具最多保存 {MAX_CUSTOM_TOOLS} 个，请先删除不再使用的工具"
            ));
        }
        let tool = CustomTool {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            description: input.description,
            category: "我的工具".to_string(),
            prompt_template: input.prompt_template,
            rules: input.rules,
            fields: input.fields,
            created_at: now,
            updated_at: now,
        };
        tools.push(tool.clone());
        (tool, false)
    };
    write_tools(path, &tools)?;
    Ok(SaveCustomToolOutcome {
        tool,
        tools,
        updated,
    })
}

fn delete_from(path: impl AsRef<Path>, tool_id: &str) -> Result<Vec<CustomTool>, String> {
    let path = path.as_ref();
    let mut tools = load_from(path)?;
    let original_length = tools.len();
    tools.retain(|tool| tool.id != tool_id);
    if tools.len() == original_length {
        return Err("个性工具不存在或已删除".to_string());
    }
    write_tools(path, &tools)?;
    Ok(tools)
}

fn write_tools(path: &Path, tools: &[CustomTool]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "个性工具保存路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建个性工具目录失败：{error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("创建个性工具临时文件失败：{error}"))?;
    serde_json::to_writer_pretty(&mut temporary, tools)
        .map_err(|error| format!("序列化个性工具失败：{error}"))?;
    temporary
        .write_all(b"\n")
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("写入个性工具失败：{error}"))?;
    let temporary_path = temporary.into_temp_path();
    crate::config::replace_file(&temporary_path, path)
        .map_err(|error| format!("替换个性工具文件失败：{error}"))?;
    Ok(())
}

fn validate_length(name: &str, value: &str, minimum: usize, maximum: usize) -> Result<(), String> {
    let length = value.chars().count();
    if length < minimum || length > maximum {
        return Err(format!("{name}长度应在 {minimum} 到 {maximum} 个字符之间"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str) -> SaveCustomToolInput {
        SaveCustomToolInput {
            tool: SaveCustomToolDefinition {
                name: name.to_string(),
                description: "按指定条件整理文件".to_string(),
                category: None,
                prompt_template:
                    "把 {{folder}} 整理到 {{outputFolder}}；先确认范围，再给出操作计划。"
                        .to_string(),
                rules: Some("实际写入仍需用户审批。".to_string()),
                fields: Vec::new(),
            },
        }
    }

    #[test]
    fn saving_same_name_preserves_identity_and_creation_time() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("custom-tools.json");
        let first = save_to(&path, input("整理工具")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let mut changed = input("整理工具");
        changed.tool.description = "新的说明".to_string();
        let second = save_to(&path, changed).unwrap();
        assert!(second.updated);
        assert_eq!(second.tools.len(), 1);
        assert_eq!(second.tool.id, first.tool.id);
        assert_eq!(second.tool.created_at, first.tool.created_at);
        assert!(second.tool.updated_at >= first.tool.updated_at);
        assert_eq!(second.tool.category, "我的工具");
        assert_eq!(load_from(path).unwrap(), second.tools);
    }

    #[test]
    fn deleting_persists_the_complete_remaining_list() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("custom-tools.json");
        let first = save_to(&path, input("工具一")).unwrap();
        let second = save_to(&path, input("工具二")).unwrap();
        let remaining = delete_from(&path, &first.tool.id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, second.tool.id);
        assert_eq!(load_from(path).unwrap(), remaining);
    }

    #[test]
    fn rejects_blank_or_oversized_definitions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("custom-tools.json");
        let mut invalid = input(" ");
        invalid.tool.prompt_template = "有效说明".to_string();
        assert!(save_to(&path, invalid).is_err());
        let mut invalid = input("有效名称");
        invalid.tool.prompt_template = "x".repeat(8001);
        assert!(save_to(&path, invalid).is_err());
    }

    #[test]
    fn enforces_the_fifty_tool_limit_unless_updating() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("custom-tools.json");
        for index in 0..MAX_CUSTOM_TOOLS {
            save_to(&path, input(&format!("工具 {index}"))).unwrap();
        }
        assert!(save_to(&path, input("第五十一个工具")).is_err());
        assert!(save_to(&path, input("工具 0")).is_ok());
        assert_eq!(load_from(path).unwrap().len(), MAX_CUSTOM_TOOLS);
    }

    #[test]
    fn custom_tool_serializes_camel_case() {
        let directory = tempfile::tempdir().unwrap();
        let saved = save_to(directory.path().join("tools.json"), input("测试")).unwrap();
        let value = serde_json::to_value(saved.tool).unwrap();
        assert!(value.get("createdAt").is_some());
        assert!(value.get("updatedAt").is_some());
        assert!(value.get("created_at").is_none());
        assert!(value["promptTemplate"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("{{folder}}")));
        assert_eq!(value["rules"], "实际写入仍需用户审批。");
        assert!(value.get("prompt").is_none());
    }

    #[test]
    fn executable_properties_are_not_accepted() {
        let result: Result<SaveCustomToolInput, _> = serde_json::from_value(serde_json::json!({
            "tool": {
                "name": "测试",
                "description": "测试定义",
                "prompt_template": "自然语言说明",
                "code": "Remove-Item C:\\"
            }
        }));
        assert!(result.is_err());
    }
}
