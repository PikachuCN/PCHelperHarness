import type { ActivationInfo, HardwareInfo, NetworkInfo, SecurityInfo, SoftwareInfo, StorageInfo, SystemBasicInfo, SystemOverviewSnapshot } from "./types";
import { boolLabel, displayText, formatBytes, formatDateOnly, formatDateTime, formatUptime, licenseStatusLabel, percentUsed, softwareCategoryLabel } from "./systemOverviewFormat";

function sectionData<T>(snapshot: SystemOverviewSnapshot, section: keyof SystemOverviewSnapshot): T | undefined {
  return snapshot[section]?.data as T | undefined;
}

function latestLoadedAt(snapshot: SystemOverviewSnapshot): string {
  const timestamps = Object.values(snapshot).map((section) => section?.loadedAt).filter((value): value is string => Boolean(value)).sort();
  return timestamps[timestamps.length - 1] ?? new Date().toISOString();
}

function systemVolume(storage?: StorageInfo) {
  return storage?.volumes.find((volume) => volume.driveLetter.toUpperCase() === "C:") ?? storage?.volumes[0];
}

export function buildDiagnosticSummary(snapshot: SystemOverviewSnapshot): string {
  const basic = sectionData<SystemBasicInfo>(snapshot, "basic");
  const activation = sectionData<ActivationInfo>(snapshot, "activation");
  const hardware = sectionData<HardwareInfo>(snapshot, "hardware");
  const storage = sectionData<StorageInfo>(snapshot, "storage");
  const network = sectionData<NetworkInfo>(snapshot, "network");
  const security = sectionData<SecurityInfo>(snapshot, "security");
  const volume = systemVolume(storage);
  const processor = hardware?.processors[0]?.name;
  const lines = [
    "Harness 系统诊断摘要",
    `系统：${basic ? `${displayText(basic.osName)} ${displayText(basic.displayVersion, "")}（${displayText(basic.build)}）` : "未读取"}`,
    `Windows 激活：${activation?.windows ? licenseStatusLabel(activation.windows.status) : "未读取"}`,
    `处理器：${displayText(processor)}`,
    `内存：${hardware?.memoryTotalBytes ? formatBytes(hardware.memoryTotalBytes) : "未读取"}`,
    `系统盘：${volume ? `${volume.driveLetter} 剩余 ${formatBytes(volume.freeBytes)} / ${formatBytes(volume.sizeBytes)}（已用 ${percentUsed(volume.sizeBytes, volume.freeBytes).toFixed(0)}%）` : "未读取"}`,
    `开机时长：${basic ? formatUptime(basic.uptimeSeconds) : "未读取"}`,
    `网络：${network ? (network.isConnected ? "已连接互联网" : "未检测到互联网连接") : "未读取"}`,
    `Defender 实时防护：${security?.defender ? boolLabel(security.defender.realTimeProtectionEnabled) : "未读取"}`,
    `等待重启：${security?.pendingReboot === null || !security ? "未读取" : security.pendingReboot ? "是" : "否"}`,
    `信息采集时间：${formatDateTime(latestLoadedAt(snapshot))}`,
    "说明：摘要仅包含诊断所需的关键状态，不包含设备名、用户名、IP、MAC、序列号或产品密钥。",
  ];
  return lines.join("\n");
}

function cell(value: unknown): string {
  return displayText(value).replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
}

function table(lines: string[], headers: string[], rows: unknown[][]) {
  lines.push(`| ${headers.join(" | ")} |`, `| ${headers.map(() => "---").join(" | ")} |`);
  if (rows.length === 0) lines.push(`| ${headers.map((_, index) => index === 0 ? "无记录" : "—").join(" | ")} |`);
  else for (const row of rows) lines.push(`| ${row.map(cell).join(" | ")} |`);
  lines.push("");
}

function unavailable(lines: string[], available: boolean): boolean {
  if (available) return false;
  lines.push("本模块未加载或读取失败。", "");
  return true;
}

