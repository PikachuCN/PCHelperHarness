use super::types::{NetworkPath, PublicIpData, PublicIpInfo, ToolOutcome};
use super::NetworkContext;
use serde::Deserialize;
use std::net::IpAddr;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct IpInfo {
    org: Option<String>,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
    hostname: Option<String>,
}

pub async fn run(path: NetworkPath, context: &NetworkContext) -> ToolOutcome {
    context.emit(
        "publicIp",
        "query",
        "running",
        "正在查询 IPv4 和 IPv6 公网地址",
        0,
        2,
        0,
        Vec::new(),
    );
    let client = match client(path) {
        Ok(client) => client,
        Err(error) => {
            return ToolOutcome::error(
                "公网 IP 查询初始化失败",
                error,
                serde_json::json!({"warnings":[]}),
            )
        }
    };
    let ipv4_future = fetch_family(
        &client,
        "https://api.ipify.org",
        true,
        context.cancel.clone(),
    );
    let ipv6_future = fetch_family(
        &client,
        "https://api6.ipify.org",
        false,
        context.cancel.clone(),
    );
    let (ipv4, ipv6) = futures_util::future::join(ipv4_future, ipv6_future).await;
    if context.cancel.is_cancelled() {
        return ToolOutcome::cancelled("公网 IP 查询已取消");
    }
    let mut warnings = Vec::new();
    let ipv4 = match ipv4 {
        Ok((info, family_warnings)) => {
            warnings.extend(family_warnings);
            Some(info)
        }
        Err(error) => {
            warnings.push(format!("IPv4 查询失败：{error}"));
            None
        }
    };
    let ipv6 = match ipv6 {
        Ok((info, family_warnings)) => {
            warnings.extend(family_warnings);
            Some(info)
        }
        Err(error) => {
            warnings.push(format!("IPv6 查询失败：{error}"));
            None
        }
    };
    let data = PublicIpData {
        ipv4,
        ipv6,
        source: "api.ipify.org / api6.ipify.org + ipinfo.io".to_string(),
        warnings,
    };
    context.emit(
        "publicIp",
        "query",
        "running",
        "公网地址查询完成",
        2,
        2,
        0,
        Vec::new(),
    );
    if data.ipv4.is_none() && data.ipv6.is_none() {
        ToolOutcome::error("未获取到公网 IP", data.warnings.join("；"), data)
    } else if !data.warnings.is_empty() {
        ToolOutcome::warning("已获取部分公网 IP 信息", data)
    } else {
        ToolOutcome::success("已获取公网 IP 及归属信息", data)
    }
}

fn client(path: NetworkPath) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(2))
        .user_agent("HarnessNetworkDiagnostic/1.0");
    if path == NetworkPath::Direct {
        builder = builder.no_proxy();
    }
    builder.build().map_err(|error| error.to_string())
}

async fn fetch_family(
    client: &reqwest::Client,
    endpoint: &str,
    ipv4: bool,
    cancel: CancellationToken,
) -> Result<(PublicIpInfo, Vec<String>), String> {
    let operation = async {
        let address = fetch_address(client, endpoint, ipv4).await?;
        let mut warnings = Vec::new();
        let info_url = format!("https://ipinfo.io/{address}/json");
        let metadata = async {
            let response = client
                .get(info_url)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                return Err(format!("ipinfo.io 返回 HTTP {}", response.status()));
            }
            let text = read_limited(response, 64 * 1024).await?;
            serde_json::from_str::<IpInfo>(&text)
                .map_err(|error| format!("ipinfo.io 响应无效：{error}"))
        }
        .await;
        let mut info = PublicIpInfo {
            address: address.to_string(),
            provider: "ipify + ipinfo.io".to_string(),
            asn: None,
            operator: None,
            region: None,
            reverse_host: None,
        };
        match metadata {
            Ok(metadata) => {
                if let Some(org) = metadata.org.filter(|value| !value.trim().is_empty()) {
                    let mut parts = org.splitn(2, char::is_whitespace);
                    info.asn = parts
                        .next()
                        .filter(|part| part.starts_with("AS"))
                        .map(str::to_string);
                    info.operator = Some(parts.next().unwrap_or(&org).trim().to_string())
                        .filter(|value| !value.is_empty());
                }
                info.region = Some(
                    [metadata.city, metadata.region, metadata.country]
                        .into_iter()
                        .flatten()
                        .filter(|value| !value.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
                .filter(|value| !value.is_empty());
                info.reverse_host = metadata.hostname.filter(|value| !value.trim().is_empty());
            }
            Err(error) => warnings.push(format!("{address} 归属信息查询失败：{error}")),
        }
        Ok((info, warnings))
    };
    tokio::select! {
        _ = cancel.cancelled() => Err("查询已取消".to_string()),
        result = operation => result,
    }
}

async fn fetch_address(
    client: &reqwest::Client,
    endpoint: &str,
    ipv4: bool,
) -> Result<IpAddr, String> {
    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("地址服务返回 HTTP {}", response.status()));
    }
    let text = read_limited(response, 1024).await?;
    let address = text
        .trim()
        .parse::<IpAddr>()
        .map_err(|_| "地址服务未返回有效 IP".to_string())?;
    if address.is_ipv4() != ipv4 {
        return Err("地址服务返回了错误的 IP 协议族".to_string());
    }
    Ok(address)
}

async fn read_limited(mut response: reqwest::Response, limit: usize) -> Result<String, String> {
    if response
        .content_length()
        .map(|length| length > limit as u64)
        .unwrap_or(false)
    {
        return Err("响应超过大小限制".to_string());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("响应超过大小限制".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "响应不是有效 UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipinfo_fields_are_optional() {
        let info: IpInfo =
            serde_json::from_str(r#"{"org":"AS4134 Chinanet","city":"Hangzhou","country":"CN"}"#)
                .unwrap();
        assert_eq!(info.org.as_deref(), Some("AS4134 Chinanet"));
        assert!(info.hostname.is_none());
    }
}
