import { useMemo, useState } from "react";
import { AppWindow, BadgeCheck, CircleCheck, CircleX, Cpu, HardDrive, Monitor, Search, ShieldCheck, Wifi } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { ActivationInfo, HardwareInfo, NetworkInfo, SecurityInfo, SoftwareInfo, StorageInfo, SystemBasicInfo } from "../lib/types";
import { boolLabel, displayText, formatBytes, formatDateOnly, formatDateTime, formatUptime, licenseStatusLabel, licenseStatusTone, percentUsed, softwareCategoryLabel } from "../lib/systemOverviewFormat";
import { InfoGrid, InfoItem, SectionCard, SensitiveValue, SettingsLink, StatusBadge, WarningList } from "./SystemOverviewCard";

export function BasicSection() {
  const data = useHarnessStore((state) => state.systemOverview.basic?.data) as SystemBasicInfo | undefined;
  return (
    <SectionCard section="basic" title="系统基础信息" description="Windows 版本、设备型号与运行环境" icon={Monitor} hasData={Boolean(data)}>
      {data && <>
        <InfoGrid>
          <InfoItem label="操作系统" wide>{displayText(data.osName)} {displayText(data.displayVersion, "")}</InfoItem>
          <InfoItem label="版本">{displayText(data.version)}</InfoItem>
          <InfoItem label="内部版本">{displayText(data.build)}</InfoItem>
          <InfoItem label="体系结构">{displayText(data.architecture)}</InfoItem>
          <InfoItem label="设备名">{displayText(data.deviceName)}</InfoItem>
          <InfoItem label="当前用户" wide>{displayText(data.userName)}</InfoItem>
          <InfoItem label="设备型号" wide>{displayText(data.manufacturer)} {displayText(data.model, "")}</InfoItem>
          <InfoItem label="工作组 / 域">{displayText(data.domain)}</InfoItem>
          <InfoItem label="区域">{displayText(data.locale)}</InfoItem>
          <InfoItem label="安装日期">{formatDateOnly(data.installDate)}</InfoItem>
          <InfoItem label="最近启动">{formatDateTime(data.lastBootTime)}</InfoItem>
          <InfoItem label="开机时长">{formatUptime(data.uptimeSeconds)}</InfoItem>
          <InfoItem label="时区" wide>{displayText(data.timeZone)}</InfoItem>
          <InfoItem label="PowerShell">{displayText(data.powerShellVersion)}</InfoItem>
          <InfoItem label="WebView2">{displayText(data.webView2Version)}</InfoItem>
        </InfoGrid>
      </>}
    </SectionCard>
  );
}

