import { useEffect, useState } from "react";
import { CheckCircle2, CircleAlert, Clock3, LoaderCircle, Square } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { TurnActivity } from "../lib/types";

function formatElapsed(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes} 分 ${String(seconds % 60).padStart(2, "0")} 秒`;
}

export default function TurnActivityCard({ activity }: { activity: TurnActivity }) {
  const cancelTurn = useHarnessStore((state) => state.cancelTurn);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    setNow(Date.now());
    if (!activity.active) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [activity.active, activity.startedAt]);

  const elapsed = (activity.active ? now : activity.updatedAt) - activity.startedAt;
  const Icon = activity.phase === "error" ? CircleAlert : activity.active ? LoaderCircle : CheckCircle2;
  const previousEvents = activity.events.slice(0, -1).slice(-3);
  const waitingHint = activity.active && (activity.phase === "preparing" || activity.phase === "connecting") && elapsed >= 5000;

  return (
    <section className={`turn-activity phase-${activity.phase} ${activity.active ? "is-active" : "is-idle"}`} aria-label="本轮执行状态">
      <div className="activity-current">
        <span className="activity-icon" aria-hidden="true"><Icon size={17} className={activity.active ? "spinning" : ""} /></span>
        <div className="activity-copy">
          <div className="activity-heading" role="status" aria-live="polite">
            <strong>{activity.label}</strong>
            <span className="activity-elapsed" aria-hidden="true"><Clock3 size={13} />{activity.active ? "已用" : "耗时"} {formatElapsed(elapsed)}</span>
          </div>
          <p>{activity.detail}</p>
          {waitingHint && <p className="activity-hint">仍在等待模型服务返回内容，可能是网络或服务端响应较慢；可以停止后重试。</p>}
          <div className="activity-meta">
            {activity.step > 0 && <span>步骤 {activity.step} / {activity.maxSteps}</span>}
            {activity.receivedChars > 0 && <span>已接收 {activity.receivedChars} 字符</span>}
            {activity.active && <span>状态会随模型与工具事件更新</span>}
          </div>
        </div>
        {activity.active && (
          <button type="button" className="activity-stop" onClick={() => void cancelTurn()} disabled={!activity.sessionId}>
            <Square size={12} fill="currentColor" />停止
          </button>
        )}
      </div>
      {activity.active && previousEvents.length > 0 && (
        <ol className="activity-timeline" aria-label="最近执行事件">
          {previousEvents.map((event) => (
            <li key={event.id}>
              <CheckCircle2 size={13} aria-hidden="true" />
              <div><strong>{event.label}</strong><span>{event.detail}</span></div>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
