import type {
  AgentMode,
  AppInfo,
  AppConfig,
  Attachment,
  AuxModelRef,
  BlockingConfig,
  BlockingStatus,
  Branch,
  CommandRun,
  CommitResult,
  FileStat,
  FilterListPreset,
  GitCommit,
  GitStatus,
  InterfaceConfig,
  Job,
  LineEnding,
  MemoryEntry,
  Message,
  ModelEntry,
  Modality,
  ModelRef,
  PermissionMode,
  Persona,
  PersonaGroup,
  Prompt,
  ProviderConfig,
  ProviderPreset,
  ProviderUsage,
  QuestionAnswer,
  ReasoningSpec,
  SaveOutcome,
  SearchProvider,
  Session,
  SessionSummary,
  StorageUsage,
  StoredMemory,
  Task,
  TextFile,
  Todo,
  ToolScope,
  TreeEntry,
  UsageCapableProvider,
  UsageSummary,
  UserProfile,
  WorkspaceInfo,
} from "../types";

/**
 * Re-exported so `lib/fileTreeOperations` and the tree component import one
 * module rather than reaching into `types` for a shape the IPC layer owns.
 */
export type { TreeEntry };

export interface ToolSpec {
  name: string;
  description: string;
  parameters: unknown;
  readOnly: boolean;
  /** Absent means a workspace tool. */
  scope?: ToolScope;
}

/** Which cookie jar a tab uses. */
export type BrowserProfile = "normal" | "ghost";

/** One open tab, as the shell reports it. */
export interface BrowserTab {
  id: number;
  title: string;
  url: string;
  profile: BrowserProfile;
  /** The tab on screen *in its own host window*; two windows can each have one. */
  active: boolean;
  /** The chat's current tool call is working in this tab. */
  driving: boolean;
  loading: boolean;
  /** Which window's panel is showing it, so a panel can pick out its own. */
  host: string;
  sessionId: string | null;
}

export interface BrowserWire {
  tabs: BrowserTab[];
  downloads: BrowserDownload[];
}

/** A download the browser saw, for the panel's rail. */
export interface BrowserDownload {
  url: string;
  path: string;
  ok: boolean;
  at: number;
}

/**
 * Where the page goes, in **logical pixels relative to its window's client
 * area** — exactly what `getBoundingClientRect` returns for an element in that
 * window. The page is a child webview of the window showing the panel, so there
 * is no screen coordinate and no scale factor involved.
 */
export interface BrowserSlot {
  x: number;
  y: number;
  width: number;
  height: number;
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
  /**
   * Why a newer release is not being offered, when one exists and this build
   * will not install it — an unsigned payload, today. `null` in every ordinary
   * case, including "already current".
   */
  refused: string | null;
}
import { call } from "./tauri";
import type {
  DockLayout,
  PtyInfo,
  PtyProfile,
  StoredBackground,
  TerminalConfig,
} from "../types";

