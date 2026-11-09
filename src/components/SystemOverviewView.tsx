import { useState, type ReactNode } from "react";
import { BadgeCheck, Clock, Copy, Cpu, Download, HardDrive, MemoryStick, Monitor, PanelLeftOpen, RefreshCw, ShieldCheck, type LucideIcon } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import { writeClipboardText } from "../lib/clipboard";
import { buildDiagnosticSummary, buildSystemOverviewMarkdown } from "../lib/systemOverviewReport";
import { displayText, formatBytes, formatDateTime, formatUptime, licenseStatusLabel, licenseStatusTone } from "../lib/systemOverviewFormat";
import type { ActivationInfo, HardwareInfo, StorageInfo, SystemBasicInfo, SystemOverviewSection } from "../lib/types";
import { IconButton } from "./Controls";
import { ActivationSection, BasicSection, HardwareSection, NetworkSection, SecuritySection, SoftwareSection, StorageSection } from "./SystemOverviewSections";
import { StatusBadge } from "./SystemOverviewCard";

interface SummaryCardProps {
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  icon: LucideIcon;
  loading: boolean;
  tone?: "success" | "warning" | "error" | "neutral";
}

function SummaryCard({ label, value, detail, icon: Icon, loading, tone = "neutral" }: SummaryCardProps) {
  return (
    <article className={`overview-summary-card tone-${tone}`}>
      <div className="overview-summary-label"><span>{label}</span><Icon size={17} /></div>
      {loading ? <div className="overview-summary-loading"><span /><span /></div> : <>
        <div className="overview-summary-value">{value}</div>
        {detail && <div className="overview-summary-detail">{detail}</div>}
      </>}
    </article>
  );
}

