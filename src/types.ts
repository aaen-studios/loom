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
export type PermissionMode = "ask" | "auto-read-only" | "auto-all" | "atelier";
/** Plan and Review refuse the write and command tools; Build lets the permission gate decide. */
export type AgentMode = "plan" | "build" | "review";
/** What a tool can reach; shown as a badge on the permission card. */
export type ToolScope = "workspace" | "harness" | "web" | "mcp" | "computer";

/**
 * Where a model's metadata came from. A refresh may correct anything below
 * `user`; a hand edit is never overwritten.
 */
export type MetadataSource = "user" | "api" | "unknown" | `catalog@${number}`;

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
  source: MetadataSource;
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
  recentModels: ModelRef[];
  permissionMode: PermissionMode;
  agentMode: AgentMode;
  maxOutputTokens: number;
  /** Tool round-trips one user turn may take (1-200). */
  maxToolRounds: number;
  /** Thinking effort on computer turns: "off", "low", or null to inherit. */
  computerVariant: string | null;
  /** Fast vision model used only while the Computer chip is armed. */
  computerModel: ModelRef | null;
  /** Longest screenshot edge; 0 is native resolution. */
  computerScreenshotEdge: number;
}

/** One `user`/`assistant` example turn attached to a persona. */
export interface PersonaExample {
  user: string;
  assistant: string;
}

/** Runtime defaults a persona applies when the session has not overridden them. */
export interface PersonaCapabilities {
  temperature: number | null;
  topP: number | null;
  maxOutputTokens: number | null;
  permissionMode: PermissionMode | null;
  agentMode: AgentMode | null;
  /** Tool-name allowlist; empty means every tool the mode offers. */
  tools: string[];
  /** MCP server-id allowlist; empty means every connected server. */
  mcpServers: string[];
}

/** Persistent memory across every chat that uses the persona. */
export interface PersonaMemory {
  enabled: boolean;
  tokenBudget: number;
}

export interface Persona {
  id: string;
  name: string;
  systemPrompt: string;
  modelRef: ModelRef | null;
  variant: string | null;
  /** One-line summary for the picker. */
  description: string;
  tags: string[];
  /** Avatar emoji, e.g. "🛠". */
  emoji: string | null;
  /** Avatar colour token, e.g. "violet". */
  color: string | null;
  favorite: boolean;
  /** Opening assistant message for chats started with this persona. */
  greeting: string;
  /** Optional prompt sections appended after the main prompt. */
  style: string;
  rules: string;
  outputFormat: string;
  examples: PersonaExample[];
  capabilities: PersonaCapabilities;
  memory: PersonaMemory;
  /** Bumped on every write; chats compare their snapshot to flag staleness. */
  revision: number;
  updatedAt: number;
}

/** Who the user is, for `{{user}}` and friends in persona prompts. */
export interface UserProfile {
  name: string;
  pronouns: string;
  about: string;
}

/** A named set of personas; `cast` groups can start multi-persona chats. */
export interface PersonaGroup {
  id: string;
  name: string;
  members: string[];
  cast: boolean;
}

/** One persistent persona memory entry. */
export interface MemoryEntry {
  id: string;
  personaId: string;
  key: string;
  value: string;
  /** `user` or `model`. */
  source: string;
  createdAt: number;
  updatedAt: number;
}

/**
 * A long-term memory: a durable fact about the user (`scope: "global"`) or
 * about one workspace (its folder path).
 */
export interface StoredMemory {
  id: string;
  scope: string;
  content: string;
  pinned: boolean;
  sourceSession: string | null;
  sourceMessage: string | null;
  /** `user`, `model`, or `auto` for the extraction pass. */
  source: string;
  createdAt: number;
  updatedAt: number;
}

export type TaskStatus =
  | "queued"
  | "running"
  | "done"
  | "failed"
  | "interrupted"
  | "cancelled"
  | "skipped";

