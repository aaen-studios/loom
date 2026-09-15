import { invoke, convertFileSrc } from "@tauri-apps/api/core";

/** True when running inside the Tauri webview (false in a plain browser tab). */
export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Typed, failure-tolerant wrapper around `invoke`.
 *
 * In the browser (vite dev without Tauri) and on command errors it resolves to
 * `null` instead of throwing, so the UI can fall back to local defaults.
 */
export async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri) return null;
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    console.error(`[loom] command "${cmd}" failed:`, error);
    return null;
  }
}

/** Turns an absolute path under an allowed asset scope into a webview URL. */
export function assetUrl(path: string): string {
  return isTauri ? convertFileSrc(path) : path;
}
