import { create } from "zustand";
import type { UpdateManifest } from "../lib/ipc";
import type { SettingsCategoryId } from "../lib/settingsCategories";

interface UiState {
  /** Which transient selector is open; only one can be at a time. */
  openMenu: string | null;
  setOpenMenu: (id: string | null) => void;
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  settingsCategory: SettingsCategoryId;
  setSettingsCategory: (category: SettingsCategoryId) => void;
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
  availableUpdate: UpdateManifest | null;
  setAvailableUpdate: (manifest: UpdateManifest | null) => void;
  shortcutsOpen: boolean;
  setShortcutsOpen: (open: boolean) => void;
  /** The Runs popup: detached tasks and scheduled jobs. */
  tasksOpen: boolean;
  setTasksOpen: (open: boolean) => void;
}

export const useUi = create<UiState>((set) => ({
  openMenu: null,
  setOpenMenu: (openMenu) => set({ openMenu }),
  settingsOpen: false,
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  settingsCategory: "general",
  setSettingsCategory: (settingsCategory) => set({ settingsCategory }),
  sidebarOpen: false,
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  availableUpdate: null,
  setAvailableUpdate: (availableUpdate) => set({ availableUpdate }),
  shortcutsOpen: false,
  setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
  tasksOpen: false,
  setTasksOpen: (tasksOpen) => set({ tasksOpen }),
}));
