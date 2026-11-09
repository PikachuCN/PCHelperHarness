use super::types::{
    NetworkPath, StageStatus, ToolOutcome, WebsiteData, WebsiteRedirect, WebsiteStage,
};
use super::validation::validate_http_url;
use super::NetworkContext;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{lookup_host, TcpStream};

pub async fn run(
    url: String,
    path: NetworkPath,
    timeout_ms: u64,
    context: &NetworkContext,
) -> ToolOutcome {
    let original = match validate_http_url(&url) {
        Ok(url) => url,
        Err(error) => {
            return ToolOutcome::error("网站地址无效", error, serde_json::json!({"warnings":[]}))
        }
    };
    let original_text = original.to_string();
    let host = match original.host() {
        Some(url::Host::Domain(host)) => host.to_string(),
        Some(url::Host::Ipv4(address)) => address.to_string(),
        Some(url::Host::Ipv6(address)) => address.to_string(),
        None => {
            return ToolOutcome::error(
                "网站地址无效",
                "URL 缺少主机",
                serde_json::json!({"warnings":[]}),
            )
        }
    };
    let port = original.port_or_known_default().unwrap_or(80);
    let timeout = Duration::from_millis(timeout_ms);
    let mut data = WebsiteData {
        url: original_text,
        final_url: original.to_string(),
        status_code: None,
        remote_address: None,
        stages: Vec::new(),
        redirects: Vec::new(),
        warnings: Vec::new(),
    };
    context.emit(
        "website",
        "dns",
        "running",
        format!("正在解析 {host}"),
        0,
        4,
        0,
        Vec::new(),
    );

    let started = Instant::now();
    let resolved = tokio::select! {
        _ = context.cancel.cancelled() => return ToolOutcome::cancelled("网站测试已取消"),
        result = tokio::time::timeout(timeout, lookup_host((host.as_str(), port))) => result,
    };
    let addresses: Vec<SocketAddr> = match resolved {
        Ok(Ok(addresses)) => addresses.take(16).collect(),
        Ok(Err(error)) => {
            data.stages.push(stage(
                "dns",
                "DNS 解析",
                StageStatus::Error,
                started,
                Some(error.to_string()),
            ));
            skip_remaining(
                &mut data.stages,
                original.scheme() == "https",
                "DNS 解析失败",
            );
            return ToolOutcome::error("网站 DNS 解析失败", error.to_string(), data);
        }
        Err(_) => {
            data.stages.push(stage(
                "dns",
                "DNS 解析",
                StageStatus::Error,
                started,
                Some("DNS 解析超时".to_string()),
            ));
            skip_remaining(
                &mut data.stages,
                original.scheme() == "https",
                "DNS 解析超时",
            );
            return ToolOutcome::error("网站 DNS 解析超时", "DNS 查询超过设定时间", data);
        }
    };
    if addresses.is_empty() {
        data.stages.push(stage(
            "dns",
            "DNS 解析",
            StageStatus::Error,
            started,
            Some("未返回地址".to_string()),
        ));
        skip_remaining(&mut data.stages, original.scheme() == "https", "无可用地址");
        return ToolOutcome::error("网站 DNS 未返回地址", "DNS 应答为空", data);
    }
    data.remote_address = addresses.first().map(|address| address.ip().to_string());
    data.stages.push(stage(
        "dns",
        "DNS 解析",
        StageStatus::Success,
        started,
        Some(
            addresses
                .iter()
                .map(SocketAddr::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ));
    context.emit(
        "website",
        "tcp",
        "running",
        "DNS 解析完成，正在测量 TCP 连接",
        1,
        4,
        0,
        Vec::new(),
    );

    let started = Instant::now();
    let tcp = tokio::select! {
        _ = context.cancel.cancelled() => return ToolOutcome::cancelled("网站测试已取消"),
        result = tokio::time::timeout(timeout, TcpStream::connect(addresses[0])) => result,
    };
    match tcp {
        Ok(Ok(stream)) => {
            drop(stream);
            data.stages.push(stage(
                "tcp",
                "TCP 连接",
                StageStatus::Success,
                started,
                Some(addresses[0].to_string()),
            ));
        }
        Ok(Err(error)) => {
            data.stages.push(stage(
                "tcp",
                "TCP 连接",
                StageStatus::Error,
                started,
                Some(error.to_string()),
            ));
            skip_tls_http(
                &mut data.stages,
                original.scheme() == "https",
                "TCP 连接失败",
            );
            return ToolOutcome::error("网站 TCP 连接失败", error.to_string(), data);
        }
        Err(_) => {
            data.stages.push(stage(
                "tcp",
                "TCP 连接",
                StageStatus::Error,
                started,
                Some("TCP 连接超时".to_string()),
            ));
            skip_tls_http(
                &mut data.stages,
                original.scheme() == "https",
                "TCP 连接超时",
            );
            return ToolOutcome::error("网站 TCP 连接超时", "TCP 连接超过设定时间", data);
        }
    }

    if original.scheme() == "https" {
        context.emit(
            "website",
            "tls",
            "running",
            "TCP 连接完成，正在进行独立 TLS 握手计时",
            2,
            4,
            0,
            Vec::new(),
        );
        let started = Instant::now();
        let handshake = tls_handshake(host.clone(), addresses[0]);
        let result = tokio::select! {
            _ = context.cancel.cancelled() => return ToolOutcome::cancelled("网站测试已取消"),
            result = tokio::time::timeout(timeout, handshake) => result,
        };
        match result {
            Ok(Ok(())) => data.stages.push(stage(
                "tls",
                "TLS 握手",
                StageStatus::Success,
                started,
                Some("证书链和主机名验证通过".to_string()),
            )),
            Ok(Err(error)) => {
                data.stages.push(stage(
                    "tls",
                    "TLS 握手",
                    StageStatus::Error,
                    started,
                    Some(error.clone()),
                ));
                data.stages
                    .push(skipped("http", "HTTP 请求", "TLS 握手失败"));
                return ToolOutcome::error("网站 TLS 握手失败", error, data);
            }
            Err(_) => {
                data.stages.push(stage(
                    "tls",
                    "TLS 握手",
                    StageStatus::Error,
                    started,
                    Some("TLS 握手超时".to_string()),
                ));
                data.stages
                    .push(skipped("http", "HTTP 请求", "TLS 握手超时"));
                return ToolOutcome::error("网站 TLS 握手超时", "TLS 握手超过设定时间", data);
            }
        }
    } else {
        data.stages
            .push(skipped("tls", "TLS 握手", "HTTP 不需要 TLS"));
    }

    context.emit(
        "website",
        "http",
        "running",
        "正在请求 HTTP 响应头（不读取响应正文）",
        3,
        4,
        0,
        Vec::new(),
    );
    let mut builder = reqwest::Client::builder()
        .connect_timeout(timeout)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("HarnessNetworkDiagnostic/1.0");
    if path == NetworkPath::Direct {
        builder = builder.no_proxy();
    }
    let client = match builder.build() {
        Ok(client) => client,
        Err(error) => return ToolOutcome::error("HTTP 客户端初始化失败", error.to_string(), data),
    };
    let http_started = Instant::now();
    let mut current = original;
    loop {
        if context.cancel.is_cancelled() {
            return ToolOutcome::cancelled("网站测试已取消");
        }
        let request = client.get(current.clone()).send();
        let response = tokio::select! {
            _ = context.cancel.cancelled() => return ToolOutcome::cancelled("网站测试已取消"),
            response = request => response,
        };
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                data.stages.push(stage(
                    "http",
                    "HTTP 请求",
                    StageStatus::Error,
                    http_started,
                    Some(error.to_string()),
                ));
                return ToolOutcome::error("网站 HTTP 请求失败", error.to_string(), data);
            }
        };
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok());
            if let Some(location) = location {
                if data.redirects.len() >= 5 {
                    data.warnings.push("重定向超过 5 次，已停止".to_string());
                    data.stages.push(stage(
                        "http",
                        "HTTP 请求",
                        StageStatus::Error,
                        http_started,
                        Some("重定向次数超过限制".to_string()),
                    ));
                    return ToolOutcome::error("网站重定向过多", "最多允许 5 次重定向", data);
                }
                let next = match current
                    .join(location)
                    .map_err(|error| error.to_string())
                    .and_then(|url| validate_http_url(url.as_str()).map_err(|error| error))
                {
                    Ok(next) => next,
                    Err(error) => {
                        data.stages.push(stage(
                            "http",
                            "HTTP 请求",
                            StageStatus::Error,
                            http_started,
                            Some(error.clone()),
                        ));
                        return ToolOutcome::error("网站返回无效重定向", error, data);
                    }
                };
                data.redirects.push(WebsiteRedirect {
                    from: current.to_string(),
                    to: next.to_string(),
                    status_code: status.as_u16(),
                });
                current = next;
                data.final_url = current.to_string();
                continue;
            }
        }
        data.status_code = Some(status.as_u16());
        let stage_status = if status.is_client_error() || status.is_server_error() {
            StageStatus::Warning
        } else {
            StageStatus::Success
        };
        if stage_status == StageStatus::Warning {
            data.warnings
                .push(format!("HTTP 状态码为 {}", status.as_u16()));
        }
        data.stages.push(stage(
            "http",
            "HTTP 请求",
            stage_status,
            http_started,
            Some(format!("HTTP {}", status.as_u16())),
        ));
        break;
    }
    let summary = format!(
        "网站返回 HTTP {}，共 {} 次重定向",
        data.status_code.unwrap_or(0),
        data.redirects.len()
    );
    if data.warnings.is_empty() {
        ToolOutcome::success(summary, data)
    } else {
        ToolOutcome::warning(summary, data)
    }
}

