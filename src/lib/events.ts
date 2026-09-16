import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { EngineEvent } from "../types";
import { isTauri } from "./tauri";
import { useChat } from "../stores/chat";
import { useTasks } from "../stores/tasks";
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
      const payload = event.payload;
      // Runs and jobs have their own store; the chat store never sees them.
      if (payload.type === "taskChanged") {
        useTasks.getState().applyTask(payload.task);
        return;
      }
      if (payload.type === "commandChanged") {
        useTasks.getState().applyCommand(payload.command);
        return;
      }
      if (payload.type === "jobChanged") {
        useTasks.getState().applyJob(payload.job);
        return;
      }
      if (payload.type === "memoryChanged") {
        useTasks.getState().setMemoryNotice({
          scope: payload.scope,
          added: payload.added,
          sessionId: payload.sessionId,
        });
        return;
      }
      applyEvent(payload);
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
  const setTasksOpen = useUi((state) => state.setTasksOpen);
  const setAvailableUpdate = useUi((state) => state.setAvailableUpdate);
  const openSession = useChat((state) => state.openSession);

  useEffect(() => {
    if (!isTauri) return;
    const disposers: Array<() => void> = [];

    void listen("loom://open-settings", () => setSettingsOpen(true)).then(
      (unlisten) => disposers.push(unlisten),
    );
    void listen("loom://open-tasks", () => {
      setTasksOpen(true);
      void useTasks.getState().load();
    }).then((unlisten) => disposers.push(unlisten));
    void listen<string>("loom://open-session", (event) => {
      if (event.payload) void openSession(event.payload);
    }).then((unlisten) => disposers.push(unlisten));
    void listen<import("../types").UpdateManifest>(
      "loom://update-available",
      (event) => {
        if (event.payload) setAvailableUpdate(event.payload);
      },
    ).then((unlisten) => disposers.push(unlisten));

    return () => disposers.forEach((dispose) => dispose());
  }, [setSettingsOpen, setTasksOpen, setAvailableUpdate, openSession]);
}
