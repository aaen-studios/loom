import { create } from "zustand";
import type { ProviderUsage, UsageCapableProvider, UsageSummary } from "../types";
import { ipc } from "../lib/ipc";

/** How often the vendor endpoints are re-read while the app is running. */
const POLL_MS = 5 * 60_000;

let poll: ReturnType<typeof setInterval> | null = null;

interface UsageState {
  capable: UsageCapableProvider[];
  /** Latest reading per provider; failed providers stay absent. */
  byProvider: Record<string, ProviderUsage>;
  errors: Record<string, string>;
  loading: boolean;
  summary: UsageSummary | null;
  summaryLoading: boolean;
  summaryError: string | null;
  started: boolean;
  /** Subscribes to the vendor endpoints: initial fetch plus a slow poll. */
  start: () => void;
  /** Re-reads the capable list and every provider's usage. */
  refresh: () => Promise<void>;
  refreshProvider: (providerId: string) => Promise<void>;
  refreshSummary: () => Promise<void>;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function dropKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  const next = { ...record };
  delete next[key];
  return next;
}

export const useUsage = create<UsageState>((set, get) => ({
  capable: [],
  byProvider: {},
  errors: {},
  loading: false,
  summary: null,
  summaryLoading: false,
  summaryError: null,
  started: false,

  start: () => {
    if (get().started) return;
    set({ started: true });
    void get().refresh();
    if (poll === null) {
      poll = setInterval(() => void get().refresh(), POLL_MS);
    }
  },

  refresh: async () => {
    set({ loading: true });
    try {
      const capable = (await ipc.usageCapableProviders()) ?? [];
      set({ capable });
      await Promise.all(
        capable.map((entry) => get().refreshProvider(entry.providerId)),
      );
    } catch (error) {
      // A missing command (older engine, browser dev) just means no cards.
      console.error("[loom] usage refresh failed:", error);
    } finally {
      set({ loading: false });
    }
  },

  refreshProvider: async (providerId) => {
    try {
      const usage = await ipc.providerUsage(providerId);
      if (!usage) return;
      set((state) => ({
        byProvider: { ...state.byProvider, [providerId]: usage },
        errors: dropKey(state.errors, providerId),
      }));
    } catch (error) {
      set((state) => ({
        errors: { ...state.errors, [providerId]: messageOf(error) },
      }));
    }
  },

  refreshSummary: async () => {
    set({ summaryLoading: true, summaryError: null });
    try {
      const summary = await ipc.usageSummary();
      if (summary) set({ summary });
    } catch (error) {
      set({ summaryError: messageOf(error) });
    } finally {
      set({ summaryLoading: false });
    }
  },
}));
