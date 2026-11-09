import { useState, type FormEvent, type ReactNode } from "react";
import { Activity, AlertTriangle, Download, Gauge, Globe2, Network, Play, Route, Server, Settings2, ShieldCheck, Square, Waypoints, XCircle, type LucideIcon } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type {
  NetworkDnsTransport,
  NetworkHistoryEntry,
  NetworkPath,
  NetworkProgressEvent,
  NetworkRecordType,
  NetworkRunStatus,
  NetworkSpeedNode,
  NetworkSpeedProfile,
  NetworkTestResult,
  NetworkTestRequest,
  NetworkTestRequestMap,
  NetworkToolId,
} from "../lib/networkTypes";
import { buildNetworkRawJson, formatNetworkResultMetrics, networkStatusLabels, networkToolLabels } from "../lib/networkReport";
import { formatDateTime } from "../lib/systemOverviewFormat";
import { ConfirmDialog } from "./Controls";
import { StatusBadge } from "./SystemOverviewCard";
import { NetworkHistoryPanel, NetworkResultChart } from "./NetworkCharts";
import "../styles/networkTools.css";

const tools: readonly { id: NetworkToolId; icon: LucideIcon; description: string }[] = [
  { id: "website", icon: Globe2, description: "分阶段检查网站连接" },
  { id: "speed", icon: Gauge, description: "下载、上传与延迟测速" },
  { id: "ping", icon: Activity, description: "ICMP 或 TCP 延迟与丢包" },
  { id: "dns", icon: Server, description: "记录查询与服务器对照" },
  { id: "trace", icon: Route, description: "逐跳路径与延迟" },
  { id: "publicIp", icon: Network, description: "出口 IPv4 / IPv6 信息" },
  { id: "ipv6", icon: Waypoints, description: "四步 IPv6 可用性检测" },
];

const speedNodes: readonly NetworkSpeedNode[] = [
  { name: "Clouvider 法兰克福", url: "https://fra.speedtest.clouvider.net/backend/", operator: "Clouvider", region: "德国 · 法兰克福" },
  { name: "Clouvider 伦敦", url: "https://lon.speedtest.clouvider.net/backend/", operator: "Clouvider", region: "英国 · 伦敦" },
  { name: "Clouvider 纽约", url: "https://nyc.speedtest.clouvider.net/backend/", operator: "Clouvider", region: "美国 · 纽约" },
];

type ToolForms = { [K in NetworkToolId]: NetworkTestRequestMap[K] };

const initialForms: ToolForms = {
  website: { tool: "website", url: "https://www.baidu.com", path: "system" },
  speed: { tool: "speed", nodeUrl: speedNodes[0].url, nodeName: speedNodes[0].name, profile: "standard", path: "system" },
  publicIp: { tool: "publicIp", path: "system" },
  ping: { tool: "ping", target: "223.5.5.5", protocol: "icmp", count: 4, timeoutMs: 1500 },
  trace: { tool: "trace", target: "223.5.5.5", maxHops: 20, timeoutMs: 1500 },
  dns: { tool: "dns", domain: "www.qq.com", recordType: "A", server: "223.5.5.5", transport: "UDP", compareSystem: true },
  ipv6: { tool: "ipv6" },
};

const targetPresets: Partial<Record<NetworkToolId, { domestic: string; international: string }>> = {
  website: { domestic: "https://www.baidu.com", international: "https://www.cloudflare.com/" },
  ping: { domestic: "223.5.5.5", international: "1.1.1.1" },
  trace: { domestic: "223.5.5.5", international: "1.1.1.1" },
  dns: { domestic: "www.qq.com", international: "www.cloudflare.com" },
};

function statusTone(status: NetworkRunStatus): "success" | "warning" | "error" | "neutral" {
  if (status === "success") return "success";
  if (status === "warning") return "warning";
  if (status === "error") return "error";
  return "neutral";
}

function Field({ label, hint, children, wide = false }: { label: string; hint?: string; children: ReactNode; wide?: boolean }) {
  return <label className={`network-field${wide ? " is-wide" : ""}`}><span>{label}</span>{children}{hint && <small>{hint}</small>}</label>;
}

