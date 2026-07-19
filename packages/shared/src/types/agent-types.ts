export type AgentCapability =
  | "sessions.list"
  | "sessions.read"
  | "sessions.stats"
  | "sessions.resume"
  | "sessions.duplicate"
  | "sessions.branch"
  | "sessions.delete"
  | "sessions.rename"
  | "sessions.export-native"
  | "sessions.import"
  | "skills.global"
  | "skills.project"
  | "skills.copy"
  | "skills.symlink"
  | "config.read"
  | "config.write"
  | "management.read"
  | "management.write";

export type AgentConfigLanguage =
  | "json"
  | "jsonc"
  | "toml"
  | "yaml"
  | "markdown"
  | "text";

export interface AgentConfigFileDefinition {
  id: string;
  label: string;
  pathTemplate: string;
  language: AgentConfigLanguage;
  kind?: "config" | "prompt";
  defaultContent?: string | null;
}

export interface AgentConfigFileSummary {
  id: string;
  label: string;
  path: string;
  language: AgentConfigLanguage;
  kind?: "config" | "prompt";
  exists: boolean;
  modifiedAt?: number | null;
}

export interface AgentConfigDocument extends AgentConfigFileSummary {
  content: string;
  revision: string;
}

export type AgentManagementSectionRenderer =
  | "overview"
  | "form"
  | "mcp"
  | "skills"
  | "prompts"
  | "providers"
  | "models"
  | "permissions"
  | "environment"
  | "raw-config";

/**
 * Opaque plugin-owned identifier. The host must never enumerate section ids.
 * `renderer` selects one of the UI's stable, reusable presentation contracts.
 */
export type AgentManagementSectionId = string;

export interface AgentManagementSectionDescriptor {
  id: AgentManagementSectionId;
  renderer: AgentManagementSectionRenderer | string;
  source: "plugin" | "host";
  label?: ManagedLocalizedText;
  description?: ManagedLocalizedText;
  readOnly: boolean;
  count?: number;
  features?: string[];
}

export interface AgentManagementDescriptor {
  schemaVersion: 1;
  agentId: string;
  instanceId: string;
  sections: AgentManagementSectionDescriptor[];
}

export interface ManagedSecretState {
  configured: boolean;
  source?: "inline" | "environment" | "system" | "unknown";
  masked?: string;
  environmentVariable?: string;
}

export interface ManagedMcpServer {
  id: string;
  name: string;
  transport: "stdio" | "http" | "sse";
  command?: string;
  args?: string[];
  url?: string;
  env?: Record<string, string>;
  headers?: Record<string, string>;
  enabled: boolean;
  scope?: "global" | "project";
}

export interface ManagedMcpSettings {
  servers: ManagedMcpServer[];
  canDisable?: boolean;
}

export interface ManagedApiProvider {
  id: string;
  name: string;
  protocol: "openai" | "anthropic" | "gemini" | "custom";
  baseUrl?: string;
  apiKey?: ManagedSecretState;
  defaultModel?: string;
  models?: string[];
  enabled: boolean;
}

export interface ManagedApiProviderSettings {
  providers: ManagedApiProvider[];
  canEditModels?: boolean;
  secretInput?: {
    mode: "value" | "environment-variable";
    defaultEnvironmentVariable?: string;
  };
}

export interface ManagedModelSettings {
  defaultModel?: string;
  smallModel?: string;
  reasoningModel?: string;
  reasoningEffort?: string;
  availableModels?: string[];
  aliases?: Record<string, string>;
}

export interface ManagedPermissionRule {
  id: string;
  decision: "allow" | "ask" | "deny";
  target: string;
  kind?: "tool" | "command" | "path" | "project" | "other";
}

export interface ManagedPermissionSettings {
  approvalMode?: string;
  sandboxMode?: string;
  rules: ManagedPermissionRule[];
}

export type ManagedLocalizedText = string | Record<string, string>;

export interface ManagedFormOption {
  value: string;
  label?: ManagedLocalizedText;
}

export interface ManagedFormField {
  key: string;
  control: "text" | "password" | "select" | "switch" | "textarea";
  label: ManagedLocalizedText;
  description?: ManagedLocalizedText;
  placeholder?: ManagedLocalizedText;
  options?: ManagedFormOption[];
  defaultValue?: unknown;
  valueType?: "string" | "string-array";
  required?: boolean;
  mono?: boolean;
  rows?: number;
}

