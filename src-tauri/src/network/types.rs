use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkPath {
    #[default]
    System,
    Direct,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpeedProfile {
    #[default]
    Standard,
    Precise,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PingProtocol {
    #[default]
    Icmp,
    Tcp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsTransport {
    #[default]
    Udp,
    Tcp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    #[default]
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    NS,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SpeedNode {
    pub name: String,
    pub url: String,
    pub operator: String,
    pub region: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "tool",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NetworkTestRequest {
    Speed {
        #[serde(default)]
        node: Option<SpeedNode>,
        #[serde(default, alias = "baseUrl", alias = "serverUrl")]
        node_url: Option<String>,
        #[serde(default)]
        node_name: Option<String>,
        #[serde(default)]
        profile: SpeedProfile,
        #[serde(default)]
        path: NetworkPath,
    },
    Website {
        url: String,
        #[serde(default)]
        path: NetworkPath,
        #[serde(default = "default_website_timeout")]
        timeout_ms: u64,
    },
    PublicIp {
        #[serde(default)]
        path: NetworkPath,
    },
    Ping {
        target: String,
        #[serde(default)]
        protocol: PingProtocol,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default = "default_ping_count")]
        count: u32,
        #[serde(default = "default_ping_timeout")]
        timeout_ms: u64,
    },
    Trace {
        target: String,
        #[serde(default = "default_max_hops")]
        max_hops: u32,
        #[serde(default = "default_ping_timeout")]
        timeout_ms: u64,
    },
    Dns {
        domain: String,
        #[serde(default)]
        record_type: DnsRecordType,
        #[serde(default)]
        transport: DnsTransport,
        #[serde(default)]
        server: Option<String>,
        #[serde(default = "default_compare_system")]
        compare_system: bool,
        #[serde(default = "default_dns_timeout")]
        timeout_ms: u64,
    },
    Ipv6 {
        #[serde(default)]
        path: NetworkPath,
    },
}

const fn default_website_timeout() -> u64 {
    10_000
}
const fn default_ping_count() -> u32 {
    4
}
const fn default_ping_timeout() -> u64 {
    1_000
}
const fn default_max_hops() -> u32 {
    30
}
const fn default_dns_timeout() -> u64 {
    5_000
}
const fn default_compare_system() -> bool {
    true
}

impl NetworkTestRequest {
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Speed { .. } => "speed",
            Self::Website { .. } => "website",
            Self::PublicIp { .. } => "publicIp",
            Self::Ping { .. } => "ping",
            Self::Trace { .. } => "trace",
            Self::Dns { .. } => "dns",
            Self::Ipv6 { .. } => "ipv6",
        }
    }

    pub fn target(&self) -> Option<String> {
        match self {
            Self::Speed { node, node_url, .. } => node
                .as_ref()
                .map(|node| node.url.clone())
                .or_else(|| node_url.clone()),
            Self::Website { url, .. } => Some(url.clone()),
            Self::Ping { target, .. } | Self::Trace { target, .. } => Some(target.clone()),
            Self::Dns { domain, .. } => Some(domain.clone()),
            Self::PublicIp { .. } | Self::Ipv6 { .. } => None,
        }
    }

    pub fn path(&self) -> Option<NetworkPath> {
        match self {
            Self::Speed { path, .. }
            | Self::Website { path, .. }
            | Self::PublicIp { path, .. }
            | Self::Ipv6 { path, .. } => Some(*path),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Success,
    Warning,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticStatus {
    Normal,
    Warning,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StageStatus {
    Success,
    Warning,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTestResult {
    pub task_id: String,
    pub tool: String,
    pub status: TestStatus,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<NetworkPath>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiagnosticReport {
    pub task_id: String,
    pub status: DiagnosticStatus,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub results: Vec<NetworkTestResult>,
    pub findings: Vec<DiagnosticFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFinding {
    pub severity: FindingSeverity,
    pub title: String,
    pub detail: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProgress {
    pub task_id: String,
    pub tool: String,
    pub phase: String,
    pub status: String,
    pub message: String,
    pub completed: u32,
    pub total: u32,
    pub bytes: u64,
    pub points: Vec<ProgressPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPoint {
    pub at: i64,
    pub kind: PointKind,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    Download,
    Upload,
    Latency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedSample {
    pub at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedData {
    pub node: SpeedNode,
    pub profile: SpeedProfile,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub bytes_downloaded: u64,
    pub bytes_uploaded: u64,
    pub samples: Vec<SpeedSample>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteStage {
    pub name: String,
    pub label: String,
    pub status: StageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteRedirect {
    pub from: String,
    pub to: String,
    pub status_code: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteData {
    pub url: String,
    pub final_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_address: Option<String>,
    pub stages: Vec<WebsiteStage>,
    pub redirects: Vec<WebsiteRedirect>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicIpInfo {
    pub address: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicIpData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4: Option<PublicIpInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<PublicIpInfo>,
    pub source: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingSample {
    pub sequence: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingData {
    pub protocol: PingProtocol,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub sent: u32,
    pub received: u32,
    pub loss_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jitter_ms: Option<f64>,
    pub samples: Vec<PingSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceHop {
    pub hop: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    pub latencies_ms: Vec<Option<f64>>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceData {
    pub target: String,
    pub max_hops: u32,
    pub hops: Vec<TraceHop>,
    pub reached_target: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsAnswer {
    pub name: String,
    pub record_type: String,
    pub ttl: u32,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsQuery {
    pub server: String,
    pub transport: DnsTransport,
    pub duration_ms: f64,
    pub status: StageStatus,
    pub answers: Vec<DnsAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsData {
    pub domain: String,
    pub record_type: DnsRecordType,
    pub transport: DnsTransport,
    pub queries: Vec<DnsQuery>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ipv6Step {
    pub key: String,
    pub label: String,
    pub status: StageStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ipv6Data {
    pub available: bool,
    pub global_addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_address: Option<String>,
    pub steps: Vec<Ipv6Step>,
    pub warnings: Vec<String>,
}

pub(crate) struct ToolOutcome {
    pub status: TestStatus,
    pub summary: String,
    pub error: Option<String>,
    pub data: Value,
}

impl ToolOutcome {
    pub fn success(summary: impl Into<String>, data: impl Serialize) -> Self {
        Self {
            status: TestStatus::Success,
            summary: summary.into(),
            error: None,
            data: serde_json::to_value(data).unwrap_or(Value::Null),
        }
    }

    pub fn warning(summary: impl Into<String>, data: impl Serialize) -> Self {
        Self {
            status: TestStatus::Warning,
            summary: summary.into(),
            error: None,
            data: serde_json::to_value(data).unwrap_or(Value::Null),
        }
    }

    pub fn error(
        summary: impl Into<String>,
        error: impl Into<String>,
        data: impl Serialize,
    ) -> Self {
        Self {
            status: TestStatus::Error,
            summary: summary.into(),
            error: Some(error.into()),
            data: serde_json::to_value(data).unwrap_or(Value::Null),
        }
    }

    pub fn cancelled(summary: impl Into<String>) -> Self {
        Self {
            status: TestStatus::Cancelled,
            summary: summary.into(),
            error: None,
            data: Value::Null,
        }
    }
}
