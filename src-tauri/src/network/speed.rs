use super::types::{
    NetworkPath, PointKind, ProgressPoint, SpeedData, SpeedNode, SpeedProfile, SpeedSample,
    ToolOutcome,
};
use super::validation::{validate_http_url, validate_speed_node};
use super::NetworkContext;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const SAMPLE_INTERVAL: Duration = Duration::from_millis(400);
const UPLOAD_CHUNK: usize = 1024 * 1024;

fn builtins() -> Vec<SpeedNode> {
    vec![
        SpeedNode {
            name: "Clouvider Frankfurt".into(),
            url: "https://fra.speedtest.clouvider.net/backend/".into(),
            operator: "Clouvider".into(),
            region: "德国 · 法兰克福".into(),
        },
        SpeedNode {
            name: "Clouvider London".into(),
            url: "https://lon.speedtest.clouvider.net/backend/".into(),
            operator: "Clouvider".into(),
            region: "英国 · 伦敦".into(),
        },
        SpeedNode {
            name: "Clouvider New York".into(),
            url: "https://nyc.speedtest.clouvider.net/backend/".into(),
            operator: "Clouvider".into(),
            region: "美国 · 纽约".into(),
        },
    ]
}

pub async fn run(
    node: Option<SpeedNode>,
    node_url: Option<String>,
    node_name: Option<String>,
    profile: SpeedProfile,
    path: NetworkPath,
    context: &NetworkContext,
) -> ToolOutcome {
    let node = match select_node(node, node_url, node_name) {
        Ok(node) => node,
        Err(error) => {
            return ToolOutcome::error("测速节点无效", error, serde_json::json!({"warnings":[]}))
        }
    };
    let base_url = match validate_http_url(&node.url) {
        Ok(url) => url,
        Err(error) => {
            return ToolOutcome::error("测速节点无效", error, serde_json::json!({"warnings":[]}))
        }
    };
    let garbage_url = base_url.join("garbage.php").unwrap();
    let empty_url = base_url.join("empty.php").unwrap();
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(2))
        .user_agent("HarnessNetworkDiagnostic/1.0");
    if path == NetworkPath::Direct {
        builder = builder.no_proxy();
    }
    let client = match builder.build() {
        Ok(client) => client,
        Err(error) => {
            return ToolOutcome::error(
                "测速客户端初始化失败",
                error.to_string(),
                serde_json::json!({"warnings":[]}),
            )
        }
    };
    let (seconds, connections, latency_count) = match profile {
        SpeedProfile::Standard => (6, 3, 5),
        SpeedProfile::Precise => (12, 6, 8),
    };
    let mut samples = Vec::new();
    let mut warnings = Vec::new();
    context.emit(
        "speed",
        "latency",
        "running",
        format!("正在测量 {} 的延迟和抖动", node.name),
        0,
        latency_count,
        0,
        Vec::new(),
    );
    let mut latencies = Vec::new();
    for sequence in 1..=latency_count {
        let request = client
            .get(empty_url.clone())
            .query(&[(
                "r",
                Utc::now()
                    .timestamp_nanos_opt()
                    .unwrap_or_default()
                    .to_string(),
            )])
            .timeout(Duration::from_secs(6))
            .send();
        let started = Instant::now();
        let response = tokio::select! {
            _ = context.cancel.cancelled() => return ToolOutcome::cancelled("测速已取消"),
            response = request => response,
        };
        match response {
            Ok(response) if response.status().is_success() => {
                let latency = round(started.elapsed().as_secs_f64() * 1000.0);
                latencies.push(latency);
                samples.push(SpeedSample {
                    at: Utc::now().timestamp_millis(),
                    download_mbps: None,
                    upload_mbps: None,
                    latency_ms: Some(latency),
                });
                context.emit(
                    "speed",
                    "latency",
                    "running",
                    format!("第 {sequence} 次延迟：{latency:.1} ms"),
                    sequence,
                    latency_count,
                    0,
                    vec![ProgressPoint {
                        at: Utc::now().timestamp_millis(),
                        kind: PointKind::Latency,
                        value: latency,
                        sequence: Some(sequence),
                    }],
                );
            }
            Ok(response) => warnings.push(format!(
                "延迟请求第 {sequence} 次返回 HTTP {}",
                response.status()
            )),
            Err(error) => warnings.push(format!("延迟请求第 {sequence} 次失败：{error}")),
        }
    }
    let latency_ms = if latencies.is_empty() {
        0.0
    } else {
        round(latencies.iter().sum::<f64>() / latencies.len() as f64)
    };
    let jitter_ms = if latencies.len() > 1 {
        round(
            latencies
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).abs())
                .sum::<f64>()
                / (latencies.len() - 1) as f64,
        )
    } else {
        0.0
    };
    if latencies.is_empty() {
        warnings.push("所有延迟请求均失败，延迟和抖动记为 0".into());
    }

    context.emit(
        "speed",
        "download",
        "running",
        format!("开始 {seconds} 秒下载测速，{connections} 个连接"),
        0,
        100,
        0,
        Vec::new(),
    );
    let download = download_test(&client, garbage_url, seconds, connections, context).await;
    if download.cancelled {
        return ToolOutcome::cancelled("下载测速已取消");
    }
    samples.extend(download.samples);
    warnings.extend(download.warnings);

    context.emit(
        "speed",
        "upload",
        "running",
        format!("开始 {seconds} 秒上传测速，{connections} 个连接（仅发送程序生成的数据，不读取用户文件）"),
        0,
        100,
        download.bytes,
        Vec::new(),
    );
    let upload = upload_test(&client, empty_url, seconds, connections, context).await;
    if upload.cancelled {
        return ToolOutcome::cancelled("上传测速已取消");
    }
    samples.extend(upload.samples);
    warnings.extend(upload.warnings);
    samples.sort_by_key(|sample| sample.at);

    let data = SpeedData {
        node,
        profile,
        download_mbps: throughput(download.bytes, download.elapsed),
        upload_mbps: throughput(upload.bytes, upload.elapsed),
        latency_ms,
        jitter_ms,
        bytes_downloaded: download.bytes,
        bytes_uploaded: upload.bytes,
        samples,
        warnings,
    };
    let summary = format!(
        "下载 {:.2} Mbps，上传 {:.2} Mbps，延迟 {:.1} ms，抖动 {:.1} ms",
        data.download_mbps, data.upload_mbps, data.latency_ms, data.jitter_ms
    );
    if download.bytes == 0 && upload.bytes == 0 {
        ToolOutcome::error("测速未传输有效数据", data.warnings.join("；"), data)
    } else if download.bytes == 0 || upload.bytes == 0 || !data.warnings.is_empty() {
        ToolOutcome::warning(summary, data)
    } else {
        ToolOutcome::success(summary, data)
    }
}

