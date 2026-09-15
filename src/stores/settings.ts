import { create } from "zustand";
import type { AppConfig, BackgroundConfig, Theme } from "../types";
import { call } from "../lib/tauri";

export const DEFAULT_CONFIG: AppConfig = {
  schemaVersion: 1,
  theme: "light",
  background: {
    kind: "builtin",
    preset: "aurora",
    path: null,
    dim: 26,
    blur: 0,
  },
  sidebarCollapsed: false,
};

interface SettingsState {
  config: AppConfig;
  loaded: boolean;
  load: () => Promise<void>;
  setTheme: (theme: Theme) => void;
  setBackground: (patch: Partial<BackgroundConfig>) => void;
  toggleSidebar: () => void;
}

let saveTimer: number | undefined;

/** Debounced persist so slider drags don't hammer the config file. */
function scheduleSave(get: () => SettingsState): void {
  if (saveTimer !== undefined) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = undefined;
    void call("save_config", { config: get().config });
  }, 250);
}

export const useSettings = create<SettingsState>((set, get) => ({
  config: DEFAULT_CONFIG,
  loaded: false,

  load: async () => {
    const remote = await call<AppConfig>("get_config");
    set({ config: remote ?? DEFAULT_CONFIG, loaded: true });
  },

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

  toggleSidebar: () => {
    set((state) => ({
      config: {
        ...state.config,
        sidebarCollapsed: !state.config.sidebarCollapsed,
      },
    }));
    scheduleSave(get);
  },
}));
