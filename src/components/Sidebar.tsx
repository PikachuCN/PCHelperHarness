import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, MouseEvent as ReactMouseEvent } from "react";
import {
  Download,
  MessageSquareText,
  Monitor,
  PanelLeftClose,
  Pencil,
  Plus,
  Settings,
  ShieldCheck,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { SessionSummary } from "../lib/types";
import appIcon from "../../assets/icon.svg";
import { ConfirmDialog, IconButton } from "./Controls";

interface SessionGroup {
  label: string;
  items: SessionSummary[];
}

function groupSessions(sessions: SessionSummary[]): SessionGroup[] {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const yesterday = today - 86_400_000;
  const sevenDays = today - 6 * 86_400_000;
  const groups: SessionGroup[] = [
    { label: "今天", items: [] },
    { label: "昨天", items: [] },
    { label: "近 7 天", items: [] },
    { label: "更早", items: [] },
  ];
  for (const session of sessions) {
    const time = new Date(session.updatedAt).getTime();
    if (time >= today) groups[0].items.push(session);
    else if (time >= yesterday) groups[1].items.push(session);
    else if (time >= sevenDays) groups[2].items.push(session);
    else groups[3].items.push(session);
  }
  return groups.filter((group) => group.items.length > 0);
}

export default function Sidebar() {
  const sessions = useHarnessStore((state) => state.sessions);
  const currentSessionId = useHarnessStore((state) => state.currentSessionId);
  const collapsed = useHarnessStore((state) => state.sidebarCollapsed);
  const activeView = useHarnessStore((state) => state.activeView);
  const openSystemOverview = useHarnessStore((state) => state.openSystemOverview);
  const envInfo = useHarnessStore((state) => state.envInfo);
  const newSession = useHarnessStore((state) => state.newSession);
  const creatingSession = useHarnessStore((state) => state.creatingSession);
  const selectSession = useHarnessStore((state) => state.selectSession);
  const renameSession = useHarnessStore((state) => state.renameSession);
  const deleteSession = useHarnessStore((state) => state.deleteSession);
  const exportSession = useHarnessStore((state) => state.exportSession);
  const toggleSidebar = useHarnessStore((state) => state.toggleSidebar);
  const setSettingsOpen = useHarnessStore((state) => state.setSettingsOpen);
  const restartAsAdmin = useHarnessStore((state) => state.restartAsAdmin);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState("");
  const [deleting, setDeleting] = useState<SessionSummary | null>(null);
  const [restartConfirm, setRestartConfirm] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ session: SessionSummary; x: number; y: number } | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);
  const groups = useMemo(() => groupSessions(sessions), [sessions]);

  useEffect(() => {
    if (!contextMenu) return;
    const closeOutside = (event: PointerEvent) => {
      if (!contextMenuRef.current?.contains(event.target as Node)) setContextMenu(null);
    };
    const closeKeyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") setContextMenu(null);
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeKeyboard);
    contextMenuRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeKeyboard);
    };
  }, [contextMenu]);

  const openContextMenu = (event: ReactMouseEvent, session: SessionSummary) => {
    event.preventDefault();
    event.stopPropagation();
    const width = 216;
    const height = 184;
    setContextMenu({
      session,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
    });
  };

  const submitRename = async (event: FormEvent, sessionId: string) => {
    event.preventDefault();
    if (await renameSession(sessionId, editingTitle)) setEditingId(null);
  };

  return (
    <>
      {!collapsed && <button className="sidebar-mobile-backdrop" aria-label="关闭侧栏" onClick={toggleSidebar} />}
      <aside className={`sidebar ${collapsed ? "is-collapsed" : ""}`} aria-label="对话侧栏" aria-hidden={collapsed}>
        <div className="sidebar-header">
          <button type="button" className="brand" onClick={() => void newSession()} disabled={creatingSession} tabIndex={collapsed ? -1 : 0}>
            <img className="brand-mark" src={appIcon} alt="" />
            <span>灵犀电脑管家</span>
          </button>
          <IconButton label="收起侧栏" onClick={toggleSidebar} tabIndex={collapsed ? -1 : 0}>
            <PanelLeftClose size={18} />
          </IconButton>
        </div>
        <div className="new-chat-wrap">
          <button type="button" className="new-chat-button" onClick={() => void newSession()} disabled={creatingSession} tabIndex={collapsed ? -1 : 0}>
            <Plus size={17} />
            {creatingSession ? "正在创建…" : "新对话"}
          </button>
        </div>
        <div className="sidebar-overview-wrap">
          <button
            type="button"
            className={`sidebar-overview-button ${activeView === "systemOverview" ? "is-active" : ""}`}
            onClick={openSystemOverview}
            aria-current={activeView === "systemOverview" ? "page" : undefined}
            tabIndex={collapsed ? -1 : 0}
          >
            <Monitor size={17} />
            <span>系统概况</span>
          </button>
        </div>
        <nav className="session-list" aria-label="历史对话">
          {groups.length === 0 && <p className="empty-sessions">还没有历史对话</p>}
          {groups.map((group) => (
            <section className="session-group" key={group.label}>
              <h2>{group.label}</h2>
              <ul>
                {group.items.map((session) => (
                  <li key={session.id} className={`session-item ${activeView === "chat" && currentSessionId === session.id ? "is-current" : ""}`} onContextMenu={(event) => openContextMenu(event, session)}>
                    {editingId === session.id ? (
                      <form className="rename-form" onSubmit={(event) => void submitRename(event, session.id)}>
                        <input
                          value={editingTitle}
                          onChange={(event) => setEditingTitle(event.target.value)}
                          onBlur={() => setEditingId(null)}
                          onKeyDown={(event) => {
                            if (event.key === "Escape") setEditingId(null);
                          }}
                          aria-label="对话名称"
                          maxLength={60}
                          autoFocus
                        />
                      </form>
                    ) : (
                      <>
                        <button
                          type="button"
                          className="session-open"
                          onClick={() => void selectSession(session.id)}
                          aria-current={activeView === "chat" && currentSessionId === session.id ? "page" : undefined}
                          tabIndex={collapsed ? -1 : 0}
                        >
                          <span>{session.title}</span>
                        </button>
                        <div className="session-actions">
                          <IconButton label={`重命名 ${session.title}`} onClick={() => { setEditingId(session.id); setEditingTitle(session.title); }} tabIndex={collapsed ? -1 : 0}>
                            <Pencil size={15} />
                          </IconButton>
                          <IconButton label={`导出 ${session.title}`} onClick={() => void exportSession(session.id)} tabIndex={collapsed ? -1 : 0}>
                            <Download size={15} />
                          </IconButton>
                          <IconButton label={`删除 ${session.title}`} onClick={() => setDeleting(session)} tabIndex={collapsed ? -1 : 0}>
                            <Trash2 size={15} />
                          </IconButton>
                        </div>
                      </>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </nav>
        <div className="sidebar-footer">
          <div className="admin-row">
            <span className={`admin-status ${envInfo?.isAdmin ? "is-admin" : ""}`}>
              {envInfo?.isAdmin ? <ShieldCheck size={16} /> : <ShieldAlert size={16} />}
              <span>{envInfo?.isAdmin ? "管理员模式" : "标准用户模式"}</span>
            </span>
            {!envInfo?.isAdmin && (
              <button type="button" className="text-button" onClick={() => setRestartConfirm(true)} tabIndex={collapsed ? -1 : 0}>
                以管理员重启
              </button>
            )}
          </div>
          <button type="button" className="settings-button" onClick={() => setSettingsOpen(true)} tabIndex={collapsed ? -1 : 0}>
            <Settings size={17} />
            设置
          </button>
        </div>
      </aside>
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="session-context-menu"
          role="menu"
          aria-label={`会话操作：${contextMenu.session.title}`}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onContextMenu={(event) => event.preventDefault()}
        >
          <button type="button" role="menuitem" onClick={() => { const id = contextMenu.session.id; setContextMenu(null); void selectSession(id); }}>
            <MessageSquareText size={16} />打开并查看原文
          </button>
          <button type="button" role="menuitem" onClick={() => { setEditingId(contextMenu.session.id); setEditingTitle(contextMenu.session.title); setContextMenu(null); }}>
            <Pencil size={16} />重命名对话
          </button>
          <button type="button" role="menuitem" onClick={() => { const session = contextMenu.session; setContextMenu(null); void exportSession(session.id); }}>
            <Download size={16} />导出 Markdown
          </button>
          <div className="context-menu-divider" />
          <button type="button" role="menuitem" className="is-danger" onClick={() => { setDeleting(contextMenu.session); setContextMenu(null); }}>
            <Trash2 size={16} />删除对话
          </button>
        </div>
      )}
      {deleting && (
        <ConfirmDialog
          title="删除这段对话？"
          description={`“${deleting.title}”将从本机历史记录中移除，此操作无法撤销。`}
          confirmLabel="删除"
          danger
          onCancel={() => setDeleting(null)}
          onConfirm={() => { void deleteSession(deleting.id); setDeleting(null); }}
        />
      )}
      {restartConfirm && (
        <ConfirmDialog
          title="以管理员身份重启？"
          description="应用将请求 Windows 管理员权限并重新启动。当前未保存的界面更改会丢失。"
          confirmLabel="继续重启"
          onCancel={() => setRestartConfirm(false)}
          onConfirm={() => { setRestartConfirm(false); void restartAsAdmin(); }}
        />
      )}
    </>
  );
}
