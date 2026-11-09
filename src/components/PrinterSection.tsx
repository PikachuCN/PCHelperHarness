import { useState } from "react";
import { Printer, Share2, ShieldCheck } from "lucide-react";
import { ipcInvoke } from "../lib/ipc";
import { useHarnessStore } from "../lib/store";
import type { PrinterInfo, PrinterListInfo } from "../lib/types";
import { displayText } from "../lib/systemOverviewFormat";
import { ConfirmDialog } from "./Controls";
import { SectionCard, StatusBadge, WarningList } from "./SystemOverviewCard";

interface ShareRequest {
  printer: PrinterInfo;
  shared: boolean;
  shareName: string;
}

function suggestedShareName(name: string): string {
  return name.replace(/[^A-Za-z0-9._-]/g, "").slice(0, 80) || "Printer";
}

function printerStatusLabel(status: string | null): string {
  const labels: Record<string, string> = { "0": "正常", "1": "已暂停", "2": "错误", "4": "卡纸", "5": "缺纸", "7": "纸张问题", "8": "脱机", Normal: "正常", Offline: "脱机", Paused: "已暂停", Error: "错误" };
  return status ? labels[status] ?? status : "未读取";
}

export default function PrinterSection() {
  const data = useHarnessStore((state) => state.systemOverview.printers?.data) as PrinterListInfo | undefined;
  const loadPrinters = useHarnessStore((state) => state.loadSystemOverviewSection);
  const notify = useHarnessStore((state) => state.notify);
  const restartAsAdmin = useHarnessStore((state) => state.restartAsAdmin);
  const [editingName, setEditingName] = useState<string | null>(null);
  const [shareNames, setShareNames] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<ShareRequest | null>(null);
  const [busyName, setBusyName] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [adminConfirm, setAdminConfirm] = useState(false);
  const needsAdmin = Boolean(operationError && /权限|拒绝访问|管理员|access denied|permissiondenied|0x80070005|unauthorizedaccess/i.test(operationError));

  const executeShare = async (request: ShareRequest) => {
    setBusyName(request.printer.name);
    setOperationError(null);
    try {
      await ipcInvoke("set_printer_share", {
        name: request.printer.name,
        shared: request.shared,
        ...(request.shared ? { shareName: request.shareName } : {}),
      });
      await loadPrinters("printers", true);
      setEditingName(null);
      notify(request.shared ? `已共享 ${request.printer.name}，共享名为 ${request.shareName}` : `已停止共享 ${request.printer.name}`, "success");
    } catch (error) {
      const message = String(error);
      setOperationError(message);
      notify(`打印机共享操作失败：${message}`, "error");
    } finally {
      setBusyName(null);
    }
  };

  const confirmShare = (printer: PrinterInfo) => {
    const shareName = (shareNames[printer.name] ?? suggestedShareName(printer.name)).trim();
    if (!/^[A-Za-z0-9._-]{1,80}$/.test(shareName)) {
      notify("共享名须为 1–80 个字母、数字、点、下划线或连字符", "error");
      return;
    }
    setPending({ printer, shared: true, shareName });
  };

  return (
    <SectionCard section="printers" title="打印机管理" description="查看打印机状态，快速共享或停止共享本机设备" icon={Printer} hasData={Boolean(data)} className="span-2 printer-section">
      {data && <>
        <div className="printer-privacy-note"><ShieldCheck size={16} /><p>共享后，同一网络中的其他用户可能看到并连接此打印机。操作前会再次确认；这里不会安装驱动或修改共享权限。</p></div>
        {operationError && <div className="printer-operation-error" role="alert"><div><strong>共享操作未完成</strong><p>{operationError}</p></div>{needsAdmin && <button type="button" className="button secondary compact" onClick={() => setAdminConfirm(true)}>以管理员身份重启</button>}</div>}
        {data.printers.length === 0 ? <div className="printer-empty"><Printer size={26} /><strong>未检测到打印机</strong><p>请确认打印机已添加，且 Windows Print Spooler 服务正在运行。</p></div> : <div className="printer-list">{data.printers.map((printer) => {
          const editing = editingName === printer.name;
          const busy = busyName === printer.name;
          return <article className="printer-item" key={printer.name}>
            <header className="printer-item-header">
              <span className="printer-item-icon"><Printer size={18} /></span>
              <div><strong>{printer.name}</strong><p>{displayText(printer.location || printer.comment, printer.isLocal ? "本机打印机" : "网络连接打印机")}</p></div>
              <div className="printer-badges">
                {printer.isDefault && <StatusBadge tone="success">默认</StatusBadge>}
                <StatusBadge tone={printer.shared ? "success" : "neutral"}>{printer.shared ? "已共享" : "未共享"}</StatusBadge>
                {printer.workOffline && <StatusBadge tone="warning">脱机</StatusBadge>}
              </div>
            </header>
            <dl className="printer-meta">
              <div><dt>驱动</dt><dd>{displayText(printer.driverName)}</dd></div>
              <div><dt>端口</dt><dd>{displayText(printer.portName)}</dd></div>
              <div><dt>状态</dt><dd>{printerStatusLabel(printer.status)}{printer.jobCount > 0 ? ` · ${printer.jobCount} 个任务` : ""}</dd></div>
              <div><dt>类型</dt><dd>{printer.isLocal ? "本机" : displayText(printer.printerType, "网络连接")}</dd></div>
              {printer.shared && <div className="is-wide"><dt>共享名</dt><dd>{displayText(printer.shareName)}</dd></div>}
            </dl>
            {printer.isLocal && <div className="printer-actions">
              {printer.shared ? <button type="button" className="button secondary compact" disabled={busyName !== null} onClick={() => setPending({ printer, shared: false, shareName: printer.shareName ?? "" })}>{busy ? "正在停止…" : "停止共享"}</button> : editing ? <div className="printer-share-editor">
                <label><span>共享名</span><input autoFocus value={shareNames[printer.name] ?? suggestedShareName(printer.name)} maxLength={80} onChange={(event) => setShareNames((current) => ({ ...current, [printer.name]: event.target.value }))} aria-label={`${printer.name} 共享名`} /></label>
                <button type="button" className="button primary compact" disabled={busyName !== null} onClick={() => confirmShare(printer)}>确认共享</button>
                <button type="button" className="button quiet compact" disabled={busyName !== null} onClick={() => setEditingName(null)}>取消</button>
              </div> : <button type="button" className="button secondary compact" disabled={busyName !== null} onClick={() => { setEditingName(printer.name); setShareNames((current) => ({ ...current, [printer.name]: current[printer.name] ?? suggestedShareName(printer.name) })); }}><Share2 size={14} />快速共享</button>}
            </div>}
          </article>;
        })}</div>}
        <WarningList warnings={data.warnings} />
      </>}
      {pending && <ConfirmDialog
        title={pending.shared ? `共享 ${pending.printer.name}？` : `停止共享 ${pending.printer.name}？`}
        description={pending.shared ? `将以“${pending.shareName}”为共享名开放此本机打印机。同一网络中的其他用户可能看到并连接它；Windows 可能要求管理员权限。` : "停止后，其他设备将无法再通过此共享名连接本机打印机，已建立的打印任务可能受影响。"}
        confirmLabel={pending.shared ? "确认共享" : "停止共享"}
        danger={!pending.shared}
        onCancel={() => setPending(null)}
        onConfirm={() => { const request = pending; setPending(null); void executeShare(request); }}
      />}
      {adminConfirm && <ConfirmDialog title="以管理员身份重启？" description="应用将请求 Windows 管理员权限并重新启动。重启后不会自动共享打印机，请返回打印机管理核对目标后重试。" confirmLabel="继续重启" onCancel={() => setAdminConfirm(false)} onConfirm={() => { setAdminConfirm(false); void restartAsAdmin(); }} />}
    </SectionCard>
  );
}
