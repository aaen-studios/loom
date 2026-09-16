import type {
  AgentMode,
  AppConfig,
  AppInfo,
  Attachment,
  CommandRun,
  InterfaceConfig,
  Job,
  MemoryEntry,
  Message,
  ModelEntry,
  ModelRef,
  Modality,
  PermissionMode,
  Persona,
  PersonaGroup,
  Prompt,
  ProviderConfig,
  ProviderPreset,
  ProviderUsage,
  QuestionAnswer,
  ReasoningSpec,
  SearchProvider,
  Session,
  StorageUsage,
  StoredMemory,
  Task,
  Todo,
  ToolScope,
  UsageCapableProvider,
  UsageSummary,
  UserProfile,
  WorkspaceInfo,
} from "../types";

export interface ToolSpec {
  name: string;
  description: string;
  parameters: unknown;
  readOnly: boolean;
  /** Absent means a workspace tool. */
  scope?: ToolScope;
}

export interface McpServerConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
}

export interface McpToolView {
  server: string;
  name: string;
  description: string;
  modelName: string;
}


export interface Skill {
  id: string;
  name: string;
  description: string;
  prompt: string;
  path: string;
}

export interface UpdateManifest {
  version: string;
  notes: string;
  payload: string;
  sha256: string;
  signature?: string | null;
}

export interface UpdateCheck {
  available: boolean;
  currentVersion: string;
  manifest: UpdateManifest | null;
}
import { call } from "./tauri";

