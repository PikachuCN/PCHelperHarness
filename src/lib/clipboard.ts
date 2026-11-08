import { readText as readNativeText, writeText as writeNativeText } from "@tauri-apps/plugin-clipboard-manager";

const isTauri = "__TAURI_INTERNALS__" in window;

function legacyCopy(text: string): boolean {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  return copied;
}

export async function writeClipboardText(text: string): Promise<void> {
  if (isTauri) {
    await writeNativeText(text);
    return;
  }
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // WebView 或浏览器拒绝异步剪贴板权限时，尝试同步复制。
    }
  }
  if (!legacyCopy(text)) throw new Error("无法访问系统剪贴板");
}

export async function readClipboardText(): Promise<string> {
  if (isTauri) return readNativeText();
  if (navigator.clipboard?.readText) return navigator.clipboard.readText();
  throw new Error("当前环境不支持读取剪贴板");
}
