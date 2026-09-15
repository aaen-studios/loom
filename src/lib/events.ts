import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { EngineEvent } from "../types";
import { isTauri } from "./tauri";
import { useChat } from "../stores/chat";
import { useUi } from "../stores/ui";

/** Diagnostics switch: `localStorage.setItem("loomDebug", "1")`. */
function debugEnabled(): boolean {
  try {
    return localStorage.getItem("loomDebug") === "1";
  } catch {
    return false;
  }
}

/** Subscribes the chat store to engine events for the app's lifetime. */
export function useEngineEvents(): void {
  const applyEvent = useChat((state) => state.applyEvent);

  useEffect(() => {
    if (!isTauri) return;
    let dispose: (() => void) | undefined;
    void listen<EngineEvent>("loom://event", (event) => {
      // Visible in the DevTools console when loomDebug is set.
      if (debugEnabled()) console.debug("[loom] <-", event.payload.type);
      applyEvent(event.payload);
    })
      .then((unlisten) => {
        dispose = unlisten;
      })
      .catch((error) => console.error("[loom] event listener failed:", error));
    return () => dispose?.();
  }, [applyEvent]);
}

/** Tray menu → open settings, and the launch-time update notice. */
export function useShellEvents(): void {
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const setAvailableUpdate = useUi((state) => state.setAvailableUpdate);

  useEffect(() => {
    if (!isTauri) return;
    const disposers: Array<() => void> = [];

    void listen("loom://open-settings", () => setSettingsOpen(true)).then(
      (unlisten) => disposers.push(unlisten),
    );
    void listen<import("../types").UpdateManifest>(
      "loom://update-available",
      (event) => {
        if (event.payload) setAvailableUpdate(event.payload);
      },
    ).then((unlisten) => disposers.push(unlisten));

    return () => disposers.forEach((dispose) => dispose());
  }, [setSettingsOpen, setAvailableUpdate]);
}
