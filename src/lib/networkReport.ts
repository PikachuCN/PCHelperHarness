import type {
  NetworkDiagnosticReport,
  NetworkDiagnosticState,
  NetworkHistoryEntry,
  NetworkRunStatus,
  NetworkTestResult,
  NetworkToolId,
  NetworkToolState,
} from "./networkTypes";
import { formatBytes, formatDateTime } from "./systemOverviewFormat";

export const networkToolLabels: Record<NetworkToolId, string> = {
  speed: "速度测试",
  website: "网站连接",
  publicIp: "公网 IP",
  ping: "Ping",
  trace: "路由追踪",
  dns: "DNS 查询",
  ipv6: "IPv6 检测",
};

export const networkStatusLabels: Record<NetworkRunStatus | "normal", string> = {
  idle: "尚未运行",
  running: "正在运行",
  success: "成功",
  warning: "有警告",
  error: "失败",
  cancelled: "已取消",
  normal: "正常",
};

type DiagnosticInput = NetworkDiagnosticReport | NetworkDiagnosticState | null | undefined;

function reportOf(diagnostic: DiagnosticInput): NetworkDiagnosticReport | undefined {
  if (!diagnostic) return undefined;
  return "results" in diagnostic ? diagnostic : diagnostic.report;
}

function isIpv4(value: string): boolean {
  const parts = value.split(".");
  return parts.length === 4 && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
}

function validIpv6Groups(value: string): boolean {
  const withoutZone = value.split("%")[0];
  const hasIpv4 = withoutZone.includes(".");
  const address = hasIpv4 ? withoutZone.slice(0, withoutZone.lastIndexOf(":")) : withoutZone;
  if (hasIpv4) {
    const ipv4 = withoutZone.slice(withoutZone.lastIndexOf(":") + 1);
    if (!isIpv4(ipv4)) return false;
  }
  const doubleColonCount = address.split("::").length - 1;
  if (doubleColonCount > 1) return false;
  const compressed = doubleColonCount === 1;
  const groups = address.split(":").filter(Boolean);
  if (!groups.every((group) => /^[0-9A-Fa-f]{1,4}$/.test(group))) return false;
  const groupCount = groups.length + (hasIpv4 ? 2 : 0);
  return compressed ? groupCount < 8 : groupCount === 8;
}

/**
 * Redacts identifiers wherever they occur, including inside errors and URLs.
 * Hostname targets remain useful for support; literal IP targets are masked.
 */
export function redactNetworkSensitive(content: string): string {
  const withoutMac = content.replace(/(?<![0-9A-Fa-f])(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}(?![0-9A-Fa-f])/g, "[MAC已脱敏]");
  const withoutIpv6 = withoutMac.replace(/(?<![0-9A-Fa-f:])(?:[0-9A-Fa-f]{0,4}:){2,}[0-9A-Fa-f:.]+(?:%[0-9A-Za-z_.-]+)?(?![0-9A-Fa-f:])/g, (candidate) => {
    const trailingDots = candidate.match(/\.+$/)?.[0] ?? "";
    const address = trailingDots ? candidate.slice(0, -trailingDots.length) : candidate;
    const hasCompressionOrFullForm = address.includes("::") || (address.match(/:/g)?.length ?? 0) >= 7;
    return hasCompressionOrFullForm && validIpv6Groups(address) ? `[IPv6已脱敏]${trailingDots}` : candidate;
  });
  return withoutIpv6.replace(/(?<![\d.])(?:\d{1,3}\.){3}\d{1,3}(?![\d.])/g, (candidate) => isIpv4(candidate) ? "[IPv4已脱敏]" : candidate);
}

function ms(value: number | null | undefined): string {
  return value === null || value === undefined || !Number.isFinite(value) ? "未读取" : `${value.toFixed(value >= 100 ? 0 : 1)} ms`;
}

function mbps(value: number | null | undefined): string {
  return value === null || value === undefined || !Number.isFinite(value) ? "未读取" : `${value.toFixed(value >= 100 ? 0 : 2)} Mbps`;
}

function percent(value: number | null | undefined): string {
  return value === null || value === undefined || !Number.isFinite(value) ? "未读取" : `${value.toFixed(1)}%`;
}

function targetOf(result: NetworkTestResult): string {
  if (result.target) return result.target;
  if (!result.data) return "未返回目标";
  switch (result.tool) {
    case "website": return result.data.url;
    case "speed": return `${result.data.node.name}（${result.data.node.url}）`;
    case "ping": return result.data.target;
    case "trace": return result.data.target;
    case "dns": return result.data.domain;
    case "publicIp": return result.data.source;
    case "ipv6": return "本机 IPv6 与公网连通性";
  }
}

