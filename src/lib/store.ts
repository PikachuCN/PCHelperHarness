import { create } from "zustand";
import { ipcInvoke } from "./ipc";
import type {
  AppConfig,
  ApprovalMode,
  ApprovalResolution,
  ChatMessage,
  DriverInfo,
  DriverUpdateScanResult,
  EnvInfo,
  HarnessEventMap,
  HarnessEventName,
  PendingApproval,
  PluginCatalog,
  RiskAssessment,
  SessionSummary,
  SkillInfo,
  SystemOverviewSection,
  SystemOverviewSectionState,
  SystemSettingsTarget,
  ToolRun,
  TurnActivity,
  TurnPhase,
} from "./types";

interface ToastState {
  id: number;
  message: string;
  tone: "info" | "success" | "error";
}

export const systemOverviewSections: SystemOverviewSection[] = ["basic", "activation", "hardware", "drivers", "software", "storage", "network", "security"];

interface HarnessState {
  initialized: boolean;
  loadingSession: boolean;
  sending: boolean;
  creatingSession: boolean;
  sessions: SessionSummary[];
  currentSessionId: string | null;
  messages: ChatMessage[];
  approvals: PendingApproval[];
  generatingSessionId: string | null;
  resolvingApprovalIds: string[];
  config: AppConfig | null;
  envInfo: EnvInfo | null;
  skills: SkillInfo[];
  pluginCatalog: PluginCatalog | null;
  sidebarCollapsed: boolean;
  settingsOpen: boolean;
  activeView: "chat" | "systemOverview";
  systemOverview: Partial<Record<SystemOverviewSection, SystemOverviewSectionState>>;
  driverUpdateScan: SystemOverviewSectionState<DriverUpdateScanResult>;
  selectedModel: string;
  availableModels: string[];
  approvalMode: ApprovalMode;
  toast: ToastState | null;
  configFallbackMessage: string | null;
  activity: TurnActivity | null;
  initialize: () => Promise<void>;
  newSession: () => Promise<void>;
  selectSession: (sessionId: string) => Promise<void>;
  refreshSession: (sessionId: string) => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  cancelTurn: () => Promise<void>;
  resolveApproval: (approvalId: string, approved: boolean, reason?: string, confirmationText?: string) => Promise<ApprovalResolution | null>;
  renameSession: (sessionId: string, title: string) => Promise<boolean>;
  deleteSession: (sessionId: string) => Promise<void>;
  exportSession: (sessionId: string) => Promise<void>;
  openSystemOverview: () => void;
  loadSystemOverviewSection: (section: SystemOverviewSection, force?: boolean) => Promise<void>;
  refreshSystemOverview: () => void;
  scanDriverUpdates: () => Promise<void>;
  exportSystemOverview: (content: string) => Promise<void>;
  openSystemSettings: (target: SystemSettingsTarget) => Promise<void>;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
  setSelectedModel: (model: string) => void;
  setAvailableModels: (models: string[]) => void;
  setApprovalMode: (mode: ApprovalMode) => void;
  saveConfig: (config: AppConfig) => Promise<boolean>;
  loadSkills: (reload?: boolean) => Promise<SkillInfo[]>;
  loadPluginCatalog: () => Promise<PluginCatalog | null>;
  restartAsAdmin: () => Promise<void>;
  handleEvent: <K extends HarnessEventName>(event: K, payload: HarnessEventMap[K]) => void;
  notify: (message: string, tone?: ToastState["tone"]) => void;
  dismissToast: () => void;
}

function normalizeRisk(risk: RiskAssessment): RiskAssessment {
  const raw = risk as RiskAssessment & { summary_zh?: string; requires_typed_confirmation?: boolean; matched_rules?: string[] };
  return {
    level: String(risk.level).toLowerCase() as RiskAssessment["level"],
    summaryZh: risk.summaryZh ?? raw.summary_zh ?? "请核对命令及其本地影响。",
    reasons: risk.reasons ?? [],
    requiresTypedConfirmation: risk.requiresTypedConfirmation ?? raw.requires_typed_confirmation ?? false,
    matchedRules: risk.matchedRules ?? raw.matched_rules ?? [],
  };
}

