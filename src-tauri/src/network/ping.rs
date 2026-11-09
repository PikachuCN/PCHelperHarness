use super::process::{
    is_permission_error, process_error, result_json, run_powershell, LineCallback,
};
use super::types::{PingData, PingProtocol, PingSample, PointKind, ProgressPoint, ToolOutcome};
use super::validation::validate_host;
use super::NetworkContext;
use chrono::Utc;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const ICMP_SCRIPT: &str = r#"
$target = [string]$params.target
$count = [int]$params.count
$timeout = [int]$params.timeoutMs
$samples = @()
$ping = [System.Net.NetworkInformation.Ping]::new()
try {
  for ($sequence = 1; $sequence -le $count; $sequence++) {
    $latency = $null
    $status = 'error'
    $errorText = $null
    try {
      $reply = $ping.Send($target, $timeout)
      if ($reply.Status -eq [System.Net.NetworkInformation.IPStatus]::Success) {
        $status = 'success'
        $latency = [double]$reply.RoundtripTime
      } elseif ($reply.Status -eq [System.Net.NetworkInformation.IPStatus]::TimedOut) {
        $status = 'timeout'
        $errorText = '请求超时'
      } else {
        $errorText = [string]$reply.Status
      }
    } catch {
      $errorText = $_.Exception.Message
      if ($_.Exception -is [System.UnauthorizedAccessException] -or $errorText -match 'denied|权限|拒绝|elevat|administrator') { $status = 'permissionDenied' }
    }
    $sample = [pscustomobject]@{ sequence = $sequence; latencyMs = $latency; status = $status; error = $errorText }
    $samples += $sample
    'HARNESS_SAMPLE:' + ($sample | ConvertTo-Json -Compress)
    if ($sequence -lt $count) { Start-Sleep -Milliseconds 200 }
  }
} finally { $ping.Dispose() }
'HARNESS_RESULT:' + ([pscustomobject]@{ samples = @($samples) } | ConvertTo-Json -Depth 5 -Compress)
"#;

pub async fn run(
    target: String,
    protocol: PingProtocol,
    port: Option<u16>,
    count: u32,
    timeout_ms: u64,
    context: &NetworkContext,
) -> ToolOutcome {
    let target = match validate_host(&target) {
        Ok(target) => target,
        Err(error) => return ToolOutcome::error("Ping 参数无效", error, Value::Null),
    };
    match protocol {
        PingProtocol::Icmp => run_icmp(target, count, timeout_ms, context).await,
        PingProtocol::Tcp => run_tcp(target, port.unwrap_or(0), count, timeout_ms, context).await,
    }
}

async fn run_icmp(
    target: String,
    count: u32,
    timeout_ms: u64,
    context: &NetworkContext,
) -> ToolOutcome {
    context.emit(
        "ping",
        "icmp",
        "running",
        format!("开始对 {target} 发送 {count} 次 ICMP Ping"),
        0,
        count,
        0,
        Vec::new(),
    );
    let callback_context = context.clone();
    let callback_target = target.clone();
    let callback: LineCallback = std::sync::Arc::new(move |line| {
        let Some(json) = line.strip_prefix("HARNESS_SAMPLE:") else {
            return;
        };
        let Ok(sample) = serde_json::from_str::<PingSample>(json) else {
            return;
        };
        let points = sample
            .latency_ms
            .map(|latency| {
                vec![ProgressPoint {
                    at: Utc::now().timestamp_millis(),
                    kind: PointKind::Latency,
                    value: latency,
                    sequence: Some(sample.sequence),
                }]
            })
            .unwrap_or_default();
        callback_context.emit(
            "ping",
            "icmp",
            "running",
            format!(
                "{callback_target} 第 {} 次：{}",
                sample.sequence, sample.status
            ),
            sample.sequence,
            count,
            0,
            points,
        );
    });
    let overall_ms = u64::from(count)
        .saturating_mul(timeout_ms.saturating_add(700))
        .saturating_add(5_000);
    let output = match run_powershell(
        ICMP_SCRIPT,
        &serde_json::json!({"target": target, "count": count, "timeoutMs": timeout_ms}),
        Duration::from_millis(overall_ms),
        context.cancel.clone(),
        Some(callback),
    )
    .await
    {
        Ok(output) => output,
        Err(error) => return ToolOutcome::error("ICMP Ping 无法启动", error, Value::Null),
    };
    if output.cancelled || context.cancel.is_cancelled() {
        return ToolOutcome::cancelled("ICMP Ping 已取消");
    }
    let samples = match result_json(&output.stdout) {
        Ok(value) => parse_samples(&value),
        Err(error) => {
            let message = process_error(&output).unwrap_or(error);
            return ToolOutcome::error("ICMP Ping 执行失败", message, Value::Null);
        }
    };
    if let Some(error) = process_error(&output) {
        return ToolOutcome::error("ICMP Ping 执行失败", error, Value::Null);
    }
    finish(target, PingProtocol::Icmp, None, count, samples)
}

