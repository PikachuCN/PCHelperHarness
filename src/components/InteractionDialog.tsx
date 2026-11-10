import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { ArrowRight, MessageSquareText, ShieldCheck, X } from "lucide-react";
import { createFieldValues, validateFieldValues } from "../lib/toolbox";
import { useHarnessStore } from "../lib/store";
import type { InteractionValue, PendingInteraction } from "../lib/types";
import { IconButton } from "./Controls";
import ToolFields from "./ToolFields";

export default function InteractionDialog({ request }: { request: PendingInteraction }) {
  const resolveInteraction = useHarnessStore((state) => state.resolveInteraction);
  const [values, setValues] = useState<Record<string, InteractionValue>>(() => createFieldValues(request.fields));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setValues(createFieldValues(request.fields));
    setError(null);
    setBusy(false);
  }, [request.id, request.fields]);

  const submit = async (cancelled: boolean) => {
    if (busy) return;
    if (!cancelled) {
      const validation = validateFieldValues(request.fields, values);
      if (validation) {
        setError(validation);
        return;
      }
    }
    setBusy(true);
    setError(null);
    const submittedValues = Object.fromEntries(request.fields.flatMap((field) => {
      const value = values[field.key];
      const empty = value === "" || value === null || value === undefined || (Array.isArray(value) && value.length === 0);
      return !field.required && empty ? [] : [[field.key, value]];
    }));
    const resolved = await resolveInteraction(request.id, cancelled ? {} : submittedValues, cancelled);
    if (!resolved) {
      setBusy(false);
      setError("提交失败，请重试；如果本轮已停止，可以关闭此窗口。");
    }
  };

  return createPortal(
    <div className="dialog-backdrop interaction-backdrop">
      <section className="interaction-dialog" role="dialog" aria-modal="true" aria-labelledby="interaction-title" aria-describedby="interaction-description">
        <header className="interaction-dialog-header">
          <div className="interaction-heading-icon"><MessageSquareText size={19} /></div>
          <div>
            <span className="eyebrow">AI 执行中请求输入</span>
            <h2 id="interaction-title">{request.title}</h2>
          </div>
          <IconButton label="取消本次输入" onClick={() => void submit(true)} disabled={busy}><X size={18} /></IconButton>
        </header>
        <div className="interaction-dialog-body">
          <p className="interaction-description" id="interaction-description">{request.description}</p>
          <ToolFields fields={request.fields} values={values} onChange={setValues} disabled={busy} />
          {error && <p className="form-error" role="alert">{error}</p>}
        </div>
        <footer className="interaction-dialog-footer">
          <span><ShieldCheck size={14} />路径仅回传给当前本地任务，文件操作仍需审批</span>
          <div className="dialog-actions">
            <button type="button" className="button secondary" onClick={() => void submit(true)} disabled={busy}>取消操作</button>
            <button type="button" className="button primary" onClick={() => void submit(false)} disabled={busy}>
              {busy ? "正在提交…" : "提交并继续"}{!busy && <ArrowRight size={15} />}
            </button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