function normalizeTool(tool: ToolRun): ToolRun {
  return { ...tool, risk: normalizeRisk(tool.risk) };
}

function normalizeMessage(message: ChatMessage): ChatMessage {
  return { ...message, tools: message.tools.map(normalizeTool) };
}

function normalizeApproval(approval: PendingApproval): PendingApproval {
  const risk = normalizeRisk(approval.risk);
  return { ...approval, risk, level: String(approval.level ?? risk.level).toLowerCase() as PendingApproval["level"] };
}

function updateAssistant(
  messages: ChatMessage[],
  messageId: string,
  update: (message: ChatMessage) => ChatMessage,
): ChatMessage[] {
  const target = messages.find((message) => message.id === messageId);
  if (!target) {
    return [...messages, update({ id: messageId, role: "assistant", content: "", reasoning: "", createdAt: new Date().toISOString(), tools: [] })];
  }
  return messages.map((message) => message.id === messageId ? update(message) : message);
}

function upsertTool(message: ChatMessage, tool: ToolRun): ChatMessage {
  const normalized = normalizeTool(tool);
  const exists = message.tools.some((item) => item.callId === normalized.callId);
  return {
    ...message,
    tools: exists
      ? message.tools.map((item) => item.callId === normalized.callId ? normalized : item)
      : [...message.tools, normalized],
  };
}

const refreshRequests = new Map<string, number>();

function hasLocalUser(messages: ChatMessage[]): boolean {
  return messages.some((message) => message.id.startsWith("local-user-"));
}

const phaseLabels: Record<TurnPhase, string> = {
  preparing: "准备诊断",
  connecting: "等待模型响应",
  reasoning: "模型正在分析",
  responding: "正在生成回复",
  approval: "等待你的审批",
  tool: "正在执行工具",
  organizing: "整理执行结果",
  stopping: "正在停止",
  done: "本轮已完成",
  cancelled: "已停止",
  error: "本轮未完成",
  max_steps: "已达最大步骤",
};

function advanceActivity(
  current: TurnActivity | null,
  sessionId: string | null,
  phase: TurnPhase,
  detail: string,
  step: number,
  maxSteps: number,
  active = true,
): TurnActivity {
  const label = phaseLabels[phase];
  const sameRun = current && phase !== "preparing" && (current.sessionId === sessionId || current.sessionId === null);
  if (sameRun && current.phase === phase && current.detail === detail && current.step === step && current.active === active) {
    return current;
  }
  const now = Date.now();
  const previousEvents = sameRun ? current.events : [];
  const last = previousEvents[previousEvents.length - 1];
  const shouldAppend = !last || last.label !== label || last.detail !== detail;
  const events = shouldAppend
    ? [...previousEvents, { id: `${now}-${previousEvents.length}`, label, detail, at: now }].slice(-6)
    : previousEvents;
  return {
    sessionId,
    active,
    phase,
    label,
    detail,
    startedAt: sameRun ? current.startedAt : now,
    updatedAt: now,
    step,
    maxSteps,
    receivedChars: sameRun ? current.receivedChars : 0,
    events,
  };
}

