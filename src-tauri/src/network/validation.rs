use super::types::{NetworkTestRequest, PingProtocol, SpeedNode};
use std::net::IpAddr;
use url::Url;

pub fn validate_task_id(task_id: &str) -> Result<(), String> {
    if task_id.is_empty() || task_id.len() > 128 {
        return Err("taskId 长度应为 1 到 128 个字符".to_string());
    }
    if !task_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("taskId 仅允许字母、数字、连字符和下划线".to_string());
    }
    Ok(())
}

pub fn validate_host(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() || value.len() > 253 {
        return Err("目标不能为空且长度不能超过 253 个字符".to_string());
    }
    if value != input
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("目标不能包含空白或控制字符".to_string());
    }
    if value.contains([
        '/', '\\', '@', '?', '#', ',', ';', '&', '|', '$', '`', '\'', '"', '[', ']',
    ]) {
        return Err("目标仅允许单个域名或 IP，不允许网段、URL 或命令字符".to_string());
    }
    if let Ok(address) = value.parse::<IpAddr>() {
        return Ok(address.to_string());
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
        || value.contains(':')
    {
        return Err("IP 地址格式无效，且不支持网段".to_string());
    }
    let name = value.strip_suffix('.').unwrap_or(value);
    if name.is_empty()
        || name.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err("域名格式无效，仅支持单域名的字母、数字、点和连字符".to_string());
    }
    Ok(name.to_ascii_lowercase())
}

pub fn validate_http_url(input: &str) -> Result<Url, String> {
    if input.trim() != input
        || input
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("URL 不能包含空白或控制字符".to_string());
    }
    let url = Url::parse(input).map_err(|_| "URL 格式无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("URL 仅支持 HTTP 或 HTTPS".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL 不允许包含用户名或密码".to_string());
    }
    match url.host() {
        Some(url::Host::Domain(host)) => {
            validate_host(host)?;
        }
        Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_)) => {}
        None => return Err("URL 必须包含单个主机".to_string()),
    }
    if url.cannot_be_a_base() {
        return Err("URL 必须是可请求的 HTTP(S) 地址".to_string());
    }
    Ok(url)
}

pub fn validate_port(port: Option<u16>, required: bool) -> Result<Option<u16>, String> {
    match port {
        Some(0) => Err("端口必须在 1 到 65535 之间，且不支持端口范围".to_string()),
        None if required => Err("TCP Ping 必须提供一个 port 参数".to_string()),
        value => Ok(value),
    }
}

pub fn validate_speed_node(node: &SpeedNode) -> Result<SpeedNode, String> {
    let mut node = node.clone();
    node.url = validate_http_url(&node.url)?.to_string();
    if node.name.trim().is_empty() {
        node.name = "自定义节点".to_string();
    }
    if node.operator.trim().is_empty() {
        node.operator = "自定义".to_string();
    }
    if node.region.trim().is_empty() {
        node.region = "未知".to_string();
    }
    for value in [&node.name, &node.operator, &node.region] {
        if value.len() > 128 || value.chars().any(char::is_control) {
            return Err("测速节点元数据无效".to_string());
        }
    }
    Ok(node)
}