export const ipc = {
  appInfo: () => call<AppInfo>("app_info"),
  getConfig: () => call<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => call<AppConfig>("save_config", { config }),

  providerPresets: () => call<ProviderPreset[]>("list_provider_presets"),
  upsertProvider: (id: string, provider: ProviderConfig) =>
    call<AppConfig>("upsert_provider", { id, provider }),
  deleteProvider: (id: string) => call<AppConfig>("delete_provider", { id }),
  /** Copies a provider so one vendor can be configured more than once. The
   *  copy inherits the catalogue and model selection but **no** API key. */
  duplicateProvider: (id: string) =>
    call<AppConfig>("duplicate_provider", { id }),
  /** Batched: "select all shown" over a search result is one call. */
  setModelsSelected: (
    providerId: string,
    modelIds: string[],
    selected: boolean,
  ) => call<AppConfig>("set_models_selected", { providerId, modelIds, selected }),
  /** Whether models a refresh discovers start selected. */
  setProviderAutoSelect: (id: string, autoSelect: boolean) =>
    call<AppConfig>("set_provider_auto_select", { id, autoSelect }),
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

  /* --- The dock ---------------------------------------------------------- */

  /** A folder's arrangement; `null` asks for the default. */
  dockLayout: (workdir?: string | null) =>
    call<DockLayout>("dock_layout", { workdir: workdir ?? null }),
  /** Stores a folder's arrangement (`workdir: null` writes the default). */
  setDockLayout: (workdir: string | null, layout: DockLayout) =>
    call<AppConfig>("set_dock_layout", { workdir, layout }),
  setTerminalSettings: (terminal: TerminalConfig) =>
    call<AppConfig>("set_terminal_settings", { terminal }),

  /* --- The terminal ------------------------------------------------------ */

  /** Probes the filesystem and `wsl.exe`, so callers cache the answer. */
  ptyProfiles: () => call<PtyProfile[]>("pty_profiles"),
  ptyOpen: (args: {
    id: string;
    profile: string;
    workdir?: string | null;
    rows?: number;
    cols?: number;
  }) => call<void>("pty_open", { ...args, workdir: args.workdir ?? null }),
  ptyWrite: (id: string, data: string) => call<void>("pty_write", { id, data }),
  ptyResize: (id: string, rows: number, cols: number) =>
    call<void>("pty_resize", { id, rows, cols }),
  ptyClose: (id: string) => call<void>("pty_close", { id }),
  ptyList: () => call<PtyInfo[]>("pty_list"),
  /** Tears a panel off into its own window, or focuses it if already out. */
  openPanelWindow: (panel: string, title: string) =>
    call<void>("open_panel_window", { panel, title }),
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
  /** Writes the hand-placed order for one workspace group, top row first. */
  reorderSessions: (ids: string[]) =>
    call<void>("reorder_sessions", { ids }),
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
  setSessionBrowserAccess: (id: string, enabled: boolean) =>
    call<void>("set_session_browser_access", { id, enabled }),

  /* --- The built-in browser ---------------------------------------------- */

  /** Opens a tab over `host`'s slot. A tab is a real window, positioned by the panel. */
  browserOpenTab: (args: {
    url: string;
    host?: string | null;
    profile?: BrowserProfile;
    sessionId?: string | null;
  }) =>
    call<BrowserTab>("browser_open_tab", {
      url: args.url,
      host: args.host ?? null,
      profile: args.profile ?? null,
      sessionId: args.sessionId ?? null,
    }),
  browserTabs: () => call<BrowserWire>("browser_tabs"),
  browserCloseTab: (id: number) => call<void>("browser_close_tab", { id }),
  /** Reports where the page goes, in the panel's own window coordinates. */
  browserSetSlot: (args: {
    host: string;
    active: number | null;
    slot: BrowserSlot | null;
  }) => call<void>("browser_set_slot", args),
  browserFocusTab: (id: number, sessionId?: string | null) =>
    call<void>("browser_focus_tab", { id, sessionId: sessionId ?? null }),
  browserNavigate: (args: {
    id: number;
    action: "goto" | "back" | "forward" | "reload" | "stop";
    url?: string | null;
  }) => call<void>("browser_navigate", { ...args, url: args.url ?? null }),
  /** Runs a browser tool from the UI, through the same path the model uses. */
  browserCall: (sessionId: string, op: string, args: Record<string, unknown>) =>
    call<unknown>("browser_call", { sessionId, op, args }),
  browserPing: (id: number) => call<unknown>("browser_ping", { id }),
  browserSetBlockedOrigins: (origins: string[]) =>
    call<AppConfig>("browser_set_blocked_origins", { origins }),

  /* --- Content blocking --------------------------------------------------- */

  browserBlockingStatus: () => call<BlockingStatus>("browser_blocking_status"),
  browserBlockingPresets: () =>
    call<FilterListPreset[]>("browser_blocking_presets"),
  /** Rebuilds from the config; takes effect on the next tab. */
  browserRefreshBlocking: () => call<BlockingStatus>("browser_refresh_blocking"),
  /** Returns the whole config, so the caller can replace its copy. */
  browserSetBlocking: (blocking: BlockingConfig) =>
    call<AppConfig>("browser_set_blocking", { blocking }),
  /** Reloads every open tab, which is how a blocking change takes effect. */
  browserReloadTabs: () => call<void>("browser_reload_tabs"),
  stopComputer: () => call<void>("stop_computer", {}),
  resumeComputer: () => call<boolean>("resume_computer", {}),
  computerStatus: () =>
    call<{
      state: "hidden" | "active" | "paused";
      sessionId: string | null;
      idleSeconds: number;
      pausedSeconds: number;
      /** The engine's auto-resume window, so the pill never hardcodes it. */
      resumeInSeconds: number;
      /** False when the input hooks could not install. */
      takeoverActive: boolean;
      takeoverError?: string | null;
    }>("computer_status", {}),
  /** Debug builds only: fakes the input that pauses a computer turn. */
  debugTripComputerTakeover: () =>
    call<void>("debug_trip_computer_takeover", {}),
  setSessionGoal: (id: string, goal: string | null) =>
    call<void>("set_session_goal", { id, goal }),
  sessionGoal: (id: string) => call<string | null>("session_goal", { id }),

  /** The chat's condensed view of its older turns; `null` until one is written. */
  sessionSummary: (id: string) => call<SessionSummary | null>("session_summary", { id }),
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
  /** Every file in a workspace folder, for the composer's `@` picker. */
  listWorkspaceFiles: (workdir: string, limit?: number) =>
    call<string[]>("list_workspace_files", { workdir, limit: limit ?? 4000 }),

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
  /** A bare `{providerId: "", modelId}` means "whichever provider serves it". */
  setImageModel: (model: AuxModelRef | null) =>
    call<AppConfig>("set_image_model", { model }),
  setEmbeddingModel: (model: AuxModelRef | null) =>
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
  /** The three backgrounds retention keeps, newest first. */
  listBackgrounds: () => call<StoredBackground[]>("list_backgrounds"),
  /** Switches to one already stored — no copy, so it cannot duplicate itself. */
  useBackgroundFile: (path: string) =>
    call<AppConfig>("use_background_file", { path }),
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

  /* --- Git ---------------------------------------------------------------- */

  /** Whether `git` can be run at all, so the panel can say so once. */
  gitAvailable: () => call<boolean>("git_available"),
  gitStatus: (workdir: string) => call<GitStatus>("git_status", { workdir }),
  gitStage: (workdir: string, paths: string[]) =>
    call<GitStatus>("git_stage", { workdir, paths }),
  gitUnstage: (workdir: string, paths: string[]) =>
    call<GitStatus>("git_unstage", { workdir, paths }),
  /** Destructive. The panel confirms before calling this. */
  gitDiscard: (workdir: string, paths: string[]) =>
    call<GitStatus>("git_discard", { workdir, paths }),
  gitCommit: (workdir: string, message: string) =>
    call<CommitResult>("git_commit", { workdir, message }),
  gitBranches: (workdir: string) => call<Branch[]>("git_branches", { workdir }),
  gitCheckout: (workdir: string, name: string) =>
    call<GitStatus>("git_checkout", { workdir, name }),
  gitCreateBranch: (workdir: string, name: string, checkout: boolean) =>
    call<GitStatus>("git_create_branch", { workdir, name, checkout }),
  gitFetch: (workdir: string) => call<string>("git_fetch", { workdir }),
  gitPull: (workdir: string) => call<string>("git_pull", { workdir }),
  gitPush: (workdir: string) => call<string>("git_push", { workdir }),
  gitLog: (workdir: string, count?: number) =>
    call<GitCommit[]>("git_log", { workdir, count: count ?? 30 }),
  gitDiff: (workdir: string, staged: boolean) =>
    call<string>("git_diff", { workdir, staged }),
  /** A file's content at HEAD or in the index; null when it is absent there. */
  gitFileAt: (workdir: string, path: string, staged: boolean) =>
    call<string | null>("git_file_at", { workdir, path, staged }),
  /**
   * Asks the chat's own model for a message. Slow: it is a real model call.
   *
   * Takes the session rather than the folder, because the message must come from
   * the model that chat selected — its provider, model id, variant and key. The
   * backend resolves all of those from the session id, so there is no way for
   * the panel to ask the wrong model by passing a different folder.
   */
  draftCommitMessage: (sessionId: string, staged: boolean) =>
    call<string>("draft_commit_message", { sessionId, staged }),

  /* --- Files -------------------------------------------------------------- */

  fileRead: (workdir: string | null, path: string) =>
    call<TextFile>("file_read", { workdir, path }),
  fileSave: (args: {
    workdir: string | null;
    path: string;
    text: string;
    /** Null writes unconditionally — what "keep mine" passes. */
    expectedHash: string | null;
    eol: LineEnding;
    bom: boolean;
  }) => call<SaveOutcome>("file_save", args),
  /** Batched: autosave fires on a timer and an editor can have a dozen tabs. */
  fileStatMany: (workdir: string | null, paths: string[]) =>
    call<FileStat[]>("file_stat_many", { workdir, paths }),
  dirList: (workdir: string | null, path: string) =>
    call<TreeEntry[]>("dir_list", { workdir, path }),
  fileCreate: (workdir: string | null, path: string, isDir: boolean) =>
    call<TreeEntry>("file_create", { workdir, path, isDir }),
  fileRename: (workdir: string | null, from: string, to: string) =>
    call<TreeEntry>("file_rename", { workdir, from, to }),
  fileDelete: (workdir: string | null, path: string) =>
    call<void>("file_delete", { workdir, path }),
  fileExists: (workdir: string | null, path: string) =>
    call<boolean>("file_exists", { workdir, path }),
};
