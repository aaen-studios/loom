import { create } from "zustand";
import type { AppConfig, BackgroundConfig, Theme } from "../types";
import { ipc } from "../lib/ipc";

export const DEFAULT_CONFIG: AppConfig = {
  schemaVersion: 1,
  theme: "dark",
  background: {
    kind: "builtin",
    preset: "rei",
    path: null,
    dim: 30,
    blur: 0,
  },
  sidebarCollapsed: false,
  providers: {},
  personas: [],
  mcpServers: {},
  chat: {
    providerId: null,
    modelId: null,
    variant: null,
    lite: null,
    imageModel: null,
    embeddingModel: null,
    autoTitle: true,
    permissionMode: "ask",
    historyLimit: 40,
    maxOutputTokens: 8192,
  },
  interface: {
    showThinking: "collapsed",
    sendKey: "enter",
    notifyOnCompletion: true,
    hotkeyEnabled: true,
    hotkey: "Ctrl+Shift+Space",
    alwaysFollow: false,
    compact: false,
  },
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