export function formatNetworkResultMetrics(result: NetworkTestResult): string[] {
  if (!result.data) return [result.status === "cancelled" ? "任务已取消，未返回详细数据" : "未返回详细数据"];
  switch (result.tool) {
    case "speed": return [
      `下载 ${mbps(result.data.downloadMbps)}`,
      `上传 ${mbps(result.data.uploadMbps)}`,
      `延迟 ${ms(result.data.latencyMs)}`,
      `抖动 ${ms(result.data.jitterMs)}`,
      `已下载 ${formatBytes(result.data.bytesDownloaded)} / 已上传 ${formatBytes(result.data.bytesUploaded)}`,
    ];
    case "website": return [
      `HTTP ${result.data.statusCode ?? "无响应"}`,
      `阶段 ${result.data.stages.filter((stage) => stage.status === "success").length}/${result.data.stages.length} 成功`,
      `重定向 ${result.data.redirects.length} 次`,
    ];
    case "publicIp": return [
      `IPv4 ${result.data.ipv4 ? `${result.data.ipv4.address} · ${result.data.ipv4.provider}` : "未获取"}`,
      `IPv6 ${result.data.ipv6 ? `${result.data.ipv6.address} · ${result.data.ipv6.provider}` : "未获取"}`,
    ];
    case "ping": return [
      `发送 ${result.data.sent} / 接收 ${result.data.received}`,
      `丢包 ${percent(result.data.lossPercent)}`,
      `最小 / 平均 / 最大 ${ms(result.data.minMs)} / ${ms(result.data.avgMs)} / ${ms(result.data.maxMs)}`,
      `抖动 ${ms(result.data.jitterMs)}`,
    ];
    case "trace": return [
      `${result.data.hops.length} 跳 / 上限 ${result.data.maxHops}`,
      result.data.reachedTarget ? "已到达目标" : "未确认到达目标",
    ];
    case "dns": {
      const durations = result.data.queries.map((query) => query.durationMs).filter(Number.isFinite);
      const average = durations.length ? durations.reduce((sum, value) => sum + value, 0) / durations.length : undefined;
      return [
        `${result.data.recordType} · ${result.data.transport}`,
        `服务器 ${result.data.queries.length} 个 · 平均 ${ms(average)}`,
        `答案 ${result.data.queries.reduce((sum, query) => sum + query.answers.length, 0)} 条`,
      ];
    }
    case "ipv6": return [
      result.data.available ? "IPv6 可用" : "IPv6 不可用或未完整连通",
      `全局地址 ${result.data.globalAddresses.length} 个`,
      `步骤 ${result.data.steps.filter((step) => step.status === "success").length}/${result.data.steps.length} 成功`,
    ];
  }
}

function historyLines(history: NetworkHistoryEntry[], limit: number): string[] {
  return history
    .slice()
    .sort((a, b) => b.completedAt.localeCompare(a.completedAt))
    .slice(0, limit)
    .map((entry) => `${networkToolLabels[entry.tool]}｜${entry.target ?? targetOf(entry.result)}｜${networkStatusLabels[entry.status]}｜${ms(entry.result.durationMs)}｜${formatNetworkResultMetrics(entry.result).slice(0, 2).join("；")}`);
}

export function buildNetworkDiagnosticSummary(diagnostic: DiagnosticInput, history: NetworkHistoryEntry[] = []): string {
  const report = reportOf(diagnostic);
  if (!report) return "网络诊断摘要\n尚未运行网络诊断。主动测试只会在你确认后执行。";
  const lines = [
    "网络诊断摘要（已脱敏）",
    `总体状态：${networkStatusLabels[report.status]}`,
    `开始时间：${formatDateTime(report.startedAt)}`,
    `完成时间：${formatDateTime(report.finishedAt)}`,
    `总耗时：${ms(report.durationMs)}`,
    `测试结果：${report.results.length} 项；发现 ${report.findings.length} 条证据结论`,
    "",
    "关键结果：",
  ];
  if (report.results.length === 0) lines.push("没有可用的测试结果。");
  for (const result of report.results) {
    lines.push(`- ${networkToolLabels[result.tool]}｜${targetOf(result)}｜${networkStatusLabels[result.status]}｜${ms(result.durationMs)}`);
    lines.push(`  ${result.summary}`);
    for (const metric of formatNetworkResultMetrics(result)) lines.push(`  ${metric}`);
    if (result.error) lines.push(`  错误：${result.error}`);
  }
  lines.push("", "证据与建议：");
  if (report.findings.length === 0) lines.push("没有额外发现；这不代表未测试项目正常。");
  for (const finding of report.findings) {
    lines.push(`- [${finding.severity}] ${finding.title}：${finding.detail}`);
    lines.push(`  建议：${finding.recommendation}`);
  }
  if (history.length > 0) lines.push("", "最近历史（按完成时间倒序）：", ...historyLines(history, 5).map((line) => `- ${line}`));
  lines.push("", "说明：IPv4、IPv6 与 MAC 地址已脱敏；未执行的项目不应被视为正常。");
  return redactNetworkSensitive(lines.join("\n"));
}

