import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FormEvent } from "react";
import type { LucideIcon } from "lucide-react";
import {
  ArrowRight,
  Copy,
  FileOutput,
  FilePenLine,
  Files,
  FolderKanban,
  Gauge,
  HardDrive,
  ImageMinus,
  PanelLeftOpen,
  Play,
  Printer,
  ReceiptText,
  ScanSearch,
  Search,
  ShieldCheck,
  Sparkles,
  Stamp,
  Trash2,
  Wand2,
  Wifi,
  X,
} from "lucide-react";
import { buildCustomToolRequest, buildToolPrompt, createFieldValues, customToolAsPreset, customToolExamples, toolPresets, toolboxCategories, validateFieldValues } from "../lib/toolbox";
import type { ToolPreset, ToolboxIcon } from "../lib/toolbox";
import { useHarnessStore } from "../lib/store";
import type { CustomTool, InteractionValue } from "../lib/types";
import { ConfirmDialog, IconButton } from "./Controls";
import ToolFields from "./ToolFields";

const toolIcons: Record<ToolboxIcon, LucideIcon> = {
  rename: FilePenLine,
  search: Search,
  organize: FolderKanban,
  duplicates: Copy,
  invoice: ReceiptText,
  watermark: Stamp,
  compress: ImageMinus,
  merge: Files,
  convert: FileOutput,
  print: Printer,
  performance: Gauge,
  network: Wifi,
  disk: HardDrive,
  health: ScanSearch,
  custom: Sparkles,
};

function ToolLaunchDialog({ tool, onClose }: { tool: ToolPreset; onClose: () => void }) {
  const sendMessage = useHarnessStore((state) => state.sendMessage);
  const generating = useHarnessStore((state) => state.generatingSessionId !== null || state.sending);
  const pythonAvailable = useHarnessStore((state) => state.pluginCatalog?.python.available ?? false);
  const [values, setValues] = useState<Record<string, InteractionValue>>(() => createFieldValues(tool.fields));
  const [error, setError] = useState<string | null>(null);
  const Icon = toolIcons[tool.icon];

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const launch = () => {
    if (generating) {
      setError("另一项 AI 任务仍在执行，请先等待完成或停止后再启动工具。");
      return;
    }
    const validation = validateFieldValues(tool.fields, values);
    if (validation) {
      setError(validation);
      return;
    }
    void sendMessage(buildToolPrompt(tool, values));
    onClose();
  };

  return createPortal(
    <div className="dialog-backdrop tool-launch-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="tool-launch-dialog" role="dialog" aria-modal="true" aria-labelledby="tool-launch-title">
        <header className="tool-launch-header">
          <span className="tool-card-icon"><Icon size={21} /></span>
          <div>
            <span className="eyebrow">{tool.category}</span>
            <h2 id="tool-launch-title">{tool.name}</h2>
          </div>
          <IconButton label="关闭工具设置" onClick={onClose}><X size={18} /></IconButton>
        </header>
        <div className="tool-launch-body">
          <p className="tool-launch-description">{tool.description}</p>
          {tool.requiresPython && !pythonAvailable && (
            <div className="tool-runtime-note">
              <Sparkles size={16} />
              <p><strong>当前未检测到 Python 运行时。</strong>你仍可发起任务，AI 会先确认可用能力；无法安装或缺少依赖时会明确说明，不会假装处理成功。</p>
            </div>
          )}
          {tool.fields.length > 0 ? (
            <ToolFields fields={tool.fields} values={values} onChange={setValues} />
          ) : (
            <div className="tool-no-fields"><ShieldCheck size={18} /><p>无需预先填写参数。AI 会先进行只读检查，需要进一步信息或修改系统时再与你确认。</p></div>
          )}
          {error && <p className="form-error" role="alert">{error}</p>}
        </div>
        <footer className="tool-launch-footer">
          <span><ShieldCheck size={14} />本机执行 · 修改操作按风险审批</span>
          <div className="dialog-actions">
            <button type="button" className="button secondary" onClick={onClose}>取消</button>
            <button type="button" className="button primary" onClick={launch} disabled={generating}><Play size={14} fill="currentColor" />{generating ? "等待当前任务完成" : tool.actionLabel}</button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  );
}

