use super::types::*;
use super::{execute, NetworkContext};
use chrono::Utc;

pub async fn run(context: NetworkContext) -> NetworkDiagnosticReport {
    let started = Utc::now();
    let timer = std::time::Instant::now();
    context.emit(
        "diagnostic",
        "parallel",
        "running",
        "正在并发执行公网 IP、网站、Ping、DNS 和 IPv6 轻量诊断（不含测速和路由跟踪）",
        0,
        5,
        0,
        Vec::new(),
    );

    let public_ip = execute(
        NetworkTestRequest::PublicIp {
            path: NetworkPath::System,
        },
        context.clone(),
    );
    let websites = async {
        let mut results = Vec::new();
        for url in ["https://www.baidu.com/", "https://www.cloudflare.com/"] {
            results.push(
                execute(
                    NetworkTestRequest::Website {
                        url: url.into(),
                        path: NetworkPath::System,
                        timeout_ms: 8_000,
                    },
                    context.clone(),
                )
                .await,
            );
        }
        results
    };
    let pings = async {
        let mut results = Vec::new();
        for target in ["223.5.5.5", "1.1.1.1"] {
            results.push(
                execute(
                    NetworkTestRequest::Ping {
                        target: target.into(),
                        protocol: PingProtocol::Icmp,
                        port: None,
                        count: 3,
                        timeout_ms: 1_000,
                    },
                    context.clone(),
                )
                .await,
            );
        }
        results
    };
    let dns = async {
        let mut results = Vec::new();
        for record_type in [DnsRecordType::A, DnsRecordType::AAAA] {
            results.push(
                execute(
                    NetworkTestRequest::Dns {
                        domain: "www.cloudflare.com".into(),
                        record_type,
                        transport: DnsTransport::Udp,
                        server: Some("223.5.5.5".into()),
                        compare_system: true,
                        timeout_ms: 5_000,
                    },
                    context.clone(),
                )
                .await,
            );
        }
        results
    };
    let ipv6 = execute(
        NetworkTestRequest::Ipv6 {
            path: NetworkPath::System,
        },
        context.clone(),
    );
    let (public_ip, websites, pings, dns, ipv6) =
        tokio::join!(public_ip, websites, pings, dns, ipv6);
    let mut results = vec![public_ip];
    results.extend(websites);
    results.extend(pings);
    results.extend(dns);
    results.push(ipv6);
    let cancelled = context.cancel.is_cancelled();
    let findings = build_findings(&results);
    let status = report_status(&findings, cancelled);
    context.emit(
        "diagnostic",
        "complete",
        match status {
            DiagnosticStatus::Normal => "success",
            DiagnosticStatus::Warning => "warning",
            DiagnosticStatus::Error => "error",
            DiagnosticStatus::Cancelled => "cancelled",
        },
        "轻量网络诊断完成",
        5,
        5,
        0,
        Vec::new(),
    );
    NetworkDiagnosticReport {
        task_id: context.task_id,
        status,
        started_at: started.to_rfc3339(),
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: timer.elapsed().as_millis().min(u64::MAX as u128) as u64,
        results,
        findings,
    }
}

pub fn immediate_error(task_id: String, error: String) -> NetworkDiagnosticReport {
    let now = Utc::now().to_rfc3339();
    NetworkDiagnosticReport {
        task_id,
        status: DiagnosticStatus::Error,
        started_at: now.clone(),
        finished_at: now,
        duration_ms: 0,
        results: Vec::new(),
        findings: vec![DiagnosticFinding {
            severity: FindingSeverity::Error,
            title: "无法启动网络诊断".into(),
            detail: error.clone(),
            recommendation: "请使用新的 taskId 重试；若问题持续，请重启应用".into(),
        }],
    }
}

