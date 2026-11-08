import { useEffect, useState } from "react";
import { Ban, Check, ChevronDown, CircleAlert, Square, TerminalSquare } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { ToolRun } from "../lib/types";

const statusLabels = {
  pending: "等待审批",
  running: "运行中",
  success: "已完成",
  error: "执行失败",
  rejected: "已拒绝",
  cancelled: "已停止",
};

export default function ToolCard({ tool }: { tool: ToolRun }) {
  const [expanded, setExpanded] = useState(tool.status === "running");
  const cancelTurn = useHarnessStore((state) => state.cancelTurn);
  const command = typeof tool.arguments.command === "string"
    ? tool.arguments.command
    : typeof tool.arguments.path === "string"
      ? tool.arguments.path
      : JSON.stringify(tool.arguments, null, 2);

  useEffect(() => {
    if (tool.status === "running") setExpanded(true);
  }, [tool.status]);

  return (
    <section className={`tool-card status-${tool.status}`}>
      <div className="tool-card-header">
        <button type="button" className="tool-toggle" onClick={() => setExpanded((value) => !value)} aria-expanded={expanded} aria-controls={`tool-output-${tool.callId}`}>
          <TerminalSquare size={17} />
          <span className="tool-title">{tool.explanationZh || tool.name}</span>
          <span className={`tool-status status-text-${tool.status}`}>
            {tool.status === "running" && <span className="status-dot" aria-hidden="true" />}
            {tool.status === "success" && <Check size={14} />}
            {tool.status === "error" && <CircleAlert size={14} />}
            {(tool.status === "rejected" || tool.status === "cancelled") && <Ban size={14} />}
            {statusLabels[tool.status]}
          </span>
          <ChevronDown size={16} className={expanded ? "chevron is-open" : "chevron"} />
        </button>
        {tool.status === "running" && <button type="button" className="tool-stop" onClick={() => void cancelTurn()}><Square size={13} fill="currentColor" />停止</button>}
      </div>
      {expanded && (
        <div className="tool-card-body" id={`tool-output-${tool.callId}`}>
          <div className="command-line"><span>$</span><code>{command}</code></div>
          <pre className="tool-output" aria-live="polite" aria-label="命令实时输出">{tool.output || (tool.status === "pending" ? "等待审批后执行。" : "正在等待输出…")}</pre>
          <div className="tool-meta">
            <span>{tool.name}</span>
            {tool.durationMs > 0 && <span>{(tool.durationMs / 1000).toFixed(2)} 秒</span>}
            {tool.exitCode !== undefined && tool.exitCode !== null && <span>退出代码 {tool.exitCode}</span>}
          </div>
        </div>
      )}
    </section>
  );
}
