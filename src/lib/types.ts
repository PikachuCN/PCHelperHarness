import type { NetworkDiagnosticReport, NetworkProgressEvent, NetworkTestRequest, NetworkTestResult } from "./networkTypes";

export type ID = string;
export type RiskLevel = "l0" | "l1" | "l2" | "l3";
export type ApprovalMode = "manual" | "auto";
export type ProviderProtocol = "chat_completions" | "responses";
export type ThemeMode = "system" | "light" | "dark";
export type TurnStatus = "done" | "cancelled" | "error" | "max_steps";

export type InteractionFieldKind = "text" | "textarea" | "number" | "select" | "multiSelect" | "toggle" | "file" | "files" | "folder" | "saveFile";
export type InteractionValue = string | number | boolean | string[] | null;

export interface InteractionOption {
  value: string;
  label: string;
  description?: string;
}

export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface InteractionField {
  key: string;
  label: string;
  description?: string;
  kind: InteractionFieldKind;
  required?: boolean;
  placeholder?: string;
  defaultValue?: InteractionValue;
  options?: InteractionOption[];
  filters?: FileFilter[];
}

export interface PendingInteraction {
  id: ID;
  sessionId: ID;
  turnId: ID;
  callId: ID;
  title: string;
  description: string;
  fields: InteractionField[];
}

export interface CustomTool {
  id: ID;
  name: string;
  description: string;
  category: string;
  promptTemplate: string;
  rules?: string;
  fields: InteractionField[];
  createdAt: number;
  updatedAt: number;
}

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

export type PromptMessage =
  | { role: "system" | "user"; content: string }
  | { role: "assistant"; text: string; reasoning?: string; tool_calls?: { id: string; name: string; arguments: string }[] }
  | { role: "tool"; call_id: string; content: string };

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
  autoApprovePythonPipInstall: boolean;
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

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  shell: string;
  path: string;
  status: "ready" | "error";
  error: string | null;
}

export interface PythonRuntimeInfo {
  available: boolean;
  source: "plugin" | "portable" | "system" | null;
  program: string | null;
  error: string | null;
}