export const ipc = {
  appInfo: () => call<AppInfo>("app_info"),
  getConfig: () => call<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => call<AppConfig>("save_config", { config }),

  providerPresets: () => call<ProviderPreset[]>("list_provider_presets"),
  upsertProvider: (id: string, provider: ProviderConfig) =>
    call<AppConfig>("upsert_provider", { id, provider }),
  deleteProvider: (id: string) => call<AppConfig>("delete_provider", { id }),
  setProviderKey: (id: string, key: string) =>
    call<void>("set_provider_key", { id, key }),
  clearProviderKey: (id: string) => call<void>("clear_provider_key", { id }),
  providerKeyStatus: (id: string) => call<boolean>("provider_key_status", { id }),
  refreshProviderModels: (id: string) =>
    call<AppConfig>("refresh_provider_models", { id }),
  setProviderEnabled: (id: string, enabled: boolean) =>
    call<AppConfig>("set_provider_enabled", { id, enabled }),

  listModels: () => call<ModelEntry[]>("list_models"),
  setDefaultModel: (args: {
    providerId: string | null;
    modelId: string | null;
    variant: string | null;
    liteProviderId: string | null;
    liteModelId: string | null;
  }) => call<AppConfig>("set_default_model", args),

  setChatSettings: (args: {
    permissionMode?: PermissionMode | null;
    agentMode?: AgentMode | null;
    maxOutputTokens?: number | null;
    autoTitle?: boolean | null;
    maxToolRounds?: number | null;
    computerVariant?: string | null;
    computerModel?: ModelRef | null;
    computerScreenshotEdge?: number | null;
  }) => call<AppConfig>("set_chat_settings", args),

  setInterfaceSettings: (interface_: InterfaceConfig) =>
    call<AppConfig>("set_interface_settings", { interface: interface_ }),
  storageUsage: () => call<StorageUsage>("storage_usage"),
  clearCache: () => call<number>("clear_cache"),
  clearGenerated: () => call<number>("clear_generated"),

  usageCapableProviders: () =>
    call<UsageCapableProvider[]>("usage_capable_providers"),
  providerUsage: (providerId: string) =>
    call<ProviderUsage>("provider_usage", { providerId }),
  usageSummary: () => call<UsageSummary>("usage_summary"),
  setHotkey: (enabled: boolean, keys: string) =>
    call<void>("set_hotkey", { enabled, keys }),

  /** Registry-backed: true while Loom is set to launch at Windows login. */
  autostartEnabled: () => call<boolean>("autostart_enabled"),
  setAutostart: (enabled: boolean) =>
    call<boolean>("set_autostart", { enabled }),

  upsertPrompt: (prompt: Prompt) => call<AppConfig>("upsert_prompt", { prompt }),
  deletePrompt: (id: string) => call<AppConfig>("delete_prompt", { id }),
  upsertPersona: (persona: Persona) =>
    call<AppConfig>("upsert_persona", { persona }),
  deletePersona: (id: string) => call<AppConfig>("delete_persona", { id }),
  setUserProfile: (profile: UserProfile) =>
    call<AppConfig>("set_user_profile", { profile }),
  upsertPersonaGroup: (group: PersonaGroup) =>
    call<AppConfig>("upsert_persona_group", { group }),
  deletePersonaGroup: (id: string) =>
    call<AppConfig>("delete_persona_group", { id }),
  personaMemory: (personaId: string) =>
    call<MemoryEntry[]>("persona_memory", { personaId }),
  setPersonaMemory: (
    personaId: string,
    key: string,
    value: string,
    source?: string | null,
  ) =>
    call<MemoryEntry[]>("set_persona_memory", {
      personaId,
      key,
      value,
      source: source ?? null,
    }),
  deletePersonaMemory: (personaId: string, id: string) =>
    call<MemoryEntry[]>("delete_persona_memory", { personaId, id }),
  clearPersonaMemory: (personaId: string) =>
    call<MemoryEntry[]>("clear_persona_memory", { personaId }),

  createSession: (
    title?: string | null,
    personaId?: string | null,
    workdir?: string | null,
  ) =>
    call<Session>("create_session", {
      title: title ?? null,
      personaId: personaId ?? null,
      workdir: workdir ?? null,
    }),
  listSessions: () => call<Session[]>("list_sessions"),
  deleteSession: (id: string) => call<void>("delete_session", { id }),
  /** Drops chats that were never used; `keep` protects the open one. */
  pruneEmptySessions: (keep: string | null) =>
    call<number>("prune_empty_sessions", { keep }),
  renameSession: (id: string, title: string) =>
    call<void>("rename_session", { id, title }),
  sessionMessages: (id: string) => call<Message[]>("session_messages", { id }),
  deleteMessage: (messageId: string) =>
    call<void>("delete_message", { messageId }),
  setModelFavorite: (providerId: string, modelId: string, favorite: boolean) =>
    call<AppConfig>("set_model_favorite", { providerId, modelId, favorite }),
  setSessionModel: (
    id: string,
    providerId: string,
    modelId: string,
    variant?: string | null,
  ) => call<void>("set_session_model", { id, providerId, modelId, variant: variant ?? null }),
  setSessionVariant: (id: string, variant: string | null) =>
    call<void>("set_session_variant", { id, variant }),
  setSessionWorkdir: (id: string, workdir: string | null) =>
    call<void>("set_session_workdir", { id, workdir }),
  setSessionPermissionMode: (id: string, mode: PermissionMode | null) =>
    call<void>("set_session_permission_mode", { id, mode }),
  setSessionAgentMode: (id: string, mode: AgentMode | null) =>
    call<void>("set_session_agent_mode", { id, mode }),
  setSessionComputerAccess: (id: string, enabled: boolean) =>
    call<void>("set_session_computer_access", { id, enabled }),
  stopComputer: () => call<void>("stop_computer", {}),
  resumeComputer: () => call<boolean>("resume_computer", {}),
  computerStatus: () =>
    call<{
      state: "hidden" | "active" | "paused";
      sessionId: string | null;
      idleSeconds: number;
      pausedSeconds: number;
    }>("computer_status", {}),
  setSessionGoal: (id: string, goal: string | null) =>
    call<void>("set_session_goal", { id, goal }),
  sessionGoal: (id: string) => call<string | null>("session_goal", { id }),
  sessionTodos: (id: string) => call<Todo[]>("session_todos", { id }),
  setTodos: (id: string, todos: Todo[]) =>
    call<Todo[]>("set_todos", { id, todos }),
  respondToolPermission: (callId: string, allow: boolean, remember?: string | null) =>
    call<void>("respond_tool_permission", {
      callId,
      allow,
      remember: remember ?? null,
    }),
  respondQuestion: (callId: string, answer: QuestionAnswer) =>
    call<void>("respond_question", { callId, answer }),
  listTools: () => call<ToolSpec[]>("list_tools"),
  workspaceInfo: (workdir: string | null) =>
    call<WorkspaceInfo>("workspace_info", { workdir }),

  addWorkspace: (path: string, name?: string | null) =>
    call<AppConfig>("add_workspace", { path, name: name ?? null }),
  renameWorkspace: (path: string, name: string) =>
    call<AppConfig>("rename_workspace", { path, name }),
  removeWorkspace: (path: string) =>
    call<AppConfig>("remove_workspace", { path }),
  setSearchProvider: (provider: SearchProvider) =>
    call<AppConfig>("set_search_provider", { provider }),
  setSearchKey: (key: string) => call<void>("set_search_key", { key }),
  searchKeyStatus: () => call<boolean>("search_key_status"),

  upsertMcpServer: (id: string, server: McpServerConfig) =>
    call<AppConfig>("upsert_mcp_server", { id, server }),
  deleteMcpServer: (id: string) => call<AppConfig>("delete_mcp_server", { id }),
  mcpTools: () => call<McpToolView[]>("mcp_tools"),
  listSkills: () => call<Skill[]>("list_skills"),
  /** Writes a skill file; validation is shared with the model's write_skill. */
  saveSkill: (args: {
    id: string;
    name: string;
    description: string;
    body: string;
  }) => call<void>("save_skill", args),
  deleteSkill: (id: string) => call<void>("delete_skill", { id }),
  readSkill: (id: string) => call<Skill>("read_skill", { id }),
  setImageModel: (model: string | null) =>
    call<AppConfig>("set_image_model", { model }),
  setEmbeddingModel: (model: string | null) =>
    call<AppConfig>("set_embedding_model", { model }),
  addModel: (providerId: string, modelId: string) =>
    call<AppConfig>("add_model", { providerId, modelId }),
  setModelSpec: (
    providerId: string,
    modelId: string,
    context: number | null,
    output: number | null,
    inputModalities: Modality[],
    reasoning: ReasoningSpec | null,
  ) =>
    call<AppConfig>("set_model_spec", {
      providerId,
      modelId,
      context,
      output,
      inputModalities,
      reasoning,
    }),
  resetModelSpec: (providerId: string, modelId: string) =>
    call<AppConfig>("reset_model_spec", { providerId, modelId }),
  exportSession: (sessionId: string, path: string) =>
    call<void>("export_session", { sessionId, path }),
  indexWorkspace: (sessionId: string) =>
    call<number>("index_workspace", { sessionId }),
  indexStatus: (sessionId: string) =>
    call<number>("workspace_index_status", { sessionId }),
  clearIndex: (sessionId: string) =>
    call<void>("clear_workspace_index", { sessionId }),
  checkForUpdates: () => call<UpdateCheck>("check_for_updates"),
  downloadUpdate: (manifest: UpdateManifest) =>
    call<string>("download_update", { manifest }),
  applyUpdate: (staging: string) => call<void>("apply_update", { staging }),
  setSessionPersona: (
    id: string,
    personaId: string | null,
    systemPrompt: string | null,
  ) => call<void>("set_session_persona", { id, personaId, systemPrompt }),
  setSessionCast: (id: string, personaIds: string[]) =>
    call<void>("set_session_cast", { id, personaIds }),
  sessionCast: (id: string) => call<Persona[]>("session_cast", { id }),

  sendMessage: (
    sessionId: string,
    text: string,
    attachments?: Attachment[],
    model?: ModelRef | null,
    providerId?: string | null,
    modelId?: string | null,
    personaId?: string | null,
  ) =>
    call<string>("send_message", {
      sessionId,
      text,
      attachments: attachments ?? [],
      providerId: providerId ?? model?.providerId ?? null,
      modelId: modelId ?? model?.modelId ?? null,
      personaId: personaId ?? null,
    }),
  attachFiles: (sessionId: string, paths: string[]) =>
    call<Attachment[]>("attach_files", { sessionId, paths }),
  attachBytes: (sessionId: string, name: string, data: string) =>
    call<Attachment>("attach_bytes", { sessionId, name, data }),
  captureScreen: (sessionId: string) =>
    call<Attachment>("capture_screen", { sessionId }),
  /** The frame taken when the overlay opened, if one is waiting. */
  claimScreen: (sessionId: string) =>
    call<Attachment | null>("claim_screen", { sessionId }),
  setBackgroundFile: (kind: "image" | "video", source: string) =>
    call<AppConfig>("set_background_file", { kind, source }),
  cancelStream: (sessionId: string) => call<void>("cancel_stream", { sessionId }),
  busySessions: () => call<string[]>("busy_sessions"),

  hideOverlay: () => call<void>("hide_overlay"),
  showMain: (sessionId?: string | null) =>
    call<void>("show_main", { sessionId: sessionId ?? null }),
  openSettings: () => call<void>("open_settings"),
  quitApp: () => call<void>("quit_app"),

  listCommands: (sessionId?: string | null) =>
    call<CommandRun[]>("list_commands", { sessionId: sessionId ?? null }),
  commandOutput: (id: string, lines?: number) =>
    call<string>("command_output", { id, lines: lines ?? 200 }),
  stopCommand: (id: string) => call<CommandRun>("stop_command", { id }),
  deleteCommand: (id: string) => call<void>("delete_command", { id }),

  listTasks: (jobId?: string | null) =>
    call<Task[]>("list_tasks", { jobId: jobId ?? null }),
  cancelTask: (id: string) => call<void>("cancel_task", { id }),
  retryTask: (id: string) => call<string>("retry_task", { id }),
  deleteTask: (id: string) => call<void>("delete_task", { id }),
  startTask: (
    prompt: string,
    title?: string | null,
    sessionId?: string | null,
    model?: ModelRef | null,
  ) =>
    call<string>("start_task", {
      prompt,
      title: title ?? null,
      sessionId: sessionId ?? null,
      model: model ?? null,
    }),

  listJobs: () => call<Job[]>("list_jobs"),
  upsertJob: (job: Job) => call<Job>("upsert_job", { job }),
  deleteJob: (id: string) => call<void>("delete_job", { id }),
  runJobNow: (id: string) => call<string>("run_job_now", { id }),
  previewSchedule: (cron: string, count?: number) =>
    call<number[]>("preview_schedule", { cron, count: count ?? 5 }),

  listMemories: (scope?: string | null) =>
    call<StoredMemory[]>("list_memories", { scope: scope ?? null }),
  upsertMemory: (
    id: string | null,
    scope: string,
    content: string,
    pinned: boolean,
  ) =>
    call<StoredMemory>("upsert_memory", { id, scope, content, pinned }),
  deleteMemory: (id: string) => call<void>("delete_memory", { id }),
  clearMemories: (scope: string) => call<number>("clear_memories", { scope }),
};
