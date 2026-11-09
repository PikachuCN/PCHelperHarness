import { emitMockEvent } from "./events";
import { mockDriverUpdateScan, mockSystemOverviewSection } from "./mockSystemOverview";
import { mockCancelNetworkTask, mockRunNetworkDiagnostic, mockRunNetworkTest } from "./mockNetwork";
import type {
  AppConfig,
  ChatMessage,
  IpcCommand,
  IpcCommandMap,
  PendingApproval,
  PluginCatalog,
  PromptMessage,
  RiskAssessment,
  SessionSummary,
  SkillInfo,
  ToolRun,
  TurnStatus,
} from "./types";

const now = Date.now();
const iso = (offsetMinutes = 0) => new Date(now + offsetMinutes * 60_000).toISOString();
const uid = () => crypto.randomUUID();

function summarizeMockTitle(content: string): string {
  const text = content.toLowerCase();
  if (/蓝屏|stop code|终止代码/.test(text)) return "蓝屏故障排查";
  if (/上不了网|无法上网|不能上网|断网|网络连接|dns|wi-fi|wifi/.test(text)) return "网络连接异常排查";
  if (/c\s*盘|磁盘空间|空间不足|磁盘满/.test(text)) return "磁盘空间不足排查";
  if (/开机慢|启动慢|开机速度|启动项/.test(text)) return "开机与启动项排查";
  if (/打印机|打印不了|无法打印/.test(text)) return "打印机故障排查";
  if (/电脑慢|运行缓慢|卡顿|明显变慢|性能/.test(text)) return "电脑运行缓慢排查";
  return content.trim().split(/[。！？!?；;\n]/)[0].slice(0, 24) || "新对话";
}
const lowRisk: RiskAssessment = { level: "l0", summaryZh: "只读取本机状态，不修改配置或文件。", reasons: ["只读系统查询"], requiresTypedConfirmation: false, matchedRules: ["readonly"] };
const kiloFreeModels = ["kilo-auto/free", "nvidia/nemotron-3.5-super-vl:free", "reflection/beam-free"];

let config: AppConfig = {
  version: 2,
  activeProviderId: "kilo-free",
  providers: [{
    id: "kilo-free",
    name: "Kilo 免费模型",
    baseUrl: "https://ai-gateway.kilo.ai/api/v1",
    apiKey: { encrypted: false, value: "" },
    apiMode: "chat_completions",
    model: "kilo-auto/free",
    models: kiloFreeModels,
    temperature: null,
  }, {
    id: "provider-main",
    name: "默认接口",
    baseUrl: "https://api.deepseek.com/v1",
    apiKey: { encrypted: false, value: "" },
    apiMode: "chat_completions",
    model: "deepseek-chat",
    models: ["deepseek-chat", "deepseek-reasoner", "cn:auto", "cn:balanced-model", "cn:deepseek-v4-flash", "cn:deepseek-v4-pro", "cn:deepseek-v4.1-flash", "cn:fast-model", "cn:glm-5.1", "cn:glm-5.2", "cn:glm-5.3", "cn:glm-5.3-flash", "cn:glm-5v-turbo", "cn:hunyuan-image-alpha-edit", "cn:hy3", "cn:hy4-preview", "cn:hy4free"],
    temperature: null,
  }],
  approval: { mode: "manual", readonlyNeedsApproval: false, autoApprovePythonPipInstall: false },
  execution: { defaultTimeoutSec: 120, maxTimeoutSec: 1800, outputLimitKb: 16, maxSteps: 25 },
  appearance: { theme: "system", fontSize: 14 },
  security: { encryptApiKey: true },
};

let sessions: SessionSummary[] = [
  { id: "session-network", title: "网络连接异常排查", createdAt: iso(-35), updatedAt: iso(-12), messageCount: 2 },
  { id: "session-disk", title: "C 盘空间清理", createdAt: iso(-1500), updatedAt: iso(-1440), messageCount: 0 },
  { id: "session-startup", title: "开机启动项检查", createdAt: iso(-12_000), updatedAt: iso(-11_900), messageCount: 0 },
];