function PathField({ value, onChange }: { value: NetworkPath; onChange: (path: NetworkPath) => void }) {
  return <Field label="网络路径" hint="系统路径遵循系统代理；直连路径绕过代理，结果可能不同。"><select value={value} onChange={(event) => onChange(event.target.value as NetworkPath)}><option value="system">系统路径（system）</option><option value="direct">直连路径（direct）</option></select></Field>;
}

function validHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:" || url.protocol === "http:";
  } catch {
    return false;
  }
}

function validateRequest(request: NetworkTestRequest): string | undefined {
  switch (request.tool) {
    case "website": return validHttpUrl(request.url) ? undefined : "请输入以 http:// 或 https:// 开头的有效网站 URL。";
    case "speed":
      if (!request.nodeUrl || !validHttpUrl(request.nodeUrl)) return "请选择公共测速节点，或输入有效的自定义 HTTP(S) 节点 URL。";
      if (!request.nodeName?.trim()) return "请输入测速节点名称。";
      return undefined;
    case "ping":
      if (!request.target.trim()) return "请输入 Ping 目标。";
      if (!Number.isInteger(request.count) || request.count < 1 || request.count > 20) return "Ping 次数须为 1–20 的整数。";
      if (!Number.isFinite(request.timeoutMs) || request.timeoutMs < 100 || request.timeoutMs > 10_000) return "超时须为 100–10000 ms。";
      if (request.protocol === "tcp" && (!Number.isInteger(request.port) || (request.port ?? 0) < 1 || (request.port ?? 0) > 65_535)) return "TCP 端口须为 1–65535 的整数。";
      return undefined;
    case "trace":
      if (!request.target.trim()) return "请输入路由追踪目标。";
      if (!Number.isInteger(request.maxHops) || request.maxHops < 1 || request.maxHops > 64) return "最大跳数须为 1–64 的整数。";
      if (!Number.isFinite(request.timeoutMs) || request.timeoutMs < 100 || request.timeoutMs > 10_000) return "超时须为 100–10000 ms。";
      return undefined;
    case "dns":
      if (!request.domain.trim()) return "请输入要查询的域名。";
      if (!request.compareSystem && !request.server?.trim()) return "请填写自定义 DNS 服务器，或启用系统 DNS 对照。";
      return undefined;
    case "publicIp":
    case "ipv6": return undefined;
  }
}

function ProgressBlock({ progress }: { progress?: NetworkProgressEvent }) {
  const percent = progress && progress.total > 0 ? Math.min(100, Math.max(0, (progress.completed / progress.total) * 100)) : undefined;
  return <div className="network-progress" role="status" aria-live="polite"><div><strong>{progress?.message || "正在启动任务…"}</strong><span>{progress ? `${progress.completed} / ${progress.total || "?"}` : "等待进度"}</span></div><div className={`network-progress-track${percent === undefined ? " is-indeterminate" : ""}`} aria-hidden="true"><span style={percent === undefined ? undefined : { width: `${percent}%` }} /></div>{progress && <small>阶段：{progress.phase}{progress.bytes > 0 ? ` · 已传输 ${(progress.bytes / 1024 / 1024).toFixed(1)} MB` : ""}</small>}</div>;
}

function ResultWarnings({ result }: { result: NetworkTestResult }) {
  if (!result.data) return null;
  const warnings = "warnings" in result.data ? result.data.warnings : [];
  if (warnings.length === 0) return null;
  return <ul className="network-result-warnings">{warnings.map((warning, index) => <li key={`${index}-${warning}`}><AlertTriangle size={13} />{warning}</li>)}</ul>;
}

