import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Braces, Copy, X } from "lucide-react";
import { writeClipboardText } from "../lib/clipboard";
import { ipcInvoke } from "../lib/ipc";
import { useHarnessStore } from "../lib/store";
import type { PromptMessage } from "../lib/types";
import { IconButton } from "./Controls";

interface PromptDialogProps {
  sessionId: string;
  messageId: string;
  onClose: () => void;
}

export default function PromptDialog({ sessionId, messageId, onClose }: PromptDialogProps) {
  const [messages, setMessages] = useState<PromptMessage[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const notify = useHarnessStore((state) => state.notify);

  useEffect(() => {
    let active = true;
    ipcInvoke("get_message_prompt", { sessionId, messageId })
      .then((result) => { if (active) setMessages(result); })
      .catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, [sessionId, messageId]);

  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onClose]);

  const rawPrompt = messages ? JSON.stringify(messages, null, 2) : "";
  const copyPrompt = async () => {
    try {
      await writeClipboardText(rawPrompt);
      notify("实际提示词已复制", "success");
    } catch (reason) {
      notify(`复制失败：${String(reason)}`, "error");
    }
  };

  return createPortal(
    <div className="dialog-backdrop prompt-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="prompt-dialog" role="dialog" aria-modal="true" aria-labelledby="prompt-dialog-title" aria-describedby="prompt-dialog-description">
        <header className="prompt-dialog-header">
          <div><Braces size={19} /><h2 id="prompt-dialog-title">实际发送的提示词</h2></div>
          <IconButton label="关闭提示词" onClick={onClose}><X size={19} /></IconButton>
        </header>
        <div className="prompt-dialog-meta">
          <p id="prompt-dialog-description">这是该条消息首次请求模型时使用的完整上下文，包含系统提示、此前对话与工具结果，不包含接口配置中的 API Key。</p>
          <button ref={closeRef} type="button" className="button secondary compact" onClick={() => void copyPrompt()} disabled={!messages}>
            <Copy size={15} />复制 JSON
          </button>
        </div>
        <div className="prompt-dialog-body">
          {error ? (
            <div className="prompt-error" role="alert">{error}</div>
          ) : messages ? (
            <pre className="prompt-raw">{rawPrompt}</pre>
          ) : (
            <div className="prompt-loading" role="status"><span className="spinner" />正在读取实际请求上下文…</div>
          )}
        </div>
        <footer className="prompt-dialog-footer">
          <span>{messages ? `共 ${messages.length} 条上下文消息` : "从本机会话记录还原"}</span>
          <button type="button" className="button secondary" onClick={onClose}>关闭</button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
