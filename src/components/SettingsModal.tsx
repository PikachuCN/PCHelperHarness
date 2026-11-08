import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  Blocks,
  Check,
  ChevronRight,
  CircleCheck,
  CircleX,
  Database,
  Eye,
  EyeOff,
  FolderOpen,
  Info,
  KeyRound,
  Monitor,
  Moon,
  Palette,
  Plus,
  Puzzle,
  RefreshCw,
  Save,
  Server,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Sun,
  Trash2,
  X,
} from "lucide-react";
import { ipcInvoke } from "../lib/ipc";
import { useHarnessStore } from "../lib/store";
import type { AppConfig, ModelTestResult, ProviderConfig } from "../lib/types";
import { ConfirmDialog, IconButton } from "./Controls";
import SelectControl from "./SelectControl";

type SectionId = "interface" | "approval" | "skills" | "plugins" | "appearance" | "security" | "about";

const sections: { id: SectionId; label: string; icon: typeof Server }[] = [
  { id: "interface", label: "接口", icon: Server },
  { id: "approval", label: "审批", icon: ShieldCheck },
  { id: "skills", label: "Skills", icon: Puzzle },
  { id: "plugins", label: "插件", icon: Blocks },
  { id: "appearance", label: "外观", icon: Palette },
  { id: "security", label: "安全与存储", icon: Database },
  { id: "about", label: "关于", icon: Info },
];

const pythonSourceLabels = {
  plugin: "内置 Python 插件（优先）",
  portable: "应用便携 Python",
  system: "系统 Python",
} as const;

function Toggle({ checked, onChange, label, description }: { checked: boolean; onChange: (checked: boolean) => void; label: string; description?: string }) {
  return (
    <div className="toggle-row">
      <div><strong>{label}</strong>{description && <p>{description}</p>}</div>
      <button type="button" role="switch" aria-checked={checked} aria-label={label} className={`switch ${checked ? "is-on" : ""}`} onClick={() => onChange(!checked)}>
        <span />
      </button>
    </div>
  );
}

function SettingSection({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <section className="settings-section" aria-labelledby={`setting-${title}`}>
      <div className="settings-section-heading">
        <h2 id={`setting-${title}`}>{title}</h2>
        {description && <p>{description}</p>}
      </div>
      {children}
    </section>
  );
}