/** A detached run: a background subagent, or one firing of a job. */
export interface Task {
  id: string;
  /** The hidden session the run's transcript lives in. */
  sessionId: string;
  originSession: string | null;
  jobId: string | null;
  title: string;
  prompt: string;
  providerId: string | null;
  modelId: string | null;
  status: TaskStatus;
  detail: string | null;
  result: string | null;
  notify: boolean;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
}

/** A scheduled job: a prompt plus a five-field cron expression. */
export interface Job {
  id: string;
  name: string;
  cron: string;
  enabled: boolean;
  prompt: string;
  providerId: string | null;
  modelId: string | null;
  personaId: string | null;
  workdir: string | null;
  permissionMode: string | null;
  notifyOnSuccess: boolean;
  catchUpMinutes: number;
  lastRunAt: number | null;
  lastStatus: string | null;
  nextRunAt: number | null;
  createdAt: number;
  updatedAt: number;
}

export interface McpServerConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
}

export type ThinkingDisplay = "collapsed" | "hidden" | "expanded";
/** How much tool-call detail the transcript shows. */
export type ToolCallDisplay = "collapsed" | "expanded" | "hidden";
export type SendKey = "enter" | "ctrl-enter";
/** Whether the chat list is split into workspace groups or shown flat. */
export type SidebarGrouping = "workspace" | "none";
/** Order of chats in the sidebar list. */
export type SidebarSort = "recent" | "oldest" | "title";

export interface InterfaceConfig {
  showThinking: ThinkingDisplay;
  showToolCalls: ToolCallDisplay;
  sendKey: SendKey;
  notifyOnCompletion: boolean;
  hotkeyEnabled: boolean;
  hotkey: string;
  alwaysFollow: boolean;
  sidebarPinned: boolean;
  sidebarWidth: number;
  sidebarGrouping: SidebarGrouping;
  sidebarSort: SidebarSort;
  compact: boolean;
  /** Attach a screenshot of the current monitor to every quick-ask send. */
  captureOnSend: boolean;
  /** Render ```loom-ui blocks as live, themed widgets. */
  generatedUi: boolean;
  /** Let the lite model propose durable facts after each reply. */
  autoMemory: boolean;
}

export interface StorageUsage {
  dataDir: string;
  database: number;
  attachments: number;
  generated: number;
  backgrounds: number;
  cache: number;
  total: number;
}

/** One line in a vendor usage card: a percent window or an amount. */
export interface UsageMetric {
  id: string;
  label: string;
  /** 0-100 when the vendor reports a window percentage. */
  percent: number | null;
  used: number | null;
  limit: number | null;
  remaining: number | null;
  /** `percent`, `usd`, `cny`, `tokens`, `calls`. */
  unit: string;
  resetsAt: string | null;
  resetsAtMs: number | null;
  status: string | null;
  /** Auxiliary text, e.g. DeepSeek's granted/topped-up split. */
  detail: string | null;
}

/** Live usage/quota for one provider, straight from the vendor. */
export interface ProviderUsage {
  providerId: string;
  source: string;
  fetchedAt: number;
  metrics: UsageMetric[];
}

export interface UsageCapableProvider {
  providerId: string;
  name: string;
  source: string;
}

export interface ProviderTotals {
  providerId: string;
  replies: number;
  pricedReplies: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCostUsd: number;
}

/** Tokens and estimated spend recorded in the local database. */
export interface UsageSummary {
  replies: number;
  pricedReplies: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCostUsd: number;
  providers: ProviderTotals[];
}

export interface Prompt {
  id: string;
  title: string;
  body: string;
}

/** A saved folder; chats point at one by path. */
export interface Workspace {
  path: string;
  name: string;
  addedAt: number;
}

/** Which service backs the web tools. */
export type SearchProvider = "auto" | "jina" | "duckduckgo";

