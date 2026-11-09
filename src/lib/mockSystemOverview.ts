import type { DriverUpdateScanResult, SystemOverviewSection, SystemOverviewSectionResponse } from "./types";

const data = {
  basic: {
    osName: "Windows 11 专业版",
    displayVersion: "24H2",
    version: "10.0.26100",
    build: "26100.1742",
    architecture: "64 位",
    deviceName: "DESKTOP-HARNESS",
    userName: "DESKTOP-HARNESS\\User",
    manufacturer: "LENOVO",
    model: "ThinkPad X1 Carbon Gen 11",
    domain: "WORKGROUP",
    installDate: "2025-10-18T09:30:00+08:00",
    lastBootTime: new Date(Date.now() - 3 * 86_400_000 - 7 * 3_600_000).toISOString(),
    uptimeSeconds: 3 * 86_400 + 7 * 3_600,
    timeZone: "中国标准时间 (UTC+08:00)",
    locale: "zh-CN",
    powerShellVersion: "5.1.26100.1591",
    webView2Version: "142.0.3595.53",
  },
  activation: {
    windows: {
      name: "Windows 11 专业版",
      version: "24H2",
      status: "Licensed",
      statusCode: 1,
      licenseChannel: "Digital",
      licenseDescription: "数字许可证",
      expiresAt: null,
      isDigitalLicense: true,
    },
    office: [{
      name: "Office 16, Office16ProPlus2021VL_KMS_Client edition",
      status: "Licensed",
      statusCode: 1,
      licenseChannel: "VOLUME_KMSCLIENT",
      licenseDescription: "Office 专业增强版 2021",
      expiresAt: "2027-02-05T09:30:00+08:00",
    }],
    warnings: [],
  },
  hardware: {
    processors: [{ name: "13th Gen Intel(R) Core(TM) i7-1360P", manufacturer: "GenuineIntel", cores: 12, logicalProcessors: 16, maxClockMHz: 2200 }],
    motherboard: { manufacturer: "LENOVO", product: "21HMCTO1WW", version: "SDK0T76538", serialNumber: "PF4ABCD123" },
    bios: { manufacturer: "LENOVO", version: "N3CET47W (1.29)", releaseDate: "2025-08-12T00:00:00+08:00", serialNumber: "PF4ABCD123" },
    memoryTotalBytes: 34_359_738_368,
    memoryUsedBytes: 19_532_110_234,
    memoryModules: [
      { capacityBytes: 17_179_869_184, speedMHz: 5200, manufacturer: "Samsung", partNumber: "K3KL3L30CM-BGCT", bankLabel: "BANK 0", deviceLocator: "ChannelA-DIMM0", serialNumber: "3000ABCD" },
      { capacityBytes: 17_179_869_184, speedMHz: 5200, manufacturer: "Samsung", partNumber: "K3KL3L30CM-BGCT", bankLabel: "BANK 2", deviceLocator: "ChannelB-DIMM0", serialNumber: "3000ABCE" },
    ],
    graphics: [{ name: "Intel(R) Iris(R) Xe Graphics", adapterRamBytes: 2_147_483_648, driverVersion: "32.0.101.6129", videoMode: "2880 x 1800 x 4294967296 colors", resolution: "2880 × 1800" }],
    audio: [{ name: "Realtek High Definition Audio", manufacturer: "Realtek", status: "OK" }],
    batteries: [{ name: "LNV-5B10W51866", chargePercent: 86, status: "Good", designVoltageMv: 15_480 }],
    warnings: [],
  },
  drivers: {
    summary: { totalDevices: 168, driverCount: 164, problemCount: 1, missingDriverCount: 1, unsignedDriverCount: 0 },
    problems: [{ deviceId: "USB\\VID_046D&PID_C534\\MOCK", deviceName: "USB 输入设备", className: "HID", manufacturer: "Logitech", provider: null, version: null, driverDate: null, status: "Error", problemCode: 28 }],
    drivers: [
      { deviceId: "PCI\\VEN_8086&DEV_A0F0\\MOCK", deviceName: "Intel(R) Wi-Fi 6E AX211 160MHz", className: "NET", manufacturer: "Intel Corporation", provider: "Intel", version: "23.110.0.5", driverDate: "2025-08-20T00:00:00+08:00", infName: "oem42.inf", isSigned: true, status: "OK", problemCode: 0 },
      { deviceId: "PCI\\VEN_8086&DEV_9A49\\MOCK", deviceName: "Intel(R) Iris(R) Xe Graphics", className: "DISPLAY", manufacturer: "Intel Corporation", provider: "Intel Corporation", version: "32.0.101.6129", driverDate: "2025-09-12T00:00:00+08:00", infName: "oem38.inf", isSigned: true, status: "OK", problemCode: 0 },
      { deviceId: "HDAUDIO\\FUNC_01&VEN_10EC\\MOCK", deviceName: "Realtek High Definition Audio", className: "MEDIA", manufacturer: "Realtek", provider: "Realtek Semiconductor Corp.", version: "6.0.9700.1", driverDate: "2025-06-18T00:00:00+08:00", infName: "oem27.inf", isSigned: true, status: "OK", problemCode: 0 },
      { deviceId: "ACPI\\LEN0300\\MOCK", deviceName: "ThinkPad UltraNav Pointing Device", className: "MOUSE", manufacturer: "LENOVO", provider: "Synaptics", version: "19.5.35.62", driverDate: "2025-04-02T00:00:00+08:00", infName: "oem19.inf", isSigned: true, status: "OK", problemCode: 0 },
      { deviceId: "USB\\VID_046D&PID_C534\\MOCK", deviceName: "USB 输入设备", className: "HID", manufacturer: "Logitech", provider: null, version: null, driverDate: null, infName: null, isSigned: null, status: "Error", problemCode: 28 },
    ],
    updateScan: null,
    warnings: [],
  },
  software: {
    common: [
      { id: "edge", name: "Microsoft Edge", category: "浏览器", installed: true, version: "142.0.3595.53", publisher: "Microsoft Corporation" },
      { id: "chrome", name: "Google Chrome", category: "浏览器", installed: true, version: "142.0.7444.60", publisher: "Google LLC" },
      { id: "firefox", name: "Mozilla Firefox", category: "浏览器", installed: false, version: "", publisher: "Mozilla" },
      { id: "wechat", name: "微信", category: "社交通讯", installed: true, version: "4.1.2.17", publisher: "腾讯科技(深圳)有限公司" },
      { id: "qq", name: "QQ", category: "社交通讯", installed: false, version: "", publisher: "Tencent" },
      { id: "wecom", name: "企业微信", category: "办公协作", installed: true, version: "4.1.38.6005", publisher: "Tencent" },
      { id: "dingtalk", name: "钉钉", category: "办公协作", installed: false, version: "", publisher: "DingTalk" },
      { id: "feishu", name: "飞书", category: "办公协作", installed: false, version: "", publisher: "北京飞书科技有限公司" },
      { id: "wps", name: "WPS Office", category: "办公软件", installed: false, version: "", publisher: "Kingsoft" },
      { id: "office", name: "Microsoft Office", category: "办公软件", installed: true, version: "16.0.18025.20160", publisher: "Microsoft Corporation" },
      { id: "7zip", name: "7-Zip", category: "压缩工具", installed: true, version: "24.09", publisher: "Igor Pavlov" },
      { id: "winrar", name: "WinRAR", category: "压缩工具", installed: false, version: "", publisher: "win.rar GmbH" },
      { id: "potplayer", name: "PotPlayer", category: "影音播放", installed: true, version: "250925", publisher: "Kakao Corp." },
      { id: "vlc", name: "VLC media player", category: "影音播放", installed: false, version: "", publisher: "VideoLAN" },
      { id: "baidunetdisk", name: "百度网盘", category: "云存储", installed: true, version: "7.59.6.103", publisher: "Beijing Baidu Netcom Science Technology Co., Ltd" },
      { id: "tencentmeeting", name: "腾讯会议", category: "办公协作", installed: false, version: "", publisher: "Tencent" },
    ],
    installed: [
      { id: "edge", name: "Microsoft Edge", version: "142.0.3595.53", publisher: "Microsoft Corporation", installDate: "20261101" },
      { id: "chrome", name: "Google Chrome", version: "142.0.7444.60", publisher: "Google LLC", installDate: "20261028" },
      { id: "wechat", name: "微信", version: "4.1.2.17", publisher: "腾讯科技(深圳)有限公司", installDate: "20261018" },
      { id: "wecom", name: "企业微信", version: "4.1.38.6005", publisher: "Tencent", installDate: "20260922" },
      { id: "office", name: "Microsoft Office 专业增强版 2021", version: "16.0.18025.20160", publisher: "Microsoft Corporation", installDate: "20251020" },
      { id: "7zip", name: "7-Zip 24.09 (x64)", version: "24.09", publisher: "Igor Pavlov", installDate: "20251018" },
      { id: "potplayer", name: "PotPlayer-64 bit", version: "250925", publisher: "Kakao Corp.", installDate: "20251103" },
      { id: "baidunetdisk", name: "百度网盘", version: "7.59.6.103", publisher: "Beijing Baidu Netcom Science Technology Co., Ltd", installDate: "20261006" },
      { id: "vscode", name: "Microsoft Visual Studio Code", version: "1.106.0", publisher: "Microsoft Corporation", installDate: "20261105" },
      { id: "git", name: "Git", version: "2.51.2", publisher: "The Git Development Community", installDate: "20261016" },
    ],
    warnings: [],
  },
  storage: {
    volumes: [
      { driveLetter: "C:", name: "Windows", fileSystem: "NTFS", sizeBytes: 512_110_190_592, freeBytes: 98_129_174_528, driveType: 3, healthStatus: "Healthy" },
      { driveLetter: "D:", name: "Data", fileSystem: "NTFS", sizeBytes: 1_512_110_190_592, freeBytes: 827_294_334_976, driveType: 3, healthStatus: "Healthy" },
    ],
    physicalDisks: [
      { model: "WD_BLACK SN770 2TB", mediaType: "SSD", busType: "NVMe", sizeBytes: 2_000_398_934_016, healthStatus: "Healthy", operationalStatus: "OK", serialNumber: "E8238FA6BF5300001001", firmwareVersion: "731100WD" },
    ],
    warnings: [],
  },
  network: {
    isConnected: true,
    profiles: [{ name: "Office-WiFi", interfaceAlias: "WLAN", networkCategory: "Private", ipv4Connectivity: "Internet" }],
    adapters: [
      { name: "WLAN", description: "Intel(R) Wi-Fi 6E AX211 160MHz", status: "Up", linkSpeed: "1.2 Gbps", macAddress: "8C:1D:96:12:AB:34", ipAddresses: ["192.168.31.108"], gateways: ["192.168.31.1"], dnsServers: ["192.168.31.1"] },
      { name: "以太网", description: "Realtek PCIe GbE Family Controller", status: "Disconnected", linkSpeed: "0 bps", macAddress: "8C:1D:96:12:CD:56", ipAddresses: [], gateways: [], dnsServers: [] },
    ],
    warnings: [],
  },
  security: {
    defender: { serviceEnabled: true, antivirusEnabled: true, realTimeProtectionEnabled: true, tamperProtected: true, signatureLastUpdated: new Date(Date.now() - 5 * 3_600_000).toISOString() },
    firewallProfiles: [{ name: "Domain", enabled: true }, { name: "Private", enabled: true }, { name: "Public", enabled: true }],
    uacEnabled: true,
    tpm: { present: true, ready: true, enabled: true, activated: true, specVersion: "2.0" },
    secureBoot: { supported: true, enabled: true },
    lastUpdate: { hotfixId: "KB5066835", installedOn: "2026-10-15T10:20:00+08:00", description: "Security Update" },
    pendingReboot: false,
    warnings: [],
  },
} as const;

