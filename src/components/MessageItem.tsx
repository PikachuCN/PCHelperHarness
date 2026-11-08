import { useEffect, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { createPortal } from "react-dom";
import { BrainCircuit, Braces, ChevronDown, Copy, LoaderCircle } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { writeClipboardText } from "../lib/clipboard";
import { useHarnessStore } from "../lib/store";
import type { ChatMessage, PendingApproval } from "../lib/types";
import ToolCard from "./ToolCard";
import ApprovalCard from "./ApprovalCard";
import PromptDialog from "./PromptDialog";

function formatMessageTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

interface MessageItemProps {
  message: ChatMessage;
  sessionId: string | null;
  approvals: PendingApproval[];
  generating: boolean;
  isLastAssistant: boolean;
}

export default function MessageItem({ message, sessionId, approvals, generating, isLastAssistant }: MessageItemProps) {
  const timestamp = formatMessageTime(message.createdAt);
  const [reasoningOpen, setReasoningOpen] = useState(true);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const [promptOpen, setPromptOpen] = useState(false);
  const contextMenuRef = useRef<HTMLDivElement>(null);
  const reasoningRef = useRef<HTMLDivElement>(null);
  const notify = useHarnessStore((state) => state.notify);
  const hasReasoning = Boolean(message.reasoning);
  useEffect(() => {
    if (hasReasoning && generating && isLastAssistant) setReasoningOpen(true);
  }, [message.id, hasReasoning, generating, isLastAssistant]);
  useEffect(() => {
    const container = reasoningRef.current;
    if (container && reasoningOpen && generating) container.scrollTop = container.scrollHeight;
  }, [message.reasoning, reasoningOpen, generating]);
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

  const openContextMenu = (event: ReactMouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    const width = 208;
    const height = 88;
    setContextMenu({
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
    });
  };

  const copyMessage = async () => {
    setContextMenu(null);
    try {
      await writeClipboardText(message.content);
    } catch (error) {
      notify(`复制失败：${String(error)}`, "error");
    }
  };

  if (message.role === "user") {
    return (
      <>
        <article className="message user-message" aria-label="你的消息">
          <div className="user-message-stack">
            <div className="user-bubble" onContextMenu={openContextMenu} title="右键可复制或查看实际提示词">{message.content}</div>
            {timestamp && <time className="message-time" dateTime={message.createdAt} title={`发送时间：${timestamp}`}>{timestamp}</time>}
          </div>
        </article>
        {contextMenu && createPortal(
          <div
            ref={contextMenuRef}
            className="message-context-menu"
            role="menu"
            aria-label="消息操作"
            style={{ left: contextMenu.x, top: contextMenu.y }}
            onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); }}
          >
            <button type="button" role="menuitem" onClick={() => void copyMessage()}><Copy size={16} />复制文字</button>
            <button type="button" role="menuitem" onClick={() => {
              setContextMenu(null);
              if (sessionId) setPromptOpen(true);
              else notify("对话尚未保存，请稍后重试", "info");
            }}><Braces size={16} />查看实际提示词</button>
          </div>,
          document.body,
        )}
        {promptOpen && sessionId && <PromptDialog sessionId={sessionId} messageId={message.id} onClose={() => setPromptOpen(false)} />}
      </>
    );
  }

  return (
    <article className="message assistant-message" aria-label="Harness 的回复">
      <div className="assistant-avatar" aria-hidden="true">H</div>
      <div className="assistant-content">
        {message.reasoning && (
          <details className="reasoning-block" open={reasoningOpen} onToggle={(event) => setReasoningOpen(event.currentTarget.open)}>
            <summary><BrainCircuit size={16} /><span>思考内容</span>{generating && isLastAssistant && <span className="reasoning-live">实时更新中</span>}<ChevronDown size={15} /></summary>
            <div className="reasoning-content" ref={reasoningRef}>{message.reasoning}</div>
          </details>
        )}
        {message.content ? (
          <div className="markdown-body">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeHighlight]}
              components={{
                a: ({ children, ...props }) => <a {...props} target="_blank" rel="noreferrer">{children}</a>,
              }}
            >
              {message.content}
            </ReactMarkdown>
          </div>
        ) : generating && isLastAssistant && message.tools.length === 0 ? (
          <div className="assistant-waiting"><LoaderCircle size={15} className="spinning" />正在等待模型响应，详细状态见下方</div>
        ) : null}
        {message.tools.map((tool) => <ToolCard key={tool.callId} tool={tool} />)}
        {approvals.map((approval) => <ApprovalCard key={approval.id} approval={approval} />)}
        {generating && isLastAssistant && message.content && <span className="streaming-caret" aria-label="正在生成" />}
        {timestamp && <time className="message-time assistant-time" dateTime={message.createdAt} title={`回复时间：${timestamp}`}>{timestamp}</time>}
      </div>
    </article>
  );
}