fn select_node(
    node: Option<SpeedNode>,
    node_url: Option<String>,
    node_name: Option<String>,
) -> Result<SpeedNode, String> {
    let mut selected = if let Some(node) = node {
        node
    } else if let Some(url) = node_url {
        builtins()
            .into_iter()
            .find(|node| node.url.trim_end_matches('/') == url.trim_end_matches('/'))
            .unwrap_or(SpeedNode {
                url,
                ..SpeedNode::default()
            })
    } else {
        builtins().remove(0)
    };
    if let Some(name) = node_name.filter(|name| !name.trim().is_empty()) {
        selected.name = name;
    }
    let mut selected = validate_speed_node(&selected)?;
    if !selected.url.ends_with('/') {
        selected.url.push('/');
    }
    Ok(selected)
}

struct TransferResult {
    bytes: u64,
    elapsed: Duration,
    samples: Vec<SpeedSample>,
    warnings: Vec<String>,
    cancelled: bool,
}

async fn download_test(
    client: &reqwest::Client,
    url: url::Url,
    seconds: u64,
    connections: u32,
    context: &NetworkContext,
) -> TransferResult {
    let total = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let mut tasks = JoinSet::new();
    for _ in 0..connections {
        tasks.spawn(download_connection(
            client.clone(),
            url.clone(),
            deadline,
            context.cancel.clone(),
            total.clone(),
        ));
    }
    let mut samples = Vec::new();
    let mut warnings = Vec::new();
    let mut previous_bytes = 0;
    let mut previous_at = started;
    let mut interval = tokio::time::interval_at(started + SAMPLE_INTERVAL, SAMPLE_INTERVAL);
    let cancelled = loop {
        if tasks.is_empty() {
            break false;
        }
        tokio::select! {
            _ = context.cancel.cancelled() => break true,
            joined = tasks.join_next() => {
                match joined {
                    Some(Ok(Err(error))) => warnings.push(format!("下载连接失败：{error}")),
                    Some(Err(error)) if !error.is_cancelled() => warnings.push(format!("下载任务失败：{error}")),
                    _ => {}
                }
            }
            _ = interval.tick() => {
                let now = Instant::now();
                let bytes = total.load(Ordering::Relaxed);
                let mbps = interval_throughput(bytes.saturating_sub(previous_bytes), now.duration_since(previous_at));
                samples.push(SpeedSample { at: Utc::now().timestamp_millis(), download_mbps: Some(mbps), upload_mbps: None, latency_ms: None });
                context.emit("speed", "download", "running", format!("实时下载 {mbps:.2} Mbps"), ((now.duration_since(started).as_secs_f64() / seconds as f64) * 100.0).min(100.0) as u32, 100, bytes, vec![ProgressPoint { at: Utc::now().timestamp_millis(), kind: PointKind::Download, value: mbps, sequence: None }]);
                previous_bytes = bytes;
                previous_at = now;
            }
            _ = tokio::time::sleep_until(deadline) => break false,
        }
    };
    tasks.abort_all();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Err(error)) if !cancelled => warnings.push(format!("下载连接失败：{error}")),
            Err(error) if !error.is_cancelled() && !cancelled => {
                warnings.push(format!("下载任务失败：{error}"))
            }
            _ => {}
        }
    }
    let bytes = total.load(Ordering::Relaxed);
    TransferResult {
        bytes,
        elapsed: started.elapsed(),
        samples,
        warnings,
        cancelled,
    }
}

