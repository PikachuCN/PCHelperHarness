mod explain;
mod rules;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// 命令风险等级，顺序即风险从低到高。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    L0,
    L1,
    L2,
    L3,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
        };
        f.write_str(label)
    }
}

/// 完全在后端生成的风险评估结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
    pub summary_zh: String,
    pub requires_typed_confirmation: bool,
    pub matched_rules: Vec<String>,
}

/// 对命令做静态风险评估；本函数不会启动进程或执行任何命令。
pub fn classify(shell: &str, command: &str, self_report: Option<RiskLevel>) -> RiskAssessment {
    let findings = rules::classify_command(shell, command);
    let rule_level = findings
        .iter()
        .map(|finding| finding.level)
        .max()
        .unwrap_or(RiskLevel::L1);
    let level = self_report.map_or(rule_level, |reported| reported.max(rule_level));

    let mut selected: Vec<rules::RuleMatch> = findings
        .into_iter()
        .filter(|finding| finding.level == level)
        .collect();

    if let Some(reported) = self_report {
        if reported > rule_level {
            selected.push(rules::RuleMatch::new(
                reported,
                format!("self_report.{}", reported.as_str()),
                explain::self_report_reason(reported),
            ));
        }
    }

    // 同一复合命令中重复的片段只报告一次，保持输出稳定且便于前端展示。
    let mut seen = HashSet::new();
    selected.retain(|finding| seen.insert((finding.rule_id.clone(), finding.reason.clone())));

    let requires_typed_confirmation = selected
        .iter()
        .any(|finding| finding.requires_typed_confirmation);
    let summary_zh = explain::summary_zh(level, requires_typed_confirmation, &selected);
    let reasons = selected
        .iter()
        .map(|finding| finding.reason.clone())
        .collect();
    let matched_rules = selected
        .into_iter()
        .map(|finding| finding.rule_id)
        .collect();

    RiskAssessment {
        level,
        reasons,
        summary_zh,
        requires_typed_confirmation,
        matched_rules,
    }
}

pub fn classify_write_file(path: &str) -> RiskAssessment {
    let normalized = path
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase();
    let protected = normalized.starts_with("c:\\windows\\")
        || normalized == "c:\\windows"
        || normalized.starts_with("c:\\program files\\")
        || normalized.starts_with("c:\\program files (x86)\\")
        || normalized == "c:\\"
        || (normalized.starts_with("c:\\") && !normalized[3..].contains('\\'));
    let level = if protected {
        RiskLevel::L3
    } else {
        RiskLevel::L1
    };
    let reason = if protected {
        format!("将向受保护的系统位置写入文件：{path}")
    } else {
        format!("将覆盖或追加写入文件：{path}")
    };
    RiskAssessment {
        level,
        reasons: vec![reason.clone()],
        summary_zh: reason,
        requires_typed_confirmation: false,
        matched_rules: vec![if protected {
            "write_file.protected_path"
        } else {
            "write_file.modify"
        }
        .to_string()],
    }
}

impl RiskLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::L0 => "l0",
            Self::L1 => "l1",
            Self::L2 => "l2",
            Self::L3 => "l3",
        }
    }
}