export interface AppConfig {
  schemaVersion: number;
  theme: Theme;
  background: BackgroundConfig;
  sidebarCollapsed: boolean;
  providers: Record<string, ProviderConfig>;
  personas: Persona[];
  personaGroups: PersonaGroup[];
  userProfile: UserProfile;
  mcpServers: Record<string, McpServerConfig>;
  chat: ChatDefaults;
  interface: InterfaceConfig;
  prompts: Prompt[];
  workspaces: Workspace[];
  searchProvider: SearchProvider;
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
  agentMode: AgentMode | null;
  /** Computer use is armed for this chat (the composer's Computer chip). */
  computerAccess: boolean;
  createdAt: number;
  updatedAt: number;
}

export type ToolCallStatus = "running" | "ok" | "error" | "denied";

/** A message waiting its turn while the assistant is busy. */
export interface QueuedMessage {
  id: string;
  text: string;
  attachments: Attachment[];
}

export type TodoStatus = "pending" | "in_progress" | "completed";

/** One item on a chat's live task list, maintained by `todo_write`. */
export interface Todo {
  id: string;
  content: string;
  status: TodoStatus;
  position: number;
}

/** An image a computer tool produced (a screenshot), stored on disk. */
export interface ToolImage {
  name: string;
  mime: string;
  path: string;
}

export interface ToolCallRecord {
  id: string;
  name: string;
  arguments: string;
  status: ToolCallStatus;
  output: string;
  /** Code points of reply text that came before this call, in stream order. */
  after?: number;
  /** Stream order within the turn, breaking ties at the same offset. */
  seq?: number;
  /** Screenshots and other images the call produced, kept on disk. */
  images?: ToolImage[];
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

export interface QuestionOption {
  label: string;
  description?: string | null;
}

/** An `ask_user` call, normalized by the engine. */
export interface AskQuestion {
  question: string;
  header?: string | null;
  options: QuestionOption[];
  allowMultiple: boolean;
  allowFreeText: boolean;
}

export interface PendingQuestion {
  sessionId: string;
  messageId: string;
  callId: string;
  question: AskQuestion;
}

/** What the user picked or typed. Mirrors `tools::Answer` in the engine. */
export interface QuestionAnswer {
  selected: string[];
  text: string | null;
  cancelled: boolean;
}

export interface Message {
  id: string;
  sessionId: string;
  role: "user" | "assistant";
  content: string;
  reasoning: string | null;
  extra: string | null;
  /** Which persona produced an assistant reply, in multi-persona chats. */
  personaId: string | null;
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
  /** Automatic screenshots: for the model, not rendered as content. */
  hidden?: boolean;
}

export type EngineEvent =
  | { type: "started"; sessionId: string; messageId: string }
  | { type: "delta"; sessionId: string; messageId: string; text: string }
  | {
      type: "reasoning";
      sessionId: string;
      messageId: string;
      text: string;
      /** Reply-text offset this thinking spell preceded; keeps it in place. */
      after: number;
      /** Stream order within the turn, for ties at the same offset. */
      seq: number;
    }
  | {
      type: "toolCallStarted";
      sessionId: string;
      messageId: string;
      callId: string;
      name: string;
      arguments: string;
      /** Stream order within the turn, for ties at the same text offset. */
      seq: number;
    }
  | {
      type: "toolCallFinished";
      sessionId: string;
      messageId: string;
      callId: string;
      ok: boolean;
      output: string;
      images?: ToolImage[];
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
      type: "questionRequest";
      sessionId: string;
      messageId: string;
      callId: string;
      question: AskQuestion;
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
  | { type: "title"; sessionId: string; title: string }
  | { type: "computerPaused"; sessionId: string }
  | { type: "computerResumed"; sessionId: string }
  | {
      /** The model rewrote this chat's task list; the panel follows live. */
      type: "todosChanged";
      sessionId: string;
      todos: Todo[];
    }
  | {
      /** A harness tool changed the config; the UI reloads to stay in step. */
      type: "harnessChanged";
      sessionId: string;
      section: string;
      summary: string;
    }
  | { type: "taskChanged"; task: Task }
  | { type: "jobChanged"; job: Job }
  | {
      /** The memory pass (or a tool) saved facts. */
      type: "memoryChanged";
      scope: string;
      added: number;
      sessionId: string;
      messageId: string;
    };
