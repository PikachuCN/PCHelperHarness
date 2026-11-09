import { emitMockEvent } from "./events";
import type {
  NetworkDiagnosticReport,
  NetworkProgressPoint,
  NetworkTestRequest,
  NetworkTestRequestMap,
  NetworkTestResult,
  NetworkToolDataMap,
  NetworkToolId,
} from "./networkTypes";

const cancelledTasks = new Set<string>();
const wait = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms));

function mockData(request: NetworkTestRequestMap["speed"]): NetworkToolDataMap["speed"];
function mockData(request: NetworkTestRequestMap["website"]): NetworkToolDataMap["website"];
function mockData(request: NetworkTestRequestMap["publicIp"]): NetworkToolDataMap["publicIp"];
function mockData(request: NetworkTestRequestMap["ping"]): NetworkToolDataMap["ping"];
function mockData(request: NetworkTestRequestMap["trace"]): NetworkToolDataMap["trace"];
function mockData(request: NetworkTestRequestMap["dns"]): NetworkToolDataMap["dns"];
function mockData(request: NetworkTestRequestMap["ipv6"]): NetworkToolDataMap["ipv6"];
function mockData(request: NetworkTestRequest): NetworkToolDataMap[NetworkToolId] {
  switch (request.tool) {
    case "speed":
      return {
        node: { name: request.nodeName || "Clouvider 法兰克福", url: request.nodeUrl || "https://fra.speedtest.clouvider.net/backend/", operator: "模拟节点", region: "浏览器预览" },
        profile: request.profile,
        downloadMbps: 286.4,
        uploadMbps: 82.7,
        latencyMs: 18.6,
        jitterMs: 2.4,
        bytesDownloaded: 48_800_000,
        bytesUploaded: 12_600_000,
        samples: Array.from({ length: 16 }, (_, index) => ({ at: Date.now() - (15 - index) * 400, downloadMbps: 248 + index * 2.6, uploadMbps: 72 + index * 0.7 })),
        warnings: [],
      };
    case "website":
      return {
        url: request.url,
        finalUrl: request.url,
        statusCode: 200,
        remoteAddress: "203.0.113.42",
        stages: [
          { name: "dns", label: "DNS 解析", status: "success", durationMs: 14, detail: "返回 2 个地址" },
          { name: "tcp", label: "TCP 连接", status: "success", durationMs: 24, detail: "连接成功" },
          { name: "tls", label: "TLS 握手", status: request.url.startsWith("https:") ? "success" : "skipped", durationMs: request.url.startsWith("https:") ? 48 : null, detail: request.url.startsWith("https:") ? "证书有效" : "HTTP 不需要 TLS" },
          { name: "http", label: "HTTP 响应", status: "success", durationMs: 86, detail: "HTTP 200" },
        ],
        redirects: [],
        warnings: [],
      };
    case "publicIp":
      return {
        ipv4: { address: "203.0.113.42", provider: "模拟数据", asn: "AS64500", operator: "示例运营商", region: "示例地区", reverseHost: null },
        ipv6: { address: "2001:db8::42", provider: "模拟数据", asn: "AS64500", operator: "示例运营商", region: "示例地区", reverseHost: null },
        source: "浏览器模拟数据（未发起网络请求）",
        warnings: [],
      };
    case "ping": {
      const samples = Array.from({ length: request.count }, (_, index) => ({ sequence: index + 1, latencyMs: 18 + (index % 3) * 3, status: "success", error: null }));
      return { protocol: request.protocol, target: request.target, port: request.port ?? null, sent: request.count, received: request.count, lossPercent: 0, minMs: 18, avgMs: 20, maxMs: 24, jitterMs: 2.2, samples };
    }
    case "trace":
      return {
        target: request.target,
        maxHops: request.maxHops,
        reachedTarget: true,
        hops: [
          { hop: 1, address: "192.168.1.1", hostname: "gateway.example", latenciesMs: [2, 3], status: "success" },
          { hop: 2, address: null, hostname: null, latenciesMs: [null, null], status: "timeout" },
          { hop: 3, address: "203.0.113.1", hostname: null, latenciesMs: [16, 18], status: "success" },
          { hop: 4, address: "203.0.113.42", hostname: request.target, latenciesMs: [22, 21], status: "success" },
        ],
      };
    case "dns":
      return {
        domain: request.domain,
        recordType: request.recordType,
        transport: request.transport,
        queries: [
          { server: request.server || "系统 DNS", transport: request.transport, durationMs: 18, status: "success", answers: [{ name: request.domain, recordType: request.recordType, ttl: 300, value: request.recordType === "AAAA" ? "2001:db8::42" : request.recordType === "MX" ? "10 mail.example.com" : request.recordType === "TXT" ? "v=example" : "203.0.113.42" }], error: null },
          ...(request.compareSystem && request.server ? [{ server: "系统 DNS", transport: request.transport, durationMs: 26, status: "success", answers: [{ name: request.domain, recordType: request.recordType, ttl: 300, value: "203.0.113.42" }], error: null }] : []),
        ],
        warnings: [],
      };
    case "ipv6":
      return {
        available: true,
        globalAddresses: ["2001:db8::42"],
        publicAddress: "2001:db8::42",
        steps: [
          { key: "address", label: "本机地址", status: "success", detail: "检测到全局 IPv6（模拟）" },
          { key: "dns", label: "AAAA 解析", status: "success", detail: "解析正常（模拟）" },
          { key: "connectivity", label: "外部连通", status: "success", detail: "IPv6 请求成功（模拟）" },
          { key: "publicIp", label: "公网 IPv6", status: "success", detail: "已获取（模拟）" },
        ],
        warnings: [],
      };
  }
}