pub fn validate_request(request: &NetworkTestRequest) -> Result<(), String> {
    match request {
        NetworkTestRequest::Speed { node, node_url, .. } => {
            if let Some(node) = node {
                validate_speed_node(node)?;
            }
            if let Some(url) = node_url {
                validate_http_url(url)?;
            }
        }
        NetworkTestRequest::Website {
            url, timeout_ms, ..
        } => {
            validate_http_url(url)?;
            if !(500..=30_000).contains(timeout_ms) {
                return Err("网站测试超时应在 500 到 30000 毫秒之间".to_string());
            }
        }
        NetworkTestRequest::Ping {
            target,
            protocol,
            port,
            count,
            timeout_ms,
        } => {
            validate_host(target)?;
            validate_port(*port, *protocol == PingProtocol::Tcp)?;
            if *protocol == PingProtocol::Icmp && port.is_some() {
                return Err("ICMP Ping 不接受 port 参数".to_string());
            }
            if !(1..=20).contains(count) {
                return Err("Ping 次数应在 1 到 20 之间".to_string());
            }
            if !(100..=10_000).contains(timeout_ms) {
                return Err("Ping 超时应在 100 到 10000 毫秒之间".to_string());
            }
        }
        NetworkTestRequest::Trace {
            target,
            max_hops,
            timeout_ms,
        } => {
            validate_host(target)?;
            if !(1..=64).contains(max_hops) {
                return Err("路由跳数应在 1 到 64 之间".to_string());
            }
            if !(100..=10_000).contains(timeout_ms) {
                return Err("路由单跳超时应在 100 到 10000 毫秒之间".to_string());
            }
        }
        NetworkTestRequest::Dns {
            domain,
            server,
            timeout_ms,
            ..
        } => {
            let domain = validate_host(domain)?;
            if domain.parse::<IpAddr>().is_ok() {
                return Err("DNS 测试的 domain 必须是域名，不是 IP".to_string());
            }
            if let Some(server) = server {
                if server.parse::<IpAddr>().is_err() {
                    return Err("自定义 DNS server 必须是单个 IP 地址".to_string());
                }
            }
            if !(500..=15_000).contains(timeout_ms) {
                return Err("DNS 超时应在 500 到 15000 毫秒之间".to_string());
            }
        }
        NetworkTestRequest::PublicIp { .. } | NetworkTestRequest::Ipv6 { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_validation_rejects_ranges_and_shell_syntax() {
        for value in [
            "192.168.1.0/24",
            "1.1.1.1,8.8.8.8",
            "example.com;calc",
            "example.com|whoami",
            "http://example.com",
            "999.1.1.1",
            "bad_host.example",
            "example..com",
            "-bad.example",
        ] {
            assert!(validate_host(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn host_validation_accepts_one_domain_or_ip() {
        assert_eq!(
            validate_host("WWW.Example.COM.").unwrap(),
            "www.example.com"
        );
        assert_eq!(
            validate_host("2001:4860:4860::8888").unwrap(),
            "2001:4860:4860::8888"
        );
        assert_eq!(validate_host("223.5.5.5").unwrap(), "223.5.5.5");
    }

    #[test]
    fn url_validation_is_http_only_and_rejects_credentials() {
        assert!(validate_http_url("https://example.com/a?b=c").is_ok());
        assert!(validate_http_url("http://[::1]:8080/health").is_ok());
        for value in [
            "file:///C:/test",
            "ftp://example.com",
            "https://user:pass@example.com",
            "https://example.com/a b",
        ] {
            assert!(validate_http_url(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn port_validation_rejects_zero_and_requires_tcp_port() {
        assert!(validate_port(Some(0), true).is_err());
        assert!(validate_port(None, true).is_err());
        assert_eq!(validate_port(Some(443), true).unwrap(), Some(443));
    }

    #[test]
    fn request_deserializes_camel_case_and_rejects_bad_ranges() {
        let request: NetworkTestRequest = serde_json::from_str(r#"{"tool":"ping","target":"1.1.1.1","protocol":"tcp","port":443,"count":4,"timeoutMs":1000}"#).unwrap();
        assert!(validate_request(&request).is_ok());
        let bad: NetworkTestRequest =
            serde_json::from_str(r#"{"tool":"trace","target":"1.1.1.1","maxHops":500}"#).unwrap();
        assert!(validate_request(&bad).is_err());
        let icmp_with_port = serde_json::from_str::<NetworkTestRequest>(
            r#"{"tool":"ping","target":"1.1.1.1","port":1,"protocol":"icmp"}"#,
        )
        .unwrap();
        assert!(validate_request(&icmp_with_port).is_err());
        let dns = serde_json::from_str::<NetworkTestRequest>(
            r#"{"tool":"dns","domain":"example.com","recordType":"AAAA","transport":"UDP","compareSystem":false}"#
        ).unwrap();
        assert!(validate_request(&dns).is_ok());
        assert!(matches!(
            dns,
            NetworkTestRequest::Dns {
                compare_system: false,
                ..
            }
        ));
    }
}
