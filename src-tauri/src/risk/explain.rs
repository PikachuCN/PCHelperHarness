use super::rules::RuleMatch;
use super::RiskLevel;

pub(crate) fn quoted_target(target: Option<&str>) -> String {
    match target.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            let cleaned: String = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
            format!("「{}」", cleaned)
        }
        None => "（目标参数不明确）".to_string(),
    }
}

pub(crate) fn self_report_reason(level: RiskLevel) -> String {
    format!(
        "调用方自报风险为 {}，高于规则判定，最终结果已按较高等级采纳。",
        level
    )
}

pub(crate) fn summary_zh(
    level: RiskLevel,
    requires_typed_confirmation: bool,
    findings: &[RuleMatch],
) -> String {
    let has = |prefix: &str| {
        findings
            .iter()
            .any(|finding| finding.rule_id.starts_with(prefix))
    };

    let mut summary = match level {
        RiskLevel::L0 => "只读操作：未发现文件写入、软件安装或破坏性风险。".to_string(),
        RiskLevel::L1 => {
            if has("file.write") || has("shell.output_redirection") {
                "低风险写入：命令会创建或修改本地文件，请核对目标路径与覆盖范围。".to_string()
            } else {
                "需要谨慎：命令不在严格只读白名单内，或其完整影响无法静态确认。".to_string()
            }
        }
        RiskLevel::L2 => {
            if has("package.install") {
                "中风险安装：将安装或更新软件，可能修改系统并引入外部代码，请核对软件来源。"
                    .to_string()
            } else if has("network.download") {
                "中风险下载：将获取网络内容，请核对 URL、来源及保存位置，不要直接运行未知文件。"
                    .to_string()
            } else {
                "中风险系统变更：命令可能修改系统、权限或启动配置，请核对参数与影响范围。"
                    .to_string()
            }
        }
        RiskLevel::L3 => {
            if has("disk.") || has("boot.delete") {
                "高风险磁盘或启动变更：可能造成数据不可恢复或 Windows 无法启动。".to_string()
            } else if has("obfuscation.") {
                "高风险混淆执行：命令包含编码或动态执行内容，无法可靠审计其实际行为。".to_string()
            } else {
                "高风险破坏性操作：可能删除数据、终止关键进程或中断系统服务。".to_string()
            }
        }
    };

    if requires_typed_confirmation {
        summary.push_str(" 执行前必须完成逐字输入确认，不能仅点击确认按钮。");
    }
    summary
}
