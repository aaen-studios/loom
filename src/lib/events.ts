import { useEffect } from "react";
import type { EngineEvent } from "../types";
import type { BrowserWire } from "./ipc";
import { useTauriEvent } from "./listen";
import { useBrowser } from "../stores/browser";
import { useChat } from "../stores/chat";
import { useDock } from "../stores/dock";
import { useEditor } from "../stores/editor";
import { useGit } from "../stores/git";
import { usePty } from "../stores/pty";
import { useTasks } from "../stores/tasks";
import { useUi } from "../stores/ui";
import { useVoice } from "../stores/voice";
import { writeToTerminal } from "./terminals";

/** Payload for `loom://fs`, mirroring `FsWire` in `src-tauri/src/ide.rs`. */
interface FsWire {
  /** Which folder changed; a window showing another one ignores it. */
  workdir: string;
  /** What caused it, for the diagnostics line. Not branchable. */
  reason: string;
}

/** Diagnostics switch: `localStorage.setItem("loomDebug", "1")`. */
function debugEnabled(): boolean {
  try {
    return localStorage.getItem("loomDebug") === "1";
  } catch {
    return false;
  }
}

/* ---------------------------------------------------------------------------
   Every hook below subscribes through `useTauriEvent`, which is what makes the
   app survive StrictMode. The previous shape — `listen(...).then(dispose => …)`
   with a cleanup that ran before the promise resolved — left a **second**
   listener on every channel, so terminal output was written twice and every
   engine event was applied to the stores twice. See `lib/listen.ts`.

   Handlers read their stores through `getState()` rather than a hook, so each
   hook subscribes once and never resubscribes. A handler closed over changing
   state would tear the subscription down on every render, which is exactly the
   kind of churn that makes a double-listener bug intermittent.
--------------------------------------------------------------------------- */

/** Subscribes the chat store to engine events for the app's lifetime. */
export function useEngineEvents(): void {
  useTauriEvent<EngineEvent>("loom://event", (payload) => {
    // Visible in the DevTools console when loomDebug is set.
    if (debugEnabled()) console.debug("[loom] <-", payload.type);

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
    // A tool may have written a file. The editor re-stats what it has open and
    // the git panel refreshes, and neither of them is the chat store's business
    // — so this is routed before the fallthrough that hands everything else to
    // `applyEvent`, which would file it under a key nothing reads.
    if (payload.type === "filesChanged") {
      useEditor.getState().onExternalChange();
      // `void`ed, like every other floating promise in this file: a rejection
      // here would surface as an unhandled promise rejection for what is
      // background work, and `refresh` already swallows its own failures.
      void useGit.getState().refresh();
      return;
    }
    useChat.getState().applyEvent(payload);
  });
}

/* ---------------------------------------------------------------------------
   The dock and the terminal

   Both live on their own channels rather than on `loom://event`, and that
   separation is load-bearing. `loom://event` carries engine events and is
   routed into the chat store; a pty produces output at whatever rate the shell
   decides, which for a progress bar is thousands of chunks a second. Mixing
   them would put terminal traffic through every store subscription in the app.
--------------------------------------------------------------------------- */

interface DockWire {
  workdir: string | null;
  layout: import("../types").DockLayout;
}

interface PtyWire {
  id: string;
  /** Base64 pty bytes; absent on exit. */
  data?: string;
  exit: boolean;
}

/**
 * Decodes a pty chunk.
 *
 * `atob` rather than `TextDecoder`, because the payload is not text: a chunk
 * can end mid-escape-sequence or mid-UTF-8 codepoint, and decoding it as a
 * string would corrupt both. xterm gets the bytes and owns the parser.
 */
function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/**
 * Subscribes the dock and the terminal to their channels.
 *
 * Mounted by the main window *and* by every torn-off panel window. That is what
 * the broadcast design buys: a shell opened from the main window keeps
 * rendering in the window it was dragged into, and closing that window does not
 * end the shell.
 */