export default function NetworkToolsSection() {
  const toolStates = useHarnessStore((state) => state.networkTools);
  const diagnostic = useHarnessStore((state) => state.networkDiagnostic);
  const history = useHarnessStore((state) => state.networkHistory);
  const historyEnabled = useHarnessStore((state) => state.networkHistoryEnabled);
  const privacyAcknowledged = useHarnessStore((state) => state.networkPrivacyAcknowledged);
  const runTest = useHarnessStore((state) => state.runNetworkTest);
  const cancelTask = useHarnessStore((state) => state.cancelNetworkTask);
  const clearHistory = useHarnessStore((state) => state.clearNetworkHistory);
  const deleteHistory = useHarnessStore((state) => state.deleteNetworkHistory);
  const restartAsAdmin = useHarnessStore((state) => state.restartAsAdmin);
  const setHistoryEnabled = useHarnessStore((state) => state.setNetworkHistoryEnabled);
  const acknowledgePrivacy = useHarnessStore((state) => state.acknowledgeNetworkPrivacy);
  const exportReport = useHarnessStore((state) => state.exportNetworkReport);
  const notify = useHarnessStore((state) => state.notify);
  const [selectedTool, setSelectedTool] = useState<NetworkToolId>("website");
  const [workspaceView, setWorkspaceView] = useState<"test" | "history">("test");
  const [forms, setForms] = useState<ToolForms>(initialForms);
  const [professionalMode, setProfessionalMode] = useState(false);
  const [startingTool, setStartingTool] = useState<NetworkToolId | null>(null);
  const [pendingRequest, setPendingRequest] = useState<NetworkTestRequest | null>(null);
  const [exportConfirm, setExportConfirm] = useState(false);
  const [adminConfirm, setAdminConfirm] = useState(false);
  const [exporting, setExporting] = useState(false);
  const selectedState = toolStates[selectedTool];
  const running = selectedState?.status === "running" || startingTool === selectedTool;
  const result = selectedState?.result;
  const permissionEvidence = [
    selectedState?.error,
    result?.error,
    result?.summary,
    ...(result?.tool === "ping" && result.data ? result.data.samples.flatMap((sample) => [sample.status, sample.error]) : []),
    ...(result?.tool === "trace" && result.data ? result.data.hops.map((hop) => hop.status) : []),
  ].filter((value): value is string => Boolean(value)).join("\n");
  const permissionError = (selectedTool === "ping" || selectedTool === "trace") && permissionEvidence.includes("PERMISSION_DENIED");
  const selectedDefinition = tools.find((tool) => tool.id === selectedTool)!;
  const SelectedIcon = selectedDefinition.icon;
  const selectedSpeedNode = speedNodes.find((node) => node.url === forms.speed.nodeUrl);
  const hasRawData = Boolean(diagnostic.report || history.length > 0 || Object.values(toolStates).some((state) => state?.result));

  function updateForm<K extends NetworkToolId>(tool: K, patch: Partial<NetworkTestRequestMap[K]>) {
    setForms((current) => ({ ...current, [tool]: { ...current[tool], ...patch } }));
  }

  const execute = async (request: NetworkTestRequest) => {
    setStartingTool(request.tool);
    try {
      await runTest(request);
    } catch (error) {
      notify(`${networkToolLabels[request.tool]}未能完成：${String(error)}`, "error");
    } finally {
      setStartingTool((current) => current === request.tool ? null : current);
    }
  };

  const start = () => {
    const request = forms[selectedTool];
    const validationError = validateRequest(request);
    if (validationError) {
      notify(validationError, "error");
      return;
    }
    if (!privacyAcknowledged) {
      setPendingRequest(request);
      return;
    }
    void execute(request);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    start();
  };

  const applyTarget = (target: string) => {
    switch (selectedTool) {
      case "website": updateForm("website", { url: target }); break;
      case "ping": updateForm("ping", { target }); break;
      case "trace": updateForm("trace", { target }); break;
      case "dns": updateForm("dns", { domain: target }); break;
      default: break;
    }
  };

  const exportRaw = async () => {
    setExporting(true);
    try {
      await exportReport(buildNetworkRawJson(diagnostic, toolStates, history), "json");
    } catch (error) {
      notify(`导出完整 JSON 失败：${String(error)}`, "error");
    } finally {
      setExporting(false);
    }
  };

  const renderForm = () => {
    switch (selectedTool) {
      case "website": return <>
        <Field label="网站 URL" hint="默认国内站点；国际对照仅填入目标，仍需手动运行。" wide><input type="url" value={forms.website.url} onChange={(event) => updateForm("website", { url: event.target.value })} placeholder="https://www.baidu.com" required /></Field>
        <PathField value={forms.website.path} onChange={(path) => updateForm("website", { path })} />
      </>;
      case "speed": return <>
        <Field label="公共测速节点" hint="公共节点由第三方运营；国际节点可反映跨境链路，但不代表本地宽带签约速率。" wide><select value={selectedSpeedNode?.url ?? "custom"} onChange={(event) => {
          const node = speedNodes.find((item) => item.url === event.target.value);
          updateForm("speed", node ? { nodeUrl: node.url, nodeName: node.name } : { nodeUrl: "", nodeName: "" });
        }}>{speedNodes.map((node) => <option key={node.url} value={node.url}>{node.name} · {node.region}</option>)}<option value="custom">自定义节点…</option></select></Field>
        {!selectedSpeedNode && <><Field label="自定义节点 URL" wide><input type="url" value={forms.speed.nodeUrl ?? ""} onChange={(event) => updateForm("speed", { nodeUrl: event.target.value })} placeholder="https://example.org/backend/" /></Field><Field label="自定义节点名称"><input value={forms.speed.nodeName ?? ""} onChange={(event) => updateForm("speed", { nodeName: event.target.value })} placeholder="我的测速节点" /></Field></>}
        <Field label="测速模式" hint={forms.speed.profile === "precise" ? "精准模式采样更多，可能消耗数百 MB 流量并持续更久。" : "标准模式采样较少，仍可能消耗数十 MB 流量。"}><select value={forms.speed.profile} onChange={(event) => updateForm("speed", { profile: event.target.value as NetworkSpeedProfile })}><option value="standard">标准（较少流量）</option><option value="precise">精准（更多采样与流量）</option></select></Field>
        <PathField value={forms.speed.path} onChange={(path) => updateForm("speed", { path })} />
        <div className="network-traffic-warning is-wide"><AlertTriangle size={16} /><p><strong>测速会产生真实上下行流量。</strong>请留意移动热点或计费网络；节点会看到出口 IP。测速期间不建议同时进行其他网络任务。</p></div>
      </>;
      case "ping": return <>
        <Field label="目标主机或 IP" hint="ICMP 需要系统支持；TCP 模式可指定端口。" wide><input value={forms.ping.target} onChange={(event) => updateForm("ping", { target: event.target.value })} placeholder="223.5.5.5" required /></Field>
        <Field label="协议"><select value={forms.ping.protocol} onChange={(event) => { const protocol = event.target.value as "icmp" | "tcp"; updateForm("ping", { protocol, port: protocol === "tcp" ? forms.ping.port ?? 443 : undefined }); }}><option value="icmp">ICMP</option><option value="tcp">TCP</option></select></Field>
        {forms.ping.protocol === "tcp" && <Field label="TCP 端口"><input type="number" min="1" max="65535" value={forms.ping.port ?? 443} onChange={(event) => updateForm("ping", { port: Number(event.target.value) })} /></Field>}
        <Field label="探测次数" hint="1–20 次"><input type="number" min="1" max="20" value={forms.ping.count} onChange={(event) => updateForm("ping", { count: Number(event.target.value) })} /></Field>
        <Field label="单次超时（ms）"><input type="number" min="100" max="10000" step="100" value={forms.ping.timeoutMs} onChange={(event) => updateForm("ping", { timeoutMs: Number(event.target.value) })} /></Field>
      </>;
      case "trace": return <>
        <Field label="目标主机或 IP" hint="逐跳结果依赖网络设备响应；中间节点无响应不一定表示故障。" wide><input value={forms.trace.target} onChange={(event) => updateForm("trace", { target: event.target.value })} placeholder="223.5.5.5" required /></Field>
        <Field label="最大跳数" hint="1–64 跳"><input type="number" min="1" max="64" value={forms.trace.maxHops} onChange={(event) => updateForm("trace", { maxHops: Number(event.target.value) })} /></Field>
        <Field label="单次超时（ms）"><input type="number" min="100" max="10000" step="100" value={forms.trace.timeoutMs} onChange={(event) => updateForm("trace", { timeoutMs: Number(event.target.value) })} /></Field>
      </>;
      case "dns": return <>
        <Field label="域名" wide><input value={forms.dns.domain} onChange={(event) => updateForm("dns", { domain: event.target.value })} placeholder="www.qq.com" required /></Field>
        <Field label="记录类型"><select value={forms.dns.recordType} onChange={(event) => updateForm("dns", { recordType: event.target.value as NetworkRecordType })}>{(["A", "AAAA", "CNAME", "MX", "TXT", "NS"] as NetworkRecordType[]).map((type) => <option key={type} value={type}>{type}</option>)}</select></Field>
        <Field label="传输协议"><select value={forms.dns.transport} onChange={(event) => updateForm("dns", { transport: event.target.value as NetworkDnsTransport })}><option value="UDP">UDP</option><option value="TCP">TCP</option></select></Field>
        <Field label="自定义 DNS 服务器" hint="留空可仅查询系统 DNS；填写 223.5.5.5 等地址可对照。"><input value={forms.dns.server ?? ""} onChange={(event) => updateForm("dns", { server: event.target.value })} placeholder="223.5.5.5" /></Field>
        <label className="network-check network-check-field"><input type="checkbox" checked={forms.dns.compareSystem} onChange={(event) => updateForm("dns", { compareSystem: event.target.checked })} />同时查询系统 DNS 进行对照</label>
      </>;
      case "publicIp": return <><PathField value={forms.publicIp.path} onChange={(path) => updateForm("publicIp", { path })} /><div className="network-form-note is-wide"><ShieldCheck size={16} /><p>向公网 IP 服务发起请求，展示外部观察到的出口地址、ASN 与运营方。结果可能属于 NAT 或代理，不等同于本机网卡地址。</p></div></>;
      case "ipv6": return <div className="network-form-note is-wide"><Waypoints size={17} /><p>依次检查本机全局 IPv6 地址、AAAA 解析、IPv6 连接与公网地址。某一步失败会保留证据并继续显示后续状态，不会伪造“可用”。</p></div>;
    }
  };

  return <section className="overview-card network-section" aria-labelledby="network-tools-title">
    <header className="overview-card-header network-section-header"><span className="overview-card-icon" aria-hidden="true"><Settings2 size={19} /></span><div className="overview-card-heading"><h2 id="network-tools-title">专业网络工具</h2><p>七种独立主动测试；每次只提交一个明确目标</p></div><div className="overview-card-actions network-header-actions"><button type="button" className="button secondary compact" onClick={() => setExportConfirm(true)} disabled={!hasRawData || exporting}><Download size={14} />{exporting ? "导出中…" : "导出完整 JSON"}</button></div></header>
    <div className="overview-card-body network-section-body">
      <div className="network-view-switch" aria-label="网络工具视图">
        <button type="button" className={workspaceView === "test" ? "is-active" : ""} aria-pressed={workspaceView === "test"} onClick={() => setWorkspaceView("test")}>单次测试</button>
        <button type="button" className={workspaceView === "history" ? "is-active" : ""} aria-pressed={workspaceView === "history"} onClick={() => setWorkspaceView("history")}>历史趋势 <span>{history.length}</span></button>
      </div>
      {workspaceView === "test" ? <>
      <div className="network-professional-row"><div><strong>{professionalMode ? "专业模式" : "引导模式"}</strong><p>{professionalMode ? "单目标、完整参数与原始结果；所有请求仍由你手动启动。" : "提供国内默认与国际对照预设；选择预设只填入参数，不会自动测试。"}</p></div><label className="network-check"><input type="checkbox" checked={professionalMode} onChange={(event) => setProfessionalMode(event.target.checked)} />专业模式（单目标）</label></div>
      <div className="network-tools-layout">
        <nav className="network-tool-nav" aria-label="选择网络工具">{tools.map(({ id, icon: Icon, description }) => {
          const state = toolStates[id];
          return <button type="button" key={id} className={selectedTool === id ? "is-active" : ""} aria-pressed={selectedTool === id} onClick={() => setSelectedTool(id)}><Icon size={17} /><span><strong>{networkToolLabels[id]}</strong><small>{description}</small></span>{state && state.status !== "idle" && <StatusBadge tone={statusTone(state.status)}>{networkStatusLabels[state.status]}</StatusBadge>}</button>;
        })}</nav>
        <div className="network-tool-workspace">
          <header className="network-tool-heading"><span><SelectedIcon size={20} /></span><div><h3>{networkToolLabels[selectedTool]}</h3><p>{selectedDefinition.description}</p></div>{selectedState && <StatusBadge tone={statusTone(selectedState.status)}>{networkStatusLabels[selectedState.status]}</StatusBadge>}</header>
          <form className="network-tool-form" onSubmit={submit}>
            {!professionalMode && targetPresets[selectedTool] && <div className="network-target-presets"><span>目标预设</span><button type="button" className="button quiet compact" onClick={() => applyTarget(targetPresets[selectedTool]!.domestic)}>国内默认</button><button type="button" className="button quiet compact" onClick={() => applyTarget(targetPresets[selectedTool]!.international)}>国际对照</button><small>仅填入，不运行</small></div>}
            <div className="network-form-grid">{renderForm()}</div>
            <div className="network-run-row"><p>运行即表示允许向当前目标发起一次主动测试。</p>{running && selectedState?.taskId ? <button type="button" className="button secondary" onClick={() => void cancelTask(selectedState.taskId!)}><Square size={14} />取消当前任务</button> : <button type="submit" className="button primary" disabled={running}><Play size={14} />{startingTool === selectedTool ? "正在启动…" : result ? "重新运行" : "运行测试"}</button>}</div>
          </form>
          {running && <ProgressBlock progress={selectedState?.progress} />}
          {selectedState?.error && <div className="network-inline-error" role="alert"><XCircle size={16} /><div><strong>{networkToolLabels[selectedTool]}失败</strong><p>{selectedState.error}</p></div></div>}
          {permissionError && !running && <div className="network-permission-action"><ShieldCheck size={16} /><div><strong>系统拒绝了当前高级测试</strong><p>普通模式无需管理员权限；仅当 Windows 明确报告权限不足时，才需要重启提权。</p></div><button type="button" className="button secondary compact" onClick={() => setAdminConfirm(true)}>以管理员身份重启后重试</button></div>}
          {running && result && <p className="network-stale-note" role="status">新任务正在运行，下方暂时显示上一次结果。</p>}
          {running && selectedTool === "speed" && !result && selectedState?.progress && <NetworkResultChart progress={selectedState.progress} />}
          {result ? <article className={`network-tool-result is-${result.status}`}>
            <header><div><h3>最近一次实测结果</h3><p>{formatDateTime(result.finishedAt)} · 耗时 {(result.durationMs / 1000).toFixed(2)} 秒</p></div><StatusBadge tone={statusTone(result.status)}>{networkStatusLabels[result.status]}</StatusBadge></header>
            <p className="network-result-summary">{result.summary}</p>
            <dl className="network-tool-metrics">{formatNetworkResultMetrics(result).map((metric, index) => <div key={`${index}-${metric}`}><dt>指标 {index + 1}</dt><dd>{metric}</dd></div>)}</dl>
            {result.error && result.error !== selectedState?.error && <p className="network-result-error" role="alert">{result.error}</p>}
            <ResultWarnings result={result} />
            <NetworkResultChart result={result} progress={running ? selectedState?.progress : undefined} />
          </article> : !running && !selectedState?.error && <div className="network-empty compact"><SelectedIcon size={24} /><strong>此工具尚未产生结果</strong><p>核对参数后点击“运行测试”。失败和取消都会如实显示。</p></div>}
        </div>
      </div>
      </> : <NetworkHistoryPanel history={history} enabled={historyEnabled} onEnabledChange={setHistoryEnabled} onClear={clearHistory} onDelete={deleteHistory} />}
    </div>
    {pendingRequest && <ConfirmDialog title="允许发起外部网络测试？" description={`即将运行“${networkToolLabels[pendingRequest.tool]}”。目标服务器、公共 DNS 或测速节点会看到你的出口 IP，并可能记录请求时间、目标和流量。仅本次明确提交的参数会被执行；取消后不会发起测试。`} confirmLabel="了解并运行" onCancel={() => setPendingRequest(null)} onConfirm={() => { const request = pendingRequest; setPendingRequest(null); acknowledgePrivacy(); void execute(request); }} />}
    {exportConfirm && <ConfirmDialog title="导出包含敏感信息的完整 JSON？" description="完整 JSON 不脱敏，可能包含完整公网/本地 IPv4、IPv6、路由跳点、DNS 答案、目标 URL、错误详情及历史记录。请仅保存到可信位置，分享前自行审查。脱敏 Markdown 请使用一键诊断区的导出按钮。" confirmLabel="导出敏感 JSON" onCancel={() => setExportConfirm(false)} onConfirm={() => { setExportConfirm(false); void exportRaw(); }} />}
    {adminConfirm && <ConfirmDialog title="以管理员身份重启后重试？" description="应用将请求 Windows 管理员权限并重新启动，当前网络任务会中断。重启不会自动运行测试；请返回此工具核对参数后手动重试。" confirmLabel="以管理员身份重启" onCancel={() => setAdminConfirm(false)} onConfirm={() => { setAdminConfirm(false); void restartAsAdmin(); }} />}
  </section>;
}
