export function displayText(value: unknown, fallback = "未读取"): string {
  if (value === null || value === undefined) return fallback;
  const text = String(value).trim();
  return text || fallback;
}

export function formatBytes(value: unknown): string {
  if (value === null || value === undefined || value === "") return "未读取";
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) return "未读取";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const amount = bytes / 1024 ** index;
  return `${amount.toFixed(index === 0 || amount >= 100 ? 0 : amount >= 10 ? 1 : 2)} ${units[index]}`;
}

function parseDate(value: unknown): Date | null {
  if (!value) return null;
  if (typeof value === "string") {
    const wmi = value.match(/\/Date\((-?\d+)(?:[+-]\d+)?\)\//);
    if (wmi) return new Date(Number(wmi[1]));
    if (/^\d{8}$/.test(value)) return new Date(`${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}T00:00:00`);
  }
  const date = new Date(String(value));
  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatDateTime(value: unknown): string {
  const date = parseDate(value);
  if (!date) return displayText(value);
  return new Intl.DateTimeFormat("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }).format(date);
}

export function formatDateOnly(value: unknown): string {
  const date = parseDate(value);
  if (!date) return displayText(value);
  return new Intl.DateTimeFormat("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit" }).format(date);
}

export function formatUptime(value: unknown): string {
  const totalSeconds = Number(value);
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return "未读取";
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分钟`;
  return `${Math.max(minutes, 1)} 分钟`;
}

export function licenseStatusLabel(status: unknown): string {
  switch (String(status ?? "").toLowerCase()) {
    case "licensed":
    case "已授权":
    case "已激活": return "已激活";
    case "notification": return "通知模式";
    case "unlicensed": return "未激活";
    case "grace": return "宽限期";
    case "outofgrace": return "宽限期已结束";
    default: return displayText(status, "状态未知");
  }
}

export function licenseStatusTone(status: unknown): "success" | "warning" | "error" | "neutral" {
  switch (String(status ?? "").toLowerCase()) {
    case "licensed": return "success";
    case "notification":
    case "grace": return "warning";
    case "unlicensed":
    case "outofgrace": return "error";
    default: return "neutral";
  }
}

export function boolLabel(value: boolean | null | undefined, yes = "已启用", no = "未启用"): string {
  if (value === null || value === undefined) return "未读取";
  return value ? yes : no;
}

export function softwareCategoryLabel(category: unknown): string {
  const labels: Record<string, string> = {
    Browser: "浏览器",
    Communication: "社交通讯",
    Office: "办公软件",
    Compression: "压缩工具",
    Media: "影音播放",
    CloudStorage: "云存储",
    Meeting: "会议协作",
  };
  const key = String(category ?? "");
  return labels[key] ?? displayText(category);
}

export function percentUsed(sizeBytes: number | null, freeBytes: number | null): number {
  if (sizeBytes === null || freeBytes === null || !Number.isFinite(sizeBytes) || !Number.isFinite(freeBytes) || sizeBytes <= 0) return 0;
  return Math.max(0, Math.min(100, ((sizeBytes - freeBytes) / sizeBytes) * 100));
}