async fn run_tcp(
    target: String,
    port: u16,
    count: u32,
    timeout_ms: u64,
    context: &NetworkContext,
) -> ToolOutcome {
    context.emit(
        "ping",
        "tcp",
        "running",
        format!("开始对 {target}:{port} 进行 {count} 次 TCP 连接测试"),
        0,
        count,
        0,
        Vec::new(),
    );
    let mut samples = Vec::with_capacity(count as usize);
    for sequence in 1..=count {
        if context.cancel.is_cancelled() {
            return ToolOutcome::cancelled("TCP Ping 已取消");
        }
        let started = Instant::now();
        let attempt = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            TcpStream::connect((target.as_str(), port)),
        );
        let sample = tokio::select! {
            _ = context.cancel.cancelled() => return ToolOutcome::cancelled("TCP Ping 已取消"),
            result = attempt => match result {
                Ok(Ok(stream)) => {
                    drop(stream);
                    PingSample { sequence, latency_ms: Some(round(started.elapsed().as_secs_f64() * 1000.0)), status: "success".to_string(), error: None }
                }
                Ok(Err(error)) => PingSample { sequence, latency_ms: None, status: "error".to_string(), error: Some(error.to_string()) },
                Err(_) => PingSample { sequence, latency_ms: None, status: "timeout".to_string(), error: Some("连接超时".to_string()) },
            }
        };
        let points = sample
            .latency_ms
            .map(|latency| {
                vec![ProgressPoint {
                    at: Utc::now().timestamp_millis(),
                    kind: PointKind::Latency,
                    value: latency,
                    sequence: Some(sequence),
                }]
            })
            .unwrap_or_default();
        context.emit(
            "ping",
            "tcp",
            "running",
            format!("{target}:{port} 第 {sequence} 次：{}", sample.status),
            sequence,
            count,
            0,
            points,
        );
        samples.push(sample);
        if sequence < count {
            tokio::select! {
                _ = context.cancel.cancelled() => return ToolOutcome::cancelled("TCP Ping 已取消"),
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
        }
    }
    finish(target, PingProtocol::Tcp, Some(port), count, samples)
}