function redactSensitiveIdentifiers(content: string, snapshot: SystemOverviewSnapshot): string {
  const basic = sectionData<SystemBasicInfo>(snapshot, "basic");
  const hardware = sectionData<HardwareInfo>(snapshot, "hardware");
  const storage = sectionData<StorageInfo>(snapshot, "storage");
  const network = sectionData<NetworkInfo>(snapshot, "network");
  const identifiers = new Set<string>();
  const add = (value: unknown) => {
    const text = String(value ?? "").trim();
    if (text.length >= 4 && !/^未读取$|^0+$/.test(text)) identifiers.add(text);
  };
  add(basic?.deviceName);
  add(basic?.userName);
  add(hardware?.motherboard.serialNumber);
  add(hardware?.bios.serialNumber);
  hardware?.memoryModules.forEach((item) => add(item.serialNumber));
  storage?.physicalDisks.forEach((item) => add(item.serialNumber));
  network?.adapters.forEach((item) => {
    add(item.macAddress);
    item.ipAddresses.forEach(add);
    item.gateways.forEach(add);
    item.dnsServers.forEach(add);
  });
  return [...identifiers].sort((a, b) => b.length - a.length).reduce((result, identifier) => result.split(identifier).join("已脱敏"), content);
}

export function buildSystemOverviewMarkdown(snapshot: SystemOverviewSnapshot): string {
  const basic = sectionData<SystemBasicInfo>(snapshot, "basic");
  const activation = sectionData<ActivationInfo>(snapshot, "activation");
  const hardware = sectionData<HardwareInfo>(snapshot, "hardware");
  const software = sectionData<SoftwareInfo>(snapshot, "software");
  const storage = sectionData<StorageInfo>(snapshot, "storage");
  const network = sectionData<NetworkInfo>(snapshot, "network");
  const security = sectionData<SecurityInfo>(snapshot, "security");
  const lines = [
    "# Harness 系统概况报告（脱敏）",
    "",
    `生成时间：${formatDateTime(new Date().toISOString())}`,
    "",
    "> 本报告由本机只读信息生成。设备名、用户名、IP、MAC 和硬件序列号已脱敏；不收集或显示产品密钥。",
    "",
    "## 诊断摘要",
    "",
    "```text",
    buildDiagnosticSummary(snapshot),
    "```",
    "",
    "## 系统基础信息",
    "",
  ];
  if (!unavailable(lines, Boolean(basic))) {
    table(lines, ["项目", "值"], [
      ["操作系统", `${displayText(basic!.osName)} ${displayText(basic!.displayVersion, "")}`],
      ["版本 / 内部版本", `${displayText(basic!.version)} / ${displayText(basic!.build)}`],
      ["体系结构", basic!.architecture],
      ["设备名", "已脱敏"],
      ["当前用户", "已脱敏"],
      ["设备型号", `${displayText(basic!.manufacturer, "")} ${displayText(basic!.model, "")}`],
      ["安装日期", formatDateTime(basic!.installDate)],
      ["最近启动", formatDateTime(basic!.lastBootTime)],
      ["开机时长", formatUptime(basic!.uptimeSeconds)],
      ["时区 / 区域", `${displayText(basic!.timeZone)} / ${displayText(basic!.locale)}`],
      ["PowerShell / WebView2", `${displayText(basic!.powerShellVersion)} / ${displayText(basic!.webView2Version)}`],
    ]);
  }

  lines.push("## 激活情况", "");
  if (!unavailable(lines, Boolean(activation))) {
    table(lines, ["产品", "状态", "授权类型", "到期时间"], [
      [activation!.windows?.name ?? "Windows", activation!.windows ? licenseStatusLabel(activation!.windows.status) : "未读取", activation!.windows?.licenseDescription || activation!.windows?.licenseChannel, activation!.windows?.expiresAt ? formatDateTime(activation!.windows.expiresAt) : "永久或未提供"],
      ...activation!.office.map((item) => [item.name, licenseStatusLabel(item.status), item.licenseDescription || item.licenseChannel, item.expiresAt ? formatDateTime(item.expiresAt) : "永久或未提供"]),
    ]);
  }

  lines.push("## 硬件信息", "");
  if (!unavailable(lines, Boolean(hardware))) {
    table(lines, ["类别", "型号 / 详情"], [
      ...hardware!.processors.map((item) => ["处理器", `${displayText(item.name)}；${displayText(item.cores, "?")} 核 ${displayText(item.logicalProcessors, "?")} 线程`]),
      ["主板", `${displayText(hardware!.motherboard.manufacturer, "")} ${displayText(hardware!.motherboard.product, "")}；序列号已脱敏`],
      ["BIOS", `${displayText(hardware!.bios.manufacturer, "")} ${displayText(hardware!.bios.version, "")}；${formatDateOnly(hardware!.bios.releaseDate)}`],
      ...hardware!.memoryModules.map((item) => ["内存", `${formatBytes(item.capacityBytes)} @ ${item.speedMHz || "未知"} MHz；${displayText(item.manufacturer, "")} ${displayText(item.partNumber, "")}`]),
      ...hardware!.graphics.map((item) => ["显卡", `${displayText(item.name)}；${formatBytes(item.adapterRamBytes)}；${displayText(item.resolution || item.videoMode)}`]),
      ...hardware!.audio.map((item) => ["音频", `${displayText(item.name)}；${displayText(item.status)}`]),
      ...hardware!.batteries.map((item) => ["电池", `${displayText(item.name)}；电量 ${item.chargePercent ?? "未知"}%`]),
    ]);
  }

  lines.push("## 存储与磁盘", "");
  if (!unavailable(lines, Boolean(storage))) {
    table(lines, ["盘符", "卷标", "文件系统", "总容量", "剩余", "已用比例"], storage!.volumes.map((item) => [item.driveLetter, item.name, item.fileSystem, formatBytes(item.sizeBytes), formatBytes(item.freeBytes), `${percentUsed(item.sizeBytes, item.freeBytes).toFixed(0)}%`]));
    table(lines, ["物理磁盘", "类型", "总线", "容量", "健康状态"], storage!.physicalDisks.map((item) => [item.model, item.mediaType, item.busType, formatBytes(item.sizeBytes), item.healthStatus || item.operationalStatus]));
  }

  lines.push("## 网络与连接", "");
  if (!unavailable(lines, Boolean(network))) {
    lines.push(`互联网连接：${network!.isConnected ? "已连接" : "未检测到"}`, "");
    table(lines, ["网卡", "状态", "链路速度", "IP / 网关 / DNS", "MAC"], network!.adapters.map((item) => [item.description || item.name, item.status, item.linkSpeed, "已脱敏", "已脱敏"]));
  }

  lines.push("## 安全与更新", "");
  if (!unavailable(lines, Boolean(security))) {
    table(lines, ["项目", "状态"], [
      ["Defender 服务", security!.defender ? boolLabel(security!.defender.serviceEnabled) : "未读取"],
      ["实时防护", security!.defender ? boolLabel(security!.defender.realTimeProtectionEnabled) : "未读取"],
      ["篡改防护", security!.defender ? boolLabel(security!.defender.tamperProtected) : "未读取"],
      ["病毒库更新", security!.defender ? formatDateTime(security!.defender.signatureLastUpdated) : "未读取"],
      ["防火墙", security!.firewallProfiles.map((item) => `${displayText(item.name)}: ${boolLabel(item.enabled)}`).join("；")],
      ["UAC", boolLabel(security!.uacEnabled)],
      ["TPM", security!.tpm ? `${security!.tpm.specVersion || "已检测"}；${security!.tpm.ready ? "就绪" : "未就绪"}` : "未检测到或未读取"],
      ["Secure Boot", security!.secureBoot ? (security!.secureBoot.supported ? boolLabel(security!.secureBoot.enabled) : "当前固件不支持") : "未读取"],
      ["最近更新", security!.lastUpdate ? `${displayText(security!.lastUpdate.hotfixId)}；${formatDateOnly(security!.lastUpdate.installedOn)}` : "未读取"],
      ["等待重启", security!.pendingReboot === null ? "未读取" : security!.pendingReboot ? "是" : "否"],
    ]);
  }

  lines.push("## 常用软件", "");
  if (!unavailable(lines, Boolean(software))) {
    table(lines, ["软件", "分类", "安装状态", "版本"], software!.common.map((item) => [item.name, softwareCategoryLabel(item.category), item.installed ? "已安装" : "未安装", item.installed ? item.version : "—"]));
    lines.push("## 全部已安装桌面软件", "", "以下来自 Windows 卸载项，已过滤系统组件和更新。", "");
    table(lines, ["名称", "版本", "发布者", "安装日期"], software!.installed.map((item) => [item.name, item.version, item.publisher, formatDateOnly(item.installDate)]));
  }

  const warnings = Object.values(snapshot).flatMap((section) => {
    const value = section?.data as { warnings?: string[] } | undefined;
    return value?.warnings ?? [];
  });
  if (warnings.length > 0) {
    lines.push("## 采集说明", "");
    for (const warning of warnings) lines.push(`- ${warning}`);
    lines.push("");
  }
  return redactSensitiveIdentifiers(lines.join("\n"), snapshot);
}