export function usePanelEvents(): void {
  useTauriEvent<DockWire>("loom://dock", (payload) => {
    // The store ignores a broadcast for a folder this window is not showing,
    // so a torn-off panel does not adopt another workspace's arrangement.
    useDock.getState().applyRemote(payload.workdir, payload.layout);
  });

  useTauriEvent<PtyWire>("loom://pty", (payload) => {
    const { id, data, exit } = payload;
    if (data) writeToTerminal(id, base64ToBytes(data));
    if (exit) usePty.getState().markExited(id);
  });

  // A folder's files may have moved on.
  //
  // The IDE commands emit this after anything that can change the disk — a save,
  // a checkout, a pull, a discard, a rename, a create, a delete — and the
  // distinction from the engine's `filesChanged` on `loom://event` is *who* did
  // it: that one fires when a tool ran, this one when the user did.
  //
  // Both channels matter, and they are not redundant. A checkout from this
  // window's git panel is not an engine event and never will be, because the
  // engine did not do it.
  useTauriEvent<FsWire>("loom://fs", (payload) => {
    // A broadcast for a folder this window is not showing is not this window's
    // business — the same rule the dock's layout follows.
    if (useEditor.getState().workdir !== payload.workdir) return;
    useEditor.getState().onExternalChange();
    useGit.getState().refresh();
  });
}

/**
 * Subscribes the browser panel to its channel.
 *
 * On its own channel rather than `loom://event`, for the reason the pty has
 * one: a console-emitting page produces traffic at whatever rate the page
 * decides, and it must not be routed through every store subscription in the
 * app. Broadcast rather than targeted because a torn-off browser panel is a
 * second webview with its own heap — so it has to be told, not read shared
 * state.
 */
export function useBrowserEvents(): void {
  useTauriEvent<BrowserWire>("loom://browser", (payload) => {
    useBrowser.getState().applyRemote(payload);
  });
}

/**
 * Subscribes the voice store to its event channels.
 *
 * Stays a store method rather than four hooks, because dictation has to reach
 * the *composer*: the microphone button lives there, and its transcript arrives
 * as a `loom://voice-listen` event after the utterance ends. A listener owned
 * by the voice surface would miss every transcript produced while that surface
 * was closed.
 */
export function useVoiceEvents(): void {
  const attach = useVoice((state) => state.attach);
  const load = useVoice((state) => state.load);
  const loadVoices = useVoice((state) => state.loadVoices);

  useEffect(() => {
    // The settings screen reads these, and the voice surface's picker needs the
    // roster, so both are fetched once at start rather than on each open.
    void load();
    void loadVoices();
    return attach();
  }, [attach, load, loadVoices]);
}

/** Tray menu → open settings, and the launch-time update notice. */
export function useShellEvents(): void {
  useTauriEvent<void>("loom://open-settings", () => {
    useUi.getState().setSettingsOpen(true);
  });

  // Runs is a dock panel now, so this opens the panel rather than an overlay.
  // The tray item and the backend event are unchanged, which is the point of
  // routing it through the dock store instead of a UI flag.
  useTauriEvent<void>("loom://open-tasks", () => {
    useDock.getState().openPanel("runs", "bottom");
    void useTasks.getState().load();
  });

  useTauriEvent<string>("loom://open-session", (sessionId) => {
    if (sessionId) void useChat.getState().openSession(sessionId);
  });

  useTauriEvent<import("../types").UpdateManifest>(
    "loom://update-available",
    (manifest) => {
      if (manifest) useUi.getState().setAvailableUpdate(manifest);
    },
  );

  // The pill's Stop and Ctrl+Alt+Esc end a turn from outside this window, so
  // the composer chip would otherwise keep showing a Resume/Stop pair for a
  // turn that is already over.
  useTauriEvent<string | null>("loom://computer-stopped", (sessionId) => {
    useChat.getState().noteComputerStopped(sessionId ?? null);
  });

  // A detached run, a scheduled job or the tray asking for the browser: the
  // backend cannot open a dock panel itself, so it asks the UI to. Routing it
  // through the dock store rather than a bespoke window call means the panel
  // lands on its own edge and is draggable like every other.
  useTauriEvent<void>("loom://browser-show-panel", () => {
    useDock.getState().openPanel("browser", "right");
    void useBrowser.getState().load();
  });
}