export default function ToolboxView() {
  const customTools = useHarnessStore((state) => state.customTools);
  const sendMessage = useHarnessStore((state) => state.sendMessage);
  const deleteCustomTool = useHarnessStore((state) => state.deleteCustomTool);
  const generating = useHarnessStore((state) => state.generatingSessionId !== null || state.sending);
  const sidebarCollapsed = useHarnessStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useHarnessStore((state) => state.toggleSidebar);
  const [category, setCategory] = useState<(typeof toolboxCategories)[number]>("全部");
  const [query, setQuery] = useState("");
  const [requirement, setRequirement] = useState("");
  const [selectedTool, setSelectedTool] = useState<ToolPreset | null>(null);
  const [deleting, setDeleting] = useState<CustomTool | null>(null);
  const generatorRef = useRef<HTMLTextAreaElement>(null);

  const allTools = useMemo(() => [...toolPresets, ...customTools.map(customToolAsPreset)], [customTools]);
  const visibleTools = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    return allTools.filter((tool) => {
      const inCategory = category === "全部" || (category === "我的工具" ? tool.custom : tool.category === category);
      const matches = !keyword || `${tool.name} ${tool.description} ${tool.category}`.toLowerCase().includes(keyword);
      return inCategory && matches;
    });
  }, [allTools, category, query]);

  const generate = (event?: FormEvent) => {
    event?.preventDefault();
    if (!requirement.trim() || generating) return;
    void sendMessage(buildCustomToolRequest(requirement));
    setRequirement("");
  };

  return (
    <main className="main-panel toolbox-panel">
      <header className="main-header toolbox-header">
        <div className="main-header-left">
          {sidebarCollapsed && <IconButton label="展开侧栏" onClick={toggleSidebar}><PanelLeftOpen size={18} /></IconButton>}
          <h1>AI 工具箱</h1>
          <span className="toolbox-header-count">{toolPresets.length} 个预设{customTools.length > 0 ? ` · ${customTools.length} 个我的工具` : ""}</span>
        </div>
        <span className="local-indicator"><span aria-hidden="true" />本地能力 · AI 编排</span>
      </header>

      <div className="toolbox-scroll">
        <div className="toolbox-content">
          <section className="toolbox-hero" aria-labelledby="toolbox-hero-title">
            <div className="toolbox-hero-copy">
              <span className="eyebrow"><Wand2 size={14} />用一句话创造工具</span>
              <h2 id="toolbox-hero-title">把重复工作，变成你的专属工具。</h2>
              <p>描述输入、输出和处理规则。AI 会补齐交互表单并保存到“我的工具”，下次直接填参使用。</p>
            </div>
            <form className="tool-generator" onSubmit={generate}>
              <textarea
                ref={generatorRef}
                value={requirement}
                onChange={(event) => setRequirement(event.target.value)}
                placeholder="例如：帮我把选中的发票按“日期_公司_金额”改名，原文件保留……"
                aria-label="描述想创建的个性工具"
                rows={3}
                maxLength={4000}
              />
              <div className="tool-generator-bottom">
                <span>只保存自然语言流程，实际执行仍受审批保护</span>
                <button type="submit" className="button primary" disabled={!requirement.trim() || generating}>
                  <Sparkles size={15} />{generating ? "AI 正在执行另一项任务" : "让 AI 创建"}<ArrowRight size={15} />
                </button>
              </div>
            </form>
            <div className="generator-examples" aria-label="个性工具示例">
              <span>试试：</span>
              {customToolExamples.map((example) => <button type="button" key={example} onClick={() => { setRequirement(example); generatorRef.current?.focus(); }}>{example}</button>)}
            </div>
          </section>

          <section className="tool-catalog" aria-labelledby="tool-catalog-title">
            <div className="tool-catalog-heading">
              <div><h2 id="tool-catalog-title">常用工具</h2><p>选择场景，补齐参数，剩下的交给 AI。</p></div>
              <label className="tool-search"><Search size={16} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索工具" aria-label="搜索工具" />{query && <button type="button" aria-label="清空搜索" onClick={() => setQuery("")}><X size={14} /></button>}</label>
            </div>
            <div className="tool-category-tabs" role="tablist" aria-label="工具分类">
              {toolboxCategories.map((item) => (
                <button key={item} type="button" role="tab" aria-selected={category === item} className={category === item ? "is-active" : ""} onClick={() => setCategory(item)}>
                  {item}{item === "我的工具" && customTools.length > 0 ? ` ${customTools.length}` : ""}
                </button>
              ))}
            </div>

            {visibleTools.length > 0 ? (
              <div className="tool-grid">
                {visibleTools.map((tool) => {
                  const Icon = toolIcons[tool.icon];
                  const custom = customTools.find((item) => item.id === tool.id);
                  return (
                    <article className="tool-card" key={tool.id}>
                      <button type="button" className="tool-card-main" onClick={() => setSelectedTool(tool)}>
                        <span className="tool-card-top"><span className="tool-card-icon"><Icon size={20} /></span>{tool.requiresPython && <span className="tool-badge">Python 增强</span>}{tool.custom && <span className="tool-badge is-custom">我的工具</span>}</span>
                        <span className="tool-card-copy"><strong>{tool.name}</strong><small>{tool.description}</small></span>
                        <span className="tool-card-action">{tool.actionLabel}<ArrowRight size={14} /></span>
                      </button>
                      {custom && <IconButton label={`删除 ${custom.name}`} className="tool-delete-button" onClick={() => setDeleting(custom)}><Trash2 size={15} /></IconButton>}
                    </article>
                  );
                })}
              </div>
            ) : (
              <div className="tool-empty">
                <Sparkles size={24} />
                <h3>{category === "我的工具" && !query ? "还没有你的专属工具" : "没有找到匹配的工具"}</h3>
                <p>{category === "我的工具" && !query ? "在上方描述一个重复任务，AI 会帮你设计交互与执行流程。" : "换个关键词或分类试试。"}</p>
                {category === "我的工具" && !query && <button type="button" className="button secondary" onClick={() => generatorRef.current?.focus()}>创建第一个工具</button>}
              </div>
            )}
          </section>
          <footer className="toolbox-note"><ShieldCheck size={14} />工具只负责收集参数和编排流程；真实文件修改、依赖安装与打印仍按 Harness 风险规则执行。</footer>
        </div>
      </div>

      {selectedTool && <ToolLaunchDialog tool={selectedTool} onClose={() => setSelectedTool(null)} />}
      {deleting && (
        <ConfirmDialog
          title={`删除“${deleting.name}”？`}
          description="只会删除这个个性工具的自然语言定义，不会删除此前生成的文件。此操作无法撤销。"
          confirmLabel="删除工具"
          danger
          onCancel={() => setDeleting(null)}
          onConfirm={() => { void deleteCustomTool(deleting.id); setDeleting(null); }}
        />
      )}
    </main>
  );
}
