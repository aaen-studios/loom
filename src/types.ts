export type Theme = "light" | "dark";

export type BackgroundKind = "builtin" | "image" | "video";

export interface BackgroundConfig {
  kind: BackgroundKind;
  preset: string;
  path: string | null;
  /** 0..=100 black overlay strength */
  dim: number;
  /** 0..=64 px blur on the background layer */
  blur: number;
}

export type ProviderKind = "openai-compatible" | "anthropic";
export type ModelsSource = "manual" | "fetched";
export type Modality = "text" | "image" | "audio" | "video" | "pdf";
export type PermissionMode = "ask" | "auto-read-only" | "auto-all";

export interface ReasoningSpec {
  enabled: boolean;
  variants: string[];
  defaultVariant: string | null;
}

export interface ModelSpec {
  name: string | null;
  context: number | null;
  output: number | null;
  inputModalities: Modality[];
  reasoning: ReasoningSpec | null;
  favorite: boolean;
}

export interface ProviderConfig {
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  headers: Record<string, string>;
  enabled: boolean;
  models: Record<string, ModelSpec>;
  modelsSource: ModelsSource;
  lastFetchedAt: number | null;
  keyRequired: boolean;
  /** Header the gateway wants filled with a stable per-chat id. */
  sessionHeader: string | null;
}

export interface ProviderPreset {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  keyRequired: boolean;
  note: string;
  sessionHeader: string | null;
}

export interface ModelEntry {
  providerId: string;
  providerName: string;
  kind: ProviderKind;
  enabled: boolean;
  keyReady: boolean;
  keyRequired: boolean;
  modelId: string;
  spec: ModelSpec;
}

export interface ModelRef {
  providerId: string;
  modelId: string;
}

export interface ChatDefaults {
  providerId: string | null;
  modelId: string | null;
  variant: string | null;
  lite: ModelRef | null;
  imageModel: string | null;
  embeddingModel: string | null;
  autoTitle: boolean;
  permissionMode: PermissionMode;
  historyLimit: number;
  maxOutputTokens: number;
}

export interface Persona {
  id: string;
  name: string;
  systemPrompt: string;
  modelRef: ModelRef | null;
  variant: string | null;
}

export interface McpServerConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
}

export type ThinkingDisplay = "collapsed" | "hidden" | "expanded";
export type SendKey = "enter" | "ctrl-enter";

export interface InterfaceConfig {
  showThinking: ThinkingDisplay;
  sendKey: SendKey;
  notifyOnCompletion: boolean;
  hotkeyEnabled: boolean;
  hotkey: string;
  alwaysFollow: boolean;
  compact: boolean;
}

export interface AppConfig {
  schemaVersion: number;
  theme: Theme;
  background: BackgroundConfig;
  sidebarCollapsed: boolean;
  providers: Record<string, ProviderConfig>;
  personas: Persona[];
  mcpServers: Record<string, McpServerConfig>;
  chat: ChatDefaults;
  interface: InterfaceConfig;
}

export interface AppInfo {
  name: string;
  version: string;
  loomHome: string;
}

export interface Session {
  id: string;
  title: string;
  providerId: string | null;
  modelId: string | null;
  variant: string | null;
  personaId: string | null;
  systemPrompt: string | null;
  workdir: string | null;
  permissionMode: PermissionMode | null;
  createdAt: number;
  updatedAt: number;
}

export type ToolCallStatus = "running" | "ok" | "error" | "denied";

export interface ToolCallRecord {
  id: string;
  name: string;
  arguments: string;
  status: ToolCallStatus;
  output: string;
}

export interface WorkspaceInfo {
  workdir: string | null;
  branch: string | null;
  isRepo: boolean;
}

export interface PendingPermission {
  sessionId: string;
  messageId: string;
  callId: string;
  name: string;
  arguments: string;
  readOnly: boolean;
}

export interface Message {
  id: string;
  sessionId: string;
  role: "user" | "assistant";
  content: string;
  reasoning: string | null;
  extra: string | null;
  createdAt: number;
}

export interface Usage {
  inputTokens: number | null;
  outputTokens: number | null;
}

export interface UpdateManifest {
  version: string;
  notes: string;
  payload: string;
  sha256: string;
  signature?: string | null;
}

export type AttachmentKind = "image" | "text" | "pdf";

export interface Attachment {
  id: string;
  kind: AttachmentKind;
  name: string;
  mime: string;
  size: number;
  path: string;
  text?: string | null;
}

export type EngineEvent =
  | { type: "started"; sessionId: string; messageId: string }
  | { type: "delta"; sessionId: string; messageId: string; text: string }
  | { type: "reasoning"; sessionId: string; messageId: string; text: string }
  | {
      type: "toolCallStarted";
      sessionId: string;
      messageId: string;
      callId: string;
      name: string;
      arguments: string;
    }
  | {
      type: "toolCallFinished";
      sessionId: string;
      messageId: string;
      callId: string;
      ok: boolean;
      output: string;
    }
  | {
      type: "toolPermissionRequest";
      sessionId: string;
      messageId: string;
      callId: string;
      name: string;
      arguments: string;
      readOnly: boolean;
    }
  | {
      type: "done";
      sessionId: string;
      messageId: string;
      content: string;
      reasoning: string | null;
      usage: Usage;
    }
  | { type: "error"; sessionId: string; messageId: string; error: string }
  | { type: "title"; sessionId: string; title: string };
