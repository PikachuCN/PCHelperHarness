import { useState } from "react";
import { File, FolderOpen, LoaderCircle, Plus, X } from "lucide-react";
import { ipcInvoke } from "../lib/ipc";
import { useHarnessStore } from "../lib/store";
import type { InteractionField, InteractionValue } from "../lib/types";

interface ToolFieldsProps {
  fields: InteractionField[];
  values: Record<string, InteractionValue>;
  onChange: (values: Record<string, InteractionValue>) => void;
  disabled?: boolean;
}

const pathKinds = new Set(["file", "files", "folder", "saveFile"]);

function pathPickerLabel(kind: InteractionField["kind"]): string {
  if (kind === "files") return "选择文件（可多选）";
  if (kind === "folder") return "选择文件夹";
  if (kind === "saveFile") return "选择保存位置";
  return "选择文件";
}

export default function ToolFields({ fields, values, onChange, disabled = false }: ToolFieldsProps) {
  const notify = useHarnessStore((state) => state.notify);
  const [picking, setPicking] = useState<string | null>(null);

  const update = (key: string, value: InteractionValue) => onChange({ ...values, [key]: value });

  const browse = async (field: InteractionField) => {
    if (picking || disabled) return;
    setPicking(field.key);
    try {
      const current = values[field.key];
      const paths = await ipcInvoke("pick_interaction_paths", {
        kind: field.kind,
        title: field.label,
        filters: field.filters,
        defaultName: field.kind === "saveFile" && typeof current === "string" ? current : undefined,
      });
      if (paths.length > 0) update(field.key, field.kind === "files" ? paths : paths[0]);
    } catch (error) {
      notify(`无法打开系统选择器：${String(error)}`, "error");
    } finally {
      setPicking(null);
    }
  };

  return (
    <div className="tool-fields">
      {fields.map((field) => {
        const value = values[field.key] ?? "";
        const descriptionId = `field-${field.key}-description`;
        return (
          <div className={`tool-field tool-field-${field.kind}`} key={field.key}>
            {field.kind !== "toggle" && (
              <label htmlFor={`field-${field.key}`}>
                {field.label}
                {field.required && <span className="required-mark" aria-label="必填">*</span>}
              </label>
            )}
            {field.description && <p className="field-description" id={descriptionId}>{field.description}</p>}

            {(field.kind === "text" || field.kind === "number") && (
              <input
                id={`field-${field.key}`}
                type={field.kind === "number" ? "number" : "text"}
                value={typeof value === "number" || typeof value === "string" ? value : ""}
                placeholder={field.placeholder}
                aria-describedby={field.description ? descriptionId : undefined}
                onChange={(event) => update(field.key, field.kind === "number" ? (event.target.value === "" ? "" : Number(event.target.value)) : event.target.value)}
                disabled={disabled}
              />
            )}

            {field.kind === "textarea" && (
              <textarea
                id={`field-${field.key}`}
                value={typeof value === "string" ? value : ""}
                placeholder={field.placeholder}
                aria-describedby={field.description ? descriptionId : undefined}
                rows={4}
                onChange={(event) => update(field.key, event.target.value)}
                disabled={disabled}
              />
            )}

            {field.kind === "select" && (
              <select
                id={`field-${field.key}`}
                value={typeof value === "string" ? value : ""}
                aria-describedby={field.description ? descriptionId : undefined}
                onChange={(event) => update(field.key, event.target.value)}
                disabled={disabled}
              >
                <option value="" disabled>请选择</option>
                {field.options?.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
              </select>
            )}

            {field.kind === "multiSelect" && (
              <div className="field-option-list" role="group" aria-label={field.label}>
                {field.options?.map((option) => {
                  const selected = Array.isArray(value) && value.includes(option.value);
                  return (
                    <label className={`field-option ${selected ? "is-selected" : ""}`} key={option.value}>
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={() => {
                          const current = Array.isArray(value) ? value : [];
                          update(field.key, selected ? current.filter((item) => item !== option.value) : [...current, option.value]);
                        }}
                        disabled={disabled}
                      />
                      <span><strong>{option.label}</strong>{option.description && <small>{option.description}</small>}</span>
                    </label>
                  );
                })}
              </div>
            )}

            {field.kind === "toggle" && (
              <label className="field-toggle" htmlFor={`field-${field.key}`}>
                <input
                  id={`field-${field.key}`}
                  type="checkbox"
                  checked={value === true}
                  onChange={(event) => update(field.key, event.target.checked)}
                  disabled={disabled}
                />
                <span className="toggle-track" aria-hidden="true"><span /></span>
                <span>{field.label}{field.required && <span className="required-mark">*</span>}</span>
              </label>
            )}

            {pathKinds.has(field.kind) && (
              <div className="path-field-control">
                {field.kind !== "files" && (
                  <div className="path-input-row">
                    <input
                      id={`field-${field.key}`}
                      type="text"
                      value={typeof value === "string" ? value : ""}
                      placeholder={field.kind === "saveFile" ? "输入文件名或选择保存位置" : "选择路径，也可以手动输入"}
                      onChange={(event) => update(field.key, event.target.value)}
                      aria-describedby={field.description ? descriptionId : undefined}
                      disabled={disabled}
                    />
                    {typeof value === "string" && value && (
                      <button type="button" className="icon-button" aria-label={`清空${field.label}`} onClick={() => update(field.key, "")} disabled={disabled}><X size={15} /></button>
                    )}
                  </div>
                )}
                {field.kind === "files" && (
                  <div className="selected-paths" id={`field-${field.key}`}>
                    {Array.isArray(value) && value.length > 0 ? value.map((path, index) => (
                      <div className="selected-path" key={`${path}-${index}`}>
                        <File size={14} />
                        <span title={path}>{path}</span>
                        <button type="button" aria-label={`移除 ${path}`} onClick={() => update(field.key, value.filter((_, itemIndex) => itemIndex !== index))} disabled={disabled}><X size={13} /></button>
                      </div>
                    )) : <span className="path-placeholder">尚未选择文件</span>}
                  </div>
                )}
                <button type="button" className="button secondary compact path-picker-button" onClick={() => void browse(field)} disabled={disabled || picking !== null}>
                  {picking === field.key ? <LoaderCircle size={15} className="spinning" /> : field.kind === "files" ? <Plus size={15} /> : <FolderOpen size={15} />}
                  {picking === field.key ? "等待选择…" : pathPickerLabel(field.kind)}
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
