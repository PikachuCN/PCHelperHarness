import { useState } from "react";
import { Activity, AlertTriangle, CheckCircle2, Copy, Download, Globe2, Info, Play, ShieldCheck, Square, XCircle, type LucideIcon } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import { writeClipboardText } from "../lib/clipboard";
import { buildNetworkDiagnosticSummary, buildNetworkMarkdown, formatNetworkResultMetrics, networkStatusLabels, networkToolLabels } from "../lib/networkReport";
import type { NetworkRunStatus, NetworkTestResult, NetworkToolId } from "../lib/networkTypes";
import { formatDateTime } from "../lib/systemOverviewFormat";
import { ConfirmDialog } from "./Controls";
import { StatusBadge } from "./SystemOverviewCard";
import { DiagnosticResultOverview, NetworkResultChart } from "./NetworkCharts";
import "../styles/networkTools.css";

const diagnosticTools: readonly { id: NetworkToolId; icon: LucideIcon; note: string }[] = [
  { id: "website", icon: Globe2, note: "DNS、TCP、TLS 与 HTTP 阶段" },
  { id: "publicIp", icon: ShieldCheck, note: "外部服务观察到的出口地址" },
  { id: "ping", icon: Activity, note: "轻量延迟与丢包探测" },
  { id: "dns", icon: Info, note: "系统与公共 DNS 对照" },
  { id: "ipv6", icon: CheckCircle2, note: "地址、DNS、连接与公网四步检测" },
];

function statusTone(status: NetworkRunStatus | "normal"): "success" | "warning" | "error" | "neutral" {
  if (status === "success" || status === "normal") return "success";
  if (status === "warning") return "warning";
  if (status === "error") return "error";
  return "neutral";
}

function ResultWarnings({ result }: { result: NetworkTestResult }) {
  if (!result.data) return null;
  const warnings = "warnings" in result.data ? result.data.warnings : [];
  if (warnings.length === 0) return null;
  return <ul className="network-result-warnings">{warnings.map((warning, index) => <li key={`${index}-${warning}`}><AlertTriangle size={13} />{warning}</li>)}</ul>;
}

