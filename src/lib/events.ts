import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { EngineEvent } from "../types";
import { isTauri } from "./tauri";
import { useChat } from "../stores/chat";
import { useTasks } from "../stores/tasks";
import { useUi } from "../stores/ui";
import { useVoice } from "../stores/voice";

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

/**
 * Subscribes the voice store to its two event channels, for the app's lifetime.
 *
 * This has to be an event hook rather than something the voice UI does when it
 * mounts, and that is not a style choice — it is what makes dictation reach the
 * composer at all. The microphone button lives in the composer, and its
 * transcript arrives as a `loom://voice-listen` event *after* the utterance
 * ends. A listener attached by the voice surface would miss every transcript
 * produced while that surface was closed.
 *
 * `attach` is called for its side-effect and returns its own disposer rather
 * than being an effect body, because the two `listen` calls resolve
 * asynchronously: an effect that returned early would leak whichever had
 * already resolved.
 */
export function useVoiceEvents(): void {
  const attach = useVoice((state) => state.attach);
  const load = useVoice((state) => state.load);
  const loadVoices = useVoice((state) => state.loadVoices);

  useEffect(() => {
    // The settings screen reads these, and the voice surface's voice picker
    // needs the roster, so both are fetched once at start rather than on each
    // open.
    void load();
    void loadVoices();
    return attach();
  }, [attach, load, loadVoices]);
}

/** Tray menu → open settings, and the launch-time update notice. */export function useShellEvents(): void {
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
    // The pill's Stop and Ctrl+Alt+Esc end a turn from outside this window, so
    // the composer chip would otherwise keep showing a Resume/Stop pair for a
    // turn that is already over. The engine emits this but nothing listened.
    void listen<string | null>("loom://computer-stopped", (event) => {
      useChat.getState().noteComputerStopped(event.payload ?? null);
    }).then((unlisten) => disposers.push(unlisten));

    return () => disposers.forEach((dispose) => dispose());
  }, [setSettingsOpen, setTasksOpen, setAvailableUpdate, openSession]);
}