fn parse_samples(value: &Value) -> Vec<PingSample> {
    value
        .get("samples")
        .and_then(Value::as_array)
        .map(|samples| {
            samples
                .iter()
                .filter_map(|sample| serde_json::from_value(sample.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn finish(
    target: String,
    protocol: PingProtocol,
    port: Option<u16>,
    sent: u32,
    samples: Vec<PingSample>,
) -> ToolOutcome {
    let data = summarize(target, protocol, port, sent, samples);
    let loss = data.loss_percent;
    let permission = data.samples.iter().find(|sample| {
        sample.status == "permissionDenied"
            || sample
                .error
                .as_deref()
                .map(is_permission_error)
                .unwrap_or(false)
    });
    if let Some(sample) = permission {
        let detail = sample
            .error
            .clone()
            .unwrap_or_else(|| "系统拒绝 ICMP Ping".to_string());
        return ToolOutcome::error(
            "Ping 需要更高权限",
            format!("PERMISSION_DENIED: {detail}。可使用“以管理员身份重启”后重试"),
            data,
        );
    }
    if data.received == 0 {
        let detail = data
            .samples
            .iter()
            .find_map(|sample| sample.error.clone())
            .unwrap_or_else(|| "全部请求未收到响应".to_string());
        return ToolOutcome::error("Ping 全部失败", detail, data);
    }
    let summary = format!(
        "发送 {sent} 次，接收 {} 次，丢包率 {:.1}%",
        data.received, loss
    );
    if loss > 0.0 {
        ToolOutcome::warning(summary, data)
    } else {
        ToolOutcome::success(summary, data)
    }
}

fn summarize(
    target: String,
    protocol: PingProtocol,
    port: Option<u16>,
    sent: u32,
    samples: Vec<PingSample>,
) -> PingData {
    let latencies: Vec<f64> = samples
        .iter()
        .filter_map(|sample| sample.latency_ms)
        .filter(|latency| latency.is_finite())
        .collect();
    let received = latencies.len() as u32;
    let min_ms = latencies.iter().copied().reduce(f64::min);
    let max_ms = latencies.iter().copied().reduce(f64::max);
    let avg_ms =
        (!latencies.is_empty()).then(|| latencies.iter().sum::<f64>() / latencies.len() as f64);
    let jitter_ms = (latencies.len() > 1).then(|| {
        latencies
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum::<f64>()
            / (latencies.len() - 1) as f64
    });
    PingData {
        protocol,
        target,
        port,
        sent,
        received,
        loss_percent: round(if sent == 0 {
            100.0
        } else {
            (sent.saturating_sub(received)) as f64 * 100.0 / sent as f64
        }),
        min_ms: min_ms.map(round),
        avg_ms: avg_ms.map(round),
        max_ms: max_ms.map(round),
        jitter_ms: jitter_ms.map(round),
        samples,
    }
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::types::TestStatus;

    #[test]
    fn statistics_use_only_successful_samples() {
        let samples = vec![
            PingSample {
                sequence: 1,
                latency_ms: Some(10.0),
                status: "success".into(),
                error: None,
            },
            PingSample {
                sequence: 2,
                latency_ms: None,
                status: "timeout".into(),
                error: None,
            },
            PingSample {
                sequence: 3,
                latency_ms: Some(16.0),
                status: "success".into(),
                error: None,
            },
            PingSample {
                sequence: 4,
                latency_ms: Some(13.0),
                status: "success".into(),
                error: None,
            },
        ];
        let data = summarize("example.com".into(), PingProtocol::Icmp, None, 4, samples);
        assert_eq!(data.received, 3);
        assert_eq!(data.loss_percent, 25.0);
        assert_eq!(data.avg_ms, Some(13.0));
        assert_eq!(data.jitter_ms, Some(4.5));
        assert_eq!(data.max_ms, Some(16.0));
    }

    #[test]
    fn structured_ping_samples_are_parsed_without_network() {
        let value = serde_json::json!({"samples":[{"sequence":1,"latencyMs":12.5,"status":"success","error":null}]});
        let samples = parse_samples(&value);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].latency_ms, Some(12.5));
    }

    fn test_context() -> NetworkContext {
        NetworkContext {
            task_id: "local-test".into(),
            cancel: tokio_util::sync::CancellationToken::new(),
            emitter: std::sync::Arc::new(|_| {}),
        }
    }

    #[tokio::test]
    async fn tcp_ping_uses_a_real_loopback_connection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        let outcome = run_tcp("127.0.0.1".into(), port, 1, 1_000, &test_context()).await;
        accepted.await.unwrap();
        assert_eq!(outcome.status, TestStatus::Success);
        assert_eq!(outcome.data["received"], 1);
        assert!(outcome.data["avgMs"].as_f64().is_some());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn icmp_template_works_against_loopback_without_public_network() {
        let outcome = run_icmp("127.0.0.1".into(), 1, 1_000, &test_context()).await;
        assert_eq!(
            outcome.status,
            TestStatus::Success,
            "{}: {:?}",
            outcome.summary,
            outcome.error
        );
        assert_eq!(outcome.data["received"], 1);
    }
}