const histories: Record<string, ChatMessage[]> = {
  "session-network": [
    { id: "history-user-network", role: "user", content: "电脑突然上不了网，但微信还显示已登录。", createdAt: iso(-34), tools: [] },
    {
      id: "history-assistant-network",
      role: "assistant",
      content: "我先检查网络适配器、网关和 DNS 解析，区分是本地连接问题还是域名解析异常。",
      createdAt: iso(-33),
      tools: [{
        callId: "history-tool-network",
        name: "run_command",
        arguments: { shell: "powershell", command: "Get-NetAdapter; Test-Connection 192.168.1.1 -Count 2; Resolve-DnsName www.microsoft.com" },
        explanationZh: "检查网络连接与 DNS",
        risk: lowRisk,
        status: "success",
        output: "以太网                     已连接\n192.168.1.1  响应正常\nDNS 解析超时",
        durationMs: 1240,
        exitCode: 0,
      }],
    },
  ],
  "session-disk": [],
  "session-startup": [],
};

const skills: SkillInfo[] = [
  { name: "system-diagnosis", description: "检查系统事件、性能、磁盘和常见故障。", source: "内置", path: "skills/system-diagnosis" },
  { name: "network-repair", description: "诊断适配器、DNS、代理与网络连接。", source: "内置", path: "skills/network-repair" },
  { name: "disk-cleanup", description: "安全分析磁盘占用并清理临时文件。", source: "用户", path: "skills/disk-cleanup" },
];

const pluginCatalog: PluginCatalog = {
  plugins: [{ id: "python", name: "Python", version: "3.13.17", shell: "python", path: "C:\\Harness\\python.dll", status: "ready", error: null }],
  python: { available: true, source: "plugin", program: "C:\\Harness\\data\\plugins\\python\\python.exe", error: null },
  directory: "C:\\Harness",
};

const timers = new Map<string, number[]>();
const approvals = new Map<string, PendingApproval>();
const confirmationStarted = new Set<string>();
const activeMessages = new Map<string, string>();
const pendingTurns = new Map<string, (result: { sessionId: string; turnId: string; status: TurnStatus }) => void>();

function completeMockTurn(sessionId: string, turnId: string, status: TurnStatus) {
  const message = [...(histories[sessionId] ?? [])].reverse().find((item) => item.role === "assistant");
  if (message) emitMockEvent("assistant_sync", { sessionId, turnId, message: structuredClone(message) });
  emitMockEvent("turn_done", { sessionId, turnId, status });
  pendingTurns.get(sessionId)?.({ sessionId, turnId, status });
  pendingTurns.delete(sessionId);
}

function later(sessionId: string, callback: () => void, delay: number) {
  const timer = window.setTimeout(() => {
    timers.set(sessionId, (timers.get(sessionId) ?? []).filter((id) => id !== timer));
    callback();
  }, delay);
  timers.set(sessionId, [...(timers.get(sessionId) ?? []), timer]);
}

function clearTimers(sessionId: string) {
  for (const timer of timers.get(sessionId) ?? []) window.clearTimeout(timer);
  timers.delete(sessionId);
}

function appendDelta(sessionId: string, messageId: string, delta: string) {
  const message = histories[sessionId]?.find((item) => item.id === messageId);
  if (message) message.content += delta;
  emitMockEvent("assistant_delta", { sessionId, turnId: "current-turn", messageId, delta });
}

function finishTool(sessionId: string, messageId: string, tool: ToolRun) {
  const message = histories[sessionId]?.find((item) => item.id === messageId);
  const index = message?.tools.findIndex((item) => item.callId === tool.callId) ?? -1;
  if (message && index >= 0) message.tools[index] = structuredClone(tool);
  emitMockEvent("tool_done", { sessionId, turnId: "current-turn", messageId, tool });
}

