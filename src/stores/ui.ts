import { create } from "zustand";
import type { UpdateManifest } from "../lib/ipc";

interface UiState {
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
  availableUpdate: UpdateManifest | null;
  setAvailableUpdate: (manifest: UpdateManifest | null) => void;
}

export const useUi = create<UiState>((set) => ({
  settingsOpen: false,
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  sidebarOpen: false,
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  availableUpdate: null,
  setAvailableUpdate: (availableUpdate) => set({ availableUpdate }),
}));
