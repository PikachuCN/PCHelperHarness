import { useMemo, useState } from "react";
import { AlertTriangle, Check, CircleSlash, Clock3, Globe2, History, Network, ShieldCheck, Trash2, X } from "lucide-react";
import type {
  NetworkDiagnosticReport,
  NetworkDnsData,
  NetworkHistoryEntry,
  NetworkIpv6Data,
  NetworkPingData,
  NetworkProgressEvent,
  NetworkPublicAddress,
  NetworkPublicIpData,
  NetworkResultStatus,
  NetworkSpeedData,
  NetworkTestResult,
  NetworkTraceData,
  NetworkWebsiteData,
} from "../lib/networkTypes";
import { formatNetworkResultMetrics, networkStatusLabels, networkToolLabels } from "../lib/networkReport";
import { formatBytes, formatDateTime } from "../lib/systemOverviewFormat";
import { ConfirmDialog } from "./Controls";
import { StatusBadge } from "./SystemOverviewCard";

const chartWidth = 680;
const chartHeight = 220;
const chartPadding = { top: 18, right: 18, bottom: 34, left: 50 };

function finite(value: number | null | undefined): value is number {
  return value !== null && value !== undefined && Number.isFinite(value);
}

function milliseconds(value: number | null | undefined): string {
  return finite(value) ? `${value.toFixed(value >= 100 ? 0 : 1)} ms` : "未读取";
}

function speed(value: number | null | undefined): string {
  return finite(value) ? `${value.toFixed(value >= 100 ? 0 : 2)} Mbps` : "未采样";
}

function resultTone(status: NetworkResultStatus): "success" | "warning" | "error" | "neutral" {
  if (status === "success") return "success";
  if (status === "warning") return "warning";
  if (status === "error") return "error";
  return "neutral";
}

function ChartEmpty({ children = "运行后这里会显示实测图表。" }: { children?: string }) {
  return <div className="network-chart-empty"><Network size={20} /><span>{children}</span></div>;
}

function ChartGrid({ maximum, unit }: { maximum: number; unit: string }) {
  const innerHeight = chartHeight - chartPadding.top - chartPadding.bottom;
  return <g className="network-chart-grid" aria-hidden="true">
    {[0, 1, 2, 3, 4].map((step) => {
      const y = chartPadding.top + (innerHeight * step) / 4;
      const value = maximum * (1 - step / 4);
      return <g key={step}>
        <line x1={chartPadding.left} x2={chartWidth - chartPadding.right} y1={y} y2={y} />
        <text x={chartPadding.left - 7} y={y + 3} textAnchor="end">{value >= 100 ? value.toFixed(0) : value.toFixed(1)}</text>
      </g>;
    })}
    <text x={chartPadding.left} y={chartHeight - 7}>{unit}</text>
  </g>;
}

interface TimedValue {
  at: number;
  value: number;
}

function SpeedLine({ samples, maximum, className, dashed = false }: { samples: TimedValue[]; maximum: number; className: string; dashed?: boolean }) {
  if (samples.length === 0) return null;
  const innerWidth = chartWidth - chartPadding.left - chartPadding.right;
  const innerHeight = chartHeight - chartPadding.top - chartPadding.bottom;
  const points = samples.map((sample, index) => {
    const x = chartPadding.left + (samples.length === 1 ? innerWidth / 2 : (index / (samples.length - 1)) * innerWidth);
    const y = chartPadding.top + innerHeight - (sample.value / maximum) * innerHeight;
    return { x, y, sample };
  });
  return <g className={className}>
    {points.length > 1 && <polyline points={points.map((point) => `${point.x},${point.y}`).join(" ")} strokeDasharray={dashed ? "6 4" : undefined} />}
    {points.map((point) => <circle key={`${point.sample.at}-${point.sample.value}`} cx={point.x} cy={point.y} r="3"><title>{formatDateTime(new Date(point.sample.at).toISOString())} · {speed(point.sample.value)}</title></circle>)}
  </g>;
}