export function mockSystemOverviewSection(section: SystemOverviewSection): SystemOverviewSectionResponse {
  return structuredClone({ section, data: data[section] }) as SystemOverviewSectionResponse;
}

export function mockDriverUpdateScan(): DriverUpdateScanResult {
  return {
    scannedAt: new Date().toISOString(),
    updates: [
      { id: "mock-intel-chipset-2026", title: "Intel Corporation - System - 10.1.19614.8666", manufacturer: "Intel Corporation", model: "Intel Chipset Device", className: "System", driverDate: "2026-08-14T00:00:00+08:00", deviceId: "PCI\\VEN_8086&DEV_9A03\\MOCK", hardwareId: "PCI\\VEN_8086&DEV_9A03", isDownloaded: false, isMandatory: false, isOptional: true, rebootRequired: true, kbArticles: [] },
      { id: "mock-realtek-audio-2026", title: "Realtek Semiconductor Corp. - MEDIA - 6.0.9812.1", manufacturer: "Realtek Semiconductor Corp.", model: "Realtek High Definition Audio", className: "MEDIA", driverDate: "2026-09-22T00:00:00+08:00", deviceId: "HDAUDIO\\FUNC_01&VEN_10EC\\MOCK", hardwareId: "HDAUDIO\\FUNC_01&VEN_10EC", isDownloaded: true, isMandatory: false, isOptional: true, rebootRequired: false, kbArticles: [] },
    ],
    warnings: [],
  };
}
