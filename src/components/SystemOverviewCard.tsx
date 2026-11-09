import { useEffect, useState, type ReactNode } from "react";
import { AlertTriangle, ExternalLink, Eye, EyeOff, RefreshCw, ShieldAlert, type LucideIcon } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import type { SystemOverviewSection, SystemSettingsTarget } from "../lib/types";
import { ConfirmDialog, IconButton } from "./Controls";
import { formatDateTime } from "../lib/systemOverviewFormat";

interface SectionCardProps {
  section: SystemOverviewSection;
  title: string;
  description: string;
  icon: LucideIcon;
  hasData: boolean;
  children: ReactNode;
  action?: ReactNode;
  className?: string;
}

export function SectionCard({ section, title, description, icon: Icon, hasData, children, action, className = "" }: SectionCardProps) {
  const state = useHarnessStore((store) => store.systemOverview[section]);
  const loadSection = useHarnessStore((store) => store.loadSystemOverviewSection);
  const restartAsAdmin = useHarnessStore((store) => store.restartAsAdmin);
  const [restartConfirm, setRestartConfirm] = useState(false);
  const loading = state?.status === "loading";
  const needsAdmin = /管理员|权限|access denied|0x80041003|0x80070005|elevat/i.test(state?.error ?? "");

  return (
    <section className={`overview-card ${className}`} aria-labelledby={`overview-${section}-title`}>
      <header className="overview-card-header">
        <span className="overview-card-icon" aria-hidden="true"><Icon size={19} /></span>
        <div className="overview-card-heading">
          <h2 id={`overview-${section}-title`}>{title}</h2>
          <p>{description}</p>
        </div>
        <div className="overview-card-actions">
          {action}
          <IconButton label={`刷新${title}`} onClick={() => void loadSection(section, true)} disabled={loading}>
            <RefreshCw size={16} className={loading ? "spinning" : ""} />
          </IconButton>
        </div>
      </header>
      <div className="overview-card-body">
        {state?.error && hasData && <div className="overview-inline-error" role="alert"><AlertTriangle size={15} /><span>本次刷新失败，仍显示上次结果：{state.error}</span></div>}
        {hasData ? children : loading ? (
          <div className="overview-skeleton" role="status" aria-label={`正在读取${title}`}>
            <span /><span /><span /><span /><span /><span />
          </div>
        ) : (
          <div className="overview-error" role="alert">
            <ShieldAlert size={24} />
            <strong>暂时无法读取此模块</strong>
            <p>{state?.error ?? "信息尚未加载。"}</p>
            <div>
              <button type="button" className="button secondary compact" onClick={() => void loadSection(section, true)}>重新读取</button>
              {needsAdmin && <button type="button" className="button quiet compact" onClick={() => setRestartConfirm(true)}>以管理员身份重启</button>}
            </div>
          </div>
        )}
      </div>
      {state?.loadedAt && <footer className="overview-card-footer"><span>更新于 {formatDateTime(state.loadedAt)}</span>{loading && <span>正在刷新…</span>}</footer>}
      {restartConfirm && (
        <ConfirmDialog
          title="以管理员身份重启？"
          description="应用将请求 Windows 管理员权限并重新启动。当前对话会保留；重启后请重新打开系统概况。"
          confirmLabel="继续重启"
          onCancel={() => setRestartConfirm(false)}
          onConfirm={() => { setRestartConfirm(false); void restartAsAdmin(); }}
        />
      )}
    </section>
  );
}

export function InfoGrid({ children }: { children: ReactNode }) {
  return <dl className="overview-info-grid">{children}</dl>;
}

export function InfoItem({ label, children, wide = false }: { label: string; children: ReactNode; wide?: boolean }) {
  return <div className={wide ? "is-wide" : ""}><dt>{label}</dt><dd>{children}</dd></div>;
}

export function StatusBadge({ tone = "neutral", children }: { tone?: "success" | "warning" | "error" | "neutral"; children: ReactNode }) {
  return <span className={`overview-status is-${tone}`}>{children}</span>;
}

export function SensitiveValue({ value }: { value: unknown }) {
  const [revealed, setRevealed] = useState(false);
  const text = value === null || value === undefined || String(value).trim() === "" ? "未读取" : String(value);
  useEffect(() => setRevealed(false), [text]);
  if (text === "未读取") return <>{text}</>;
  return (
    <span className="sensitive-value">
      <span>{revealed ? text : "••••••••"}</span>
      <IconButton label={revealed ? "隐藏敏感信息" : "显示敏感信息"} onClick={() => setRevealed((current) => !current)}>
        {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
      </IconButton>
    </span>
  );
}

export function WarningList({ warnings }: { warnings?: string[] }) {
  if (!warnings?.length) return null;
  return (
    <div className="overview-warnings">
      {warnings.map((warning, index) => <p key={`${index}-${warning}`}><AlertTriangle size={14} /><span>{warning}</span></p>)}
    </div>
  );
}

export function SettingsLink({ target, children }: { target: SystemSettingsTarget; children: ReactNode }) {
  const openSettings = useHarnessStore((store) => store.openSystemSettings);
  return <button type="button" className="overview-settings-link" onClick={() => void openSettings(target)}>{children}<ExternalLink size={13} /></button>;
}