function cell(value: unknown): string {
  const text = value === null || value === undefined || String(value).trim() === "" ? "—" : String(value);
  return redactNetworkSensitive(text).replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
}

function addTable(lines: string[], headers: string[], rows: unknown[][]): void {
  lines.push(`| ${headers.join(" | ")} |`, `| ${headers.map(() => "---").join(" | ")} |`);
  if (rows.length === 0) lines.push(`| ${headers.map((_, index) => index === 0 ? "无记录" : "—").join(" | ")} |`);
  else for (const row of rows) lines.push(`| ${row.map(cell).join(" | ")} |`);
  lines.push("");
}

function addResultDetails(lines: string[], result: NetworkTestResult): void {
  if (!result.data) {
    lines.push(result.status === "cancelled" ? "任务已取消，未返回详细数据。" : "未返回详细数据。", "");
    return;
  }
  switch (result.tool) {
    case "speed":
      addTable(lines, ["节点", "地区 / 运营方", "模式", "路径"], [[`${result.data.node.name}（${result.data.node.url}）`, `${result.data.node.region} / ${result.data.node.operator}`, result.data.profile, result.path ?? "—"]]);
      addTable(lines, ["下载", "上传", "延迟", "抖动", "下载流量", "上传流量"], [[mbps(result.data.downloadMbps), mbps(result.data.uploadMbps), ms(result.data.latencyMs), ms(result.data.jitterMs), formatBytes(result.data.bytesDownloaded), formatBytes(result.data.bytesUploaded)]]);
      addTable(lines, ["采样时间", "下载", "上传", "延迟"], result.data.samples.map((sample) => [formatDateTime(new Date(sample.at).toISOString()), mbps(sample.downloadMbps), mbps(sample.uploadMbps), ms(sample.latencyMs)]));
      break;
    case "website":
      addTable(lines, ["原始 URL", "最终 URL", "HTTP", "远端地址"], [[result.data.url, result.data.finalUrl, result.data.statusCode, result.data.remoteAddress]]);
      addTable(lines, ["阶段", "状态", "耗时", "详情"], result.data.stages.map((stage) => [stage.label, stage.status, ms(stage.durationMs), stage.detail]));
      addTable(lines, ["重定向自", "重定向至", "HTTP"], result.data.redirects.map((redirect) => [redirect.from, redirect.to, redirect.statusCode]));
      break;
    case "publicIp":
      addTable(lines, ["协议", "地址", "来源", "ASN", "运营方", "地区", "反向解析"], [
        ...(result.data.ipv4 ? [["IPv4", result.data.ipv4.address, result.data.ipv4.provider, result.data.ipv4.asn, result.data.ipv4.operator, result.data.ipv4.region, result.data.ipv4.reverseHost] as unknown[]] : []),
        ...(result.data.ipv6 ? [["IPv6", result.data.ipv6.address, result.data.ipv6.provider, result.data.ipv6.asn, result.data.ipv6.operator, result.data.ipv6.region, result.data.ipv6.reverseHost] as unknown[]] : []),
      ]);
      lines.push(`来源：${cell(result.data.source)}`, "");
      break;
    case "ping":
      addTable(lines, ["目标", "协议", "端口", "发送", "接收", "丢包"], [[result.data.target, result.data.protocol.toUpperCase(), result.data.port, result.data.sent, result.data.received, percent(result.data.lossPercent)]]);
      addTable(lines, ["最小", "平均", "最大", "抖动"], [[ms(result.data.minMs), ms(result.data.avgMs), ms(result.data.maxMs), ms(result.data.jitterMs)]]);
      addTable(lines, ["序号", "状态", "延迟", "错误"], result.data.samples.map((sample) => [sample.sequence, sample.status, ms(sample.latencyMs), sample.error]));
      break;
    case "trace":
      lines.push(`目标：${cell(result.data.target)}；跳数上限：${result.data.maxHops}；${result.data.reachedTarget ? "已到达目标" : "未确认到达目标"}`, "");
      addTable(lines, ["跳", "地址", "主机名", "状态", "各次延迟"], result.data.hops.map((hop) => [hop.hop, hop.address, hop.hostname, hop.status, hop.latenciesMs.map(ms).join(" / ")]));
      break;
    case "dns":
      lines.push(`域名：${cell(result.data.domain)}；记录：${result.data.recordType}；传输：${result.data.transport}`, "");
      addTable(lines, ["服务器", "传输", "状态", "耗时", "错误"], result.data.queries.map((query) => [query.server, query.transport, query.status, ms(query.durationMs), query.error]));
      addTable(lines, ["名称", "类型", "TTL", "答案"], result.data.queries.flatMap((query) => query.answers.map((answer) => [answer.name, answer.recordType, answer.ttl, answer.value])));
      break;
    case "ipv6":
      lines.push(`IPv6：${result.data.available ? "可用" : "不可用或未完整连通"}；公网地址：${cell(result.data.publicAddress)}；全局地址数量：${result.data.globalAddresses.length}`, "");
      addTable(lines, ["步骤", "状态", "证据"], result.data.steps.map((step) => [step.label, step.status, step.detail]));
      break;
  }
  const warnings = "warnings" in result.data ? result.data.warnings : [];
  if (warnings.length > 0) lines.push("采集警告：", ...warnings.map((warning) => `- ${redactNetworkSensitive(warning)}`), "");
}