async fn download_connection(
    client: reqwest::Client,
    url: url::Url,
    deadline: Instant,
    cancel: CancellationToken,
    total: Arc<AtomicU64>,
) -> Result<(), String> {
    while Instant::now() < deadline && !cancel.is_cancelled() {
        let request = client
            .get(url.clone())
            .query(&[("ckSize", "100")])
            .query(&[(
                "r",
                Utc::now()
                    .timestamp_nanos_opt()
                    .unwrap_or_default()
                    .to_string(),
            )])
            .send();
        let mut response = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            result = tokio::time::timeout_at(deadline, request) => match result {
                Ok(result) => result.map_err(|error| error.to_string())?,
                Err(_) => return Ok(()),
            },
        };
        if !response.status().is_success() {
            return Err(format!("节点返回 HTTP {}", response.status()));
        }
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = tokio::time::timeout_at(deadline, response.chunk()) => match result {
                    Ok(result) => result,
                    Err(_) => return Ok(()),
                },
            };
            match chunk {
                Ok(Some(chunk)) => {
                    total.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
                Ok(None) => break,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    Ok(())
}

async fn upload_test(
    client: &reqwest::Client,
    url: url::Url,
    seconds: u64,
    connections: u32,
    context: &NetworkContext,
) -> TransferResult {
    let total = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let mut tasks = JoinSet::new();
    for _ in 0..connections {
        tasks.spawn(upload_connection(
            client.clone(),
            url.clone(),
            deadline,
            context.cancel.clone(),
            total.clone(),
        ));
    }
    let mut samples = Vec::new();
    let mut warnings = Vec::new();
    let mut previous_bytes = 0;
    let mut previous_at = started;
    let mut interval = tokio::time::interval_at(started + SAMPLE_INTERVAL, SAMPLE_INTERVAL);
    let cancelled = loop {
        if tasks.is_empty() {
            break false;
        }
        tokio::select! {
            _ = context.cancel.cancelled() => break true,
            joined = tasks.join_next() => {
                match joined {
                    Some(Ok(Err(error))) => warnings.push(format!("上传连接失败：{error}")),
                    Some(Err(error)) if !error.is_cancelled() => warnings.push(format!("上传任务失败：{error}")),
                    _ => {}
                }
            }
            _ = interval.tick() => {
                let now = Instant::now();
                let bytes = total.load(Ordering::Relaxed);
                let mbps = interval_throughput(bytes.saturating_sub(previous_bytes), now.duration_since(previous_at));
                samples.push(SpeedSample { at: Utc::now().timestamp_millis(), download_mbps: None, upload_mbps: Some(mbps), latency_ms: None });
                context.emit("speed", "upload", "running", format!("实时上传 {mbps:.2} Mbps"), ((now.duration_since(started).as_secs_f64() / seconds as f64) * 100.0).min(100.0) as u32, 100, bytes, vec![ProgressPoint { at: Utc::now().timestamp_millis(), kind: PointKind::Upload, value: mbps, sequence: None }]);
                previous_bytes = bytes;
                previous_at = now;
            }
            _ = tokio::time::sleep_until(deadline) => break false,
        }
    };
    tasks.abort_all();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Err(error)) if !cancelled => warnings.push(format!("上传连接失败：{error}")),
            Err(error) if !error.is_cancelled() && !cancelled => {
                warnings.push(format!("上传任务失败：{error}"))
            }
            _ => {}
        }
    }
    let bytes = total.load(Ordering::Relaxed);
    TransferResult {
        bytes,
        elapsed: started.elapsed(),
        samples,
        warnings,
        cancelled,
    }
}