export default function SystemOverviewView() {
  const snapshot = useHarnessStore((state) => state.systemOverview);
  const sidebarCollapsed = useHarnessStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useHarnessStore((state) => state.toggleSidebar);
  const refresh = useHarnessStore((state) => state.refreshSystemOverview);
  const exportReport = useHarnessStore((state) => state.exportSystemOverview);
  const notify = useHarnessStore((state) => state.notify);
  const [copying, setCopying] = useState(false);
  const [exporting, setExporting] = useState(false);
  const basic = snapshot.basic?.data as SystemBasicInfo | undefined;
  const activation = snapshot.activation?.data as ActivationInfo | undefined;
  const hardware = snapshot.hardware?.data as HardwareInfo | undefined;
  const storage = snapshot.storage?.data as StorageInfo | undefined;
  const volume = storage?.volumes.find((item) => item.driveLetter.toUpperCase() === "C:") ?? storage?.volumes[0];
  const processor = hardware?.processors[0];
  const refreshing = Object.values(snapshot).some((section) => section?.status === "loading");
  const hasData = Object.values(snapshot).some((section) => Boolean(section?.data));
  const latestUpdate = Object.values(snapshot).map((section) => section?.loadedAt).filter((value): value is string => Boolean(value)).sort().at(-1);
  const loading = (section: SystemOverviewSection) => !snapshot[section]?.data && snapshot[section]?.status === "loading";
  const failed = (section: SystemOverviewSection) => !snapshot[section]?.data && snapshot[section]?.status === "error";
  const volumeHasCapacity = Boolean(volume && volume.sizeBytes !== null && volume.freeBytes !== null && volume.sizeBytes > 0);
  const volumeFreeRatio = volumeHasCapacity && volume ? volume.freeBytes! / volume.sizeBytes! : 1;

  const copySummary = async () => {
    if (!hasData || copying) return;
    setCopying(true);
    try {
      await writeClipboardText(buildDiagnosticSummary(snapshot));
      notify("诊断摘要已复制，不包含设备标识或网络标识", "success");
    } catch (error) {
      notify(`复制摘要失败：${String(error)}`, "error");
    } finally {
      setCopying(false);
    }
  };

  const saveReport = async () => {
    if (!hasData || exporting) return;
    setExporting(true);
    try {
      await exportReport(buildSystemOverviewMarkdown(snapshot));
    } finally {
      setExporting(false);
    }
  };

  return (
    <main className="main-panel system-overview-panel">
      <header className="main-header system-overview-header">
        <div className="main-header-left">
          {sidebarCollapsed && <IconButton label="展开侧栏" onClick={toggleSidebar}><PanelLeftOpen size={18} /></IconButton>}
          <div className="system-overview-title"><h1>系统概况</h1><span>本机只读信息</span></div>
        </div>
        <div className="system-overview-header-actions">
          <button type="button" className="button quiet compact" onClick={() => void copySummary()} disabled={!hasData || copying}><Copy size={15} />{copying ? "正在复制…" : "复制摘要"}</button>
          <button type="button" className="button secondary compact" onClick={() => void saveReport()} disabled={!hasData || exporting}><Download size={15} />{exporting ? "正在导出…" : "导出报告"}</button>
          <button type="button" className="button secondary compact" onClick={refresh} disabled={refreshing}><RefreshCw size={15} className={refreshing ? "spinning" : ""} />{refreshing ? "正在刷新" : "全部刷新"}</button>
        </div>
      </header>
      <div className="system-overview-scroll">
        <div className="system-overview-content">
          <section className="overview-page-intro">
            <div><h2>这台电脑的关键状态</h2><p>信息按模块独立读取；刷新失败不会影响其他模块，也不会更改系统设置。</p></div>
            <div className="overview-privacy-note"><ShieldCheck size={16} /><span>敏感标识默认脱敏{latestUpdate ? ` · 最近更新 ${formatDateTime(latestUpdate)}` : ""}</span></div>
          </section>

          <section className="overview-summary-grid" aria-label="设备摘要">
            <SummaryCard label="操作系统" icon={Monitor} loading={loading("basic")} value={failed("basic") ? "读取失败" : basic ? `${displayText(basic.osName)} ${displayText(basic.displayVersion, "")}` : "尚未读取"} detail={basic ? `内部版本 ${displayText(basic.build)} · ${displayText(basic.architecture)}` : undefined} />
            <SummaryCard label="Windows 激活" icon={BadgeCheck} loading={loading("activation")} tone={activation ? licenseStatusTone(activation.windows?.status) : "neutral"} value={failed("activation") ? "读取失败" : activation?.windows ? licenseStatusLabel(activation.windows.status) : "尚未读取"} detail={activation?.windows ? displayText(activation.windows.licenseDescription || activation.windows.licenseChannel) : undefined} />
            <SummaryCard label="处理器" icon={Cpu} loading={loading("hardware")} value={failed("hardware") ? "读取失败" : processor?.name ?? "尚未读取"} detail={processor ? `${displayText(processor.cores, "?")} 核 ${displayText(processor.logicalProcessors, "?")} 线程` : undefined} />
            <SummaryCard label="内存" icon={MemoryStick} loading={loading("hardware")} value={failed("hardware") ? "读取失败" : hardware ? (hardware.memoryTotalBytes > 0 ? formatBytes(hardware.memoryTotalBytes) : "未读取") : "尚未读取"} detail={hardware?.memoryModules.length ? `${hardware.memoryModules.length} 条内存 · ${hardware.memoryModules[0]?.speedMHz || "未知"} MHz` : undefined} />
            <SummaryCard label="系统盘" icon={HardDrive} loading={loading("storage")} tone={!volumeHasCapacity || failed("storage") ? "neutral" : volumeFreeRatio < 0.1 ? "error" : volumeFreeRatio < 0.2 ? "warning" : "success"} value={failed("storage") ? "读取失败" : volume ? `剩余 ${formatBytes(volume.freeBytes)}` : "尚未读取"} detail={volume ? `${volume.driveLetter} · 共 ${formatBytes(volume.sizeBytes)}` : undefined} />
            <SummaryCard label="本次开机" icon={Clock} loading={loading("basic")} value={failed("basic") ? "读取失败" : basic ? formatUptime(basic.uptimeSeconds) : "尚未读取"} detail={basic ? `启动于 ${formatDateTime(basic.lastBootTime)}` : undefined} />
          </section>

          <div className="overview-grid" aria-busy={refreshing}>
            <BasicSection />
            <ActivationSection />
            <HardwareSection />
            <StorageSection />
            <NetworkSection />
            <SecuritySection />
            <SoftwareSection />
          </div>

          <footer className="overview-page-footer">
            <StatusBadge tone="neutral">只读采集</StatusBadge>
            <span>不会读取完整产品密钥；导出的 Markdown 报告默认移除设备、网络与硬件标识。</span>
          </footer>
        </div>
      </div>
    </main>
  );
}