function mockResult(taskId: string, request: NetworkTestRequest, status: NetworkTestResult["status"] = "success"): NetworkTestResult {
  const base = {
    taskId,
    status,
    startedAt: new Date(Date.now() - 800).toISOString(),
    finishedAt: new Date().toISOString(),
    durationMs: 800,
    target: "url" in request ? request.url : "target" in request ? request.target : "domain" in request ? request.domain : null,
    path: "path" in request ? request.path : null,
    summary: status === "cancelled" ? "测试已取消" : "模拟测试完成",
    error: null,
  };
  switch (request.tool) {
    case "speed": return { ...base, tool: "speed", data: mockData(request) };
    case "website": return { ...base, tool: "website", data: mockData(request) };
    case "publicIp": return { ...base, tool: "publicIp", data: mockData(request) };
    case "ping": return { ...base, tool: "ping", data: mockData(request) };
    case "trace": return { ...base, tool: "trace", data: mockData(request) };
    case "dns": return { ...base, tool: "dns", data: mockData(request) };
    case "ipv6": return { ...base, tool: "ipv6", data: mockData(request) };
  }
}

function progress(taskId: string, tool: NetworkToolId | "diagnostic", completed: number, total: number, message: string, points: NetworkProgressPoint[] = []) {
  emitMockEvent("network_progress", { taskId, tool, phase: message, status: "running", message, completed, total, bytes: 0, points });
}

export async function mockRunNetworkTest(taskId: string, request: NetworkTestRequest): Promise<NetworkTestResult> {
  cancelledTasks.delete(taskId);
  const steps = request.tool === "speed" ? 12 : request.tool === "ping" ? Math.min(request.count, 8) : request.tool === "trace" ? 6 : 3;
  for (let index = 0; index < steps; index += 1) {
    await wait(request.tool === "speed" ? 180 : 110);
    if (cancelledTasks.has(taskId)) return mockResult(taskId, request, "cancelled");
    const points: NetworkProgressPoint[] = request.tool === "speed"
      ? [{ at: Date.now(), kind: "download", value: 245 + index * 3 }, { at: Date.now(), kind: "upload", value: 72 + index }]
      : request.tool === "ping" ? [{ at: Date.now(), kind: "latency", value: 18 + (index % 3) * 3, sequence: index + 1 }] : [];
    progress(taskId, request.tool, index + 1, steps, request.tool === "speed" ? "正在测量下载与上传吞吐量（模拟）" : "正在采集测试结果（模拟）", points);
  }
  return mockResult(taskId, request);
}

export async function mockRunNetworkDiagnostic(taskId: string): Promise<NetworkDiagnosticReport> {
  cancelledTasks.delete(taskId);
  const startedAt = new Date().toISOString();
  progress(taskId, "diagnostic", 0, 5, "正在执行五项轻量检测（模拟）");
  await wait(900);
  const cancelled = cancelledTasks.has(taskId);
  const requests: NetworkTestRequest[] = [
    { tool: "publicIp", path: "system" },
    { tool: "website", url: "https://www.baidu.com", path: "system" },
    { tool: "website", url: "https://www.cloudflare.com", path: "system" },
    { tool: "ping", target: "223.5.5.5", protocol: "icmp", count: 4, timeoutMs: 1000 },
    { tool: "dns", domain: "www.qq.com", recordType: "A", transport: "UDP", compareSystem: true },
    { tool: "ipv6" },
  ];
  const results = requests.map((request, index) => mockResult(`${taskId}-${index}`, request, cancelled ? "cancelled" : "success"));
  return {
    taskId,
    status: cancelled ? "cancelled" : "normal",
    startedAt,
    finishedAt: new Date().toISOString(),
    durationMs: 900,
    results,
    findings: cancelled ? [] : [{ severity: "info", title: "国内与国际目标均可访问", detail: "这是浏览器模拟结果，未对当前电脑发起真实测试。", recommendation: "在 Tauri 桌面端运行以获取真实结果。" }],
  };
}

export function mockCancelNetworkTask(taskId: string) {
  cancelledTasks.add(taskId);
}