export interface ManagedFormGroup {
  id: string;
  title?: ManagedLocalizedText;
  description?: ManagedLocalizedText;
  columns?: 1 | 2;
  fields: ManagedFormField[];
}

export interface ManagedFormSettings {
  schemaVersion: 1;
  groups: ManagedFormGroup[];
  values: Record<string, unknown>;
}

export interface ManagedEnvironmentVariable {
  name: string;
  value?: string;
  secret: boolean;
  source?: string;
}

export interface ManagedEnvironmentSettings {
  configFiles?: Array<{
    id: string;
    label: string;
    path: string;
    exists: boolean;
  }>;
  variables?: ManagedEnvironmentVariable[];
  cliRoot?: string | null;
  sessionRoot?: string | null;
  skillRoot?: string | null;
}

export interface AgentManagementOverviewData {
  cliName: string;
  cliVersion?: string | null;
  configRoot?: string | null;
  sessionRoot?: string | null;
  skillRoot?: string | null;
  defaultModel?: string;
  defaultProvider?: string;
  mcpServerCount?: number;
  providerCount?: number;
  skillTargetCount?: number;
  warnings?: string[];
}

export interface AgentManagementSection<T = unknown> {
  id: AgentManagementSectionId;
  revision: string;
  data: T;
  warnings?: string[];
}

export interface AgentManagementMutation {
  section: AgentManagementSectionId;
  action: string;
  entityId?: string;
  expectedRevision: string;
  payload?: unknown;
}

export interface AgentManagementMutationResult {
  section: AgentManagementSectionId;
  revision: string;
  changed: boolean;
  changedResources: string[];
  restartRequired?: boolean;
  warnings?: string[];
}

export interface AgentPluginDescriptor {
  id: string;
  name: string;
  version: string;
  description?: string;
  icon?: string;
  enabled: boolean;
  capabilities: AgentCapability[];
  instanceConfig: AgentInstanceConfig;
  configFiles: AgentConfigFileDefinition[];
  instances: AgentInstance[];
  sessionRoots: string[];
  skillTargets: AgentSkillTarget[];
  error?: string | null;
}

export interface AgentInstanceConfig {
  sessionPathKind: "directory" | "file";
  homeLevelsUp: number;
  sessionPathTemplate?: string | null;
  wslSessionPathTemplate?: string | null;
  skillPathTemplate?: string | null;
  command?: string | null;
  resumeArguments: string[];
  pathHints?: string[];
}

export interface AgentInstance {
  id: string;
  agentId: string;
  label: string;
  cliRoot?: string | null;
  sessionRoot?: string | null;
  skillRoot?: string | null;
  resumeCommand?: string | null;
  enabled: boolean;
}

export interface AgentInstanceEntry {
  plugin: AgentPluginDescriptor;
  instance: AgentInstance;
}

export interface AgentInstanceInput {
  agentId: string;
  configRoot: string;
}

export interface AgentSkillTarget {
  id: string;
  agentId: string;
  label: string;
  scope: "global" | "project";
  pathTemplate: string;
  resolvedPath?: string | null;
  mode: "copy" | "symlink" | "native";
  format: "agents-skill" | "prompt-rules" | "commands";
  projectPathRequired?: boolean;
}

export type SessionMessageKind =
  | "text"
  | "reasoning"
  | "tool-call"
  | "tool-result"
  | "error"
  | "system";

export interface AgentSessionAttachment {
  id: string;
  kind: "image" | "file";
  name?: string | null;
  mimeType?: string | null;
  size?: number | null;
  reference?: string | null;
}

export interface AgentSessionAttachmentData {
  id: string;
  name?: string | null;
  mimeType?: string | null;
  dataUrl: string;
}

export interface AgentSessionMessage {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  kind: SessionMessageKind;
  text?: string | null;
  toolName?: string | null;
  toolInput?: unknown;
  toolOutput?: unknown;
  toolCallId?: string | null;
  model?: string | null;
  timestamp?: number | null;
  rawType?: string | null;
  attachments?: AgentSessionAttachment[];
}

export interface UserMessageNavItem {
  messageId: string;
  originalIndex: number;
  text: string;
  timestamp?: number | null;
}

export interface VisibleAgentSessionMessage {
  item: AgentSessionMessage;
  originalIndex: number;
}