export interface PluginCatalog {
  plugins: PluginInfo[];
  python: PythonRuntimeInfo;
  directory: string;
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

export type SystemOverviewSection = "basic" | "activation" | "hardware" | "printers" | "drivers" | "software" | "storage" | "network" | "security";
export type SystemSettingsTarget = "activation" | "windowsUpdate" | "deviceManager" | "installedApps" | "network" | "security";

export interface SystemBasicInfo {
  osName: string | null;
  displayVersion: string | null;
  version: string | null;
  build: string | null;
  architecture: string | null;
  deviceName: string | null;
  userName: string | null;
  manufacturer: string | null;
  model: string | null;
  domain: string | null;
  installDate: string | null;
  lastBootTime: string | null;
  uptimeSeconds: number;
  timeZone: string | null;
  locale: string | null;
  powerShellVersion: string | null;
  webView2Version: string | null;
}

export type LicenseStatus = "Licensed" | "Notification" | "Unlicensed" | "Grace" | "OutOfGrace" | "Unknown" | string;

export interface WindowsLicenseInfo {
  name: string | null;
  version: string | null;
  status: LicenseStatus | null;
  statusCode: number | null;
  licenseChannel: string | null;
  licenseDescription: string | null;
  expiresAt: string | null;
  isDigitalLicense: boolean;
}

export interface OfficeLicenseInfo {
  name: string | null;
  status: LicenseStatus | null;
  statusCode: number | null;
  licenseChannel: string | null;
  licenseDescription: string | null;
  expiresAt: string | null;
}

export interface ActivationInfo {
  windows: WindowsLicenseInfo | null;
  office: OfficeLicenseInfo[];
  warnings: string[];
}

export interface HardwareInfo {
  processors: { name: string | null; manufacturer: string | null; cores: number | null; logicalProcessors: number | null; maxClockMHz: number | null }[];
  motherboard: { manufacturer: string | null; product: string | null; version: string | null; serialNumber: string | null };
  bios: { manufacturer: string | null; version: string | null; releaseDate: string | null; serialNumber: string | null };
  memoryTotalBytes: number;
  memoryUsedBytes: number | null;
  memoryModules: { capacityBytes: number | null; speedMHz: number | null; manufacturer: string | null; partNumber: string | null; bankLabel: string | null; deviceLocator: string | null; serialNumber: string | null }[];
  graphics: { name: string | null; adapterRamBytes: number | null; driverVersion: string | null; videoMode: string | null; resolution: string | null }[];
  audio: { name: string | null; manufacturer: string | null; status: string | null }[];
  batteries: { name: string | null; chargePercent: number | null; status: string | null; designVoltageMv: number | null }[];
  warnings: string[];
}

export interface PrinterInfo {
  name: string;
  printerType: string | null;
  isLocal: boolean;
  isDefault: boolean;
  driverName: string | null;
  portName: string | null;
  status: string | null;
  jobCount: number;
  shared: boolean;
  shareName: string | null;
  published: boolean;
  workOffline: boolean;
  location: string | null;
  comment: string | null;
}

export interface PrinterListInfo {
  printers: PrinterInfo[];
  defaultPrinterName: string | null;
  warnings: string[];
}

export interface PrinterShareResponse {
  shared: boolean;
  shareName: string | null;
}

export interface DriverDeviceInfo {
  deviceId: string | null;
  deviceName: string | null;
  className: string | null;
  manufacturer: string | null;
  provider: string | null;
  version: string | null;
  driverDate: string | null;
  infName?: string | null;
  isSigned?: boolean | null;
  status: string | null;
  problemCode: number;
}

export interface DriverUpdateInfo {
  id: string | null;
  title: string | null;
  manufacturer: string | null;
  model: string | null;
  className: string | null;
  driverDate: string | null;
  deviceId: string | null;
  hardwareId: string | null;
  isDownloaded: boolean | null;
  isMandatory: boolean | null;
  isOptional: boolean | null;
  rebootRequired: boolean | null;
  kbArticles: string[];
}

export interface DriverUpdateScanResult {
  scannedAt: string;
  updates: DriverUpdateInfo[];
  warnings: string[];
}

export interface DriverInfo {
  summary: {
    totalDevices: number;
    driverCount: number;
    problemCount: number;
    missingDriverCount: number;
    unsignedDriverCount: number;
  };
  problems: DriverDeviceInfo[];
  drivers: DriverDeviceInfo[];
  updateScan: DriverUpdateScanResult | null;
  warnings: string[];
}

export interface CommonSoftwareInfo {
  id: string;
  name: string;
  category: string;
  installed: boolean;
  version: string | null;
  publisher: string | null;
}

export interface InstalledSoftwareInfo {
  id: string;
  name: string;
  version: string | null;
  publisher: string | null;
  installDate: string | null;
}

export interface SoftwareInfo {
  common: CommonSoftwareInfo[];
  installed: InstalledSoftwareInfo[];
  warnings: string[];
}

export interface StorageInfo {
  volumes: { driveLetter: string; name: string | null; fileSystem: string | null; sizeBytes: number | null; freeBytes: number | null; driveType: number; healthStatus: string | null }[];
  physicalDisks: { model: string | null; mediaType: string | null; busType: string | null; sizeBytes: number | null; healthStatus: string | null; operationalStatus: string | null; serialNumber: string | null; firmwareVersion: string | null }[];
  warnings: string[];
}

export interface NetworkInfo {
  isConnected: boolean;
  profiles: { name: string | null; interfaceAlias: string | null; networkCategory: string | null; ipv4Connectivity: string | null }[];
  adapters: { name: string | null; description: string | null; status: string | null; linkSpeed: string | null; macAddress: string | null; ipAddresses: string[]; gateways: string[]; dnsServers: string[] }[];
  warnings: string[];
}

export interface SecurityInfo {
  defender: { serviceEnabled: boolean; antivirusEnabled: boolean; realTimeProtectionEnabled: boolean; tamperProtected: boolean; signatureLastUpdated: string | null } | null;
  firewallProfiles: { name: string | null; enabled: boolean }[];
  uacEnabled: boolean | null;
  tpm: { present: boolean; ready: boolean; enabled: boolean; activated: boolean; specVersion: string | null } | null;
  secureBoot: { supported: boolean; enabled: boolean | null } | null;
  lastUpdate: { hotfixId: string | null; installedOn: string | null; description: string | null } | null;
  pendingReboot: boolean | null;
  warnings: string[];
}

export type SystemOverviewData = SystemBasicInfo | ActivationInfo | HardwareInfo | PrinterListInfo | DriverInfo | SoftwareInfo | StorageInfo | NetworkInfo | SecurityInfo;

export type SystemOverviewSectionResponse =
  | { section: "basic"; data: SystemBasicInfo }
  | { section: "activation"; data: ActivationInfo }
  | { section: "hardware"; data: HardwareInfo }
  | { section: "printers"; data: PrinterListInfo }
  | { section: "drivers"; data: DriverInfo }
  | { section: "software"; data: SoftwareInfo }
  | { section: "storage"; data: StorageInfo }
  | { section: "network"; data: NetworkInfo }
  | { section: "security"; data: SecurityInfo };

export interface SystemOverviewSectionState<T = SystemOverviewData> {
  status: "idle" | "loading" | "success" | "error";
  data?: T;
  error?: string;
  loadedAt?: string;
}

export type SystemOverviewSnapshot = Partial<Record<SystemOverviewSection, SystemOverviewSectionState>>;

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

export type TurnPhase = "preparing" | "connecting" | "reasoning" | "responding" | "approval" | "interaction" | "tool" | "organizing" | "stopping" | "done" | "cancelled" | "error" | "max_steps";

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
  network_progress: NetworkProgressEvent;
  interaction_request: PendingInteraction;
  interaction_resolved: { interactionId: ID; sessionId: ID; callId: ID; cancelled: boolean };
  custom_tools_updated: CustomTool[];
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
  get_message_prompt: { args: { sessionId: ID; messageId: ID }; result: PromptMessage[] };
  rename_session: { args: { sessionId: ID; title: string }; result: void };
  delete_session: { args: { sessionId: ID }; result: void };
  export_session: { args: { sessionId: ID }; result: string };
  get_config: { args: Record<string, never>; result: AppConfig };
  save_config: { args: { config: AppConfig }; result: void };
  fetch_models: { args: { provider: ProviderConfig }; result: string[] };
  test_provider: { args: { provider: ProviderConfig }; result: ModelTestResult };
  list_skills: { args: Record<string, never>; result: SkillInfo[] };
  reload_skills: { args: Record<string, never>; result: SkillInfo[] };
  get_plugin_catalog: { args: Record<string, never>; result: PluginCatalog };
  open_plugin_dir: { args: Record<string, never>; result: void };
  open_skills_dir: { args: Record<string, never>; result: void };
  open_config_dir: { args: Record<string, never>; result: void };
  get_env_info: { args: Record<string, never>; result: EnvInfo };
  get_system_overview_section: { args: { section: SystemOverviewSection }; result: SystemOverviewSectionResponse };
  scan_driver_updates: { args: Record<string, never>; result: DriverUpdateScanResult };
  set_printer_share: { args: { name: string; shareName?: string; shared: boolean }; result: PrinterShareResponse };
  open_system_settings: { args: { target: SystemSettingsTarget }; result: void };
  export_system_overview_report: { args: { content: string }; result: string };
  run_network_test: { args: { taskId: string; request: NetworkTestRequest }; result: NetworkTestResult };
  run_network_diagnostic: { args: { taskId: string }; result: NetworkDiagnosticReport };
  cancel_network_task: { args: { taskId: string }; result: void };
  export_network_report: { args: { content: string; format: "markdown" | "json" }; result: string };
  restart_as_admin: { args: { sessionId: ID | null }; result: void };
  resolve_interaction: { args: { interactionId: ID; values: Record<string, unknown>; cancelled: boolean }; result: void };
  pick_interaction_paths: { args: { kind: InteractionFieldKind; title?: string; filters?: FileFilter[]; defaultName?: string }; result: string[] };
  list_custom_tools: { args: Record<string, never>; result: CustomTool[] };
  delete_custom_tool: { args: { toolId: ID }; result: CustomTool[] };
}

export type IpcCommand = keyof IpcCommandMap;