export function SpeedChart({ data, progress }: { data?: NetworkSpeedData; progress?: NetworkProgressEvent }) {
  const download = data
    ? data.samples.filter((sample) => finite(sample.downloadMbps)).map((sample) => ({ at: sample.at, value: sample.downloadMbps! }))
    : (progress?.points ?? []).filter((point) => point.kind === "download").map((point) => ({ at: point.at, value: point.value }));
  const upload = data
    ? data.samples.filter((sample) => finite(sample.uploadMbps)).map((sample) => ({ at: sample.at, value: sample.uploadMbps! }))
    : (progress?.points ?? []).filter((point) => point.kind === "upload").map((point) => ({ at: point.at, value: point.value }));
  const maximum = Math.max(1, ...download.map((item) => item.value), ...upload.map((item) => item.value)) * 1.1;
  const summary = data
    ? `节点 ${data.node.name}，下载 ${speed(data.downloadMbps)}，上传 ${speed(data.uploadMbps)}，延迟 ${milliseconds(data.latencyMs)}，抖动 ${milliseconds(data.jitterMs)}。`
    : `实时测速：已记录下载 ${download.length} 个采样，上传 ${upload.length} 个采样。`;
  return <figure className="network-chart" aria-label="速度测试图表">
    <div className="network-chart-legend"><span className="is-download">━ 下载（实线）</span><span className="is-upload">┅ 上传（虚线）</span>{data && <span>{data.profile === "precise" ? "精准模式" : "标准模式"}</span>}</div>
    {download.length === 0 && upload.length === 0 ? <ChartEmpty>正在等待测速采样；尚未产生速率数据。</ChartEmpty> : <>
      <svg viewBox={`0 0 ${chartWidth} ${chartHeight}`} role="img" aria-label={summary} preserveAspectRatio="xMidYMid meet">
        <ChartGrid maximum={maximum} unit="Mbps" />
        <SpeedLine samples={download} maximum={maximum} className="network-chart-download" />
        <SpeedLine samples={upload} maximum={maximum} className="network-chart-upload" dashed />
      </svg>
      <p className="network-chart-summary">{summary}</p>
    </>}
    {data && <div className="network-chart-facts"><span>下载流量 {formatBytes(data.bytesDownloaded)}</span><span>上传流量 {formatBytes(data.bytesUploaded)}</span><span>节点 {data.node.region} · {data.node.operator}</span></div>}
  </figure>;
}

export function WebsiteChart({ data }: { data: NetworkWebsiteData }) {
  const durations = data.stages.map((stage) => finite(stage.durationMs) ? stage.durationMs : 0);
  const total = durations.reduce((sum, value) => sum + value, 0);
  const scale = Math.max(total, 1);
  let cursor = 0;
  const bars = data.stages.map((stage, index) => {
    const start = cursor;
    cursor += durations[index];
    return { stage, start, duration: durations[index] };
  });
  const height = 42 + data.stages.length * 43;
  const summary = `网站 ${data.url}，HTTP ${data.statusCode ?? "无响应"}，阶段总耗时 ${milliseconds(total)}。${data.stages.map((stage) => `${stage.label}${stage.status} ${milliseconds(stage.durationMs)}`).join("；")}`;
  return <figure className="network-chart">
    <svg viewBox={`0 0 ${chartWidth} ${height}`} role="img" aria-label={summary} preserveAspectRatio="xMidYMid meet">
      {bars.map(({ stage, start, duration }, index) => {
        const y = 24 + index * 43;
        const x = 116 + (start / scale) * 420;
        const width = Math.max(duration > 0 ? 3 : 0, (duration / scale) * 420);
        return <g key={stage.name} className={`network-stage is-${stage.status}`}>
          <text x="0" y={y + 13}>{stage.label}</text>
          {stage.status === "skipped" ? <text x="116" y={y + 13}>跳过</text> : <>
            <rect x={x} y={y} width={width} height="19" rx="3" />
            <text x={Math.min(x + width + 7, 568)} y={y + 13}>{milliseconds(stage.durationMs)} · {stage.status}</text>
          </>}
        </g>;
      })}
    </svg>
    <p className="network-chart-summary">{summary}</p>
    <div className="network-chart-facts"><span>最终 URL：{data.finalUrl || "未获取"}</span><span>重定向 {data.redirects.length} 次</span></div>
    {data.stages.some((stage) => stage.detail) && <ul className="network-chart-details">{data.stages.filter((stage) => stage.detail).map((stage) => <li key={stage.name}><strong>{stage.label}</strong>：{stage.detail}</li>)}</ul>}
  </figure>;
}

