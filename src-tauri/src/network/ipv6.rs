use super::process::{process_error, result_json, run_powershell};
use super::types::{Ipv6Data, Ipv6Step, NetworkPath, StageStatus, ToolOutcome};
use super::NetworkContext;
use std::net::{Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::net::{lookup_host, TcpStream};

const ADDRESS_SCRIPT: &str = r#"
$addresses = @(Get-NetIPAddress -AddressFamily IPv6 -ErrorAction Stop | Select-Object -ExpandProperty IPAddress)
'HARNESS_RESULT:' + ([pscustomobject]@{ addresses = @($addresses) } | ConvertTo-Json -Compress)
"#;

pub async fn run(path: NetworkPath, context: &NetworkContext) -> ToolOutcome {
    context.emit(
        "ipv6",
        "address",
        "running",
        "正在读取本机 IPv6 地址（只读）",
        0,
        4,
        0,
        Vec::new(),
    );
    let output = run_powershell(
        ADDRESS_SCRIPT,
        &serde_json::json!({}),
        Duration::from_secs(8),
        context.cancel.clone(),
        None,
    )
    .await;
    let mut steps = Vec::new();
    let mut global_addresses = Vec::new();
    match output {
        Ok(output) if output.cancelled || context.cancel.is_cancelled() => {
            return ToolOutcome::cancelled("IPv6 检测已取消")
        }
        Ok(output) => match result_json(&output.stdout) {
            Ok(value) => {
                global_addresses = value
                    .get("addresses")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .filter_map(|value| value.parse::<Ipv6Addr>().ok())
                    .filter(|address| is_global(address))
                    .map(|address| address.to_string())
                    .collect();
                global_addresses.sort();
                global_addresses.dedup();
                let (status, detail) = if global_addresses.is_empty() {
                    (
                        StageStatus::Warning,
                        "未发现全局 IPv6 地址；fe80:: 链路本地地址不计入".to_string(),
                    )
                } else {
                    (
                        StageStatus::Success,
                        format!("发现 {} 个全局 IPv6 地址", global_addresses.len()),
                    )
                };
                steps.push(Ipv6Step {
                    key: "address".into(),
                    label: "本机 IPv6 地址".into(),
                    status,
                    detail,
                });
            }
            Err(error) => steps.push(Ipv6Step {
                key: "address".into(),
                label: "本机 IPv6 地址".into(),
                status: StageStatus::Error,
                detail: process_error(&output).unwrap_or(error),
            }),
        },
        Err(error) => steps.push(Ipv6Step {
            key: "address".into(),
            label: "本机 IPv6 地址".into(),
            status: StageStatus::Error,
            detail: error,
        }),
    }

    context.emit(
        "ipv6",
        "dns",
        "running",
        "正在检查 IPv6 DNS 记录",
        1,
        4,
        0,
        Vec::new(),
    );
    let resolved = tokio::select! {
        _ = context.cancel.cancelled() => return ToolOutcome::cancelled("IPv6 检测已取消"),
        result = tokio::time::timeout(Duration::from_secs(5), lookup_host(("ipv6.google.com", 80))) => result,
    };
    let mut ipv6_routes = Vec::<SocketAddr>::new();
    match resolved {
        Ok(Ok(addresses)) => {
            ipv6_routes = addresses
                .filter(|address| address.is_ipv6())
                .take(3)
                .collect();
            if ipv6_routes.is_empty() {
                steps.push(Ipv6Step {
                    key: "dns".into(),
                    label: "IPv6 DNS 解析".into(),
                    status: StageStatus::Warning,
                    detail: "ipv6.google.com 未返回 AAAA 地址".into(),
                });
            } else {
                steps.push(Ipv6Step {
                    key: "dns".into(),
                    label: "IPv6 DNS 解析".into(),
                    status: StageStatus::Success,
                    detail: ipv6_routes
                        .iter()
                        .map(SocketAddr::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
        }
        Ok(Err(error)) => steps.push(Ipv6Step {
            key: "dns".into(),
            label: "IPv6 DNS 解析".into(),
            status: StageStatus::Warning,
            detail: error.to_string(),
        }),
        Err(_) => steps.push(Ipv6Step {
            key: "dns".into(),
            label: "IPv6 DNS 解析".into(),
            status: StageStatus::Warning,
            detail: "DNS 查询超时".into(),
        }),
    }

    context.emit(
        "ipv6",
        "connectivity",
        "running",
        "正在测试真实 IPv6 TCP 连接",
        2,
        4,
        0,
        Vec::new(),
    );
    let has_global = !global_addresses.is_empty();
    let mut connected = false;
    if !has_global || ipv6_routes.is_empty() {
        steps.push(Ipv6Step {
            key: "connectivity".into(),
            label: "IPv6 连接".into(),
            status: StageStatus::Skipped,
            detail: if !has_global {
                "没有全局 IPv6 地址，跳过外网连接".into()
            } else {
                "没有可用 AAAA 地址，跳过连接".into()
            },
        });
    } else {
        let mut last_error = "连接失败".to_string();
        for address in ipv6_routes {
            let attempt = tokio::select! {
                _ = context.cancel.cancelled() => return ToolOutcome::cancelled("IPv6 检测已取消"),
                result = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(address)) => result,
            };
            match attempt {
                Ok(Ok(stream)) => {
                    drop(stream);
                    connected = true;
                    break;
                }
                Ok(Err(error)) => last_error = error.to_string(),
                Err(_) => last_error = "连接超时".to_string(),
            }
        }
        steps.push(Ipv6Step {
            key: "connectivity".into(),
            label: "IPv6 连接".into(),
            status: if connected {
                StageStatus::Success
            } else {
                StageStatus::Warning
            },
            detail: if connected {
                "已通过 IPv6 建立 TCP 连接".into()
            } else {
                last_error
            },
        });
    }

    context.emit(
        "ipv6",
        "publicIp",
        "running",
        "正在查询公网 IPv6 地址",
        3,
        4,
        0,
        Vec::new(),
    );
    let public_address = if has_global && connected {
        match fetch_public_ipv6(path, context).await {
            Ok(address) => {
                steps.push(Ipv6Step {
                    key: "publicIp".into(),
                    label: "公网 IPv6".into(),
                    status: StageStatus::Success,
                    detail: address.clone(),
                });
                Some(address)
            }
            Err(error) => {
                steps.push(Ipv6Step {
                    key: "publicIp".into(),
                    label: "公网 IPv6".into(),
                    status: StageStatus::Warning,
                    detail: error,
                });
                None
            }
        }
    } else {
        steps.push(Ipv6Step {
            key: "publicIp".into(),
            label: "公网 IPv6".into(),
            status: StageStatus::Skipped,
            detail: "IPv6 外网连接不可用，未伪造公网地址".into(),
        });
        None
    };
    if context.cancel.is_cancelled() {
        return ToolOutcome::cancelled("IPv6 检测已取消");
    }
    let available = connected && public_address.is_some();
    let warnings = steps
        .iter()
        .filter(|step| matches!(step.status, StageStatus::Warning | StageStatus::Error))
        .map(|step| format!("{}：{}", step.label, step.detail))
        .collect::<Vec<_>>();
    let data = Ipv6Data {
        available,
        global_addresses,
        public_address,
        steps,
        warnings,
    };
    if available {
        ToolOutcome::success("IPv6 地址、连接和公网地址均可用", data)
    } else {
        ToolOutcome::warning("IPv6 未完全可用，详见各检测步骤", data)
    }
}

async fn fetch_public_ipv6(path: NetworkPath, context: &NetworkContext) -> Result<String, String> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(7))
        .redirect(reqwest::redirect::Policy::limited(1));
    if path == NetworkPath::Direct {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|error| error.to_string())?;
    let operation = async {
        let response = client
            .get("https://api6.ipify.org")
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("地址服务返回 HTTP {}", response.status()));
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            if bytes.len().saturating_add(chunk.len()) > 1024 {
                return Err("地址服务响应过长".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(bytes).map_err(|_| "地址服务响应不是 UTF-8".to_string())?;
        let address = text
            .trim()
            .parse::<Ipv6Addr>()
            .map_err(|_| "地址服务未返回 IPv6 地址".to_string())?;
        if !is_global(&address) {
            return Err("返回的 IPv6 不是全局单播地址".to_string());
        }
        Ok(address.to_string())
    };
    tokio::select! { _ = context.cancel.cancelled() => Err("查询已取消".into()), result = operation => result }
}

fn is_global(address: &Ipv6Addr) -> bool {
    !address.is_loopback()
        && !address.is_multicast()
        && !address.is_unspecified()
        && (address.segments()[0] & 0xe000) == 0x2000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_local_and_unique_local_are_not_global() {
        assert!(!is_global(&"fe80::1234".parse().unwrap()));
        assert!(!is_global(&"fc00::1234".parse().unwrap()));
        assert!(!is_global(&"::1".parse().unwrap()));
        assert!(is_global(&"2001:4860:4860::8888".parse().unwrap()));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn address_template_reads_local_configuration_only() {
        let output = run_powershell(
            ADDRESS_SCRIPT,
            &serde_json::json!({}),
            Duration::from_secs(8),
            tokio_util::sync::CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(output.exit_code, Some(0), "{}", output.stderr);
        let value = result_json(&output.stdout).unwrap();
        assert!(value["addresses"].is_array());
    }
}