function runReadOnlyTool(sessionId: string, messageId: string) {
  const tool: ToolRun = {
    callId: uid(),
    name: "run_command",
    arguments: { shell: "powershell", command: "Get-NetAdapter; Get-DnsClientServerAddress; Get-Volume -DriveLetter C" },
    explanationZh: "读取网络与系统状态",
    risk: lowRisk,
    status: "running",
    output: "",
    durationMs: 0,
    exitCode: null,
  };
  histories[sessionId]?.find((item) => item.id === messageId)?.tools.push(tool);
  emitMockEvent("tool_started", { sessionId, turnId: "current-turn", messageId, tool });
  later(sessionId, () => emitMockEvent("tool_output", { sessionId, turnId: "current-turn", callId: tool.callId, stream: "stdout", text: "以太网：已连接\nDNS：自动获取\n" }), 300);
  later(sessionId, () => emitMockEvent("tool_output", { sessionId, turnId: "current-turn", callId: tool.callId, stream: "stdout", text: "C: 剩余 86.4 GB / 237 GB\n" }), 650);
  later(sessionId, () => {
    finishTool(sessionId, messageId, { ...tool, status: "success", output: "以太网：已连接\nDNS：自动获取\nC: 剩余 86.4 GB / 237 GB\n", durationMs: 920, exitCode: 0 });
    appendDelta(sessionId, messageId, "\n\n检查结果显示网络适配器工作正常，C 盘空间也充足。建议下一步核对 DNS 与代理设置；如果需要，我可以继续逐项处理。");
  }, 950);
  later(sessionId, () => completeMockTurn(sessionId, "current-turn", "done"), 1250);
}

function sendMockMessage(args: IpcCommandMap["send_message"]["args"]) {
  const sessionId = args.sessionId ?? uid();
  const turnId = uid();
  const messageId = uid();
  const timestamp = new Date().toISOString();
  const user: ChatMessage = { id: uid(), role: "user", content: args.content, createdAt: timestamp, tools: [] };
  const assistant: ChatMessage = { id: messageId, role: "assistant", content: "", reasoning: "", createdAt: timestamp, tools: [] };
  histories[sessionId] = [...(histories[sessionId] ?? []), user, assistant];
  activeMessages.set(sessionId, messageId);
  const existing = sessions.find((item) => item.id === sessionId);
  const summary: SessionSummary = existing
    ? { ...existing, title: existing.title === "新对话" ? summarizeMockTitle(args.content) : existing.title, updatedAt: timestamp, messageCount: histories[sessionId].length }
    : { id: sessionId, title: summarizeMockTitle(args.content), createdAt: timestamp, updatedAt: timestamp, messageCount: 2 };
  sessions = [summary, ...sessions.filter((item) => item.id !== sessionId)];
  emitMockEvent("user_message", { sessionId, turnId, message: user });
  emitMockEvent("session_updated", summary);
  emitMockEvent("assistant_started", { sessionId, turnId, message: assistant });

  const response = args.content.includes("网")
    ? "我会先做只读检查，确认适配器、网关和 DNS 的状态，不会修改你的网络配置。"
    : args.content.includes("慢")
      ? "我会先检查 CPU、内存、磁盘占用和启动项，找出最可能影响速度的原因。"
      : args.content.includes("体检")
        ? "我会按系统状态、磁盘、网络和常见错误四个方向做一次基础检查。"
        : "收到。我会先收集必要的本机信息，再给出清晰的排查步骤；涉及系统变更时会先征求你的同意。";
  if (!args.content.includes("无流式")) {
    later(sessionId, () => {
      const message = histories[sessionId]?.find((item) => item.id === messageId);
      const delta = "先确认用户描述的问题范围，优先选择不会修改系统的检查。";
      if (message) message.reasoning = (message.reasoning ?? "") + delta;
      emitMockEvent("reasoning_delta", { sessionId, turnId, messageId, delta });
    }, 160);
    later(sessionId, () => {
      const message = histories[sessionId]?.find((item) => item.id === messageId);
      const delta = "随后根据读取到的状态整理下一步，再决定是否需要用户审批。";
      if (message) message.reasoning = (message.reasoning ?? "") + delta;
      emitMockEvent("reasoning_delta", { sessionId, turnId, messageId, delta });
    }, 320);
    later(sessionId, () => appendDelta(sessionId, messageId, response), 450);
  }

  if (args.content.includes("无流式")) {
    later(sessionId, () => {
      const assistant = histories[sessionId]?.find((item) => item.id === messageId);
      if (assistant) assistant.content = "这是只发送最终结果、没有逐字 delta 的回复。无需切换会话即可看到。";
      completeMockTurn(sessionId, turnId, "done");
    }, 450);
  } else if (/C\s*盘|清理|删除|格式化/i.test(args.content)) {
    const command = "Get-ChildItem $env:TEMP | Where-Object LastWriteTime -lt (Get-Date).AddDays(-7) | Remove-Item -Recurse";
    const pendingTool: ToolRun = {
      callId: uid(), name: "run_command", arguments: { shell: "powershell", command }, explanationZh: "清理过期临时文件",
      risk: { level: "l3", summaryZh: "此操作会永久删除部分临时文件。正在使用的文件不会被强制删除，但请确认没有需要保留的内容。", reasons: ["删除临时目录中的文件"], requiresTypedConfirmation: false, matchedRules: ["remove_item"] },
      status: "pending", output: "", durationMs: 0, exitCode: null,
    };
    later(sessionId, () => {
      histories[sessionId]?.find((item) => item.id === messageId)?.tools.push(pendingTool);
      emitMockEvent("tool_started", { sessionId, turnId, messageId, tool: pendingTool });
      const approval: PendingApproval = {
        id: uid(), sessionId, turnId, callId: pendingTool.callId, toolName: "run_command", shell: "powershell", command,
        arguments: pendingTool.arguments, explanationZh: "清理当前用户临时目录中已超过 7 天的文件，以释放系统盘空间。",
        risk: pendingTool.risk, level: "l3", mode: args.approvalMode, requiresTypedConfirmation: false, messageId,
      };
      approvals.set(approval.id, approval);
      emitMockEvent("approval_request", approval);
    }, 650);
  } else {
    later(sessionId, () => runReadOnlyTool(sessionId, messageId), 700);
  }
  return new Promise<{ sessionId: string; turnId: string; status: TurnStatus }>((resolve) => {
    pendingTurns.set(sessionId, resolve);
  });
}

