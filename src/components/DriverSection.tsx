import { useMemo, useState } from "react";
import { CircleAlert, RefreshCw, Search, ShieldCheck, Wrench } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { DriverInfo } from "../lib/types";
import { displayText, driverProblemLabel, formatDateOnly, formatDateTime } from "../lib/systemOverviewFormat";
import { InfoGrid, InfoItem, SectionCard, SensitiveValue, SettingsLink, StatusBadge, WarningList } from "./SystemOverviewCard";

export default function DriverSection() {
  const data = useHarnessStore((state) => state.systemOverview.drivers?.data) as DriverInfo | undefined;
  const scanState = useHarnessStore((state) => state.driverUpdateScan);
  const scanDriverUpdates = useHarnessStore((state) => state.scanDriverUpdates);
  const openSystemSettings = useHarnessStore((state) => state.openSystemSettings);
  const [query, setQuery] = useState("");
  const scanning = scanState.status === "loading";
  const scanResult = data?.updateScan ?? scanState.data;
  const filteredDrivers = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword) return data?.drivers ?? [];
    return (data?.drivers ?? []).filter((item) => `${displayText(item.deviceName, "")} ${displayText(item.provider, "")} ${displayText(item.version, "")} ${displayText(item.className, "")}`.toLowerCase().includes(keyword));
  }, [data, query]);

  return (
    <SectionCard section="drivers" title="驱动管理" description="本地设备检测与 Windows 更新驱动修复" icon={Wrench} hasData={Boolean(data)} action={<SettingsLink target="deviceManager">设备管理器</SettingsLink>}>
      {data && <>
        <div className="driver-summary-grid">
          <div><span>设备总数</span><strong>{data.summary.totalDevices}</strong><small>{data.summary.driverCount} 个已安装驱动</small></div>
          <div><span>异常设备</span><strong>{data.summary.problemCount}</strong><StatusBadge tone={data.summary.problemCount ? "error" : "success"}>{data.summary.problemCount ? "需要关注" : "状态正常"}</StatusBadge></div>
          <div><span>缺失驱动</span><strong>{data.summary.missingDriverCount}</strong><StatusBadge tone={data.summary.missingDriverCount ? "warning" : "success"}>{data.summary.missingDriverCount ? "待安装" : "未发现"}</StatusBadge></div>
          <div><span>签名异常</span><strong>{data.summary.unsignedDriverCount}</strong><StatusBadge tone={data.summary.unsignedDriverCount ? "warning" : "success"}>{data.summary.unsignedDriverCount ? "需要核对" : "未发现"}</StatusBadge></div>
        </div>

        <section className="driver-repair-panel" aria-labelledby="driver-repair-title">
          <div className="driver-repair-heading">
            <span className="driver-repair-icon"><ShieldCheck size={19} /></span>
            <div><h3 id="driver-repair-title">通过 Windows 更新修复驱动</h3><p>使用系统官方更新渠道获取适配驱动。安装与重启均由你在 Windows 更新中确认。</p></div>
          </div>
          <div className="driver-repair-actions">
            <button type="button" className="button secondary compact" onClick={() => void scanDriverUpdates()} disabled={scanning}><RefreshCw size={15} className={scanning ? "spinning" : ""} />{scanning ? "正在扫描…" : "扫描驱动更新"}</button>
            <button type="button" className="button primary compact" onClick={() => void openSystemSettings("windowsUpdate")}>打开 Windows 更新</button>
          </div>
          <ol className="driver-repair-steps">
            <li><span>1</span>扫描可用驱动更新</li>
            <li><span>2</span>在 Windows 更新中检查并安装</li>
            <li><span>3</span>按系统提示完成重启</li>
          </ol>
          <p className="driver-repair-note">扫描只查询更新，不会下载或安装驱动。Windows 更新通常位于“可选更新 → 驱动程序更新”。</p>
        </section>

        {scanState.error && <div className="overview-inline-error driver-scan-error" role="alert"><CircleAlert size={15} /><span>{scanState.error}。也可以直接打开 Windows 更新手动检查。</span></div>}
        {scanResult && <section className="driver-update-results" aria-labelledby="driver-update-title">
          <div className="driver-subsection-heading"><div><h3 id="driver-update-title">Windows 更新提供的驱动</h3><p>扫描于 {formatDateTime(scanResult.scannedAt)}{scanning ? " · 正在重新扫描" : ""}</p></div><StatusBadge tone={scanResult.updates.length ? "warning" : "success"}>{scanResult.updates.length ? `${scanResult.updates.length} 个可用更新` : "暂无更新"}</StatusBadge></div>
          {scanResult.updates.length === 0 ? <p className="driver-empty-state">Windows 更新当前没有提供适用于这台电脑的驱动更新。</p> : <div className="driver-update-list">{scanResult.updates.map((update, index) => <article className="driver-update-row" key={`${update.id ?? update.title}-${index}`}>
            <div className="driver-update-copy"><strong>{displayText(update.title)}</strong><span>{displayText(update.manufacturer)}{update.model ? ` · ${update.model}` : ""}{update.className ? ` · ${update.className}` : ""}</span><small>驱动日期：{formatDateOnly(update.driverDate)}{update.kbArticles.length ? ` · ${update.kbArticles.join("、")}` : ""}</small></div>
            <div className="driver-update-badges">{update.isDownloaded && <StatusBadge tone="success">已下载</StatusBadge>}{update.isOptional && <StatusBadge>可选更新</StatusBadge>}{update.rebootRequired && <StatusBadge tone="warning">可能需重启</StatusBadge>}</div>
          </article>)}</div>}
          <WarningList warnings={scanResult.warnings} />
        </section>}

        <section className="driver-problems" aria-labelledby="driver-problems-title">
          <div className="driver-subsection-heading"><div><h3 id="driver-problems-title">异常设备</h3><p>已排除主动禁用和当前未连接的设备</p></div><StatusBadge tone={data.problems.length ? "error" : "success"}>{data.problems.length ? `${data.problems.length} 项异常` : "未发现异常"}</StatusBadge></div>
          {data.problems.length === 0 ? <p className="driver-empty-state">本地检测未发现需要处理的设备驱动异常。</p> : data.problems.map((problem, index) => <article className="driver-problem-row" key={`${problem.deviceId}-${index}`}>
            <div className="driver-problem-title"><div><strong>{displayText(problem.deviceName)}</strong><span>{displayText(problem.manufacturer)}{problem.className ? ` · ${problem.className}` : ""}</span></div><StatusBadge tone="error">代码 {problem.problemCode}</StatusBadge></div>
            <p className="driver-problem-reason">{driverProblemLabel(problem.problemCode)}</p>
            <InfoGrid>
              <InfoItem label="驱动提供商">{displayText(problem.provider)}</InfoItem>
              <InfoItem label="驱动版本">{displayText(problem.version)}</InfoItem>
              <InfoItem label="驱动日期">{formatDateOnly(problem.driverDate)}</InfoItem>
              <InfoItem label="设备状态">{displayText(problem.status)}</InfoItem>
              <InfoItem label="设备标识" wide><SensitiveValue value={problem.deviceId} /></InfoItem>
            </InfoGrid>
          </article>)}
        </section>

        <details className="installed-software driver-catalog">
          <summary><span>查看完整驱动清单</span><small>{data.drivers.length} 项</small></summary>
          <div className="installed-software-body">
            <label className="software-search"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索设备、提供商、版本或类别" aria-label="搜索驱动" />{query && <button type="button" onClick={() => setQuery("")}>清除</button>}</label>
            <p className="overview-muted">驱动信息来自 Windows 即插即用与签名驱动数据库。{query ? ` 找到 ${filteredDrivers.length} 项。` : ""}</p>
            <div className="installed-software-list">{filteredDrivers.map((driver, index) => <div className="driver-catalog-row" key={`${driver.deviceId}-${driver.infName}-${index}`}>
              <div><strong>{displayText(driver.deviceName)}</strong><span>{displayText(driver.provider)} · {displayText(driver.className)}</span><small>{displayText(driver.infName)}</small></div>
              <div><span>{displayText(driver.version)}</span><small>{formatDateOnly(driver.driverDate)}</small>{driver.problemCode ? <StatusBadge tone="warning">{driverProblemLabel(driver.problemCode)}</StatusBadge> : driver.isSigned === false ? <StatusBadge tone="warning">签名异常</StatusBadge> : <StatusBadge tone="success">正常</StatusBadge>}</div>
            </div>)}{filteredDrivers.length === 0 && <p className="overview-muted">没有匹配的驱动。</p>}</div>
          </div>
        </details>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}