async fn upload_connection(
    client: reqwest::Client,
    url: url::Url,
    deadline: Instant,
    cancel: CancellationToken,
    total: Arc<AtomicU64>,
) -> Result<(), String> {
    while Instant::now() < deadline && !cancel.is_cancelled() {
        // Count only completed requests: generated buffers are not evidence of uploaded bytes.
        let request = client
            .post(url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(vec![0; UPLOAD_CHUNK])
            .send();
        let response = tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            result = tokio::time::timeout_at(deadline + Duration::from_secs(3), request) => match result {
                Ok(result) => result.map_err(|error| error.to_string())?,
                Err(_) => return Ok(()),
            },
        };
        if !response.status().is_success() {
            return Err(format!("节点返回 HTTP {}", response.status()));
        }
        total.fetch_add(UPLOAD_CHUNK as u64, Ordering::Relaxed);
    }
    Ok(())
}

fn throughput(bytes: u64, elapsed: Duration) -> f64 {
    if elapsed.is_zero() {
        0.0
    } else {
        round(bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0)
    }
}

fn interval_throughput(bytes: u64, elapsed: Duration) -> f64 {
    if elapsed.is_zero() {
        0.0
    } else {
        round(bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0)
    }
}

fn round(value: f64) -> f64 {
    if value.is_finite() {
        (value * 1000.0).round() / 1000.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_and_custom_nodes_are_validated() {
        assert_eq!(builtins().len(), 3);
        let custom =
            select_node(None, Some("https://speed.example.com/backend".into()), None).unwrap();
        assert_eq!(custom.url, "https://speed.example.com/backend/");
        assert!(select_node(None, Some("file:///tmp/backend".into()), None).is_err());
    }

    #[test]
    fn throughput_uses_actual_bytes_and_elapsed_time() {
        assert_eq!(throughput(1_000_000, Duration::from_secs(1)), 8.0);
        assert_eq!(throughput(0, Duration::from_secs(6)), 0.0);
        assert_eq!(throughput(100, Duration::ZERO), 0.0);
    }
}