fn build_findings(results: &[NetworkTestResult]) -> Vec<DiagnosticFinding> {
    let mut findings = Vec::new();
    for result in results {
        if result.status == TestStatus::Cancelled {
            continue;
        }
        let previous_finding_count = findings.len();
        match result.tool.as_str() {
            "publicIp" => {
                if let Ok(data) = serde_json::from_value::<PublicIpData>(result.data.clone()) {
                    if data.ipv4.is_none() && data.ipv6.is_none() {
                        findings.push(finding(
                            FindingSeverity::Error,
                            "未检测到公网 IP",
                            "IPv4 与 IPv6 地址服务均未返回有效地址",
                            "检查网关、代理和 DNS；可先测试网站与 Ping 结果以区分公网连接问题",
                        ));
                    } else if !data.warnings.is_empty() {
                        findings.push(finding(FindingSeverity::Warning, "公网 IP 信息不完整", data.warnings.join("；"), "若仅 IPv6 失败而 IPv4 网站正常，通常不影响普通上网；否则检查对应协议族连接"));
                    }
                }
            }
            "website" => {
                if result.status == TestStatus::Error {
                    findings.push(finding(
                        FindingSeverity::Error,
                        format!(
                            "网站访问失败：{}",
                            result.target.as_deref().unwrap_or("未知目标")
                        ),
                        result
                            .error
                            .clone()
                            .unwrap_or_else(|| result.summary.clone()),
                        "查看 DNS、TCP、TLS、HTTP 各阶段；仅国际目标失败时优先检查国际链路或代理",
                    ));
                } else if result.status == TestStatus::Warning {
                    findings.push(finding(
                        FindingSeverity::Warning,
                        format!(
                            "网站访问存在异常：{}",
                            result.target.as_deref().unwrap_or("未知目标")
                        ),
                        result.summary.clone(),
                        "根据 HTTP 状态码和重定向记录检查站点或链路",
                    ));
                }
            }
            "ping" => {
                if let Ok(data) = serde_json::from_value::<PingData>(result.data.clone()) {
                    if data.received == 0 {
                        findings.push(finding(
                            FindingSeverity::Error,
                            format!("Ping 目标无响应：{}", data.target),
                            format!("发送 {} 次，接收 0 次，丢包率 100%", data.sent),
                            "结合网站测试判断是否为 ICMP 被禁用；不要仅凭 Ping 失败认定断网",
                        ));
                    } else if data.loss_percent > 0.0 {
                        findings.push(finding(
                            FindingSeverity::Warning,
                            format!("Ping 存在丢包：{}", data.target),
                            format!(
                                "实测丢包率 {:.1}%，平均延迟 {} ms",
                                data.loss_percent,
                                data.avg_ms
                                    .map(|value| value.to_string())
                                    .unwrap_or_else(|| "未知".into())
                            ),
                            "重复测试并与另一目标对比，持续丢包时检查 Wi-Fi、路由器和运营商链路",
                        ));
                    } else if data.avg_ms.unwrap_or(0.0) > 200.0 {
                        findings.push(finding(
                            FindingSeverity::Warning,
                            format!("Ping 延迟较高：{}", data.target),
                            format!("实测平均延迟 {:.1} ms", data.avg_ms.unwrap()),
                            "对比国内与国际目标，若仅国际延迟高通常与路由距离和跨境链路有关",
                        ));
                    }
                }
            }
            "dns" => {
                if result.status == TestStatus::Error {
                    findings.push(finding(FindingSeverity::Error, format!("DNS 查询失败：{}", result.target.as_deref().unwrap_or("未知域名")), result.error.clone().unwrap_or_else(|| result.summary.clone()), "对照系统 DNS 与自定义 DNS 的结果；两者都失败时检查网络，仅系统 DNS 失败时检查本机 DNS 配置"));
                } else if result.status == TestStatus::Warning {
                    findings.push(finding(
                        FindingSeverity::Warning,
                        format!(
                            "DNS 对照结果不一致或部分失败：{}",
                            result.target.as_deref().unwrap_or("未知域名")
                        ),
                        result.summary.clone(),
                        "检查各 DNS 服务器返回的状态和记录，必要时更换可信 DNS",
                    ));
                }
            }
            "ipv6" => {
                if let Ok(data) = serde_json::from_value::<Ipv6Data>(result.data.clone()) {
                    if !data.available {
                        findings.push(finding(FindingSeverity::Info, "IPv6 未完全可用", data.steps.iter().map(|step| format!("{}：{}", step.label, step.detail)).collect::<Vec<_>>().join("；"), "IPv4 正常时可暂不处理；需要 IPv6 时检查光猫/路由器前缀下发和系统 IPv6 设置"));
                    }
                }
            }
            _ => {}
        }
        if matches!(result.status, TestStatus::Error | TestStatus::Warning)
            && findings.len() == previous_finding_count
        {
            let severity = if result.status == TestStatus::Error {
                FindingSeverity::Error
            } else {
                FindingSeverity::Warning
            };
            findings.push(finding(
                severity,
                format!("{} 测试报告异常", result.tool),
                result
                    .error
                    .clone()
                    .unwrap_or_else(|| result.summary.clone()),
                "查看该测试保留的错误信息和其他项目结果，针对异常层级继续排查",
            ));
        }
    }
    if findings.is_empty() {
        findings.push(finding(
            FindingSeverity::Info,
            "未发现明显网络异常",
            "公网地址、国内与国际网站、Ping、DNS 和 IPv6 检查已完成，未触发异常规则",
            "若仍有应用无法访问，请提供具体应用、网址和错误时间进一步定位",
        ));
    }
    findings
}