export function PingChart({ data }: { data: NetworkPingData }) {
  const valid = data.samples.filter((sample) => finite(sample.latencyMs));
  const maximum = Math.max(1, ...valid.map((sample) => sample.latencyMs!)) * 1.15;
  const innerWidth = chartWidth - chartPadding.left - chartPadding.right;
  const innerHeight = chartHeight - chartPadding.top - chartPadding.bottom;
  const pointFor = (sample: NetworkPingData["samples"][number], index: number) => ({
    x: chartPadding.left + (data.samples.length <= 1 ? innerWidth / 2 : (index / (data.samples.length - 1)) * innerWidth),
    y: chartPadding.top + innerHeight - ((sample.latencyMs ?? 0) / maximum) * innerHeight,
  });
  const segments: { x: number; y: number }[][] = [];
  for (const [index, sample] of data.samples.entries()) {
    if (!finite(sample.latencyMs)) continue;
    const previous = data.samples[index - 1];
    if (!previous || !finite(previous.latencyMs) || segments.length === 0) segments.push([]);
    segments[segments.length - 1].push(pointFor(sample, index));
  }
  const summary = `Ping ${data.target}，发送 ${data.sent} 次，接收 ${data.received} 次，丢包 ${data.lossPercent.toFixed(1)}%，平均延迟 ${milliseconds(data.avgMs)}。叉号表示丢包。`;
  return <figure className="network-chart">
    <div className="network-chart-legend"><span className="is-download">━ 延迟</span><span className="is-error">× 丢包</span><span>{data.protocol.toUpperCase()}{data.port ? ` · ${data.port}` : ""}</span></div>
    {data.samples.length === 0 ? <ChartEmpty>没有 Ping 采样；请查看失败原因。</ChartEmpty> : <>
      <svg viewBox={`0 0 ${chartWidth} ${chartHeight}`} role="img" aria-label={summary} preserveAspectRatio="xMidYMid meet">
        <ChartGrid maximum={maximum} unit="ms / 次数" />
        <g className="network-chart-download">{segments.map((segment, index) => <g key={index}>
          {segment.length > 1 && <polyline points={segment.map((point) => `${point.x},${point.y}`).join(" ")} />}
          {segment.map((point, pointIndex) => <circle key={pointIndex} cx={point.x} cy={point.y} r="3" />)}
        </g>)}</g>
        <g className="network-ping-loss">{data.samples.map((sample, index) => {
          if (finite(sample.latencyMs)) return null;
          const point = pointFor(sample, index);
          const y = chartHeight - chartPadding.bottom - 4;
          return <g key={sample.sequence} transform={`translate(${point.x} ${y})`}>
            <path d="M-4 -4 L4 4 M4 -4 L-4 4" />
            <text x="0" y="17" textAnchor="middle">{sample.sequence}</text>
            <title>第 {sample.sequence} 次：{sample.status}{sample.error ? `，${sample.error}` : ""}</title>
          </g>;
        })}</g>
      </svg>
      <p className="network-chart-summary">{summary}</p>
    </>}
    <div className="network-chart-facts"><span>最小 {milliseconds(data.minMs)}</span><span>最大 {milliseconds(data.maxMs)}</span><span>抖动 {milliseconds(data.jitterMs)}</span></div>
  </figure>;
}

export function TraceChart({ data }: { data: NetworkTraceData }) {
  const rows = data.hops.map((hop) => {
    const values = hop.latenciesMs.filter(finite);
    const average = values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : undefined;
    return { hop, average };
  });
  const maximum = Math.max(1, ...rows.map((row) => row.average ?? 0));
  const summary = `路由追踪 ${data.target}，记录 ${data.hops.length} 跳，${data.reachedTarget ? "已到达目标" : "未确认到达目标"}。无响应节点以文字标出。`;
  return <figure className="network-chart">
    <p className="network-chart-summary">{summary}</p>
    {rows.length === 0 ? <ChartEmpty>没有收到任何路由跳点。</ChartEmpty> : <ol className="network-trace-list" aria-label={summary}>{rows.map(({ hop, average }) => <li key={hop.hop} className="network-trace-hop">
      <div className="network-trace-node" aria-hidden="true"><span>{hop.hop}</span></div>
      <div className="network-trace-copy"><strong>{hop.address || hop.hostname || "无响应（*）"}</strong><small>{hop.hostname && hop.address ? hop.hostname : hop.status}{hop.latenciesMs.some((value) => value === null) ? " · 含超时探测" : ""}</small></div>
      <div className="network-trace-latency"><div className="network-horizontal-track" aria-hidden="true"><span style={{ width: `${average ? Math.max(2, (average / maximum) * 100) : 0}%` }} /></div><span>{milliseconds(average)}</span></div>
    </li>)}</ol>}
  </figure>;
}

