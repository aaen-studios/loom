import { create } from "zustand";
import type { AppConfig, BackgroundConfig, Theme } from "../types";
import { ipc } from "../lib/ipc";

export const DEFAULT_CONFIG: AppConfig = {
  schemaVersion: 1,
  theme: "light",
  background: {
    kind: "builtin",
    preset: "porcelain",
    path: null,
    dim: 0,
    blur: 0,
  },
  sidebarCollapsed: false,
  providers: {},
  personas: [],
  personaGroups: [],
  userProfile: { name: "", pronouns: "", about: "" },
  mcpServers: {},
  chat: {
    providerId: null,
    modelId: null,
    variant: null,
    lite: null,
    imageModel: null,
    embeddingModel: null,
    autoTitle: true,
    recentModels: [],
    permissionMode: "ask",
    agentMode: "build",
    maxOutputTokens: 0,
    maxToolRounds: 40,
    computerVariant: "low",
    computerModel: null,
    computerScreenshotEdge: 0,
  },
  interface: {
    showThinking: "collapsed",
    showToolCalls: "collapsed",
    sendKey: "enter",
    notifyOnCompletion: true,
    hotkeyEnabled: true,
    hotkey: "Ctrl+Shift+Space",
    alwaysFollow: false,
    sidebarPinned: false,
    sidebarWidth: 264,
    sidebarGrouping: "workspace",
    sidebarSort: "recent",
    sidebarWorkspaceOrder: [],
    compact: false,
    generatedUi: true,
    showCondensing: true,
    captureOnSend: true,
    autoMemory: true,
  },
  prompts: [],
  workspaces: [],
  searchProvider: "auto",
};

interface SettingsState {
  config: AppConfig;
  loaded: boolean;
  load: () => Promise<void>;
  applyRemote: (config: AppConfig) => void;
  setTheme: (theme: Theme) => void;
  setBackground: (patch: Partial<BackgroundConfig>) => void;
}

let saveTimer: number | undefined;

/** Debounced persist so slider drags don't hammer the config file. */
function scheduleSave(get: () => SettingsState): void {
  if (saveTimer !== undefined) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = undefined;
    void ipc.saveConfig(get().config);
  }, 250);
}

export const useSettings = create<SettingsState>((set, get) => ({
  config: DEFAULT_CONFIG,
  loaded: false,

  load: async () => {
    const remote = await ipc.getConfig();
    set({ config: remote ?? DEFAULT_CONFIG, loaded: true });
  },

  applyRemote: (config) => set({ config }),

  setTheme: (theme) => {
    set((state) => ({ config: { ...state.config, theme } }));
    scheduleSave(get);
  },

  setBackground: (patch) => {
    set((state) => ({
      config: {
        ...state.config,
        background: { ...state.config.background, ...patch },
      },
    }));
    scheduleSave(get);
  },
}));