async fn tls_handshake(host: String, address: SocketAddr) -> Result<(), String> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect(),
    };
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(host).map_err(|_| "TLS 主机名无效".to_string())?;
    let stream = TcpStream::connect(address)
        .await
        .map_err(|error| error.to_string())?;
    let stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|error| error.to_string())?;
    drop(stream);
    Ok(())
}

fn stage(
    name: &str,
    label: &str,
    status: StageStatus,
    started: Instant,
    detail: Option<String>,
) -> WebsiteStage {
    WebsiteStage {
        name: name.to_string(),
        label: label.to_string(),
        status,
        duration_ms: Some(round(started.elapsed().as_secs_f64() * 1000.0)),
        detail,
    }
}

fn skipped(name: &str, label: &str, reason: &str) -> WebsiteStage {
    WebsiteStage {
        name: name.to_string(),
        label: label.to_string(),
        status: StageStatus::Skipped,
        duration_ms: None,
        detail: Some(reason.to_string()),
    }
}

fn skip_tls_http(stages: &mut Vec<WebsiteStage>, https: bool, reason: &str) {
    if https {
        stages.push(skipped("tls", "TLS 握手", reason));
    }
    stages.push(skipped("http", "HTTP 请求", reason));
}

fn skip_remaining(stages: &mut Vec<WebsiteStage>, https: bool, reason: &str) {
    stages.push(skipped("tcp", "TCP 连接", reason));
    skip_tls_http(stages, https, reason);
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_targets_must_remain_http_urls() {
        assert!(validate_http_url("https://example.com/next").is_ok());
        assert!(validate_http_url("file:///C:/Windows/win.ini").is_err());
        assert!(validate_http_url("https://user@example.com/").is_err());
    }
}