export function DnsChart({ data }: { data: NetworkDnsData }) {
  const maximum = Math.max(1, ...data.queries.map((query) => query.durationMs));
  const answerCount = data.queries.reduce((sum, query) => sum + query.answers.length, 0);
  const summary = `DNS ${data.domain} 的 ${data.recordType} 查询，${data.queries.length} 个服务器，共 ${answerCount} 条答案。`;
  return <figure className="network-chart">
    <p className="network-chart-summary">{summary}</p>
    <div className="network-dns-bars" aria-label="DNS 服务器耗时对比">{data.queries.map((query, index) => <div className="network-dns-row" key={`${query.server}-${query.transport}-${index}`}>
      <div><strong>{query.server}</strong><small>{query.transport} · {query.status}{query.error ? ` · ${query.error}` : ""}</small></div>
      <div className="network-horizontal-track" aria-hidden="true"><span className={query.status === "success" ? "" : "is-warning"} style={{ width: `${Math.max(2, (query.durationMs / maximum) * 100)}%` }} /></div>
      <span>{milliseconds(query.durationMs)}</span>
    </div>)}</div>
    {answerCount > 0 && <div className="overview-table-wrap network-answer-table"><table className="overview-table"><thead><tr><th>名称</th><th>类型</th><th>TTL</th><th>答案</th></tr></thead><tbody>{data.queries.flatMap((query, queryIndex) => query.answers.map((answer, answerIndex) => <tr key={`${queryIndex}-${answerIndex}`}><td>{answer.name}</td><td>{answer.recordType}</td><td>{answer.ttl}</td><td>{answer.value}</td></tr>))}</tbody></table></div>}
  </figure>;
}

export function Ipv6Steps({ data }: { data: NetworkIpv6Data }) {
  const summary = `IPv6 ${data.available ? "可用" : "不可用或未完整连通"}。${data.steps.map((step) => `${step.label}：${step.status}`).join("；")}`;
  return <figure className="network-chart">
    <p className="network-chart-summary">{summary}</p>
    <ol className="network-ipv6-steps" aria-label={summary}>{data.steps.map((step, index) => <li key={step.key} className={`is-${step.status}`}>
      <span className="network-ipv6-index">{step.status === "success" ? <Check size={15} /> : step.status === "error" ? <X size={15} /> : step.status === "skipped" ? <CircleSlash size={15} /> : <AlertTriangle size={15} />}</span>
      <div><strong>{index + 1}. {step.label}<StatusBadge tone={step.status === "success" ? "success" : step.status === "error" ? "error" : step.status === "warning" ? "warning" : "neutral"}>{step.status}</StatusBadge></strong><p>{step.detail}</p></div>
    </li>)}</ol>
    <div className="network-chart-facts"><span>全局 IPv6 地址 {data.globalAddresses.length} 个</span><span>公网 IPv6：{data.publicAddress || "未获取"}</span></div>
  </figure>;
}

function PublicAddressCard({ label, address }: { label: string; address?: NetworkPublicAddress | null }) {
  return <article className="network-address-card"><div className="network-address-heading"><Globe2 size={17} /><strong>{label}</strong><StatusBadge tone={address ? "success" : "neutral"}>{address ? "已获取" : "未获取"}</StatusBadge></div>{address ? <dl><div><dt>地址</dt><dd><code>{address.address}</code></dd></div><div><dt>来源</dt><dd>{address.provider}</dd></div><div><dt>ASN</dt><dd>{address.asn || "未提供"}</dd></div><div><dt>运营方</dt><dd>{address.operator || "未提供"}</dd></div><div><dt>地区</dt><dd>{address.region || "未提供"}</dd></div><div><dt>反向解析</dt><dd>{address.reverseHost || "未提供"}</dd></div></dl> : <p>本次未获得该协议地址，不代表系统未配置。</p>}</article>;
}

