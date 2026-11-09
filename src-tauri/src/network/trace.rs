use super::process::{process_error, result_json, run_powershell, LineCallback};
use super::types::{ToolOutcome, TraceData, TraceHop};
use super::validation::validate_host;
use super::NetworkContext;
use serde_json::Value;
use std::time::Duration;

const TRACE_SCRIPT: &str = r#"
$target = [string]$params.target
$maxHops = [int]$params.maxHops
$timeout = [int]$params.timeoutMs
$hops = @()
$reached = $false
$ping = [System.Net.NetworkInformation.Ping]::new()
$buffer = [byte[]]::new(32)
try {
  for ($hop = 1; $hop -le $maxHops -and -not $reached; $hop++) {
    $latencies = @()
    $address = $null
    $hopStatus = 'timeout'
    $options = [System.Net.NetworkInformation.PingOptions]::new($hop, $false)
    for ($probe = 0; $probe -lt 2; $probe++) {
      $latency = $null
      try {
        $reply = $ping.Send($target, $timeout, $buffer, $options)
        if ($reply.Status -eq [System.Net.NetworkInformation.IPStatus]::Success -or $reply.Status -eq [System.Net.NetworkInformation.IPStatus]::TtlExpired) {
          $latency = [double]$reply.RoundtripTime
          $address = [string]$reply.Address
          if ($reply.Status -eq [System.Net.NetworkInformation.IPStatus]::Success) { $hopStatus = 'success'; $reached = $true }
          elseif ($hopStatus -ne 'success') { $hopStatus = 'ttlExpired' }
        } elseif ($reply.Status -eq [System.Net.NetworkInformation.IPStatus]::TimedOut) {
          if ($hopStatus -eq 'timeout') { $hopStatus = 'timeout' }
        } else { $hopStatus = ([string]$reply.Status).ToLowerInvariant() }
      } catch {
        $hopStatus = 'error'
        if ($_.Exception.Message -match 'denied|权限|拒绝|elevat|administrator') { $hopStatus = 'permissionDenied' }
      }
      $latencies += $latency
    }
    $entry = [pscustomobject]@{ hop = $hop; address = $address; hostname = $null; latenciesMs = @($latencies); status = $hopStatus }
    $hops += $entry
    'HARNESS_HOP:' + ($entry | ConvertTo-Json -Depth 5 -Compress)
  }
} finally { $ping.Dispose() }
'HARNESS_RESULT:' + ([pscustomobject]@{ hops = @($hops); reachedTarget = $reached } | ConvertTo-Json -Depth 6 -Compress)
"#;

pub async fn run(
    target: String,
    max_hops: u32,
    timeout_ms: u64,
    context: &NetworkContext,
) -> ToolOutcome {
    let target = match validate_host(&target) {
        Ok(target) => target,
        Err(error) => return ToolOutcome::error("路由跟踪参数无效", error, Value::Null),
    };
    context.emit(
        "trace",
        "route",
        "running",
        format!("开始跟踪到 {target} 的路由，最多 {max_hops} 跳"),
        0,
        max_hops,
        0,
        Vec::new(),
    );
    let callback_context = context.clone();
    let callback: LineCallback = std::sync::Arc::new(move |line| {
        let Some(json) = line.strip_prefix("HARNESS_HOP:") else {
            return;
        };
        let Ok(hop) = serde_json::from_str::<TraceHop>(json) else {
            return;
        };
        let detail = hop.address.clone().unwrap_or_else(|| "未响应".to_string());
        callback_context.emit(
            "trace",
            "route",
            "running",
            format!("第 {} 跳：{detail}（{}）", hop.hop, hop.status),
            hop.hop,
            max_hops,
            0,
            Vec::new(),
        );
    });
    let overall_ms = u64::from(max_hops)
        .saturating_mul(2)
        .saturating_mul(timeout_ms.saturating_add(500))
        .saturating_add(5_000)
        .min(1_200_000);
    let output = match run_powershell(
        TRACE_SCRIPT,
        &serde_json::json!({"target":target,"maxHops":max_hops,"timeoutMs":timeout_ms}),
        Duration::from_millis(overall_ms),
        context.cancel.clone(),
        Some(callback),
    )
    .await
    {
        Ok(output) => output,
        Err(error) => return ToolOutcome::error("路由跟踪无法启动", error, Value::Null),
    };
    if output.cancelled || context.cancel.is_cancelled() {
        return ToolOutcome::cancelled("路由跟踪已取消");
    }
    let value = match result_json(&output.stdout) {
        Ok(value) => value,
        Err(error) => {
            return ToolOutcome::error(
                "路由跟踪执行失败",
                process_error(&output).unwrap_or(error),
                Value::Null,
            )
        }
    };
    if let Some(error) = process_error(&output) {
        return ToolOutcome::error("路由跟踪执行失败", error, Value::Null);
    }
    let hops: Vec<TraceHop> = value
        .get("hops")
        .and_then(Value::as_array)
        .map(|hops| {
            hops.iter()
                .filter_map(|hop| serde_json::from_value(hop.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let reached_target = value
        .get("reachedTarget")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(hop) = hops.iter().find(|hop| hop.status == "permissionDenied") {
        return ToolOutcome::error(
            "路由跟踪需要更高权限",
            format!(
                "PERMISSION_DENIED: 第 {} 跳被系统拒绝，可使用“以管理员身份重启”后重试",
                hop.hop
            ),
            TraceData {
                target,
                max_hops,
                hops,
                reached_target,
            },
        );
    }
    let data = TraceData {
        target,
        max_hops,
        hops,
        reached_target,
    };
    if data.hops.is_empty() {
        ToolOutcome::error(
            "路由跟踪未获得任何跳点",
            "ICMP 请求未执行或未返回结果",
            data,
        )
    } else if reached_target {
        ToolOutcome::success(format!("已在 {} 跳内到达目标", data.hops.len()), data)
    } else if data.hops.iter().any(|hop| hop.address.is_some()) {
        ToolOutcome::warning(
            format!(
                "已记录 {} 跳，但未在 {max_hops} 跳内到达目标",
                data.hops.len()
            ),
            data,
        )
    } else {
        ToolOutcome::error(
            "路由跳点均未响应",
            "目标或中间路由器可能禁用了 ICMP，也可能需要管理员权限",
            data,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_json_preserves_unanswered_probes() {
        let hop: TraceHop = serde_json::from_str(r#"{"hop":2,"address":"1.2.3.4","hostname":null,"latenciesMs":[12.0,null],"status":"ttlExpired"}"#).unwrap();
        assert_eq!(hop.latencies_ms, vec![Some(12.0), None]);
        assert_eq!(hop.address.as_deref(), Some("1.2.3.4"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn trace_template_reaches_loopback_without_public_network() {
        let context = NetworkContext {
            task_id: "local-trace".into(),
            cancel: tokio_util::sync::CancellationToken::new(),
            emitter: std::sync::Arc::new(|_| {}),
        };
        let outcome = run("127.0.0.1".into(), 1, 1_000, &context).await;
        assert_eq!(
            outcome.status,
            super::super::types::TestStatus::Success,
            "{}: {:?}",
            outcome.summary,
            outcome.error
        );
        assert_eq!(outcome.data["reachedTarget"], true);
        assert_eq!(outcome.data["hops"].as_array().map(Vec::len), Some(1));
    }
}