export interface AgentSessionMessageGroup {
  key: string;
  role: "user" | "assistant" | "system";
  messages: VisibleAgentSessionMessage[];
  continuation: boolean;
}

export interface AgentSessionSummary {
  id: string;
  agentId: string;
  nativeId: string;
  nativeSessionId?: string | null;
  sourceInstanceId?: string | null;
  sourceLabel?: string | null;
  title: string;
  cwd?: string | null;
  repository?: string | null;
  model?: string | null;
  createdAt?: number | null;
  updatedAt?: number | null;
  messageCount: number;
  sourceRef: string;
  parentNativeId?: string | null;
  active?: boolean;
}

export interface AgentSessionGroup {
  key: string;
  plugin: AgentPluginDescriptor;
  groups: Array<{
    key: string;
    label: string;
    path: string | null;
    sessions: AgentSessionSummary[];
  }>;
}

export interface AgentImportTarget {
  value: string;
  agentId: string;
  instanceId?: string;
  label: string;
}

export interface AgentSession extends AgentSessionSummary {
  messages: AgentSessionMessage[];
  messageCursor?: number | null;
  hasMoreMessages?: boolean;
  rawMetadata?: Record<string, unknown>;
}

export interface SessionStats {
  inputTokens?: number;
  outputTokens?: number;
  cachedInputTokens?: number;
  cacheWriteTokens?: number;
  reasoningTokens?: number;
  totalTokens?: number;
  cost?: number;
  contextWindow?: number;
  source: "reported";
}

export interface SessionMessagePageOptions {
  before?: number;
  limit?: number;
}

export interface SessionListOptions {
  agentId?: string;
  query?: string;
  cwd?: string;
  limit?: number;
  refresh?: boolean;
}

export interface SessionOperationResult {
  ok: boolean;
  agentId: string;
  nativeId?: string | null;
  command?: string | null;
  sourceNativeId?: string | null;
  warnings?: string[];
  backupPath?: string | null;
}

export interface AgentSessionStoreState {
  plugins: AgentPluginDescriptor[];
  query: string;
  sessions: AgentSessionSummary[];
  loadedAgentIds: string[];
  loadingAgentIds: string[];
  selectedKey: string | null;
  selected: AgentSession | null;
  isLoading: boolean;
  isLoadingSession: boolean;
  error: string | null;
  loadPlugins: () => Promise<void>;
  loadSessions: () => Promise<void>;
  loadAgentSessions: (agentId: string, refresh?: boolean) => Promise<void>;
  refreshSessions: () => void;
  selectSession: (summary: AgentSessionSummary) => Promise<void>;
  loadOlderMessages: () => Promise<boolean>;
  loadMessagesThrough: (messageId: string) => Promise<boolean>;
  deleteSession: (
    summary: AgentSessionSummary,
  ) => Promise<SessionOperationResult>;
  renameSession: (summary: AgentSessionSummary, title: string) => Promise<void>;
  setQuery: (value: string) => void;
  setSelected: (session: AgentSession | null) => void;
  clearSelected: () => void;
  clearError: () => void;
  clearStore: () => void;
}

export type SessionExportFormat = "html" | "markdown" | "json" | "native";

export interface SessionExportOptions {
  format: SessionExportFormat;
  includeReasoning?: boolean;
  includeToolCalls?: boolean;
  includeToolResults?: boolean;
  sanitize?: boolean;
  fromMessage?: number;
  toMessage?: number;
}

export interface SessionExportResult {
  fileName: string;
  mimeType: string;
  content: string;
  encoding: "utf8" | "base64";
}

export interface SessionExportSaveResult {
  saved: boolean;
  fileName: string;
  path?: string;
}

export interface SessionImportOptions {
  targetAgentId: string;
  targetInstanceId?: string;
  title?: string;
  cwd?: string;
  openAfterImport?: boolean;
}

export interface SkillInstallation {
  id: string;
  skillId: string;
  agentId: string;
  targetId: string;
  scope: "global" | "project";
  projectPath?: string | null;
  mode: "copy" | "symlink" | "native";
  status:
    | "synced"
    | "disabled"
    | "conflict"
    | "missing-agent"
    | "unsupported"
    | "error";
  installedPath?: string | null;
  error?: string | null;
  updatedAt: number;
}
