import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "./tauri";

function currentWindow() {
  return isTauri ? getCurrentWindow() : null;
}

export async function minimizeWindow(): Promise<void> {
  await currentWindow()?.minimize();
}

export async function toggleMaximizeWindow(): Promise<void> {
  await currentWindow()?.toggleMaximize();
}

export async function closeWindow(): Promise<void> {
  await currentWindow()?.close();
}

export async function isWindowMaximized(): Promise<boolean> {
  return (await currentWindow()?.isMaximized()) ?? false;
}

/**
 * Subscribes to resize events and reports the current maximized state.
 * Returns an unlisten function; no-op outside Tauri.
 */
export function onWindowResized(handler: (maximized: boolean) => void): () => void {
  const win = currentWindow();
  if (!win) return () => {};
  let dispose: (() => void) | undefined;
  void win
    .onResized(() => {
      void isWindowMaximized().then(handler);
    })
    .then((unlisten) => {
      dispose = unlisten;
    });
  return () => dispose?.();
}
