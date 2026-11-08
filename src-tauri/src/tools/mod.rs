mod probes;
mod python;
mod shell;

use crate::risk::RiskAssessment;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use probes::{available_shells, execute_probe};
pub use python::{status as python_status, PythonRuntimeInfo, PythonRuntimeSource};
pub use shell::{execute_shell, truncate_for_model};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequest {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub risk: RiskAssessment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub content: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub cancelled: bool,
    pub truncated: bool,
    pub risk: RiskAssessment,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    pub call_id: String,
    pub stream: String,
    pub text: String,
}

pub type OutputCallback = Arc<dyn Fn(ToolOutput) + Send + Sync>;

pub fn tool_definitions() -> Vec<crate::llm::ToolDefinition> {
    let skills_available = crate::skills::list()
        .map(|skills| !skills.is_empty())
        .unwrap_or(false);
    tool_definitions_for(available_shells(), skills_available)
}

fn tool_definitions_for(
    shells: Vec<String>,
    skills_available: bool,
) -> Vec<crate::llm::ToolDefinition> {
    use crate::llm::{FunctionDefinition, ToolDefinition};
    use serde_json::json;

    let explanation = json!({
        "type": "string",
        "description": "用简明中文解释这一步要做什么，以及为什么需要执行"
    });
    let mut command_description = format!(
        "在本机通过可用 shell（{}）执行命令。命令必须是非交互式的。",
        shells.join("、")
    );
    if shells.iter().any(|shell| shell == "python") {
        command_description.push_str(
            " shell=python 默认执行 Python 源码；安装或查询 pip 包时，command 使用 python -m pip ... 形式。",
        );
    }
    let mut definitions = vec![
        (
            "run_command",
            command_description.as_str(),
            json!({
                "type": "object",
                "properties": {
                    "shell": {"type": "string", "enum": shells},
                    "command": {"type": "string"},
                    "explanation_zh": explanation,
                    "risk_self": {"type": "string", "enum": ["l0", "l1", "l2", "l3"]},
                    "timeout_sec": {"type": "integer", "minimum": 1, "maximum": 1800}
                },
                "required": ["shell", "command", "explanation_zh", "risk_self"]
            }),
        ),
        (
            "get_system_info",
            "读取操作系统、CPU、内存、磁盘、网卡和开机时间",
            json!({"type":"object","properties":{}}),
        ),
        (
            "list_processes",
            "列出按 CPU 或内存排序的前 N 个进程",
            json!({
                "type":"object","properties":{"sort_by":{"type":"string","enum":["cpu","memory"]},"limit":{"type":"integer","minimum":1,"maximum":100}}
            }),
        ),
        (
            "list_services",
            "按状态和名称筛选 Windows 服务",
            json!({
                "type":"object","properties":{"status":{"type":"string","enum":["running","stopped","all"]},"name":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":200}}
            }),
        ),
        (
            "query_event_log",
            "读取最近一段时间的 Windows 事件日志",
            json!({
                "type":"object","properties":{"log_name":{"type":"string"},"level":{"type":"string","enum":["critical","error","warning","information","all"]},"hours":{"type":"integer","minimum":1,"maximum":720},"source":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":200}},
                "required":["log_name"]
            }),
        ),
        (
            "read_file",
            "读取本机文本文件，可只读末尾 N 行",
            json!({
                "type":"object","properties":{"path":{"type":"string"},"tail_lines":{"type":"integer","minimum":1,"maximum":5000},"max_bytes":{"type":"integer","minimum":1,"maximum":1048576}},"required":["path"]
            }),
        ),
        (
            "list_directory",
            "列出一个目录中的文件和子目录",
            json!({
                "type":"object","properties":{"path":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":500}},"required":["path"]
            }),
        ),
        (
            "read_registry",
            "读取 Windows 注册表键或指定值",
            json!({
                "type":"object","properties":{"path":{"type":"string"},"value_name":{"type":"string"}},"required":["path"]
            }),
        ),
        (
            "write_file",
            "覆盖或追加写入文本文件。执行前必须明确说明影响。",
            json!({
                "type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"append":{"type":"boolean"},"explanation_zh":explanation},"required":["path","content","explanation_zh"]
            }),
        ),
    ];

    if skills_available {
        definitions.extend([
            (
                "load_skill",
                "按需加载指定维修 Skill 的完整说明和文件清单",
                json!({
                    "type":"object","properties":{"name":{"type":"string"}},"required":["name"]
                }),
            ),
            (
                "read_skill_file",
                "读取已加载 Skill 目录中的附带文本文件",
                json!({
                    "type":"object","properties":{"name":{"type":"string"},"path":{"type":"string"}},"required":["name","path"]
                }),
            ),
        ]);
    }

    definitions
        .into_iter()
        .map(|(name, description, parameters)| ToolDefinition {
            function: FunctionDefinition {
                name: name.to_string(),
                description: Some(description.to_string()),
                parameters,
            },
        })
        .collect()
}

pub async fn execute(
    request: ToolRequest,
    cancel: CancellationToken,
    on_output: OutputCallback,
) -> ToolResult {
    let started = std::time::Instant::now();
    let name = request.name.clone();
    let call_id = request.id.clone();
    let risk = request.risk.clone();

    let mut result = if name == "run_command" {
        shell::execute_shell(&request, cancel, on_output).await
    } else {
        probes::execute_probe(&request, cancel, on_output).await
    };

    result.call_id = call_id;
    result.name = name;
    result.risk = risk;
    result.duration_ms = started.elapsed().as_millis() as u64;
    result
}

impl ToolResult {
    pub(crate) fn error(request: &ToolRequest, message: impl Into<String>) -> Self {
        Self {
            call_id: request.id.clone(),
            name: request.name.clone(),
            content: message.into(),
            exit_code: None,
            duration_ms: 0,
            timed_out: false,
            cancelled: false,
            truncated: false,
            risk: request.risk.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_tools_are_exposed_only_when_skills_exist() {
        let without_skills = tool_definitions_for(vec!["cmd".to_string()], false);
        assert!(without_skills.iter().all(|tool| !matches!(
            tool.function.name.as_str(),
            "load_skill" | "read_skill_file"
        )));

        let with_skills = tool_definitions_for(vec!["cmd".to_string()], true);
        assert!(with_skills
            .iter()
            .any(|tool| tool.function.name == "load_skill"));
        assert!(with_skills
            .iter()
            .any(|tool| tool.function.name == "read_skill_file"));
    }

    #[test]
    fn command_tool_only_lists_detected_shells() {
        let definitions =
            tool_definitions_for(vec!["cmd".to_string(), "python".to_string()], false);
        let command = definitions
            .iter()
            .find(|tool| tool.function.name == "run_command")
            .unwrap();
        assert_eq!(
            command.function.parameters["properties"]["shell"]["enum"],
            serde_json::json!(["cmd", "python"])
        );
    }
}
