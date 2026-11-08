import { useEffect, useState } from "react";
import { AlertTriangle, Check, ShieldAlert, X } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { PendingApproval } from "../lib/types";

const riskLabels = { l0: "只读", l1: "低风险", l2: "中风险", l3: "高风险" };
const diskCommandPattern = /(diskpart|clear-disk|remove-partition|format-volume|format\s+[a-z]:|\bclean\b)/i;

export default function ApprovalCard({ approval }: { approval: PendingApproval }) {
  const resolveApproval = useHarnessStore((state) => state.resolveApproval);
  const resolving = useHarnessStore((state) => state.resolvingApprovalIds.includes(approval.id));
  const [confirming, setConfirming] = useState(Boolean(approval.confirmationReadyAt));
  const [seconds, setSeconds] = useState(() => approval.confirmationReadyAt ? Math.max(0, Math.ceil((approval.confirmationReadyAt - Date.now()) / 1000)) : 3);
  const [typed, setTyped] = useState("");
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const needsTyping = approval.requiresTypedConfirmation || diskCommandPattern.test(approval.command);
  const isHighRisk = approval.level === "l3";

  useEffect(() => {
    if (!confirming || seconds <= 0) return;
    const timer = window.setTimeout(() => setSeconds((value) => value - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [confirming, seconds]);

  const requestApproval = async () => {
    if (isHighRisk && !confirming) {
      const resolution = await resolveApproval(approval.id, true);
      if (resolution?.status === "confirmation_required") {
        setSeconds(Math.max(0, Math.ceil(((resolution.readyAt ?? Date.now() + 3000) - Date.now()) / 1000)));
        setConfirming(true);
      }
      return;
    }
    await resolveApproval(approval.id, true, undefined, isHighRisk && needsTyping ? typed : undefined);
  };

  const canConfirm = !resolving && (!isHighRisk || !confirming || seconds === 0) && (!needsTyping || !confirming || typed === "确认");

  return (
    <section className={`approval-card risk-${approval.level}`} aria-labelledby={`approval-title-${approval.id}`}>
      <div className="approval-heading">
        <span className="approval-icon"><ShieldAlert size={18} /></span>
        <div>
          <div className="approval-title-row">
            <h3 id={`approval-title-${approval.id}`}>需要你的批准</h3>
            <span className={`risk-badge risk-badge-${approval.level}`}>{approval.level.toUpperCase()} · {riskLabels[approval.level]}</span>
          </div>
          <p>{approval.explanationZh}</p>
        </div>
      </div>
      <div className="approval-command"><span>将执行的命令 · {approval.shell}</span><code>{approval.command}</code></div>
      <div className="risk-explanation"><AlertTriangle size={16} /><p><strong>本地风险说明：</strong>{approval.risk.summaryZh}</p></div>
      {approval.risk.reasons.length > 0 && <ul className="approval-reasons">{approval.risk.reasons.map((item) => <li key={item}>{item}</li>)}</ul>}
      {confirming && (
        <div className="second-confirm">
          <p><strong>请再次确认高风险操作。</strong>后端会再次校验等待时间与确认文本。</p>
          {needsTyping && <label className="field-label">磁盘级操作，请输入“确认”<input value={typed} onChange={(event) => setTyped(event.target.value)} autoComplete="off" placeholder="确认" /></label>}
        </div>
      )}
      {rejecting && <label className="field-label reject-reason">拒绝原因（可选）<textarea value={reason} onChange={(event) => setReason(event.target.value)} rows={2} maxLength={300} placeholder="告诉助手为何拒绝，以便调整方案" /></label>}
      <div className="approval-actions">
        <button type="button" className={`button ${isHighRisk ? "danger" : "primary"}`} onClick={() => void requestApproval()} disabled={!canConfirm}>
          <Check size={16} />{resolving ? "正在提交…" : confirming ? seconds > 0 ? `确认允许（${seconds} 秒）` : "确认允许" : isHighRisk ? "继续审批" : "允许执行"}
        </button>
        <button type="button" className="button secondary" onClick={() => setRejecting((value) => !value)} disabled={resolving}><X size={16} />拒绝</button>
        {rejecting && <button type="button" className="button quiet" onClick={() => void resolveApproval(approval.id, false, reason.trim() || undefined)} disabled={resolving}>提交拒绝</button>}
      </div>
    </section>
  );
}
