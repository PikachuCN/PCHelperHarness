import { useEffect } from "react";
import { CheckCircle2, CircleAlert, Info, X } from "lucide-react";
import Sidebar from "./components/Sidebar";
import ChatView from "./components/ChatView";
import Composer from "./components/Composer";
import SettingsModal from "./components/SettingsModal";
import SystemOverviewView from "./components/SystemOverviewView";
import TextContextMenu from "./components/TextContextMenu";
import { IconButton } from "./components/Controls";
import { listenToHarnessEvent } from "./lib/events";
import { useHarnessStore } from "./lib/store";
import type { HarnessEventName } from "./lib/types";

const eventNames: HarnessEventName[] = [
  "user_message",
  "session_updated",
  "assistant_started",
  "assistant_sync",
  "assistant_delta",
  "reasoning_delta",
  "approval_request",
  "approval_confirmation_required",
  "approval_resolved",
  "tool_started",
  "tool_output",
  "tool_done",
  "turn_done",
  "config_path_fallback",
  "config_warning",
  "network_progress",
];

function Toast() {
  const toast = useHarnessStore((state) => state.toast);
  const dismiss = useHarnessStore((state) => state.dismissToast);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(dismiss, toast.tone === "error" ? 6500 : 4000);
    return () => window.clearTimeout(timer);
  }, [toast, dismiss]);

  if (!toast) return null;
  const Icon = toast.tone === "success" ? CheckCircle2 : toast.tone === "error" ? CircleAlert : Info;
  return (
    <div className={`toast toast-${toast.tone}`} role={toast.tone === "error" ? "alert" : "status"}>
      <Icon size={18} />
      <span>{toast.message}</span>
      <IconButton label="关闭通知" onClick={dismiss}><X size={16} /></IconButton>
    </div>
  );
}

export default function App() {
  const initialize = useHarnessStore((state) => state.initialize);
  const settingsOpen = useHarnessStore((state) => state.settingsOpen);
  const activeView = useHarnessStore((state) => state.activeView);
  const theme = useHarnessStore((state) => state.config?.appearance.theme ?? "system");
  const fontSize = useHarnessStore((state) => state.config?.appearance.fontSize ?? 14);

  useEffect(() => {
    void initialize();
  }, [initialize]);

  useEffect(() => {
    const handleEvent = useHarnessStore.getState().handleEvent;
    const disposers = eventNames.map((event) => listenToHarnessEvent(
      event,
      (payload) => handleEvent(event, payload),
      (message) => useHarnessStore.getState().notify(message, "error"),
    ));
    return () => disposers.forEach((dispose) => dispose());
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "dark" : "light") : theme;
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [theme]);

  useEffect(() => {
    document.body.style.fontSize = `${fontSize}px`;
  }, [fontSize]);

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="workspace">
        {activeView === "systemOverview" ? <SystemOverviewView /> : <><ChatView /><Composer /></>}
      </div>
      {settingsOpen && <SettingsModal />}
      <TextContextMenu />
      <Toast />
    </div>
  );
}
