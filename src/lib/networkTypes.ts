export type NetworkToolId = "speed" | "website" | "publicIp" | "ping" | "trace" | "dns" | "ipv6";
export type NetworkRunStatus = "idle" | "running" | "success" | "warning" | "error" | "cancelled";
export type NetworkResultStatus = Exclude<NetworkRunStatus, "idle" | "running">;
export type NetworkPath = "system" | "direct";
export type NetworkSpeedProfile = "standard" | "precise";
export type NetworkRecordType = "A" | "AAAA" | "CNAME" | "MX" | "TXT" | "NS";
export type NetworkDnsTransport = "UDP" | "TCP";

export interface NetworkSpeedNode {
  name: string;
  url: string;
  operator: string;
  region: string;
}

export interface NetworkSpeedSample {
  at: number;
  downloadMbps?: number;
  uploadMbps?: number;
  latencyMs?: number;
}

export interface NetworkSpeedData {
  node: NetworkSpeedNode;
  profile: NetworkSpeedProfile;
  downloadMbps: number;
  uploadMbps: number;
  latencyMs: number;
  jitterMs: number;
  bytesDownloaded: number;
  bytesUploaded: number;
  samples: NetworkSpeedSample[];
  warnings: string[];
}

export type NetworkStageStatus = "success" | "warning" | "error" | "skipped";

export interface NetworkWebsiteStage {
  name: "dns" | "tcp" | "tls" | "http";
  label: string;
  status: NetworkStageStatus;
  durationMs?: number | null;
  detail?: string | null;
}

export interface NetworkWebsiteData {
  url: string;
  finalUrl: string;
  statusCode?: number | null;
  remoteAddress?: string | null;
  stages: NetworkWebsiteStage[];
  redirects: { from: string; to: string; statusCode: number }[];
  warnings: string[];
}

export interface NetworkPublicAddress {
  address: string;
  provider: string;
  asn?: string | null;
  operator?: string | null;
  region?: string | null;
  reverseHost?: string | null;
}

export interface NetworkPublicIpData {
  ipv4?: NetworkPublicAddress | null;
  ipv6?: NetworkPublicAddress | null;
  source: string;
  warnings: string[];
}

export interface NetworkPingSample {
  sequence: number;
  latencyMs?: number | null;
  status: string;
  error?: string | null;
}

export interface NetworkPingData {
  protocol: "icmp" | "tcp";
  target: string;
  port?: number | null;
  sent: number;
  received: number;
  lossPercent: number;
  minMs?: number | null;
  avgMs?: number | null;
  maxMs?: number | null;
  jitterMs?: number | null;
  samples: NetworkPingSample[];
}

export interface NetworkTraceHop {
  hop: number;
  address?: string | null;
  hostname?: string | null;
  latenciesMs: (number | null)[];
  status: string;
}

export interface NetworkTraceData {
  target: string;
  maxHops: number;
  hops: NetworkTraceHop[];
  reachedTarget: boolean;
}

export interface NetworkDnsAnswer {
  name: string;
  recordType: string;
  ttl: number;
  value: string;
}

export interface NetworkDnsQuery {
  server: string;
  transport: NetworkDnsTransport;
  durationMs: number;
  status: string;
  answers: NetworkDnsAnswer[];
  error?: string | null;
}

export interface NetworkDnsData {
  domain: string;
  recordType: NetworkRecordType;
  transport: NetworkDnsTransport;
  queries: NetworkDnsQuery[];
  warnings: string[];
}

export interface NetworkIpv6Data {
  available: boolean;
  globalAddresses: string[];
  publicAddress?: string | null;
  steps: { key: "address" | "dns" | "connectivity" | "publicIp"; label: string; status: NetworkStageStatus; detail: string }[];
  warnings: string[];
}

export interface NetworkToolDataMap {
  speed: NetworkSpeedData;
  website: NetworkWebsiteData;
  publicIp: NetworkPublicIpData;
  ping: NetworkPingData;
  trace: NetworkTraceData;
  dns: NetworkDnsData;
  ipv6: NetworkIpv6Data;
}

interface NetworkResultBase<K extends NetworkToolId> {
  taskId: string;
  tool: K;
  status: NetworkResultStatus;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  target?: string | null;
  path?: NetworkPath | null;
  summary: string;
  error?: string | null;
  data: NetworkToolDataMap[K];
}

export type NetworkTestResult<K extends NetworkToolId = NetworkToolId> = {
  [P in K]: NetworkResultBase<P>;
}[K];

export interface NetworkTestRequestMap {
  speed: { tool: "speed"; nodeUrl?: string; nodeName?: string; profile: NetworkSpeedProfile; path: NetworkPath };
  website: { tool: "website"; url: string; path: NetworkPath };
  publicIp: { tool: "publicIp"; path: NetworkPath };
  ping: { tool: "ping"; target: string; protocol: "icmp" | "tcp"; count: number; timeoutMs: number; port?: number };
  trace: { tool: "trace"; target: string; maxHops: number; timeoutMs: number };
  dns: { tool: "dns"; domain: string; recordType: NetworkRecordType; server?: string; transport: NetworkDnsTransport; compareSystem: boolean };
  ipv6: { tool: "ipv6" };
}

export type NetworkTestRequest<K extends NetworkToolId = NetworkToolId> = NetworkTestRequestMap[K];

export interface NetworkProgressPoint {
  at: number;
  kind: "download" | "upload" | "latency";
  value: number;
  sequence?: number;
}

export interface NetworkProgressEvent {
  taskId: string;
  tool: NetworkToolId | "diagnostic";
  phase: string;
  status: "running" | "success" | "warning" | "error" | "cancelled";
  message: string;
  completed: number;
  total: number;
  bytes: number;
  points: NetworkProgressPoint[];
}

export interface NetworkFinding {
  severity: "info" | "warning" | "error";
  title: string;
  detail: string;
  recommendation: string;
}

export interface NetworkDiagnosticReport {
  taskId: string;
  status: "normal" | "warning" | "error" | "cancelled";
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  results: NetworkTestResult[];
  findings: NetworkFinding[];
}

export interface NetworkToolState<K extends NetworkToolId = NetworkToolId> {
  status: NetworkRunStatus;
  taskId?: string;
  request?: NetworkTestRequest<K>;
  result?: NetworkTestResult<K>;
  error?: string;
  progress?: NetworkProgressEvent;
}

export interface NetworkDiagnosticState {
  status: NetworkRunStatus;
  taskId?: string;
  report?: NetworkDiagnosticReport;
  error?: string;
  progress?: NetworkProgressEvent;
}

export interface NetworkHistoryEntry {
  id: string;
  tool: NetworkToolId;
  target?: string | null;
  completedAt: string;
  status: NetworkResultStatus;
  result: NetworkTestResult;
}