export function buildNetworkMarkdown(diagnostic: DiagnosticInput, history: NetworkHistoryEntry[] = []): string {
  const report = reportOf(diagnostic);
  const lines = [
    "# 网络诊断报告（脱敏）",
    "",
    `生成时间：${formatDateTime(new Date().toISOString())}`,
    "",
    "> IPv4、IPv6 与 MAC 地址已脱敏。报告保留主机名/URL 目标、状态、耗时与诊断证据；主动测试仅在你确认后运行。",
    "",
    "## 诊断摘要",
    "",
    "```text",
    buildNetworkDiagnosticSummary(report, history),
    "```",
    "",
  ];
  if (!report) {
    lines.push("尚未运行网络诊断。", "");
  } else {
    addTable(lines, ["总体状态", "开始时间", "完成时间", "总耗时", "结果数"], [[networkStatusLabels[report.status], formatDateTime(report.startedAt), formatDateTime(report.finishedAt), ms(report.durationMs), report.results.length]]);
    lines.push("## 测试结果", "");
    for (const result of report.results) {
      lines.push(`### ${networkToolLabels[result.tool]}`, "");
      addTable(lines, ["目标", "状态", "耗时", "摘要", "错误"], [[targetOf(result), networkStatusLabels[result.status], ms(result.durationMs), result.summary, result.error]]);
      addResultDetails(lines, result);
    }
    lines.push("## 证据结论与建议", "");
    addTable(lines, ["级别", "结论", "证据", "建议"], report.findings.map((finding) => [finding.severity, finding.title, finding.detail, finding.recommendation]));
  }
  lines.push("## 最近 30 次历史", "");
  const recent = history.slice().sort((a, b) => b.completedAt.localeCompare(a.completedAt)).slice(0, 30);
  addTable(lines, ["完成时间", "工具", "目标", "状态", "耗时", "关键指标"], recent.map((entry) => [formatDateTime(entry.completedAt), networkToolLabels[entry.tool], entry.target ?? targetOf(entry.result), networkStatusLabels[entry.status], ms(entry.result.durationMs), formatNetworkResultMetrics(entry.result).join("；")]));
  return redactNetworkSensitive(lines.join("\n"));
}

/** Intentionally unredacted. Callers must obtain explicit sensitive-export consent. */
export function buildNetworkRawJson(
  diagnostic: DiagnosticInput,
  tools: Partial<Record<NetworkToolId, NetworkToolState>>,
  history: NetworkHistoryEntry[],
): string {
  return JSON.stringify({
    generatedAt: new Date().toISOString(),
    sensitive: true,
    notice: "包含未脱敏的公网/本地 IP、目标、DNS、路由和错误详情，仅供专业排查。",
    diagnostic: diagnostic ?? null,
    tools,
    history,
  }, null, 2);
}