export const useHarnessStore = create<HarnessState>((set, get) => ({
  initialized: false,
  loadingSession: false,
  sending: false,
  creatingSession: false,
  sessions: [],
  currentSessionId: null,
  messages: [],
  approvals: [],
  generatingSessionId: null,
  resolvingApprovalIds: [],
  config: null,
  envInfo: null,
  skills: [],
  pluginCatalog: null,
  sidebarCollapsed: false,
  settingsOpen: false,
  activeView: "chat",
  systemOverview: {},
  driverUpdateScan: { status: "idle" },
  selectedModel: "",
  availableModels: [],
  approvalMode: "manual",
  toast: null,
  configFallbackMessage: null,
  activity: null,

  initialize: async () => {
    const [configResult, sessionsResult, envResult, skillsResult, pluginResult] = await Promise.allSettled([
      ipcInvoke("get_config", {}),
      ipcInvoke("list_sessions", {}),
      ipcInvoke("get_env_info", {}),
      ipcInvoke("list_skills", {}),
      ipcInvoke("get_plugin_catalog", {}),
    ]);
    if (configResult.status === "fulfilled") {
      const config = configResult.value;
      const provider = config.providers.find((item) => item.id === config.activeProviderId) ?? config.providers[0];
      set({ config, selectedModel: provider?.model ?? "", availableModels: provider?.models ?? [], approvalMode: config.approval.mode });
    } else {
      get().notify(`读取配置失败：${String(configResult.reason)}`, "error");
    }
    if (sessionsResult.status === "fulfilled") set({ sessions: sessionsResult.value });
    else get().notify(`读取会话失败：${String(sessionsResult.reason)}`, "error");
    if (envResult.status === "fulfilled") {
      set({ envInfo: envResult.value });
      if (envResult.value.configUsedFallback) {
        set({ configFallbackMessage: `程序目录不可写，配置已回退到用户数据目录（${envResult.value.configPath}）` });
      }
    }
    if (skillsResult.status === "fulfilled") set({ skills: skillsResult.value });
    if (pluginResult.status === "fulfilled") set({ pluginCatalog: pluginResult.value });
    else get().notify(`读取插件状态失败：${String(pluginResult.reason)}`, "error");
    set({ initialized: true });
    const resumeSessionId = envResult.status === "fulfilled" ? envResult.value.resumeSessionId : null;
    if (resumeSessionId) await get().selectSession(resumeSessionId);
  },

  newSession: async () => {
    const current = get();
    if (current.creatingSession || current.sending) {
      set({ activeView: "chat" });
      return;
    }
    if (current.currentSessionId && current.messages.length === 0) {
      set({ activeView: "chat" });
      if (window.innerWidth <= 900) set({ sidebarCollapsed: true });
      return;
    }
    set({ creatingSession: true, activeView: "chat" });
    try {
      const summary = await ipcInvoke("create_session", {});
      set((state) => ({
        sessions: [summary, ...state.sessions.filter((session) => session.id !== summary.id)],
        currentSessionId: summary.id,
        messages: [],
        approvals: [],
        activity: null,
        loadingSession: false,
        creatingSession: false,
      }));
      if (window.innerWidth <= 900) set({ sidebarCollapsed: true });
    } catch (error) {
      set({ creatingSession: false });
      get().notify(`新建对话失败：${String(error)}`, "error");
    }
  },

  selectSession: async (sessionId) => {
    set({ activeView: "chat" });
    if (get().currentSessionId === sessionId && !get().loadingSession) {
      if (window.innerWidth <= 900) set({ sidebarCollapsed: true });
      return;
    }
    set({ currentSessionId: sessionId, loadingSession: true, messages: [] });
    if (window.innerWidth <= 900) set({ sidebarCollapsed: true });
    try {
      const result = await ipcInvoke("load_session", { sessionId });
      const loaded = Array.isArray(result) ? result : (result as unknown as { messages: ChatMessage[] }).messages;
      if (get().currentSessionId === sessionId) set({ messages: loaded.map(normalizeMessage), loadingSession: false });
    } catch (error) {
      if (get().currentSessionId === sessionId) set({ loadingSession: false });
      get().notify(`无法加载对话：${String(error)}`, "error");
    }
  },

  refreshSession: async (sessionId) => {
    const requestId = (refreshRequests.get(sessionId) ?? 0) + 1;
    refreshRequests.set(sessionId, requestId);
    const [messagesResult, sessionsResult] = await Promise.allSettled([
      ipcInvoke("load_session", { sessionId }),
      ipcInvoke("list_sessions", {}),
    ]);
    if (refreshRequests.get(sessionId) !== requestId) return;
    const current = get();
    if (current.currentSessionId !== sessionId || current.generatingSessionId === sessionId || current.sending) return;
    const messages = messagesResult.status === "fulfilled"
      ? (Array.isArray(messagesResult.value) ? messagesResult.value : (messagesResult.value as unknown as { messages: ChatMessage[] }).messages).map(normalizeMessage)
      : null;
    set((state) => ({
      messages: messages ?? state.messages,
      sessions: sessionsResult.status === "fulfilled" ? sessionsResult.value : state.sessions,
    }));
  },

  sendMessage: async (rawContent) => {
    const content = rawContent.trim();
    const state = get();
    if (!content || state.generatingSessionId || state.sending || state.creatingSession) return;
    let sessionId = state.currentSessionId;
    if (!sessionId) {
      await get().newSession();
      sessionId = get().currentSessionId;
      if (!sessionId) return;
    }
    const now = new Date().toISOString();
    const userMessage: ChatMessage = {
      id: `local-user-${crypto.randomUUID()}`,
      role: "user",
      content,
      createdAt: now,
      tools: [],
    };
    const provider = state.config?.providers.find((item) => item.id === state.config?.activeProviderId);
    const modelLabel = state.selectedModel || "未选择模型";
    set((current) => ({
      messages: [...current.messages, userMessage],
      sending: sessionId === null,
      generatingSessionId: sessionId,
      activity: advanceActivity(
        current.activity,
        sessionId,
        "preparing",
        `正在准备 ${provider?.name ?? "模型服务"} · ${modelLabel} 的请求`,
        0,
        current.config?.execution.maxSteps ?? 25,
      ),
      sessions: sessionId
        ? current.sessions.map((session) => session.id === sessionId ? { ...session, updatedAt: now } : session)
        : current.sessions,
    }));
    try {
      const result = await ipcInvoke("send_message", {
        sessionId,
        content,
        model: get().selectedModel,
        approvalMode: get().approvalMode,
      });
      set((current) => {
        const adopt = current.currentSessionId === result.sessionId
          || (current.currentSessionId === null && hasLocalUser(current.messages))
          || current.sending;
        if (!adopt) return current;
        const detail = result.status === "done"
          ? "模型回复与本机执行已结束，可继续描述下一步"
          : result.status === "cancelled"
            ? "已停止当前请求，已产生的对话与工具结果仍保留"
            : result.status === "max_steps"
              ? "本轮已达到步骤上限，请根据现有结果继续指导助手"
              : "请求未能完成，请检查接口配置或稍后重试";
        return {
          currentSessionId: result.sessionId,
          sending: false,
          generatingSessionId: null,
          activity: advanceActivity(current.activity, result.sessionId, result.status, detail, current.activity?.step ?? 0, current.config?.execution.maxSteps ?? 25, false),
        };
      });
      await get().refreshSession(result.sessionId);
    } catch (error) {
      set((current) => ({
        sending: false,
        generatingSessionId: null,
        activity: advanceActivity(
          current.activity,
          current.activity?.sessionId ?? sessionId,
          "error",
          `请求失败：${String(error)}`,
          current.activity?.step ?? 0,
          current.config?.execution.maxSteps ?? 25,
          false,
        ),
        messages: current.messages.some((message) => message.id === userMessage.id)
          ? [...current.messages, { id: `local-error-${crypto.randomUUID()}`, role: "assistant", content: "请求未能完成。请检查接口配置后重试。", createdAt: new Date().toISOString(), tools: [] }]
          : current.messages,
      }));
      get().notify(`发送失败：${String(error)}`, "error");
    }
  },

  cancelTurn: async () => {
    const sessionId = get().generatingSessionId;
    if (!sessionId) return;
    set((current) => ({
      activity: current.activity
        ? advanceActivity(current.activity, sessionId, "stopping", "正在停止模型请求或本机进程，并保存已产生的结果", current.activity.step, current.activity.maxSteps)
        : null,
    }));
    try {
      await ipcInvoke("cancel_turn", { sessionId });
    } catch (error) {
      get().notify(`停止失败：${String(error)}`, "error");
    }
  },

  resolveApproval: async (approvalId, approved, reason, confirmationText) => {
    if (get().resolvingApprovalIds.includes(approvalId)) return null;
    set((state) => ({ resolvingApprovalIds: [...state.resolvingApprovalIds, approvalId] }));
    try {
      const resolution = await ipcInvoke("resolve_approval", { approvalId, approved, reason, confirmationText });
      set((state) => ({
        approvals: resolution.status === "confirmation_required"
          ? state.approvals.map((approval) => approval.id === approvalId ? { ...approval, confirmationReadyAt: resolution.readyAt ?? Date.now() + 3000 } : approval)
          : state.approvals.filter((approval) => approval.id !== approvalId),
        resolvingApprovalIds: state.resolvingApprovalIds.filter((id) => id !== approvalId),
      }));
      return resolution;
    } catch (error) {
      set((state) => ({ resolvingApprovalIds: state.resolvingApprovalIds.filter((id) => id !== approvalId) }));
      get().notify(`审批提交失败：${String(error)}`, "error");
      return null;
    }
  },

  renameSession: async (sessionId, rawTitle) => {
    const title = rawTitle.trim();
    if (!title) return false;
    try {
      await ipcInvoke("rename_session", { sessionId, title });
      set((state) => ({ sessions: state.sessions.map((session) => session.id === sessionId ? { ...session, title } : session) }));
      return true;
    } catch (error) {
      get().notify(`重命名失败：${String(error)}`, "error");
      return false;
    }
  },

  deleteSession: async (sessionId) => {
    try {
      await ipcInvoke("delete_session", { sessionId });
      set((state) => ({
        sessions: state.sessions.filter((session) => session.id !== sessionId),
        currentSessionId: state.currentSessionId === sessionId ? null : state.currentSessionId,
        messages: state.currentSessionId === sessionId ? [] : state.messages,
        approvals: state.approvals.filter((approval) => approval.sessionId !== sessionId),
        generatingSessionId: state.generatingSessionId === sessionId ? null : state.generatingSessionId,
      }));
      get().notify("对话已删除", "success");
    } catch (error) {
      get().notify(`删除失败：${String(error)}`, "error");
    }
  },

  exportSession: async (sessionId) => {
    try {
      const path = await ipcInvoke("export_session", { sessionId });
      get().notify(`已导出到 ${path}`, "success");
    } catch (error) {
      get().notify(`导出失败：${String(error)}`, "error");
    }
  },

  openSystemOverview: () => {
    set({ activeView: "systemOverview" });
    if (window.innerWidth <= 900) set({ sidebarCollapsed: true });
    for (const section of systemOverviewSections) {
      const current = get().systemOverview[section];
      if (!current?.data && current?.status !== "loading") void get().loadSystemOverviewSection(section);
    }
  },

  loadSystemOverviewSection: async (section, force = false) => {
    const current = get().systemOverview[section];
    if (!force && (current?.status === "loading" || current?.data)) return;
    if (current?.status === "loading") return;
    set((state) => ({
      systemOverview: {
        ...state.systemOverview,
        [section]: { ...state.systemOverview[section], status: "loading", error: undefined },
      },
    }));
    try {
      const response = await ipcInvoke("get_system_overview_section", { section });
      const data = section === "drivers" && response.section === "drivers"
        ? { ...response.data, updateScan: get().driverUpdateScan.data ?? null }
        : response.data;
      set((state) => ({
        systemOverview: {
          ...state.systemOverview,
          [section]: { status: "success", data, loadedAt: new Date().toISOString() },
        },
      }));
    } catch (error) {
      set((state) => ({
        systemOverview: {
          ...state.systemOverview,
          [section]: { ...state.systemOverview[section], status: "error", error: String(error) },
        },
      }));
    }
  },

  refreshSystemOverview: () => {
    for (const section of systemOverviewSections) void get().loadSystemOverviewSection(section, true);
  },

  scanDriverUpdates: async () => {
    if (get().driverUpdateScan.status === "loading") return;
    set((state) => ({ driverUpdateScan: { ...state.driverUpdateScan, status: "loading", error: undefined } }));
    try {
      const result = await ipcInvoke("scan_driver_updates", {});
      set((state) => {
        const driverState = state.systemOverview.drivers;
        const driverData = driverState?.data as DriverInfo | undefined;
        return {
          driverUpdateScan: { status: "success", data: result, loadedAt: new Date().toISOString() },
          systemOverview: driverData ? {
            ...state.systemOverview,
            drivers: { status: driverState?.status ?? "success", data: { ...driverData, updateScan: result }, loadedAt: driverState?.loadedAt, error: driverState?.error },
          } : state.systemOverview,
        };
      });
      get().notify(result.updates.length ? `发现 ${result.updates.length} 个可用驱动更新` : "Windows 更新暂未提供适用的驱动更新", "success");
    } catch (error) {
      set((state) => ({ driverUpdateScan: { ...state.driverUpdateScan, status: "error", error: String(error) } }));
      get().notify(`驱动更新扫描失败：${String(error)}`, "error");
    }
  },

  exportSystemOverview: async (content) => {
    try {
      const path = await ipcInvoke("export_system_overview_report", { content });
      get().notify(`脱敏报告已导出到 ${path}`, "success");
    } catch (error) {
      get().notify(`导出系统概况失败：${String(error)}`, "error");
    }
  },

  openSystemSettings: async (target) => {
    try {
      await ipcInvoke("open_system_settings", { target });
    } catch (error) {
      get().notify(`无法打开系统设置：${String(error)}`, "error");
    }
  },

  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setSidebarCollapsed: (sidebarCollapsed) => set({ sidebarCollapsed }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setSelectedModel: (selectedModel) => set({ selectedModel }),
  setAvailableModels: (availableModels) => set({ availableModels }),
  setApprovalMode: (approvalMode) => set({ approvalMode }),

  saveConfig: async (config) => {
    try {
      await ipcInvoke("save_config", { config });
      const provider = config.providers.find((item) => item.id === config.activeProviderId) ?? config.providers[0];
      const savedConfig = { ...config, providers: config.providers.map((item) => ({ ...item, apiKey: { ...item.apiKey, value: "" } })) };
      set({ config: savedConfig, approvalMode: config.approval.mode, selectedModel: provider?.model ?? "", availableModels: provider?.models ?? [] });
      get().notify("设置已保存", "success");
      return true;
    } catch (error) {
      get().notify(`保存失败：${String(error)}`, "error");
      return false;
    }
  },

  loadSkills: async (reload = false) => {
    try {
      const skills = await ipcInvoke(reload ? "reload_skills" : "list_skills", {});
      set({ skills });
      return skills;
    } catch (error) {
      get().notify(`读取 Skills 失败：${String(error)}`, "error");
      return get().skills;
    }
  },

  loadPluginCatalog: async () => {
    try {
      const pluginCatalog = await ipcInvoke("get_plugin_catalog", {});
      set({ pluginCatalog });
      return pluginCatalog;
    } catch (error) {
      get().notify(`读取插件状态失败：${String(error)}`, "error");
      return get().pluginCatalog;
    }
  },

  restartAsAdmin: async () => {
    try {
      await ipcInvoke("restart_as_admin", { sessionId: get().currentSessionId });
    } catch (error) {
      get().notify(`无法以管理员身份重启：${String(error)}`, "error");
    }
  },

  handleEvent: (event, payload) => {
    const state = get();
    if (event === "config_path_fallback") {
      const data = payload as HarnessEventMap["config_path_fallback"];
      set({ configFallbackMessage: `${data.message}（${data.path}）` });
      return;
    }
    if (event === "config_warning") {
      const data = payload as HarnessEventMap["config_warning"];
      get().notify(data.message, "error");
      return;
    }
    if (event === "session_updated") {
      const summary = payload as HarnessEventMap["session_updated"];
      set((current) => ({
        sessions: [summary, ...current.sessions.filter((session) => session.id !== summary.id)],
        currentSessionId: current.sending && current.currentSessionId === null && hasLocalUser(current.messages) ? summary.id : current.currentSessionId,
        generatingSessionId: current.sending ? summary.id : current.generatingSessionId,
        activity: current.sending && current.activity ? { ...current.activity, sessionId: summary.id } : current.activity,
      }));
      return;
    }
    if (event === "user_message") {
      const data = payload as HarnessEventMap["user_message"];
      const adopt = state.currentSessionId === data.sessionId || (state.sending && state.currentSessionId === null && hasLocalUser(state.messages));
      if (!adopt) return;
      set((current) => ({
        currentSessionId: data.sessionId,
        sending: false,
        generatingSessionId: data.sessionId,
        activity: current.activity ? { ...current.activity, sessionId: data.sessionId } : current.activity,
        messages: current.messages.some((message) => message.id === data.message.id)
          ? current.messages
          : current.messages.some((message) => message.id.startsWith("local-user-"))
            ? current.messages.map((message) => message.id.startsWith("local-user-") ? normalizeMessage(data.message) : message)
            : [...current.messages, normalizeMessage(data.message)],
      }));
      return;
    }
    if (event === "assistant_started") {
      const data = payload as HarnessEventMap["assistant_started"];
      const adopt = state.currentSessionId === data.sessionId || (state.sending && state.currentSessionId === null && hasLocalUser(state.messages));
      if (!adopt) return;
      set((current) => {
        const exists = current.messages.some((message) => message.id === data.message.id);
        const step = current.messages.filter((message) => message.role === "assistant").length + (exists ? 0 : 1);
        const provider = current.config?.providers.find((item) => item.id === current.config?.activeProviderId);
        return {
          currentSessionId: data.sessionId,
          sending: false,
          generatingSessionId: data.sessionId,
          messages: exists ? current.messages : [...current.messages, normalizeMessage(data.message)],
          activity: advanceActivity(
            current.activity,
            data.sessionId,
            "connecting",
            `正在请求 ${provider?.name ?? "模型服务"} · ${get().selectedModel || "当前模型"}，等待首个响应`,
            step,
            current.config?.execution.maxSteps ?? 25,
          ),
        };
      });
      return;
    }
    if (event === "assistant_sync") {
      const data = payload as HarnessEventMap["assistant_sync"];
      const adopt = state.currentSessionId === data.sessionId || (state.sending && state.currentSessionId === null && hasLocalUser(state.messages));
      if (!adopt) return;
      set((current) => ({
        currentSessionId: data.sessionId,
        sending: false,
        generatingSessionId: data.sessionId,
        messages: updateAssistant(current.messages, data.message.id, () => normalizeMessage(data.message)),
      }));
      return;
    }
    if (event === "turn_done") {
      const data = payload as HarnessEventMap["turn_done"];
      const detail = data.status === "done"
        ? "模型回复与本机执行已结束，可继续描述下一步"
        : data.status === "cancelled"
          ? "已停止当前请求，已产生的对话与工具结果仍保留"
          : data.status === "max_steps"
            ? "本轮已达到步骤上限，请根据现有结果继续指导助手"
            : data.error ?? "请求未能完成，请检查接口配置或稍后重试";
      if (state.generatingSessionId === data.sessionId || state.sending) {
        set((current) => ({
          generatingSessionId: null,
          sending: false,
          activity: advanceActivity(
            current.activity,
            data.sessionId,
            data.status,
            detail,
            current.activity?.step ?? 0,
            current.config?.execution.maxSteps ?? 25,
            false,
          ),
        }));
      }
      if (state.currentSessionId === data.sessionId && (data.status === "cancelled" || data.status === "error")) {
        set((current) => ({
          messages: current.messages.map((message) => ({
            ...message,
            content: message.role === "assistant" && !message.content && message.tools.length === 0
              ? data.status === "cancelled" ? "已停止生成。" : "本次请求未能完成。"
              : message.content,
            tools: message.tools.map((tool) => tool.status === "running" || tool.status === "pending" ? { ...tool, status: "cancelled" } : tool),
          })),
        }));
      }
      if (data.status === "error" && data.error) get().notify(data.error, "error");
      void get().refreshSession(data.sessionId);
      return;
    }
    if (event === "approval_request") {
      const pending = normalizeApproval(payload as HarnessEventMap["approval_request"]);
      set((current) => ({
        approvals: current.approvals.some((approval) => approval.id === pending.id) ? current.approvals : [...current.approvals, pending],
        activity: (current.currentSessionId === pending.sessionId || current.sending)
          ? advanceActivity(current.activity, pending.sessionId, "approval", `${pending.level.toUpperCase()} · ${pending.explanationZh}`, current.activity?.step ?? 0, current.config?.execution.maxSteps ?? 25)
          : current.activity,
      }));
      return;
    }
    if (event === "approval_confirmation_required") {
      const data = payload as HarnessEventMap["approval_confirmation_required"];
      set((current) => ({
        approvals: current.approvals.map((approval) => approval.id === data.approvalId ? { ...approval, confirmationReadyAt: data.readyAt, requiresTypedConfirmation: data.requiresTypedConfirmation } : approval),
        activity: current.currentSessionId === data.sessionId
          ? advanceActivity(current.activity, data.sessionId, "approval", "高风险二次确认已开启，请核对命令并等待倒计时结束", current.activity?.step ?? 0, current.config?.execution.maxSteps ?? 25)
          : current.activity,
      }));
      return;
    }
    if (event === "approval_resolved") {
      const data = payload as HarnessEventMap["approval_resolved"];
      set((current) => ({
        approvals: current.approvals.filter((approval) => approval.id !== data.approvalId),
        activity: current.currentSessionId === data.sessionId
          ? advanceActivity(current.activity, data.sessionId, data.allowed ? "organizing" : "reasoning", data.allowed ? "审批已通过，准备执行本机操作" : "操作已拒绝，正在把原因回传给模型", current.activity?.step ?? 0, current.config?.execution.maxSteps ?? 25)
          : current.activity,
      }));
      return;
    }
    if (state.currentSessionId !== dataSessionId(payload)) return;
    if (event === "assistant_delta" || event === "reasoning_delta") {
      const data = payload as HarnessEventMap["assistant_delta"];
      set((current) => ({
        messages: updateAssistant(current.messages, data.messageId, (message) => event === "assistant_delta"
          ? { ...message, content: message.content + data.delta }
          : { ...message, reasoning: (message.reasoning ?? "") + data.delta }),
        activity: advanceActivity(
          current.activity,
          data.sessionId,
          event === "reasoning_delta" ? "reasoning" : "responding",
          event === "reasoning_delta" ? "模型正在分析系统信息与排查路径" : "正在生成诊断说明与下一步建议",
          current.activity?.step ?? 0,
          current.config?.execution.maxSteps ?? 25,
        ),
      }));
    } else if (event === "tool_started" || event === "tool_done") {
      const data = payload as HarnessEventMap["tool_started"];
      set((current) => {
        const done = event === "tool_done";
        const phase: TurnPhase = done ? "organizing" : data.tool.status === "running" ? "tool" : "approval";
        const detail = done
          ? `${data.tool.explanationZh || data.tool.name} · 耗时 ${(data.tool.durationMs / 1000).toFixed(1)} 秒${data.tool.exitCode === 0 ? "，等待模型整理结果" : "，将把错误与输出回传给模型"}`
          : data.tool.status === "running"
            ? `${data.tool.explanationZh || data.tool.name} · 输出将实时显示在工具卡片中`
            : `${data.tool.explanationZh || data.tool.name} · 审批前不会执行`;
        return {
          messages: updateAssistant(current.messages, data.messageId, (message) => upsertTool(message, data.tool)),
          activity: advanceActivity(current.activity, data.sessionId, phase, detail, current.activity?.step ?? 0, current.config?.execution.maxSteps ?? 25),
        };
      });
    } else if (event === "tool_output") {
      const data = payload as HarnessEventMap["tool_output"];
      set((current) => ({
        messages: current.messages.map((message) => ({
          ...message,
          tools: message.tools.map((tool) => tool.callId === data.callId ? { ...tool, output: tool.output + data.text } : tool),
        })),
        activity: current.activity ? {
          ...current.activity,
          updatedAt: Date.now(),
          receivedChars: current.activity.receivedChars + data.text.length,
          detail: `正在接收工具输出 · 已接收 ${current.activity.receivedChars + data.text.length} 个字符`,
        } : null,
      }));
    }
  },

  notify: (message, tone = "info") => set({ toast: { id: Date.now(), message, tone } }),
  dismissToast: () => set({ toast: null }),
}));

function dataSessionId(payload: unknown): string {
  return (payload as { sessionId: string }).sessionId;
}
