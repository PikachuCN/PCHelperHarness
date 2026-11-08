export type ID = string;
export type RiskLevel = "l0" | "l1" | "l2" | "l3";
export type ApprovalMode = "manual" | "auto";
export type ProviderProtocol = "chat_completions" | "responses";
export type ThemeMode = "system" | "light" | "dark";
export type TurnStatus = "done" | "cancelled" | "error" | "max_steps";

export interface SessionSummary {
  id: ID;
  title: string;
  createdAt: string;
  updatedAt: string;
  messageCount?: number;
}

export interface RiskAssessment {
  level: RiskLevel;
  summaryZh: string;
  reasons: string[];
  requiresTypedConfirmation: boolean;
  matchedRules: string[];
}

export type ToolStatus = "pending" | "running" | "success" | "error" | "rejected" | "cancelled";

export interface ToolRun {
  callId: ID;
  name: string;
  arguments: Record<string, unknown>;
  explanationZh: string;
  risk: RiskAssessment;
  status: ToolStatus;
  output: string;
  durationMs: number;
  exitCode?: number | null;
}

export interface ChatMessage {
  id: ID;
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  createdAt: string;
  tools: ToolRun[];
}

export interface PendingApproval {
  id: ID;
  sessionId: ID;
  turnId: ID;
  callId: ID;
  toolName: string;
  shell: string;
  command: string;
  arguments: Record<string, unknown>;
  explanationZh: string;
  risk: RiskAssessment;
  level: RiskLevel;
  mode: string;
  requiresTypedConfirmation: boolean;
  messageId?: ID;
  confirmationReadyAt?: number;
}

export interface ApiKeyConfig {
  encrypted: boolean;
  value: string;
}

export interface ProviderConfig {
  id: ID;
  name: string;
  baseUrl: string;
  apiKey: ApiKeyConfig;
  apiMode: ProviderProtocol;
  model: string;
  models: string[];
  temperature: number | null;
}

export interface ApprovalConfig {
  mode: ApprovalMode;
  readonlyNeedsApproval: boolean;
}

export interface ExecutionConfig {
  defaultTimeoutSec: number;
  maxTimeoutSec: number;
  outputLimitKb: number;
  maxSteps: number;
}

export interface AppearanceConfig {
  theme: ThemeMode;
  fontSize: number;
}

export interface SecurityConfig {
  encryptApiKey: boolean;
}

export interface AppConfig {
  version: number;
  activeProviderId: ID;
  providers: ProviderConfig[];
  approval: ApprovalConfig;
  execution: ExecutionConfig;
  appearance: AppearanceConfig;
  security: SecurityConfig;
}

export interface SkillInfo {
  name: string;
  description: string;
  source: "内置" | "用户";
  path: string;
}

export interface EnvInfo {
  isAdmin: boolean;
  osVersion: string;
  computerName: string;
  userName: string;
  configPath: string;
  configUsedFallback: boolean;
  webview2Version: string;
  resumeSessionId: string | null;
  availableShells: string[];
}

export interface RunOutcome {
  sessionId: ID;
  turnId: ID;
  status: TurnStatus;
}

export interface ApprovalResolution {
  status: "approved" | "rejected" | "confirmation_required";
  readyAt?: number | null;
  requiresTypedConfirmation: boolean;
}

export interface ModelTestResult {
  connected: boolean;
  latencyMs: number;
  models: { id: string; name?: string; ownedBy?: string }[];
  supportsTools: boolean;
}

export interface SessionEvent {
  sessionId: ID;
  turnId: ID;
  message: ChatMessage;
}

export interface AssistantDeltaEvent {
  sessionId: ID;
  turnId: ID;
  messageId: ID;
  delta: string;
}

export interface ToolEvent {
  sessionId: ID;
  turnId: ID;
  messageId: ID;
  tool: ToolRun;
}

export interface ToolOutputEvent {
  sessionId: ID;
  turnId: ID;
  callId: ID;
  stream: "stdout" | "stderr";
  text: string;
}

export interface TurnDoneEvent {
  sessionId: ID;
  turnId: ID;
  status: TurnStatus;
  error?: string;
}

export type TurnPhase = "preparing" | "connecting" | "reasoning" | "responding" | "approval" | "tool" | "organizing" | "stopping" | "done" | "cancelled" | "error" | "max_steps";

export interface TurnActivityEvent {
  id: string;
  label: string;
  detail: string;
  at: number;
}

export interface TurnActivity {
  sessionId: ID | null;
  active: boolean;
  phase: TurnPhase;
  label: string;
  detail: string;
  startedAt: number;
  updatedAt: number;
  step: number;
  maxSteps: number;
  receivedChars: number;
  events: TurnActivityEvent[];
}

export interface ApprovalConfirmationEvent {
  approvalId: ID;
  sessionId: ID;
  callId: ID;
  readyAt: number;
  requiresTypedConfirmation: boolean;
}

export interface ApprovalResolvedEvent {
  approvalId: ID;
  sessionId: ID;
  callId: ID;
  allowed: boolean;
}

export interface ConfigPathFallbackEvent {
  path: string;
  message: string;
}

export interface ConfigWarningEvent {
  message: string;
  providerIds: string[];
}

export interface HarnessEventMap {
  user_message: SessionEvent;
  session_updated: SessionSummary;
  assistant_started: SessionEvent;
  assistant_sync: SessionEvent;
  assistant_delta: AssistantDeltaEvent;
  reasoning_delta: AssistantDeltaEvent;
  approval_request: PendingApproval;
  approval_confirmation_required: ApprovalConfirmationEvent;
  approval_resolved: ApprovalResolvedEvent;
  tool_started: ToolEvent;
  tool_output: ToolOutputEvent;
  tool_done: ToolEvent;
  turn_done: TurnDoneEvent;
  config_path_fallback: ConfigPathFallbackEvent;
  config_warning: ConfigWarningEvent;
}

export type HarnessEventName = keyof HarnessEventMap;

export interface IpcCommandMap {
  create_session: { args: Record<string, never>; result: SessionSummary };
  send_message: {
    args: { sessionId: ID | null; content: string; model: string; approvalMode: ApprovalMode };
    result: RunOutcome;
  };
  cancel_turn: { args: { sessionId: ID }; result: void };
  resolve_approval: {
    args: { approvalId: ID; approved: boolean; reason?: string; confirmationText?: string };
    result: ApprovalResolution;
  };
  list_sessions: { args: Record<string, never>; result: SessionSummary[] };
  load_session: { args: { sessionId: ID }; result: ChatMessage[] };
  rename_session: { args: { sessionId: ID; title: string }; result: void };
  delete_session: { args: { sessionId: ID }; result: void };
  export_session: { args: { sessionId: ID }; result: string };
  get_config: { args: Record<string, never>; result: AppConfig };
  save_config: { args: { config: AppConfig }; result: void };
  fetch_models: { args: { provider: ProviderConfig }; result: string[] };
  test_provider: { args: { provider: ProviderConfig }; result: ModelTestResult };
  list_skills: { args: Record<string, never>; result: SkillInfo[] };
  reload_skills: { args: Record<string, never>; result: SkillInfo[] };
  open_skills_dir: { args: Record<string, never>; result: void };
  open_config_dir: { args: Record<string, never>; result: void };
  get_env_info: { args: Record<string, never>; result: EnvInfo };
  restart_as_admin: { args: { sessionId: ID | null }; result: void };
}

export type IpcCommand = keyof IpcCommandMap;
