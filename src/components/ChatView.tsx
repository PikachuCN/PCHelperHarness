import { useEffect, useMemo, useRef } from "react";
import { Gauge, HardDrive, PanelLeftOpen, ScanSearch, Wifi } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import MessageItem from "./MessageItem";
import { IconButton } from "./Controls";
import TurnActivityCard from "./TurnActivityCard";

const shortcuts = [
  { icon: Gauge, title: "电脑变慢", description: "检查性能与启动项", prompt: "我的电脑最近明显变慢，请帮我检查原因并优化。" },
  { icon: Wifi, title: "上不了网", description: "排查网络与 DNS", prompt: "我的电脑上不了网，请帮我逐步排查并修复。" },
  { icon: HardDrive, title: "C 盘满了", description: "分析占用并安全清理", prompt: "我的 C 盘满了，请分析空间占用并安全清理。" },
  { icon: ScanSearch, title: "全面体检", description: "检查系统关键状态", prompt: "请给我的电脑做一次全面体检，先进行只读检查。" },
];

export default function ChatView() {
  const messages = useHarnessStore((state) => state.messages);
  const approvals = useHarnessStore((state) => state.approvals);
  const currentSessionId = useHarnessStore((state) => state.currentSessionId);
  const sessions = useHarnessStore((state) => state.sessions);
  const loading = useHarnessStore((state) => state.loadingSession);
  const generatingSessionId = useHarnessStore((state) => state.generatingSessionId);
  const sidebarCollapsed = useHarnessStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useHarnessStore((state) => state.toggleSidebar);
  const sendMessage = useHarnessStore((state) => state.sendMessage);
  const activity = useHarnessStore((state) => state.activity);
  const scrollRef = useRef<HTMLDivElement>(null);
  const currentSession = sessions.find((session) => session.id === currentSessionId);
  const lastAssistantId = useMemo(() => [...messages].reverse().find((message) => message.role === "assistant")?.id, [messages]);
  const activityHere = activity && (activity.sessionId === currentSessionId || (activity.sessionId === null && currentSessionId === null)) ? activity : null;

  useEffect(() => {
    const container = scrollRef.current;
    if (container) container.scrollTop = container.scrollHeight;
  }, [messages, approvals, activity?.updatedAt, activity?.phase]);

  return (
    <main className="main-panel">
      <header className="main-header">
        <div className="main-header-left">
          {sidebarCollapsed && (
            <IconButton label="展开侧栏" onClick={toggleSidebar}><PanelLeftOpen size={18} /></IconButton>
          )}
          <h1>{currentSession?.title ?? "新对话"}</h1>
        </div>
        <span className="local-indicator"><span aria-hidden="true" />本机执行</span>
      </header>
      <div className="chat-scroll" ref={scrollRef}>
        <div className="chat-column">
          {loading ? (
            <div className="loading-session" role="status"><span className="spinner" />正在加载对话…</div>
          ) : messages.length === 0 ? (
            <section className="empty-state" aria-labelledby="empty-title">
              <div className="empty-symbol" aria-hidden="true">H</div>
              <h2 id="empty-title">电脑遇到问题了吗？</h2>
              <p>描述现象，Harness 会先诊断，再在你同意后执行修复。</p>
              <div className="shortcut-grid">
                {shortcuts.map(({ icon: Icon, title, description, prompt }) => (
                  <button key={title} type="button" className="shortcut-card" onClick={() => void sendMessage(prompt)}>
                    <Icon size={19} />
                    <span><strong>{title}</strong><small>{description}</small></span>
                  </button>
                ))}
              </div>
            </section>
          ) : (
            <div className="message-list">
              {messages.map((message) => (
                <MessageItem
                  key={message.id}
                  message={message}
                  sessionId={currentSessionId}
                  generating={generatingSessionId === currentSessionId}
                  isLastAssistant={message.id === lastAssistantId}
                  approvals={approvals.filter((approval) => {
                    if (approval.sessionId !== currentSessionId) return false;
                    if (approval.messageId === message.id) return true;
                    if (message.tools.some((tool) => tool.callId === approval.callId)) return true;
                    const belongsToKnownMessage = messages.some((item) => item.tools.some((tool) => tool.callId === approval.callId));
                    return !approval.messageId && message.id === lastAssistantId && !belongsToKnownMessage;
                  })}
                />
              ))}
            </div>
          )}
          {!loading && activityHere && <TurnActivityCard activity={activityHere} />}
        </div>
      </div>
    </main>
  );
}