fn report_status(findings: &[DiagnosticFinding], cancelled: bool) -> DiagnosticStatus {
    if cancelled {
        DiagnosticStatus::Cancelled
    } else if findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Error)
    {
        DiagnosticStatus::Error
    } else if findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Warning)
    {
        DiagnosticStatus::Warning
    } else {
        DiagnosticStatus::Normal
    }
}

fn finding(
    severity: FindingSeverity,
    title: impl Into<String>,
    detail: impl Into<String>,
    recommendation: impl Into<String>,
) -> DiagnosticFinding {
    DiagnosticFinding {
        severity,
        title: title.into(),
        detail: detail.into(),
        recommendation: recommendation.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(tool: &str, status: TestStatus, data: serde_json::Value) -> NetworkTestResult {
        NetworkTestResult {
            task_id: "test".into(),
            tool: tool.into(),
            status,
            started_at: String::new(),
            finished_at: String::new(),
            duration_ms: 1,
            target: Some("target".into()),
            path: None,
            summary: "实测摘要".into(),
            error: None,
            data,
        }
    }

    #[test]
    fn complete_packet_loss_produces_error_finding() {
        let data = PingData {
            protocol: PingProtocol::Icmp,
            target: "1.1.1.1".into(),
            port: None,
            sent: 3,
            received: 0,
            loss_percent: 100.0,
            min_ms: None,
            avg_ms: None,
            max_ms: None,
            jitter_ms: None,
            samples: Vec::new(),
        };
        let findings = build_findings(&[result(
            "ping",
            TestStatus::Error,
            serde_json::to_value(data).unwrap(),
        )]);
        assert!(findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Error
                && finding.detail.contains("接收 0 次")));
        assert_eq!(report_status(&findings, false), DiagnosticStatus::Error);
    }

    #[test]
    fn ipv6_unavailable_alone_is_informational() {
        let data = Ipv6Data {
            available: false,
            global_addresses: Vec::new(),
            public_address: None,
            steps: vec![Ipv6Step {
                key: "address".into(),
                label: "本机 IPv6 地址".into(),
                status: StageStatus::Warning,
                detail: "仅链路本地地址".into(),
            }],
            warnings: Vec::new(),
        };
        let findings = build_findings(&[result(
            "ipv6",
            TestStatus::Warning,
            serde_json::to_value(data).unwrap(),
        )]);
        assert_eq!(findings[0].severity, FindingSeverity::Info);
        assert_eq!(report_status(&findings, false), DiagnosticStatus::Normal);
    }

    #[test]
    fn malformed_error_data_cannot_be_misreported_as_normal() {
        let findings = build_findings(&[result(
            "publicIp",
            TestStatus::Error,
            serde_json::Value::Null,
        )]);
        assert!(findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Error));
        assert_eq!(report_status(&findings, false), DiagnosticStatus::Error);
    }

    #[test]
    fn cancellation_has_highest_status_precedence() {
        assert_eq!(report_status(&[], true), DiagnosticStatus::Cancelled);
        assert_eq!(report_status(&[], false), DiagnosticStatus::Normal);
    }
}
