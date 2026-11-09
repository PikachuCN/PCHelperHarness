use super::process::{process_error, result_json, run_powershell};
use super::types::{
    DnsAnswer, DnsData, DnsQuery, DnsRecordType, DnsTransport, StageStatus, ToolOutcome,
};
use super::validation::validate_host;
use super::NetworkContext;
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};

const DNS_SCRIPT: &str = r#"
$domain = [string]$params.domain
$recordType = [string]$params.recordType
$server = [string]$params.server
$transport = [string]$params.transport
$answers = @()
$status = 'success'
$errorText = $null
try {
  $arguments = @{ Name = $domain; Type = $recordType; DnsOnly = $true; ErrorAction = 'Stop' }
  if (-not [string]::IsNullOrWhiteSpace($server)) { $arguments.Server = $server }
  if ($transport -eq 'tcp') { $arguments.TcpOnly = $true }
  $records = @(Resolve-DnsName @arguments)
  $answers = @($records | ForEach-Object {
    $value = $null
    switch ([string]$_.Type) {
      'A' { $value = [string]$_.IPAddress }
      'AAAA' { $value = [string]$_.IPAddress }
      'CNAME' { $value = [string]$_.NameHost }
      'NS' { $value = [string]$_.NameHost }
      'MX' { $value = ([string]$_.Preference + ' ' + [string]$_.NameExchange).Trim() }
      'TXT' { $value = [string]($_.Strings -join ' ') }
    }
    if (-not [string]::IsNullOrWhiteSpace($value)) {
      $ttl = 0
      if ($null -ne $_.TTL) { $ttl = [uint32]$_.TTL }
      [pscustomobject]@{ name = [string]$_.Name; recordType = [string]$_.Type; ttl = $ttl; value = $value }
    }
  })
  if ($answers.Count -eq 0) { $status = 'warning'; $errorText = '查询成功但未返回匹配记录' }
} catch {
  $status = 'error'
  $errorText = $_.Exception.Message
}
'HARNESS_RESULT:' + ([pscustomobject]@{ status = $status; answers = @($answers); error = $errorText } | ConvertTo-Json -Depth 6 -Compress)
"#;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDnsPayload {
    status: StageStatus,
    #[serde(default)]
    answers: Vec<DnsAnswer>,
    error: Option<String>,
}

pub async fn run(
    domain: String,
    record_type: DnsRecordType,
    transport: DnsTransport,
    server: Option<String>,
    compare_system: bool,
    timeout_ms: u64,
    context: &NetworkContext,
) -> ToolOutcome {
    let domain = match validate_host(&domain) {
        Ok(domain) => domain,
        Err(error) => return ToolOutcome::error("DNS 参数无效", error, Value::Null),
    };
    let servers = match server {
        Some(server) if compare_system => vec![String::new(), server],
        Some(server) => vec![server],
        None => vec![String::new()],
    };
    let total = servers.len() as u32;
    let mut queries = Vec::with_capacity(servers.len());
    for (index, current_server) in servers.into_iter().enumerate() {
        if context.cancel.is_cancelled() {
            return ToolOutcome::cancelled("DNS 查询已取消");
        }
        let label = if current_server.is_empty() {
            "system".to_string()
        } else {
            current_server.clone()
        };
        context.emit(
            "dns",
            "query",
            "running",
            format!("正在通过 {label} 查询 {domain} 的 {record_type:?} 记录"),
            index as u32,
            total,
            0,
            Vec::new(),
        );
        let started = Instant::now();
        let output = run_powershell(
            DNS_SCRIPT,
            &serde_json::json!({"domain":domain,"recordType":format!("{record_type:?}"),"server":current_server,"transport":format!("{transport:?}").to_ascii_lowercase()}),
            Duration::from_millis(timeout_ms.saturating_add(2_000)),
            context.cancel.clone(),
            None,
        ).await;
        let duration_ms = round(started.elapsed().as_secs_f64() * 1000.0);
        let query = match output {
            Ok(output) if output.cancelled || context.cancel.is_cancelled() => {
                return ToolOutcome::cancelled("DNS 查询已取消")
            }
            Ok(output) => match result_json(&output.stdout) {
                Ok(value) => match parse_payload(&value) {
                    Ok(payload) => DnsQuery {
                        server: label.clone(),
                        transport,
                        duration_ms,
                        status: payload.status,
                        answers: payload.answers,
                        error: payload.error,
                    },
                    Err(error) => DnsQuery {
                        server: label.clone(),
                        transport,
                        duration_ms,
                        status: StageStatus::Error,
                        answers: Vec::new(),
                        error: Some(error),
                    },
                },
                Err(error) => DnsQuery {
                    server: label.clone(),
                    transport,
                    duration_ms,
                    status: StageStatus::Error,
                    answers: Vec::new(),
                    error: Some(process_error(&output).unwrap_or(error)),
                },
            },
            Err(error) => DnsQuery {
                server: label.clone(),
                transport,
                duration_ms,
                status: StageStatus::Error,
                answers: Vec::new(),
                error: Some(error),
            },
        };
        context.emit(
            "dns",
            "query",
            if query.status == StageStatus::Error {
                "error"
            } else {
                "running"
            },
            format!("{label} 查询完成：{} 条记录", query.answers.len()),
            index as u32 + 1,
            total,
            0,
            Vec::new(),
        );
        queries.push(query);
    }
    let successful = queries
        .iter()
        .filter(|query| query.status != StageStatus::Error)
        .count();
    let answer_count: usize = queries.iter().map(|query| query.answers.len()).sum();
    let warnings: Vec<String> = queries
        .iter()
        .filter_map(|query| {
            query
                .error
                .as_ref()
                .map(|error| format!("{}：{error}", query.server))
        })
        .collect();
    let data = DnsData {
        domain,
        record_type,
        transport,
        queries,
        warnings,
    };
    if successful == 0 {
        ToolOutcome::error("DNS 查询全部失败", data.warnings.join("；"), data)
    } else if !data.warnings.is_empty() {
        ToolOutcome::warning(
            format!("DNS 返回 {answer_count} 条记录，部分查询失败"),
            data,
        )
    } else {
        ToolOutcome::success(format!("DNS 返回 {answer_count} 条记录"), data)
    }
}

fn parse_payload(value: &Value) -> Result<RawDnsPayload, String> {
    serde_json::from_value(value.clone()).map_err(|error| format!("DNS 结果结构无效：{error}"))
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_dns_answers_without_network() {
        let value = serde_json::json!({
            "status":"success",
            "answers":[
                {"name":"example.com","recordType":"A","ttl":60,"value":"93.184.216.34"},
                {"name":"example.com","recordType":"AAAA","ttl":60,"value":"2606:2800:220:1:248:1893:25c8:1946"},
                {"name":"example.com","recordType":"MX","ttl":60,"value":"10 mail.example.com"},
                {"name":"example.com","recordType":"TXT","ttl":60,"value":"v=spf1 -all"}
            ],
            "error":null
        });
        let payload = parse_payload(&value).unwrap();
        assert_eq!(payload.answers.len(), 4);
        assert_eq!(payload.answers[1].record_type, "AAAA");
        assert_eq!(payload.answers[2].value, "10 mail.example.com");
    }

    #[test]
    fn failed_query_is_not_reported_as_success() {
        let value =
            serde_json::json!({"status":"error","answers":[],"error":"DNS server unreachable"});
        let payload = parse_payload(&value).unwrap();
        assert_eq!(payload.status, StageStatus::Error);
        assert!(payload.answers.is_empty());
    }
}