export function ActivationSection() {
  const data = useHarnessStore((state) => state.systemOverview.activation?.data) as ActivationInfo | undefined;
  return (
    <SectionCard section="activation" title="激活情况" description="只读取授权状态，不显示产品密钥" icon={BadgeCheck} hasData={Boolean(data)} action={<SettingsLink target="activation">激活设置</SettingsLink>}>
      {data && <>
        <div className="license-panel">
          <div className="license-panel-heading">
            <div><strong>{displayText(data.windows?.name, "Windows")}</strong><span>{displayText(data.windows?.version, "")}</span></div>
            <StatusBadge tone={licenseStatusTone(data.windows?.status)}>{data.windows ? licenseStatusLabel(data.windows.status) : "未读取"}</StatusBadge>
          </div>
          <InfoGrid>
            <InfoItem label="授权类型">{displayText(data.windows?.licenseDescription || data.windows?.licenseChannel)}</InfoItem>
            <InfoItem label="数字许可证">{data.windows ? (data.windows.isDigitalLicense ? "是" : "否") : "未读取"}</InfoItem>
            <InfoItem label="到期时间" wide>{data.windows?.expiresAt ? formatDateTime(data.windows.expiresAt) : "永久授权或未提供"}</InfoItem>
          </InfoGrid>
        </div>
        <div className="office-licenses">
          <h3>Office</h3>
          {data.office.length === 0 ? <p className="overview-muted">未检测到 Office 授权信息；未安装 Office 时属于正常情况。</p> : data.office.map((item, index) => (
            <div className="office-license" key={`${item.name}-${index}`}>
              <div><strong>{displayText(item.licenseDescription || item.name)}</strong><span>{displayText(item.licenseChannel)}</span></div>
              <StatusBadge tone={licenseStatusTone(item.status)}>{licenseStatusLabel(item.status)}</StatusBadge>
              <small>{item.expiresAt ? `授权至 ${formatDateTime(item.expiresAt)}` : "永久授权或未提供到期时间"}</small>
            </div>
          ))}
        </div>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}

export function HardwareSection() {
  const data = useHarnessStore((state) => state.systemOverview.hardware?.data) as HardwareInfo | undefined;
  return (
    <SectionCard section="hardware" title="硬件信息" description="主要部件与插槽明细，不展开完整驱动树" icon={Cpu} hasData={Boolean(data)} className="span-2" action={<SettingsLink target="deviceManager">设备管理器</SettingsLink>}>
      {data && <>
        <InfoGrid>
          <InfoItem label="处理器" wide>{data.processors.length ? data.processors.map((item) => `${displayText(item.name)}（${displayText(item.cores, "?")} 核 ${displayText(item.logicalProcessors, "?")} 线程）`).join("；") : "未读取"}</InfoItem>
          <InfoItem label="主板" wide>{displayText(data.motherboard.manufacturer)} {displayText(data.motherboard.product, "")} · 序列号 <SensitiveValue value={data.motherboard.serialNumber} /></InfoItem>
          <InfoItem label="BIOS" wide>{displayText(data.bios.manufacturer)} {displayText(data.bios.version, "")} · {formatDateOnly(data.bios.releaseDate)}</InfoItem>
          <InfoItem label="内存总容量">{data.memoryTotalBytes > 0 ? formatBytes(data.memoryTotalBytes) : "未读取"}</InfoItem>
          <InfoItem label="内存条数量">{data.memoryModules.length > 0 ? `${data.memoryModules.length} 条` : "未读取"}</InfoItem>
        </InfoGrid>
        {data.memoryModules.length > 0 && <div className="overview-subsection"><h3>内存插槽</h3><div className="overview-table-wrap"><table className="overview-table"><thead><tr><th>插槽</th><th>容量</th><th>频率</th><th>制造商 / 型号</th><th>序列号</th></tr></thead><tbody>{data.memoryModules.map((item, index) => <tr key={`${item.deviceLocator}-${index}`}><td>{displayText(item.deviceLocator || item.bankLabel)}</td><td>{formatBytes(item.capacityBytes)}</td><td>{item.speedMHz ? `${item.speedMHz} MHz` : "未读取"}</td><td>{displayText(`${displayText(item.manufacturer, "")} ${displayText(item.partNumber, "")}`)}</td><td><SensitiveValue value={item.serialNumber} /></td></tr>)}</tbody></table></div></div>}
        <div className="hardware-list">
          {data.graphics.map((item, index) => <div className="hardware-row" key={`${item.name}-${index}`}><span>显卡</span><div><strong>{displayText(item.name)}</strong><small>{item.adapterRamBytes !== null && item.adapterRamBytes > 0 ? formatBytes(item.adapterRamBytes) : "显存未读取"} · 驱动 {displayText(item.driverVersion)} · {displayText(item.resolution || item.videoMode)}</small></div></div>)}
          {data.audio.map((item, index) => <div className="hardware-row" key={`${item.name}-${index}`}><span>声卡</span><div><strong>{displayText(item.name)}</strong><small>{displayText(item.manufacturer)} · {displayText(item.status)}</small></div></div>)}
          {data.batteries.map((item, index) => <div className="hardware-row" key={`${item.name}-${index}`}><span>电池</span><div><strong>{displayText(item.name)}</strong><small>{item.chargePercent === null ? "电量未读取" : `当前电量 ${item.chargePercent}%`} · {displayText(item.status)}</small></div></div>)}
        </div>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}

export function StorageSection() {
  const data = useHarnessStore((state) => state.systemOverview.storage?.data) as StorageInfo | undefined;
  return (
    <SectionCard section="storage" title="存储与磁盘" description="分区容量、剩余空间与物理磁盘健康状态" icon={HardDrive} hasData={Boolean(data)}>
      {data && <>
        <div className="volume-list">
          {data.volumes.map((volume) => {
            const used = percentUsed(volume.sizeBytes, volume.freeBytes);
            const hasCapacity = volume.sizeBytes !== null && volume.freeBytes !== null && volume.sizeBytes > 0;
            const freeRatio = hasCapacity ? volume.freeBytes! / volume.sizeBytes! : 1;
            return <div className="volume-item" key={volume.driveLetter}>
              <div className="volume-heading"><strong>{volume.driveLetter} <span>{displayText(volume.name, "本地磁盘")}</span></strong>{!hasCapacity ? <StatusBadge>容量未读取</StatusBadge> : freeRatio < 0.1 ? <StatusBadge tone="error">空间不足</StatusBadge> : freeRatio < 0.2 ? <StatusBadge tone="warning">空间偏少</StatusBadge> : <StatusBadge tone="success">空间正常</StatusBadge>}</div>
              <div className="volume-capacity"><span style={{ width: `${used}%` }} /></div>
              <div className="volume-meta"><span>剩余 {formatBytes(volume.freeBytes)} / 共 {formatBytes(volume.sizeBytes)}</span><span>已用 {used.toFixed(0)}% · {displayText(volume.fileSystem)}</span></div>
            </div>;
          })}
          {data.volumes.length === 0 && <p className="overview-muted">未读取到固定磁盘分区。</p>}
        </div>
        <div className="overview-subsection"><h3>物理磁盘</h3>{data.physicalDisks.map((disk, index) => <div className="disk-row" key={`${disk.model}-${index}`}><div><strong>{displayText(disk.model)}</strong><small>{displayText(disk.mediaType)} · {displayText(disk.busType)} · {formatBytes(disk.sizeBytes)} · 固件 {displayText(disk.firmwareVersion)}</small><small>序列号 <SensitiveValue value={disk.serialNumber} /></small></div><StatusBadge tone={/healthy|ok|正常/i.test(`${disk.healthStatus} ${disk.operationalStatus}`) ? "success" : "warning"}>{displayText(disk.healthStatus || disk.operationalStatus)}</StatusBadge></div>)}</div>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}

export function NetworkSection() {
  const data = useHarnessStore((state) => state.systemOverview.network?.data) as NetworkInfo | undefined;
  return (
    <SectionCard section="network" title="网络与连接" description="网卡、IP、网关、DNS 与连接状态" icon={Wifi} hasData={Boolean(data)} action={<SettingsLink target="network">网络设置</SettingsLink>}>
      {data && <>
        <div className="network-summary"><StatusBadge tone={data.isConnected ? "success" : "warning"}>{data.isConnected ? "互联网已连接" : "未检测到互联网连接"}</StatusBadge>{data.profiles.map((profile, index) => <span key={`${profile.name}-${index}`}>{displayText(profile.name)} · {displayText(profile.networkCategory)} · {displayText(profile.ipv4Connectivity)}</span>)}</div>
        <div className="network-list">
          {data.adapters.map((adapter, index) => <div className="network-adapter" key={`${adapter.name}-${index}`}>
            <div className="network-adapter-heading"><div><strong>{displayText(adapter.name)}</strong><span>{displayText(adapter.description)}</span></div><StatusBadge tone={adapter.status === "Up" ? "success" : "neutral"}>{adapter.status === "Up" ? "已连接" : displayText(adapter.status)}</StatusBadge></div>
            <InfoGrid>
              <InfoItem label="链路速度">{displayText(adapter.linkSpeed)}</InfoItem>
              <InfoItem label="MAC 地址"><SensitiveValue value={adapter.macAddress} /></InfoItem>
              <InfoItem label="IPv4">{adapter.ipAddresses.length ? adapter.ipAddresses.join("、") : "未分配"}</InfoItem>
              <InfoItem label="默认网关">{adapter.gateways.length ? adapter.gateways.join("、") : "未分配"}</InfoItem>
              <InfoItem label="DNS" wide>{adapter.dnsServers.length ? adapter.dnsServers.join("、") : "未分配"}</InfoItem>
            </InfoGrid>
          </div>)}
        </div>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}

export function SecuritySection() {
  const data = useHarnessStore((state) => state.systemOverview.security?.data) as SecurityInfo | undefined;
  return (
    <SectionCard section="security" title="安全与更新" description="Defender、防火墙、系统安全状态与更新" icon={ShieldCheck} hasData={Boolean(data)} className="span-2" action={<SettingsLink target="security">安全设置</SettingsLink>}>
      {data && <>
        <div className="security-status-grid">
          <div><span>Defender 实时防护</span><StatusBadge tone={data.defender?.realTimeProtectionEnabled ? "success" : "warning"}>{data.defender ? boolLabel(data.defender.realTimeProtectionEnabled) : "未读取"}</StatusBadge></div>
          <div><span>篡改防护</span><StatusBadge tone={data.defender?.tamperProtected ? "success" : "neutral"}>{data.defender ? boolLabel(data.defender.tamperProtected) : "未读取"}</StatusBadge></div>
          <div><span>UAC</span><StatusBadge tone={data.uacEnabled ? "success" : "warning"}>{boolLabel(data.uacEnabled)}</StatusBadge></div>
          <div><span>TPM</span><StatusBadge tone={data.tpm?.ready ? "success" : "neutral"}>{data.tpm ? `${data.tpm.ready ? "已就绪" : "未就绪"} ${data.tpm.specVersion}` : "未读取"}</StatusBadge></div>
          <div><span>Secure Boot</span><StatusBadge tone={data.secureBoot?.enabled ? "success" : "neutral"}>{data.secureBoot ? (data.secureBoot.supported ? boolLabel(data.secureBoot.enabled) : "不支持") : "未读取"}</StatusBadge></div>
          <div><span>等待重启</span><StatusBadge tone={data.pendingReboot === null ? "neutral" : data.pendingReboot ? "warning" : "success"}>{data.pendingReboot === null ? "未读取" : data.pendingReboot ? "需要重启" : "无需重启"}</StatusBadge></div>
        </div>
        <InfoGrid>
          <InfoItem label="病毒库更新">{data.defender ? formatDateTime(data.defender.signatureLastUpdated) : "未读取"}</InfoItem>
          <InfoItem label="最近质量更新">{data.lastUpdate ? `${data.lastUpdate.hotfixId} · ${formatDateOnly(data.lastUpdate.installedOn)}` : "未读取"}</InfoItem>
          <InfoItem label="防火墙" wide>{data.firewallProfiles.length ? data.firewallProfiles.map((profile) => `${displayText(profile.name)}：${boolLabel(profile.enabled)}`).join("；") : "未读取"}</InfoItem>
        </InfoGrid>
        <div className="overview-action-row"><SettingsLink target="windowsUpdate">检查 Windows 更新</SettingsLink></div>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}

export function SoftwareSection() {
  const data = useHarnessStore((state) => state.systemOverview.software?.data) as SoftwareInfo | undefined;
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword) return data?.installed ?? [];
    return (data?.installed ?? []).filter((item) => `${item.name} ${displayText(item.publisher, "")} ${displayText(item.version, "")}`.toLowerCase().includes(keyword));
  }, [data, query]);
  return (
    <SectionCard section="software" title="软件安装情况" description="常用软件状态与完整桌面软件清单" icon={AppWindow} hasData={Boolean(data)} className="span-2" action={<SettingsLink target="installedApps">已安装应用</SettingsLink>}>
      {data && <>
        <div className="common-software-grid">
          {data.common.map((item) => <div className={`software-item ${item.installed ? "is-installed" : ""}`} key={item.id}>
            <span className="software-state">{item.installed ? <CircleCheck size={17} /> : <CircleX size={17} />}</span>
            <div><strong>{item.name}</strong><span>{softwareCategoryLabel(item.category)}{item.installed && item.version ? ` · ${item.version}` : ""}</span></div>
            <StatusBadge tone={item.installed ? "success" : "neutral"}>{item.installed ? "已安装" : "未安装"}</StatusBadge>
          </div>)}
        </div>
        <details className="installed-software">
          <summary><span>查看全部已安装软件</span><small>{data.installed.length} 项</small></summary>
          <div className="installed-software-body">
            <label className="software-search"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索名称、发布者或版本" aria-label="搜索已安装软件" />{query && <button type="button" onClick={() => setQuery("")}>清除</button>}</label>
            <p className="overview-muted">来自 Windows 卸载项，已过滤系统组件和更新。{query ? ` 找到 ${filtered.length} 项。` : ""}</p>
            <div className="installed-software-list">
              {filtered.map((item, index) => <div className="installed-software-row" key={`${item.id}-${index}`}><div><strong>{displayText(item.name)}</strong><span>{displayText(item.publisher)}</span></div><div><span>{displayText(item.version)}</span><small>{formatDateOnly(item.installDate)}</small></div></div>)}
              {filtered.length === 0 && <p className="overview-muted">没有匹配的软件。</p>}
            </div>
          </div>
        </details>
        <WarningList warnings={data.warnings} />
      </>}
    </SectionCard>
  );
}