export function PublicIpCard({ data }: { data: NetworkPublicIpData }) {
  return <div className="network-public-ip" aria-label={`公网 IP 信息，来源 ${data.source}`}><PublicAddressCard label="IPv4" address={data.ipv4} /><PublicAddressCard label="IPv6" address={data.ipv6} /><p className="network-chart-summary">信息来源：{data.source}。这是外部服务观察到的出口地址，可能属于运营商、代理或 NAT。</p></div>;
}

export function NetworkResultChart({ result, progress }: { result?: NetworkTestResult; progress?: NetworkProgressEvent }) {
  if (progress?.tool === "speed" && progress.status === "running") return <SpeedChart progress={progress} />;
  if (result?.data) {
    switch (result.tool) {
      case "speed": return <SpeedChart data={result.data} />;
      case "website": return <WebsiteChart data={result.data} />;
      case "publicIp": return <PublicIpCard data={result.data} />;
      case "ping": return <PingChart data={result.data} />;
      case "trace": return <TraceChart data={result.data} />;
      case "dns": return <DnsChart data={result.data} />;
      case "ipv6": return <Ipv6Steps data={result.data} />;
    }
  }
  if (progress?.tool === "speed") return <SpeedChart progress={progress} />;
  return null;
}

function historyTarget(entry: NetworkHistoryEntry): string {
  if (entry.target) return entry.target;
  const result = entry.result;
  if (!result.data) return "未返回目标";
  switch (result.tool) {
    case "speed": return result.data.node.name;
    case "website": return result.data.url;
    case "ping": return result.data.target;
    case "trace": return result.data.target;
    case "dns": return result.data.domain;
    case "publicIp": return "公网出口";
    case "ipv6": return "IPv6";
  }
}

interface HistoryMetric {
  value: number;
  label: string;
  unit: "Mbps" | "ms";
}

function historyMetric(entry: NetworkHistoryEntry): HistoryMetric {
  const fallback = { value: entry.result.durationMs, label: "任务总耗时", unit: "ms" as const };
  const result = entry.result;
  if (!result.data) return fallback;
  switch (result.tool) {
    case "speed": return finite(result.data.downloadMbps) ? { value: result.data.downloadMbps, label: "下载速率", unit: "Mbps" } : fallback;
    case "website": return {
      value: result.data.stages.reduce((sum, stage) => sum + (finite(stage.durationMs) ? stage.durationMs : 0), 0),
      label: "网站阶段总耗时",
      unit: "ms",
    };
    case "ping": return finite(result.data.avgMs) ? { value: result.data.avgMs, label: "平均延迟", unit: "ms" } : fallback;
    case "dns": {
      const durations = result.data.queries.map((query) => query.durationMs).filter(finite);
      return durations.length ? { value: durations.reduce((sum, value) => sum + value, 0) / durations.length, label: "DNS 平均查询耗时", unit: "ms" } : fallback;
    }
    case "trace":
    case "publicIp":
    case "ipv6": return fallback;
  }
}

function HistorySparkline({ entries }: { entries: NetworkHistoryEntry[] }) {
  const chronological = entries.slice().reverse();
  const metrics = chronological.map(historyMetric);
  const maximum = Math.max(1, ...metrics.map((metric) => metric.value));
  const points = chronological.map((entry, index) => ({
    x: chronological.length === 1 ? 48 : (index / (chronological.length - 1)) * 96,
    y: 25 - (metrics[index].value / maximum) * 20,
    entry,
    metric: metrics[index],
  }));
  const metricLabel = metrics[0]?.label ?? "指标";
  const label = `最近 ${entries.length} 次${metricLabel}趋势，${metrics.map((metric) => `${metric.value.toFixed(1)} ${metric.unit}`).join("、")}`;
  return <svg className="network-history-sparkline" viewBox="0 0 100 31" role="img" aria-label={label}>
    <line x1="0" x2="100" y1="27" y2="27" className="network-history-axis" />
    {points.length > 1 && <polyline points={points.map((point) => `${point.x},${point.y}`).join(" ")} />}
    {points.map((point) => <circle key={point.entry.id} cx={point.x} cy={point.y} r="2.5"><title>{formatDateTime(point.entry.completedAt)} · {point.metric.label} {point.metric.unit === "Mbps" ? speed(point.metric.value) : milliseconds(point.metric.value)} · {networkStatusLabels[point.entry.status]}</title></circle>)}
  </svg>;
}

interface NetworkHistoryPanelProps {
  history: NetworkHistoryEntry[];
  enabled: boolean;
  onEnabledChange: (enabled: boolean) => void;
  onClear: () => void;
  onDelete: (id: string) => void;
}