export default function SettingsModal() {
  const config = useHarnessStore((state) => state.config);
  const envInfo = useHarnessStore((state) => state.envInfo);
  const skills = useHarnessStore((state) => state.skills);
  const pluginCatalog = useHarnessStore((state) => state.pluginCatalog);
  const fallbackMessage = useHarnessStore((state) => state.configFallbackMessage);
  const setOpen = useHarnessStore((state) => state.setSettingsOpen);
  const saveConfig = useHarnessStore((state) => state.saveConfig);
  const loadSkills = useHarnessStore((state) => state.loadSkills);
  const loadPluginCatalog = useHarnessStore((state) => state.loadPluginCatalog);
  const setAvailableModels = useHarnessStore((state) => state.setAvailableModels);
  const notify = useHarnessStore((state) => state.notify);
  const [draft, setDraft] = useState<AppConfig | null>(() => config ? structuredClone(config) : null);
  const [section, setSection] = useState<SectionId>("interface");
  const [selectedProviderId, setSelectedProviderId] = useState(config?.activeProviderId ?? "");
  const [showKey, setShowKey] = useState(false);
  const [fetchingModels, setFetchingModels] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ModelTestResult | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [customModelInput, setCustomModelInput] = useState(false);
  const [reloadingSkills, setReloadingSkills] = useState(false);
  const [refreshingPlugins, setRefreshingPlugins] = useState(false);
  const [saving, setSaving] = useState(false);
  const [closeConfirm, setCloseConfirm] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const dirtyRef = useRef(false);
  const closeConfirmRef = useRef(false);
  const dirty = useMemo(() => Boolean(config && draft && JSON.stringify(config) !== JSON.stringify(draft)), [config, draft]);
  dirtyRef.current = dirty;
  closeConfirmRef.current = closeConfirm;
  const provider = draft?.providers.find((item) => item.id === selectedProviderId) ?? draft?.providers[0];

  useEffect(() => {
    if (config && !draft) {
      setDraft(structuredClone(config));
      setSelectedProviderId(config.activeProviderId);
    }
  }, [config, draft]);

  useLayoutEffect(() => {
    const content = contentRef.current;
    if (content) content.scrollTop = 0;
  }, [section]);

  const requestClose = () => {
    if (dirty) setCloseConfirm(true);
    else setOpen(false);
  };

  useEffect(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    const dialog = dialogRef.current;
    dialog?.querySelector<HTMLElement>("button")?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (closeConfirmRef.current) return;
      if (event.key === "Escape") {
        event.preventDefault();
        if (dirtyRef.current) setCloseConfirm(true);
        else setOpen(false);
      }
      if (event.key === "Tab" && dialog) {
        const focusable = Array.from(dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex='0']"));
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      previousFocus?.focus();
    };
  }, []);

  const updateDraft = (update: (current: AppConfig) => AppConfig) => {
    setDraft((current) => current ? update(current) : current);
  };

  const updateProvider = (patch: Partial<ProviderConfig>) => {
    if (!provider) return;
    updateDraft((current) => ({
      ...current,
      providers: current.providers.map((item) => item.id === provider.id ? { ...item, ...patch } : item),
    }));
    setTestResult(null);
    setTestError(null);
  };

  const addProvider = () => {
    const id = crypto.randomUUID();
    const newProvider: ProviderConfig = { id, name: "新接口", baseUrl: "", apiKey: { encrypted: false, value: "" }, apiMode: "chat_completions", model: "", models: [], temperature: null };
    updateDraft((current) => ({ ...current, providers: [...current.providers, newProvider], activeProviderId: current.activeProviderId || id }));
    setSelectedProviderId(id);
    setTestResult(null);
    setTestError(null);
  };

  const removeProvider = () => {
    if (!provider || !draft || draft.providers.length <= 1) return;
    const remaining = draft.providers.filter((item) => item.id !== provider.id);
    updateDraft((current) => ({
      ...current,
      providers: remaining,
      activeProviderId: current.activeProviderId === provider.id ? remaining[0].id : current.activeProviderId,
    }));
    setSelectedProviderId(remaining[0].id);
  };

  const fetchModels = async () => {
    if (!provider) return;
    setFetchingModels(true);
    setTestResult(null);
    setTestError(null);
    try {
      const models = await ipcInvoke("fetch_models", { provider });
      const model = models.includes(provider.model) ? provider.model : models[0] || "";
      updateProvider({ models, model });
      setCustomModelInput(false);
      if (draft?.activeProviderId === provider.id) setAvailableModels(models);
      notify(models.length ? `已获取 ${models.length} 个模型` : "接口未返回可用模型", models.length ? "success" : "info");
    } catch (error) {
      notify(`获取模型失败：${String(error)}`, "error");
    } finally {
      setFetchingModels(false);
    }
  };

  const testProvider = async () => {
    if (!provider) return;
    setTesting(true);
    setTestResult(null);
    setTestError(null);
    try {
      const result = await ipcInvoke("test_provider", { provider });
      if (result.connected && result.models.length > 0) {
        const models = result.models.map((model) => model.id);
        const model = models.includes(provider.model) ? provider.model : models[0] || "";
        updateProvider({ models, model });
        setCustomModelInput(false);
        if (draft?.activeProviderId === provider.id) setAvailableModels(models);
      }
      setTestResult(result);
    } catch (error) {
      setTestError(String(error));
    } finally {
      setTesting(false);
    }
  };

  const reloadSkills = async () => {
    setReloadingSkills(true);
    await loadSkills(true);
    setReloadingSkills(false);
  };

  const refreshPlugins = async () => {
    setRefreshingPlugins(true);
    await loadPluginCatalog();
    setRefreshingPlugins(false);
  };

  const submit = async () => {
    if (!draft || !dirty) return;
    setSaving(true);
    const saved = await saveConfig(draft);
    setSaving(false);
    if (saved) setOpen(false);
  };

  return createPortal(
    <>
      <div className="settings-backdrop" onMouseDown={(event) => event.target === event.currentTarget && requestClose()}>
        <div className="settings-modal" role="dialog" aria-modal="true" aria-label="Harness 设置" ref={dialogRef}>
          <header className="settings-header">
            <div><Settings2 size={19} /><h1>设置</h1></div>
            <IconButton label="关闭设置" onClick={requestClose}><X size={19} /></IconButton>
          </header>
          <div className="settings-layout">
            <nav className="settings-nav" aria-label="设置分类">
              {sections.map(({ id, label, icon: Icon }) => (
                <button key={id} type="button" className={section === id ? "is-active" : ""} onClick={() => setSection(id)} aria-current={section === id ? "page" : undefined}>
                  <Icon size={17} /><span>{label}</span><ChevronRight size={14} className="settings-nav-chevron" />
                </button>
              ))}
            </nav>
            <div className="settings-content" ref={contentRef}>
              {!draft ? (
                <div className="settings-unavailable"><CircleX size={24} /><p>配置尚未加载，请关闭设置后重试。</p></div>
              ) : section === "interface" ? (
                <SettingSection title="模型接口" description="配置兼容的模型服务。密钥仅随本机配置保存。">
                  <div className="provider-workspace">
                    <div className="provider-list">
                      <div className="provider-list-heading"><span>接口配置</span><IconButton label="添加接口配置" onClick={addProvider}><Plus size={16} /></IconButton></div>
                      {draft.providers.map((item) => (
                        <button
                          type="button"
                          key={item.id}
                          className={`provider-item ${provider?.id === item.id ? "is-selected" : ""}`}
                          onClick={() => { setSelectedProviderId(item.id); setShowKey(false); setTestResult(null); setTestError(null); setCustomModelInput(false); }}
                        >
                          <span className="provider-radio">{draft.activeProviderId === item.id && <span />}</span>
                          <span>{item.name || "未命名接口"}</span>
                        </button>
                      ))}
                    </div>
                    {provider && (
                      <div className="provider-form">
                        <div className="provider-form-top">
                          <label className="field-label">配置名称<input value={provider.name} onChange={(event) => updateProvider({ name: event.target.value })} maxLength={40} /></label>
                          <IconButton label="删除当前接口" onClick={removeProvider} disabled={draft.providers.length <= 1}><Trash2 size={16} /></IconButton>
                        </div>
                        <label className="active-provider-check">
                          <input type="radio" name="activeProvider" checked={draft.activeProviderId === provider.id} onChange={() => updateDraft((current) => ({ ...current, activeProviderId: provider.id }))} />
                          设为当前使用的接口
                        </label>
                        <label className="field-label">接口地址<input value={provider.baseUrl} onChange={(event) => updateProvider({ baseUrl: event.target.value })} placeholder="https://api.example.com/v1" spellCheck={false} /></label>
                        <label className="field-label">API Key
                          <span className="input-with-action">
                            <input type={showKey ? "text" : "password"} value={provider.apiKey.value} onChange={(event) => updateProvider({ apiKey: { encrypted: false, value: event.target.value } })} placeholder="留空保留已保存 Key，或输入新 Key" autoComplete="off" spellCheck={false} />
                            <IconButton label={showKey ? "隐藏 API Key" : "显示 API Key"} onClick={() => setShowKey((value) => !value)}>{showKey ? <EyeOff size={16} /> : <Eye size={16} />}</IconButton>
                          </span>
                        </label>
                        <div className="field-label"><span>接口协议</span>
                          <SelectControl
                            label="接口协议"
                            value={provider.apiMode}
                            options={[{ value: "chat_completions", label: "Chat Completions" }, { value: "responses", label: "Responses" }]}
                            onChange={(apiMode) => updateProvider({ apiMode: apiMode as ProviderConfig["apiMode"] })}
                            className="settings-select"
                          />
                        </div>
                        <div className="field-label">
                          <div className="field-label-heading"><span>模型</span><button type="button" className="text-button" onClick={() => setCustomModelInput((value) => !value)} disabled={provider.models.length === 0}>{customModelInput ? "从列表选择" : "手动输入"}</button></div>
                          <div className="model-input-row">
                            {customModelInput || provider.models.length === 0 ? (
                              <input aria-label="模型名称" value={provider.model} onChange={(event) => updateProvider({ model: event.target.value })} placeholder="输入自定义模型名称" spellCheck={false} />
                            ) : (
                              <SelectControl
                                label="选择上游模型"
                                value={provider.model}
                                options={provider.models.map((model) => ({ value: model, label: model }))}
                                onChange={(model) => updateProvider({ model })}
                                className="settings-select model-select-settings"
                                direction="up"
                              />
                            )}
                            <button type="button" className="button secondary compact" onClick={() => void fetchModels()} disabled={fetchingModels}><RefreshCw size={15} className={fetchingModels ? "spinning" : ""} />获取模型</button>
                          </div>
                        </div>
                        <label className="field-label">可用模型（使用逗号分隔）
                          <input value={provider.models.join(", ")} onChange={(event) => updateProvider({ models: event.target.value.split(",").map((model) => model.trim()).filter(Boolean) })} placeholder="deepseek-chat, deepseek-reasoner" spellCheck={false} />
                        </label>
                        <label className="field-label">Temperature（留空使用服务默认值）
                          <input type="number" min="0" max="2" step="0.1" value={provider.temperature ?? ""} onChange={(event) => updateProvider({ temperature: event.target.value === "" ? null : Number(event.target.value) })} placeholder="0.7" />
                        </label>
                        <div className="provider-test-row">
                          <button type="button" className="button secondary" onClick={() => void testProvider()} disabled={testing}><KeyRound size={16} />{testing ? "正在测试…" : "测试连接"}</button>
                          {testResult && <div className={`test-result ${testResult.connected ? "is-success" : "is-error"}`} role="status">{testResult.connected ? <CircleCheck size={16} /> : <CircleX size={16} />}<span>{testResult.connected ? `连接成功，发现 ${testResult.models.length} 个模型，${testResult.supportsTools ? "支持工具调用" : "未通过工具调用验证"}` : "连接失败"} · {testResult.latencyMs} ms</span></div>}
                          {testError && <div className="test-result is-error" role="alert"><CircleX size={16} /><span>{testError}</span></div>}
                        </div>
                      </div>
                    )}
                  </div>
                </SettingSection>
              ) : section === "approval" ? (
                <SettingSection title="命令审批" description="决定助手执行本机命令前，何时需要你的明确同意。">
                  <div className="choice-cards two-columns">
                    <button type="button" className={`choice-card ${draft.approval.mode === "manual" ? "is-selected" : ""}`} onClick={() => updateDraft((current) => ({ ...current, approval: { ...current.approval, mode: "manual" } }))}>
                      <ShieldCheck size={19} /><span><strong>手动审批</strong><small>每条需要权限的命令都由你确认</small></span>{draft.approval.mode === "manual" && <Check size={17} />}
                    </button>
                    <button type="button" className={`choice-card ${draft.approval.mode === "auto" ? "is-selected" : ""}`} onClick={() => updateDraft((current) => ({ ...current, approval: { ...current.approval, mode: "auto" } }))}>
                      <SlidersHorizontal size={19} /><span><strong>自动审批</strong><small>按风险等级自动处理低风险命令</small></span>{draft.approval.mode === "auto" && <Check size={17} />}
                    </button>
                  </div>
                  <div className="settings-group">
                    <Toggle checked={draft.approval.readonlyNeedsApproval} onChange={(checked) => updateDraft((current) => ({ ...current, approval: { ...current.approval, readonlyNeedsApproval: checked } }))} label="手动模式下审批只读命令" description="开启后，L0 状态查询也会先展示命令并等待确认。L2 与 L3 始终需要审批。" />
                  </div>
                  <div className="risk-guide">
                    <h3>风险等级</h3>
                    <div><span className="risk-badge">L0</span><p><strong>只读</strong><small>查询系统信息，不主动修改配置或文件。</small></p></div>
                    <div><span className="risk-badge">L1</span><p><strong>修改</strong><small>修改配置、写文件或结束进程；自动模式下可直接执行。</small></p></div>
                    <div><span className="risk-badge risk-badge-l2">L2</span><p><strong>下载安装</strong><small>下载文件、安装软件或包；自动模式下仍需确认一次。</small></p></div>
                    <div><span className="risk-badge risk-badge-l3">L3</span><p><strong>破坏性</strong><small>删除数据、修改磁盘或启动配置，始终需要二次确认。</small></p></div>
                  </div>
                  <div className="execution-settings">
                    <h3>执行限制</h3>
                    <div className="numeric-grid">
                      <label className="field-label">默认超时（秒）<input type="number" min="1" max="1800" value={draft.execution.defaultTimeoutSec} onChange={(event) => updateDraft((current) => ({ ...current, execution: { ...current.execution, defaultTimeoutSec: Number(event.target.value) } }))} /></label>
                      <label className="field-label">最长超时（秒）<input type="number" min="1" max="1800" value={draft.execution.maxTimeoutSec} onChange={(event) => updateDraft((current) => ({ ...current, execution: { ...current.execution, maxTimeoutSec: Number(event.target.value) } }))} /></label>
                      <label className="field-label">输出上限（KB）<input type="number" min="2" max="1024" value={draft.execution.outputLimitKb} onChange={(event) => updateDraft((current) => ({ ...current, execution: { ...current.execution, outputLimitKb: Number(event.target.value) } }))} /></label>
                      <label className="field-label">单轮最大步骤<input type="number" min="1" max="100" value={draft.execution.maxSteps} onChange={(event) => updateDraft((current) => ({ ...current, execution: { ...current.execution, maxSteps: Number(event.target.value) } }))} /></label>
                    </div>
                  </div>
                </SettingSection>
              ) : section === "skills" ? (
                <SettingSection title="Skills" description="查看助手可调用的本地维修能力与来源。">
                  <div className="section-toolbar">
                    <button type="button" className="button secondary compact" onClick={() => void reloadSkills()} disabled={reloadingSkills}><RefreshCw size={15} className={reloadingSkills ? "spinning" : ""} />重新加载</button>
                    <button type="button" className="button secondary compact" onClick={() => { void ipcInvoke("open_skills_dir", {}).catch((error) => notify(`打开目录失败：${String(error)}`, "error")); }}><FolderOpen size={15} />打开目录</button>
                  </div>
                  <div className="skills-list">
                    {skills.length === 0 && <p className="muted-empty">未发现 Skills，可将技能文件放入本地 Skills 目录后重新加载。</p>}
                    {skills.map((skill) => (
                      <div className="skill-row" key={`${skill.source}-${skill.name}`}>
                        <div><strong>{skill.name}</strong><p>{skill.description}</p><code>{skill.path}</code></div>
                        <span className="skill-source">{skill.source}</span>
                      </div>
                    ))}
                  </div>
                </SettingSection>
              ) : section === "plugins" ? (
                <SettingSection title="运行时插件" description="检查插件加载状态和 Python 执行来源。插件在启动时加载，更换 DLL 后需重启应用。">
                  <div className="section-toolbar">
                    <button type="button" className="button secondary compact" onClick={() => void refreshPlugins()} disabled={refreshingPlugins}><RefreshCw size={15} className={refreshingPlugins ? "spinning" : ""} />刷新状态</button>
                    <button type="button" className="button secondary compact" onClick={() => { void ipcInvoke("open_plugin_dir", {}).catch((error) => notify(`打开插件目录失败：${String(error)}`, "error")); }}><FolderOpen size={15} />打开插件目录</button>
                  </div>
                  <div className={`python-runtime ${pluginCatalog?.python.available ? "is-ready" : "is-unavailable"}`}>
                    {pluginCatalog?.python.available ? <CircleCheck size={20} /> : <CircleX size={20} />}
                    <div>
                      <strong>Python 执行环境</strong>
                      <p>{!pluginCatalog ? "正在读取运行时状态…" : pluginCatalog.python.available ? `当前使用${pythonSourceLabels[pluginCatalog.python.source ?? "system"]}` : "未检测到可用的 Python 运行时"}</p>
                      {pluginCatalog?.python.program && <code>{pluginCatalog.python.program}</code>}
                      {pluginCatalog?.python.error && <p className="plugin-error">{pluginCatalog.python.error}</p>}
                    </div>
                    <span className="runtime-status">{pluginCatalog?.python.available ? "可用" : "不可用"}</span>
                  </div>
                  <p className="runtime-priority"><Blocks size={16} />内置插件优先执行；其临时运行时、pip 包与缓存仅保留在本次会话，退出时清理。</p>
                  <div className="skills-list plugin-list">
                    {!pluginCatalog && <p className="muted-empty">正在读取插件清单…</p>}
                    {pluginCatalog?.plugins.length === 0 && <p className="muted-empty">未检测到运行时插件。可将受信任的 python.dll 放入应用目录并重启。</p>}
                    {pluginCatalog?.plugins.map((plugin) => (
                      <div className="skill-row plugin-row" key={`${plugin.path}-${plugin.status}`}>
                        <div>
                          <strong>{plugin.name}{plugin.version && ` ${plugin.version}`}</strong>
                          {plugin.status === "ready" ? <p>ID：{plugin.id} · shell={plugin.shell}</p> : <p className="plugin-error">{plugin.error ?? "插件加载失败"}</p>}
                          <code>{plugin.path}</code>
                        </div>
                        <span className={`skill-source plugin-status ${plugin.status === "error" ? "is-error" : ""}`}>{plugin.status === "ready" ? "已加载" : "加载失败"}</span>
                      </div>
                    ))}
                  </div>
                  {pluginCatalog && <p className="plugin-directory">插件目录：<code>{pluginCatalog.directory}</code></p>}
                  <p className="security-note"><ShieldCheck size={16} />DLL 插件拥有本机代码权限，只应使用来源可信的插件。</p>
                </SettingSection>
              ) : section === "appearance" ? (
                <SettingSection title="外观" description="选择适合当前工作环境的显示方式。">
                  <h3 className="field-group-title">颜色模式</h3>
                  <div className="choice-cards three-columns">
                    {([
                      { id: "system", label: "跟随系统", icon: Monitor },
                      { id: "light", label: "浅色", icon: Sun },
                      { id: "dark", label: "深色", icon: Moon },
                    ] as const).map(({ id, label, icon: Icon }) => (
                      <button key={id} type="button" className={`choice-card theme-choice ${draft.appearance.theme === id ? "is-selected" : ""}`} onClick={() => updateDraft((current) => ({ ...current, appearance: { ...current.appearance, theme: id } }))}>
                        <Icon size={19} /><strong>{label}</strong>{draft.appearance.theme === id && <Check size={16} />}
                      </button>
                    ))}
                  </div>
                  <div className="font-size-setting">
                    <label className="field-label">正文字号（px）<input type="number" min="12" max="20" step="1" value={draft.appearance.fontSize} onChange={(event) => updateDraft((current) => ({ ...current, appearance: { ...current.appearance, fontSize: Number(event.target.value) } }))} /></label>
                    <p>默认 14px。减少动态效果会跟随操作系统的辅助功能设置。</p>
                  </div>
                  <div className="appearance-note"><Palette size={17} /><p>界面使用系统字体与高对比度中性色。风险色仅用于标记命令风险。</p></div>
                </SettingSection>
              ) : section === "security" ? (
                <SettingSection title="安全与存储" description="了解本机数据如何保存，以及诊断信息的用途。">
                  {fallbackMessage && <div className="inline-alert"><Info size={17} /><p>{fallbackMessage}</p></div>}
                  <div className="settings-group no-top-border">
                    <Toggle checked={draft.security.encryptApiKey} onChange={(checked) => updateDraft((current) => ({ ...current, security: { ...current.security, encryptApiKey: checked } }))} label="加密保存 API Key" description="使用 Windows 当前用户的 DPAPI 加密本机密钥；更换设备或用户后需重新填写。" />
                  </div>
                  <div className="storage-details">
                    <h3>本机环境</h3>
                    <dl>
                      <div><dt>系统</dt><dd>{envInfo?.osVersion ?? "未读取"}</dd></div>
                      <div><dt>设备</dt><dd>{envInfo?.computerName ?? "未读取"}</dd></div>
                      <div><dt>权限</dt><dd>{envInfo?.isAdmin ? "管理员" : "标准用户"}</dd></div>
                      <div><dt>配置位置</dt><dd><code>{envInfo?.configPath ?? "未读取"}</code></dd></div>
                    </dl>
                  </div>
                  <div className="section-toolbar storage-actions">
                    <button type="button" className="button secondary compact" onClick={() => { void ipcInvoke("open_config_dir", {}).catch((error) => notify(`打开配置目录失败：${String(error)}`, "error")); }}><FolderOpen size={15} />打开配置目录</button>
                  </div>
                  <p className="security-note"><ShieldCheck size={16} />诊断内容仅发送给你配置的模型服务，不会另传至 Harness 服务。</p>
                </SettingSection>
              ) : (
                <SettingSection title="关于 Harness" description="安静、精确、可信的 Windows 电脑维修助手。">
                  <div className="about-brand"><span className="brand-mark">H</span><div><h3>Harness</h3><p>版本 0.1.0</p></div></div>
                  <p className="about-copy">Harness 将模型诊断能力与本机命令执行连接起来。每个需要权限的步骤都清晰展示命令、影响与风险，最终决定始终由你做出。</p>
                  <div className="about-meta"><span>运行平台</span><strong>Windows · Tauri</strong></div>
                  <div className="about-meta"><span>WebView2</span><strong>{envInfo?.webview2Version ?? "未读取"}</strong></div>
                  <div className="about-meta"><span>界面设计</span><strong>unslop by @heydamir, CC BY 4.0</strong></div>
                  <div className="about-principles"><div><ShieldCheck size={17} /><span>本机数据优先</span></div><div><KeyRound size={17} /><span>权限透明可控</span></div><div><Monitor size={17} /><span>为 Windows 设计</span></div></div>
                </SettingSection>
              )}
            </div>
          </div>
          <footer className="settings-footer">
            <span>{dirty ? "有未保存的更改" : "所有更改已保存"}</span>
            <div><button type="button" className="button secondary" onClick={requestClose}>取消</button><button type="button" className="button primary" onClick={() => void submit()} disabled={!dirty || saving}><Save size={16} />{saving ? "正在保存…" : "保存设置"}</button></div>
          </footer>
        </div>
      </div>
      {closeConfirm && (
        <ConfirmDialog
          title="放弃未保存的修改？"
          description="关闭后，本次在设置中的更改不会写入本机配置。"
          confirmLabel="放弃修改"
          danger
          onCancel={() => setCloseConfirm(false)}
          onConfirm={() => { setCloseConfirm(false); setOpen(false); }}
        />
      )}
    </>,
    document.body,
  );
}
