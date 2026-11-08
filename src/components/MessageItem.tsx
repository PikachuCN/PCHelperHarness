import { useEffect, useRef, useState } from "react";
import { BrainCircuit, ChevronDown, LoaderCircle } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import type { ChatMessage, PendingApproval } from "../lib/types";
import ToolCard from "./ToolCard";
import ApprovalCard from "./ApprovalCard";

function formatMessageTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

interface MessageItemProps {
  message: ChatMessage;
  approvals: PendingApproval[];
  generating: boolean;
  isLastAssistant: boolean;
}

export default function MessageItem({ message, approvals, generating, isLastAssistant }: MessageItemProps) {
  const timestamp = formatMessageTime(message.createdAt);
  const [reasoningOpen, setReasoningOpen] = useState(true);
  const reasoningRef = useRef<HTMLDivElement>(null);
  const hasReasoning = Boolean(message.reasoning);
  useEffect(() => {
    if (hasReasoning && generating && isLastAssistant) setReasoningOpen(true);
  }, [message.id, hasReasoning, generating, isLastAssistant]);
  useEffect(() => {
    const container = reasoningRef.current;
    if (container && reasoningOpen && generating) container.scrollTop = container.scrollHeight;
  }, [message.reasoning, reasoningOpen, generating]);
  if (message.role === "user") {
    return (
      <article className="message user-message" aria-label="你的消息">
        <div className="user-message-stack">
          <div className="user-bubble">{message.content}</div>
          {timestamp && <time className="message-time" dateTime={message.createdAt} title={`发送时间：${timestamp}`}>{timestamp}</time>}
        </div>
      </article>
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