export function NetworkHistoryPanel({ history, enabled, onEnabledChange, onClear, onDelete }: NetworkHistoryPanelProps) {
  const [clearConfirm, setClearConfirm] = useState(false);
  const groups = useMemo(() => {
    const recent = history.slice().sort((a, b) => b.completedAt.localeCompare(a.completedAt)).slice(0, 30);
    const grouped = new Map<string, NetworkHistoryEntry[]>();
    for (const entry of recent) {
      const key = `${entry.tool}\u0000${historyTarget(entry)}`;
      grouped.set(key, [...(grouped.get(key) ?? []), entry]);
    }
    return [...grouped.values()];
  }, [history]);
  return <section className="network-history" aria-labelledby="network-history-title">
    <header className="network-subsection-header"><div><h3 id="network-history-title"><History size={16} />最近 30 次趋势</h3><p>按工具与目标分组；折线展示下载速率、平均延迟、网站阶段或 DNS 查询耗时等关键指标。</p></div><div className="network-history-actions"><label className="network-check"><input type="checkbox" checked={enabled} onChange={(event) => onEnabledChange(event.target.checked)} />记录历史</label><button type="button" className="button quiet compact" onClick={() => setClearConfirm(true)} disabled={history.length === 0}>清空</button></div></header>
    {!enabled && <p className="network-history-disabled"><ShieldCheck size={15} />历史记录已关闭。已有记录仍保留，可手动清空；新结果不会加入趋势。</p>}
    {groups.length === 0 ? <ChartEmpty>暂无历史结果。完成测试后会按工具与目标显示在这里。</ChartEmpty> : <div className="network-history-groups">{groups.map((entries) => {
      const latest = entries[0];
      return <article className="network-history-row" key={`${latest.tool}-${historyTarget(latest)}`}>
        <div className="network-history-label"><strong>{networkToolLabels[latest.tool]}</strong><span title={historyTarget(latest)}>{historyTarget(latest)}</span><small>{entries.length} 次 · 最近 {formatDateTime(latest.completedAt)}</small></div>
        <HistorySparkline entries={entries} />
        <div className="network-history-statuses" aria-label={`${entries.length} 次状态`}>{entries.slice(0, 8).map((entry) => <span key={entry.id} className={`is-${entry.status}`} title={`${formatDateTime(entry.completedAt)} · ${networkStatusLabels[entry.status]}`}><i aria-hidden="true">{entry.status === "success" ? "✓" : entry.status === "warning" ? "!" : entry.status === "error" ? "×" : "–"}</i>{networkStatusLabels[entry.status]}</span>)}</div>
        <div className="network-history-latest"><StatusBadge tone={resultTone(latest.status)}>{networkStatusLabels[latest.status]}</StatusBadge><span>{formatNetworkResultMetrics(latest.result)[0]}</span><small><Clock3 size={12} />{milliseconds(latest.result.durationMs)}</small><button type="button" className="icon-button network-history-delete" aria-label={`删除${networkToolLabels[latest.tool]} ${historyTarget(latest)} 于 ${formatDateTime(latest.completedAt)} 的这条历史`} title="删除这条历史" onClick={() => onDelete(latest.id)}><Trash2 size={13} /></button></div>
      </article>;
    })}</div>}
    {clearConfirm && <ConfirmDialog title="清空网络测试历史？" description="将删除本机保留的全部网络测试历史和趋势，不会影响当前显示的最新结果。此操作无法撤销。" confirmLabel="清空历史" danger onCancel={() => setClearConfirm(false)} onConfirm={() => { setClearConfirm(false); onClear(); }} />}
  </section>;
}

export function DiagnosticResultOverview({ report }: { report: NetworkDiagnosticReport }) {
  const success = report.results.filter((result) => result.status === "success").length;
  const warnings = report.results.filter((result) => result.status === "warning").length;
  const failures = report.results.filter((result) => result.status === "error").length;
  return <div className="network-diagnostic-overview" aria-label={`诊断共 ${report.results.length} 项，成功 ${success} 项，警告 ${warnings} 项，失败 ${failures} 项`}><span><Check size={15} />成功 {success}</span><span><AlertTriangle size={15} />警告 {warnings}</span><span><X size={15} />失败 {failures}</span></div>;
}
