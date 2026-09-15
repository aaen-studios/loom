import { create } from "zustand";
import type { ModelEntry, ProviderPreset } from "../types";
import { ipc } from "../lib/ipc";

interface ProvidersState {
  presets: ProviderPreset[];
  models: ModelEntry[];
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
  refresh: () => Promise<void>;
}

/**
 * Provider + model catalogue. Kept separate from the settings store because
 * the composer and the settings drawer both read it.
 */
export const useProviders = create<ProvidersState>((set) => ({
  presets: [],
  models: [],
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const [presets, models] = await Promise.all([
        ipc.providerPresets(),
        ipc.listModels(),
      ]);
      set({
        presets: presets ?? [],
        models: models ?? [],
        loading: false,
      });
    } catch (error) {
      set({ loading: false, error: messageOf(error) });
    }
  },

  refresh: async () => {
    const models = await ipc.listModels();
    set({ models: models ?? [] });
  },
}));

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function enabledModels(models: ModelEntry[]): ModelEntry[] {
  return models.filter((entry) => entry.enabled);
}

export function findModel(
  models: ModelEntry[],
  providerId: string | null | undefined,
  modelId: string | null | undefined,
): ModelEntry | undefined {
  if (!providerId || !modelId) return undefined;
  return models.find(
    (entry) => entry.providerId === providerId && entry.modelId === modelId,
  );
}