export default function NetworkDiagnosticsSection() {
  const diagnostic = useHarnessStore((state) => state.networkDiagnostic);
  const history = useHarnessStore((state) => state.networkHistory);
  const privacyAcknowledged = useHarnessStore((state) => state.networkPrivacyAcknowledged);
  const runDiagnostic = useHarnessStore((state) => state.runNetworkDiagnostic);
  const cancelTask = useHarnessStore((state) => state.cancelNetworkTask);
  const acknowledgePrivacy = useHarnessStore((state) => state.acknowledgeNetworkPrivacy);
  const exportReport = useHarnessStore((state) => state.exportNetworkReport);
  const notify = useHarnessStore((state) => state.notify);
  const [privacyConfirm, setPrivacyConfirm] = useState(false);
  const [starting, setStarting] = useState(false);
  const [copying, setCopying] = useState(false);
  const [exporting, setExporting] = useState(false);
  const report = diagnostic.report;
  const running = diagnostic.status === "running" || starting;
  const progress = diagnostic.progress;
  const progressPercent = progress && progress.total > 0 ? Math.min(100, Math.max(0, (progress.completed / progress.total) * 100)) : undefined;

  const execute = async () => {
    setStarting(true);
    try {
      await runDiagnostic();
    } catch (error) {
      notify(`网络诊断未能完成：${String(error)}`, "error");
    } finally {
      setStarting(false);
    }
  };

  const start = () => {
    if (!privacyAcknowledged) {
      setPrivacyConfirm(true);
      return;
    }
    void execute();
  };

  const copySummary = async () => {
    if (!report || copying) return;
    setCopying(true);
    try {
      await writeClipboardText(buildNetworkDiagnosticSummary(report, history));
      notify("网络诊断摘要已复制，IP 与 MAC 已脱敏", "success");
    } catch (error) {
      notify(`复制网络摘要失败：${String(error)}`, "error");
    } finally {
      setCopying(false);
    }
  };

  const saveMarkdown = async () => {
    if (!report || exporting) return;
    setExporting(true);
    try {
      await exportReport(buildNetworkMarkdown(report, history), "markdown");
    } catch (error) {
      notify(`导出网络报告失败：${String(error)}`, "error");
    } finally {
      setExporting(false);
    }
  };

  return <section className="overview-card network-section" aria-labelledby="network-diagnostic-title">
    <header className="overview-card-header network-section-header">
      <span className="overview-card-icon" aria-hidden="true"><Activity size={19} /></span>
      <div className="overview-card-heading"><h2 id="network-diagnostic-title">一键网络诊断</h2><p>五项轻量检查，用实测证据定位连接、DNS 与协议问题</p></div>
      <div className="overview-card-actions network-header-actions">
        <button type="button" className="button quiet compact" onClick={() => void copySummary()} disabled={!report || copying}><Copy size={14} />{copying ? "复制中…" : "复制摘要"}</button>
        <button type="button" className="button secondary compact" onClick={() => void saveMarkdown()} disabled={!report || exporting}><Download size={14} />{exporting ? "导出中…" : "导出脱敏 Markdown"}</button>
        {running && diagnostic.taskId ? <button type="button" className="button secondary compact" onClick={() => void cancelTask(diagnostic.taskId!)}><Square size={13} />取消诊断</button> : <button type="button" className="button primary compact" onClick={start} disabled={running}><Play size={14} />{starting ? "正在启动…" : report ? "重新诊断" : "开始诊断"}</button>}
      </div>
    </header>
    <div className="overview-card-body network-section-body">
      <div className="network-privacy-note"><ShieldCheck size={17} /><div><strong>按需主动测试，不会在打开页面或“全部刷新”时自动运行</strong><p>诊断不包含测速和路由追踪。网站、公共 DNS 与公网 IP 服务会收到请求；首次运行前会明确说明第三方可看到你的出口 IP。</p></div></div>

      {running && <div className="network-progress" role="status" aria-live="polite">
        <div><strong>{progress?.message || "正在准备诊断任务…"}</strong><span>{progress ? `${progress.completed} / ${progress.total || "?"}` : "等待进度"}</span></div>
        <div className={`network-progress-track${progressPercent === undefined ? " is-indeterminate" : ""}`} aria-hidden="true"><span style={progressPercent === undefined ? undefined : { width: `${progressPercent}%` }} /></div>
        {progress?.phase && <small>阶段：{progress.phase}</small>}
      </div>}

      {diagnostic.error && <div className="network-inline-error" role="alert"><XCircle size={16} /><div><strong>诊断未完成</strong><p>{diagnostic.error}</p></div></div>}

      {!report && !running && !diagnostic.error && <div className="network-empty"><Activity size={28} /><strong>尚未运行网络诊断</strong><p>点击“开始诊断”后才会发起外部请求。失败项目会如实标记，不会用推测结果填充。</p></div>}

      {report && <>
        <div className="network-diagnostic-summary">
          <div><span>总体状态</span><StatusBadge tone={statusTone(report.status)}>{networkStatusLabels[report.status]}</StatusBadge></div>
          <div><span>总耗时</span><strong>{(report.durationMs / 1000).toFixed(1)} 秒</strong></div>
          <div><span>完成时间</span><strong>{formatDateTime(report.finishedAt)}</strong></div>
          <DiagnosticResultOverview report={report} />
        </div>
        {running && <p className="network-stale-note" role="status">正在生成新诊断，下面暂时保留上一次结果。</p>}
        <div className="network-diagnostic-grid">{diagnosticTools.map(({ id, icon: Icon, note }) => {
          const results = report.results.filter((item) => item.tool === id);
          const result = results.find((item) => item.status === "error") ?? results.find((item) => item.status === "warning") ?? results[0];
          return <article className={`network-result-card${result ? ` is-${result.status}` : ""}`} key={id}>
            <header><span className="network-result-icon"><Icon size={17} /></span><div><h3>{networkToolLabels[id]}</h3><p>{note}</p></div><StatusBadge tone={result ? statusTone(result.status) : "neutral"}>{result ? networkStatusLabels[result.status] : "未包含"}</StatusBadge></header>
            {results.length > 0 ? <div className="network-result-content">{results.map((item) => <section className="network-result-run" key={item.taskId}>
              {results.length > 1 && <div className="network-result-target"><strong>{item.target ?? (item.tool === "website" ? item.data?.url ?? "网站目标" : "目标")}</strong><StatusBadge tone={statusTone(item.status)}>{networkStatusLabels[item.status]}</StatusBadge></div>}
              <p className="network-result-summary">{item.summary}</p>
              <dl className="network-result-metrics">{formatNetworkResultMetrics(item).map((metric, index) => <div key={`${index}-${metric}`}><dt>{index === 0 ? "关键指标" : "指标"}</dt><dd>{metric}</dd></div>)}<div><dt>耗时</dt><dd>{(item.durationMs / 1000).toFixed(2)} 秒</dd></div></dl>
              {item.error && <p className="network-result-error" role="alert">{item.error}</p>}
              <ResultWarnings result={item} />
              <NetworkResultChart result={item} />
            </section>)}</div> : <p className="network-muted">本次报告没有返回此项目；这不表示测试通过。</p>}
          </article>;
        })}</div>

        <section className="network-findings" aria-labelledby="network-findings-title"><div className="network-subsection-header"><div><h3 id="network-findings-title">证据结论与处理建议</h3><p>结论只来自本次返回的阶段、采样与错误，不把未测试项目当作正常。</p></div></div>
          {report.findings.length === 0 ? <p className="network-muted">本次没有额外结论。请结合各项原始状态和警告判断。</p> : <div className="network-finding-list">{report.findings.map((finding, index) => <article key={`${index}-${finding.title}`} className={`network-finding is-${finding.severity}`}>
            <span aria-hidden="true">{finding.severity === "error" ? <XCircle size={18} /> : finding.severity === "warning" ? <AlertTriangle size={18} /> : <Info size={18} />}</span>
            <div><h4>{finding.title}<StatusBadge tone={finding.severity === "error" ? "error" : finding.severity === "warning" ? "warning" : "neutral"}>{finding.severity}</StatusBadge></h4><p>{finding.detail}</p><p><strong>建议：</strong>{finding.recommendation}</p></div>
          </article>)}</div>}
        </section>
      </>}
    </div>
    {privacyConfirm && <ConfirmDialog title="允许发起外部网络诊断？" description="诊断将连接目标网站、公共 DNS 和公网 IP 服务。这些第三方会看到你的出口 IP，并可能记录请求时间与目标。诊断不包含测速或路由追踪；你可以取消，取消后不会发起测试。" confirmLabel="了解并开始" onCancel={() => setPrivacyConfirm(false)} onConfirm={() => { setPrivacyConfirm(false); acknowledgePrivacy(); void execute(); }} />}
  </section>;
}
