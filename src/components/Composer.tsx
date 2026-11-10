import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { ArrowUp, Square } from "lucide-react";
import { useHarnessStore } from "../lib/store";
import SelectControl from "./SelectControl";

export default function Composer() {
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const sendMessage = useHarnessStore((state) => state.sendMessage);
  const cancelTurn = useHarnessStore((state) => state.cancelTurn);
  const currentSessionId = useHarnessStore((state) => state.currentSessionId);
  const generatingSessionId = useHarnessStore((state) => state.generatingSessionId);
  const sending = useHarnessStore((state) => state.sending);
  const config = useHarnessStore((state) => state.config);
  const selectedModel = useHarnessStore((state) => state.selectedModel);
  const availableModels = useHarnessStore((state) => state.availableModels);
  const setSelectedModel = useHarnessStore((state) => state.setSelectedModel);
  const approvalMode = useHarnessStore((state) => state.approvalMode);
  const setApprovalMode = useHarnessStore((state) => state.setApprovalMode);
  const setSettingsOpen = useHarnessStore((state) => state.setSettingsOpen);
  const starting = sending && generatingSessionId === null;
  const generatingHere = (generatingSessionId !== null && generatingSessionId === currentSessionId) || starting;
  const busyElsewhere = generatingSessionId !== null && !generatingHere;
  const busy = sending || generatingSessionId !== null;
  const activeProvider = config?.providers.find((provider) => provider.id === config.activeProviderId);
  const models = Array.from(new Set([selectedModel, ...availableModels, activeProvider?.model].filter((model): model is string => Boolean(model))));

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "0px";
    textarea.style.height = `${Math.min(Math.max(textarea.scrollHeight, 28), 160)}px`;
  }, [value]);

  const submit = () => {
    if (!value.trim() || busy) return;
    void sendMessage(value);
    setValue("");
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <footer className="composer-area">
      <div className="composer-column">
        <div className={`composer-box ${generatingHere ? "is-generating" : ""}`}>
          <textarea
            ref={textareaRef}
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder={busyElsewhere ? "另一段对话正在生成，请稍候…" : "描述电脑问题或想完成的文件、图片、PDF 任务…"}
            aria-label="输入电脑问题或工具任务"
            rows={1}
            disabled={busyElsewhere}
          />
          <div className="composer-toolbar">
            <div className="composer-options">
              <SelectControl
                label="选择模型"
                value={selectedModel}
                options={models.map((model) => ({ value: model, label: model }))}
                onChange={setSelectedModel}
                disabled={!config || busy}
                placeholder="未配置模型"
                className="model-select"
                direction="up"
              />
              <div className="approval-mode" role="group" aria-label="审批模式">
                <button type="button" className={approvalMode === "manual" ? "is-selected" : ""} aria-pressed={approvalMode === "manual"} onClick={() => setApprovalMode("manual")}>手动审批</button>
                <button type="button" className={approvalMode === "auto" ? "is-selected" : ""} aria-pressed={approvalMode === "auto"} onClick={() => setApprovalMode("auto")}>自动审批</button>
              </div>
            </div>
            {generatingHere ? (
              <button type="button" className="send-button stop-generation" onClick={() => void cancelTurn()} aria-label={starting ? "正在建立连接" : "停止生成"} title={starting ? "正在建立连接" : "停止生成"} disabled={starting}>
                <Square size={15} fill="currentColor" />
              </button>
            ) : (
              <button type="button" className="send-button" onClick={submit} disabled={!value.trim() || busy} aria-label="发送消息" title="发送消息">
                <ArrowUp size={18} />
              </button>
            )}
          </div>
        </div>
        <div className="composer-footnote">
          <span>Enter 发送 · Shift + Enter 换行</span>
          {!config && <button type="button" className="text-button" onClick={() => setSettingsOpen(true)}>配置模型接口</button>}
          <span>命令在本机执行，请核对审批内容</span>
        </div>
      </div>
    </footer>
  );
}