export async function mockInvoke<K extends IpcCommand>(command: K, rawArgs: IpcCommandMap[K]["args"]): Promise<unknown> {
  await new Promise((resolve) => window.setTimeout(resolve, 70));
  switch (command) {
    case "create_session": {
      const timestamp = new Date().toISOString();
      const summary: SessionSummary = { id: uid(), title: "新对话", createdAt: timestamp, updatedAt: timestamp, messageCount: 0 };
      sessions = [summary, ...sessions];
      histories[summary.id] = [];
      return summary;
    }
    case "send_message":
      return sendMockMessage(rawArgs as IpcCommandMap["send_message"]["args"]);
    case "cancel_turn": {
      const { sessionId } = rawArgs as IpcCommandMap["cancel_turn"]["args"];
      clearTimers(sessionId);
      completeMockTurn(sessionId, "current-turn", "cancelled");
      return undefined;
    }
    case "resolve_approval": {
      const args = rawArgs as IpcCommandMap["resolve_approval"]["args"];
      const approval = approvals.get(args.approvalId);
      if (!approval) return { status: "rejected", requiresTypedConfirmation: false };
      if (args.approved && approval.level === "l3" && !confirmationStarted.has(approval.id)) {
        confirmationStarted.add(approval.id);
        const readyAt = Date.now() + 3000;
        emitMockEvent("approval_confirmation_required", { approvalId: approval.id, sessionId: approval.sessionId, callId: approval.callId, readyAt, requiresTypedConfirmation: approval.requiresTypedConfirmation });
        return { status: "confirmation_required", readyAt, requiresTypedConfirmation: approval.requiresTypedConfirmation };
      }
      if (args.approved && approval.level === "l3" && approval.requiresTypedConfirmation && args.confirmationText !== "确认") throw new Error("磁盘级操作需要输入“确认”");
      approvals.delete(args.approvalId);
      confirmationStarted.delete(args.approvalId);
      emitMockEvent("approval_resolved", { approvalId: approval.id, sessionId: approval.sessionId, callId: approval.callId, allowed: args.approved });
      const messageId = approval.messageId ?? activeMessages.get(approval.sessionId) ?? "";
      if (!args.approved) {
        const message = histories[approval.sessionId]?.find((item) => item.id === messageId);
        const tool = message?.tools.find((item) => item.callId === approval.callId);
        if (tool) finishTool(approval.sessionId, messageId, { ...tool, status: "rejected", output: `用户未批准此操作${args.reason ? `：${args.reason}` : ""}` });
        appendDelta(approval.sessionId, messageId, `\n\n已按你的要求取消操作${args.reason ? `：${args.reason}` : "。"}`);
        completeMockTurn(approval.sessionId, approval.turnId, "done");
      } else {
        const message = histories[approval.sessionId]?.find((item) => item.id === messageId);
        const previous = message?.tools.find((item) => item.callId === approval.callId);
        if (!previous) return { status: "approved", requiresTypedConfirmation: false };
        const running = { ...previous, status: "running" as const };
        emitMockEvent("tool_started", { sessionId: approval.sessionId, turnId: approval.turnId, messageId, tool: running });
        later(approval.sessionId, () => emitMockEvent("tool_output", { sessionId: approval.sessionId, turnId: approval.turnId, callId: approval.callId, stream: "stdout", text: "已检查临时目录，跳过正在使用的文件。\n" }), 300);
        later(approval.sessionId, () => {
          emitMockEvent("tool_output", { sessionId: approval.sessionId, turnId: approval.turnId, callId: approval.callId, stream: "stdout", text: "已安全释放 1.28 GB。\n" });
          finishTool(approval.sessionId, messageId, { ...running, status: "success", output: "已检查临时目录，跳过正在使用的文件。\n已安全释放 1.28 GB。\n", durationMs: 820, exitCode: 0 });
          appendDelta(approval.sessionId, messageId, "\n\n清理已完成，共释放约 **1.28 GB**。系统文件和个人文档未被修改。");
          completeMockTurn(approval.sessionId, approval.turnId, "done");
        }, 850);
      }
      return { status: args.approved ? "approved" : "rejected", requiresTypedConfirmation: false };
    }
    case "list_sessions": return structuredClone(sessions);
    case "load_session": {
      const { sessionId } = rawArgs as IpcCommandMap["load_session"]["args"];
      return structuredClone(histories[sessionId] ?? []);
    }
    case "get_message_prompt": {
      const { sessionId, messageId } = rawArgs as IpcCommandMap["get_message_prompt"]["args"];
      const history = histories[sessionId] ?? [];
      const messageIndex = history.findIndex((item) => item.id === messageId && item.role === "user");
      if (messageIndex < 0) throw new Error("找不到该用户消息对应的请求上下文");
      const context: PromptMessage[] = [{ role: "system", content: "你是灵犀电脑管家，一名严谨的 Windows 电脑维修工程师。先诊断再动手，执行本机操作前必须获得用户批准。" }];
      for (const item of history.slice(0, messageIndex + 1)) {
        if (item.role === "user") context.push({ role: "user", content: item.content });
        else {
          context.push({ role: "assistant", text: item.content, reasoning: item.reasoning, tool_calls: [] });
          for (const tool of item.tools) context.push({ role: "tool", call_id: tool.callId, content: tool.output });
        }
      }
      return context;
    }
    case "rename_session": {
      const args = rawArgs as IpcCommandMap["rename_session"]["args"];
      sessions = sessions.map((item) => item.id === args.sessionId ? { ...item, title: args.title } : item);
      return undefined;
    }
    case "delete_session": {
      const { sessionId } = rawArgs as IpcCommandMap["delete_session"]["args"];
      sessions = sessions.filter((item) => item.id !== sessionId);
      delete histories[sessionId];
      clearTimers(sessionId);
      return undefined;
    }
    case "export_session": return "C:\\Users\\User\\Documents\\Harness\\对话导出.md";
    case "get_config": return structuredClone(config);
    case "save_config":
      config = structuredClone((rawArgs as IpcCommandMap["save_config"]["args"]).config);
      return undefined;
    case "fetch_models": {
      const { provider } = rawArgs as IpcCommandMap["fetch_models"]["args"];
      await new Promise((resolve) => window.setTimeout(resolve, 500));
      if (!provider.baseUrl.trim()) throw new Error("请先填写接口地址");
      if (provider.id === "kilo-free") return kiloFreeModels;
      return provider.apiMode === "responses" ? ["gpt-5.2", "gpt-5-mini"] : ["cn:auto", "cn:balanced-model", "cn:deep-model", "cn:deepseek-v4-flash", "cn:glm-5.3"];
    }
    case "test_provider": {
      const { provider } = rawArgs as IpcCommandMap["test_provider"]["args"];
      await new Promise((resolve) => window.setTimeout(resolve, 650));
      if (!provider.baseUrl.trim()) return { connected: false, latencyMs: 0, models: [], supportsTools: false };
      const models = provider.id === "kilo-free" ? kiloFreeModels : provider.apiMode === "responses" ? ["gpt-5.2", "gpt-5-mini"] : ["cn:auto", "cn:balanced-model", "cn:deep-model", "cn:deepseek-v4-flash", "cn:glm-5.3"];
      return { connected: true, latencyMs: 328, models: models.map((id) => ({ id })), supportsTools: true };
    }
    case "list_skills": return structuredClone(skills);
    case "reload_skills":
      await new Promise((resolve) => window.setTimeout(resolve, 300));
      return structuredClone(skills);
    case "get_plugin_catalog": return structuredClone(pluginCatalog);
    case "open_plugin_dir":
    case "open_skills_dir":
    case "open_config_dir":
    case "restart_as_admin": return undefined;
    case "get_env_info": return { isAdmin: false, osVersion: "Windows 11 专业版 24H2", computerName: "DESKTOP-HARNESS", userName: "User", configPath: "C:\\Users\\User\\AppData\\Roaming\\Harness\\config.json", configUsedFallback: false, webview2Version: "142.0", resumeSessionId: null, availableShells: ["powershell", "cmd", "python"] };
    case "get_system_overview_section": {
      const { section } = rawArgs as IpcCommandMap["get_system_overview_section"]["args"];
      await new Promise((resolve) => window.setTimeout(resolve, 260));
      return mockSystemOverviewSection(section);
    }
    case "scan_driver_updates":
      await new Promise((resolve) => window.setTimeout(resolve, 650));
      return mockDriverUpdateScan();
    case "open_system_settings": return undefined;
    case "export_system_overview_report": return "C:\\Users\\User\\Documents\\Harness\\系统概况-脱敏.md";
    case "run_network_test": {
      const args = rawArgs as IpcCommandMap["run_network_test"]["args"];
      return mockRunNetworkTest(args.taskId, args.request);
    }
    case "run_network_diagnostic": {
      const args = rawArgs as IpcCommandMap["run_network_diagnostic"]["args"];
      return mockRunNetworkDiagnostic(args.taskId);
    }
    case "cancel_network_task": {
      const args = rawArgs as IpcCommandMap["cancel_network_task"]["args"];
      mockCancelNetworkTask(args.taskId);
      return undefined;
    }
    case "export_network_report": {
      const args = rawArgs as IpcCommandMap["export_network_report"]["args"];
      return `C:\\Users\\User\\Documents\\Harness\\网络报告-模拟.${args.format === "json" ? "json" : "md"}`;
    }
    default: {
      const exhaustive: never = command;
      throw new Error(`未支持的命令：${String(exhaustive)}`);
    }
  }
}
