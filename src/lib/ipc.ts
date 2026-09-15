import type {
  AppConfig,
  AppInfo,
  Attachment,
  InterfaceConfig,
  Message,
  ModelEntry,
  ModelRef,
  PermissionMode,
  Persona,
  Prompt,
  ProviderConfig,
  ProviderPreset,
  Session,
  WorkspaceInfo,
} from "../types";

export interface ToolSpec {
  name: string;
  description: string;
  parameters: unknown;
  readOnly: boolean;
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
    historyLimit?: number | null;
    maxOutputTokens?: number | null;
    autoTitle?: boolean | null;
  }) => call<AppConfig>("set_chat_settings", args),

  setInterfaceSettings: (interface_: InterfaceConfig) =>
    call<AppConfig>("set_interface_settings", { interface: interface_ }),
  setHotkey: (enabled: boolean, keys: string) =>
    call<void>("set_hotkey", { enabled, keys }),

  upsertPrompt: (prompt: Prompt) => call<AppConfig>("upsert_prompt", { prompt }),
  deletePrompt: (id: string) => call<AppConfig>("delete_prompt", { id }),
  upsertPersona: (persona: Persona) =>
    call<AppConfig>("upsert_persona", { persona }),
  deletePersona: (id: string) => call<AppConfig>("delete_persona", { id }),

  createSession: (title?: string | null, personaId?: string | null) =>
    call<Session>("create_session", { title: title ?? null, personaId: personaId ?? null }),
  listSessions: () => call<Session[]>("list_sessions"),
  deleteSession: (id: string) => call<void>("delete_session", { id }),
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
  respondToolPermission: (callId: string, allow: boolean, remember?: string | null) =>
    call<void>("respond_tool_permission", {
      callId,
      allow,
      remember: remember ?? null,
    }),
  listTools: () => call<ToolSpec[]>("list_tools"),
  workspaceInfo: (workdir: string | null) =>
    call<WorkspaceInfo>("workspace_info", { workdir }),

  upsertMcpServer: (id: string, server: McpServerConfig) =>
    call<AppConfig>("upsert_mcp_server", { id, server }),
  deleteMcpServer: (id: string) => call<AppConfig>("delete_mcp_server", { id }),
  mcpTools: () => call<McpToolView[]>("mcp_tools"),
  listSkills: () => call<Skill[]>("list_skills"),
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
  ) => call<AppConfig>("set_model_spec", { providerId, modelId, context, output }),
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

  sendMessage: (
    sessionId: string,
    text: string,
    attachments?: Attachment[],
    model?: ModelRef | null,
    providerId?: string | null,
    modelId?: string | null,
  ) =>
    call<string>("send_message", {
      sessionId,
      text,
      attachments: attachments ?? [],
      providerId: providerId ?? model?.providerId ?? null,
      modelId: modelId ?? model?.modelId ?? null,
    }),
  attachFiles: (sessionId: string, paths: string[]) =>
    call<Attachment[]>("attach_files", { sessionId, paths }),
  attachBytes: (sessionId: string, name: string, data: string) =>
    call<Attachment>("attach_bytes", { sessionId, name, data }),
  setBackgroundFile: (kind: "image" | "video", source: string) =>
    call<AppConfig>("set_background_file", { kind, source }),
  cancelStream: (sessionId: string) => call<void>("cancel_stream", { sessionId }),
  busySessions: () => call<string[]>("busy_sessions"),

  hideOverlay: () => call<void>("hide_overlay"),
  showMain: () => call<void>("show_main"),
  openSettings: () => call<void>("open_settings"),
  quitApp: () => call<void>("quit_app"),
};
