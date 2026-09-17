import { useEffect, useLayoutEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { backdropFor } from "./lib/background";
import { usePalette } from "./lib/applyPalette";
import { AskOverlay } from "./components/AskOverlay";
import { Background } from "./components/Background";
import { ChatCanvas } from "./components/ChatCanvas";
import { ComputerPill } from "./components/ComputerPill";
import { SettingsPanel } from "./components/SettingsPanel";
import { ShortcutsSheet } from "./components/ShortcutsSheet";
import { TitleBar } from "./components/TitleBar";
import { UpdateToast } from "./components/UpdateToast";
import { VoiceMode } from "./components/VoiceMode";
import { DockHost } from "./dock/DockShell";
import { panelDef } from "./dock/registry";
import { useEngineEvents, usePanelEvents, useShellEvents, useVoiceEvents } from "./lib/events";
import { useGlassVars } from "./lib/glass";
import { raceSafe } from "./lib/listen";
import { useShortcuts } from "./lib/shortcuts";
import { isTauri } from "./lib/tauri";
import { useChat } from "./stores/chat";
import { useDock } from "./stores/dock";
import { useProviders } from "./stores/providers";
import { useSettings } from "./stores/settings";
import { useUsage } from "./stores/usage";

function MainShell() {
  const config = useSettings((state) => state.config);
  const theme = config.theme;
  const load = useSettings((state) => state.load);
  const loadProviders = useProviders((state) => state.load);
  const loadSessions = useChat((state) => state.loadSessions);
  const startUsage = useUsage((state) => state.start);

  useEngineEvents();
  useShellEvents();
  // The dock's layout and the terminal's output, on their own channels.
  usePanelEvents();
  // Voice has to be subscribed for the whole session, not when its surface
  // opens: the composer's microphone button produces transcripts while the
  // surface is closed, and they arrive as events.
  useVoiceEvents();
  useShortcuts();

  // Installs a custom or adaptive palette as custom properties on `<html>`, or
  // clears them for the default. Called here, before the layout effect below,
  // so the tokens are in place before anything paints with them.
  const palette = usePalette(config);

  useEffect(() => {
    void load();
    void loadProviders();
    void loadSessions();
    startUsage();
  }, [load, loadProviders, loadSessions, startUsage]);

  // The quick-ask overlay can create chats while this window is hidden.
  //
  // `raceSafe` rather than a `dispose` variable: this returns a promise, and
  // under StrictMode cleanup runs before it resolves, so the plain shape left a
  // second focus listener attached.
  useEffect(() => {
    if (!isTauri) return;
    return raceSafe(() =>
      getCurrentWindow().onFocusChanged(({ payload }) => {
        if (payload) void loadSessions();
      }),
    );
  }, [loadSessions]);

  // `useLayoutEffect`, not `useEffect`: this toggles the class every colour in
  // the app hangs off, and `useEffect` runs after the browser has painted. The
  // difference is one frame of the wrong theme, which is visible.
  useLayoutEffect(() => {
    const root = document.documentElement;
    const dark = theme === "dark";
    root.classList.toggle("dark", dark);
    // Match the window backdrop so there is no white flash before paint. Only
    // when no palette is installed: a custom palette sets its own backdrop, and
    // overwriting it here would undo it on every theme change.
    if (!palette) root.style.background = backdropFor(dark);
  }, [theme, palette]);

  return (
    <div className="relative h-full w-full overflow-hidden">
      <Background />

      <div className="relative z-10 flex h-full flex-col">
        <TitleBar />
        <main className="relative flex min-h-0 flex-1">
          {/* There is no sidebar element here any more. The chats list is a dock
              panel, so it lives inside `DockHost` with every other panel —
              which is also what lets it be dragged, moved to another edge, or
              torn off into its own window without a second implementation of
              the same list. */}
          <DockHost>
            <ChatCanvas />
          </DockHost>
        </main>
      </div>

      <SettingsPanel />
      {/* The Runs overlay is gone: `TasksPanel` is a dock panel now, reached
          from the title bar or Ctrl+` and able to be torn off. It stays
          imported for a window that docks it. */}
      <VoiceMode />
      <ShortcutsSheet />
      <UpdateToast />
    </div>
  );
}

/**
 * A panel torn off into its own window.
 *
 * Reuses the query-parameter routing `?computer` and `?ask` already use, which
 * is why a torn-off panel needed no new entry point — the window is built with
 * `index.html?panel=<id>` and lands here.
 *
 * The window holds the panel and nothing else: a second full Loom would mean a
 * second chat store and two windows that can disagree about which chat is
 * active. The header exists so the panel can be sent home.
 */
function PanelWindow({ panel }: { panel: string }) {
  const theme = useSettings((state) => state.config.theme);
  const glass = useSettings((state) => state.config.interface.glass);
  // A torn-off window is a second webview with its own document, so it needs the
  // multipliers written onto its own <html> — the main window's are not shared.
  useGlassVars(glass);
  const load = useSettings((state) => state.load);
  const loadSessions = useChat((state) => state.loadSessions);
  const workdir = useChat(
    (state) => state.sessions.find((item) => item.id === state.activeId)?.workdir ?? null,
  );
  const dock = useDock((state) => state.openPanel);
  const def = panelDef(panel);
  usePanelEvents();

  useEffect(() => {
    void load();
    void loadSessions();
  }, [load, loadSessions]);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", theme === "dark");
    root.style.background = theme === "dark" ? "#070a12" : "#eef1f7";
  }, [theme]);

  const Body = def?.render;

  return (
    <div className="relative h-full w-full overflow-hidden">
      <Background />
      <div className="relative z-10 flex h-full flex-col">
        <header className="chrome relative z-20 flex h-11 shrink-0 items-center gap-2 px-3">
          <div className="absolute inset-0 -z-10" data-tauri-drag-region />
          <span className="text-[13px] font-medium">{def?.title ?? panel}</span>
          <button
            type="button"
            onClick={() => {
              // Sending it home docks it back where it was, or opens its zone.
              dock(panel, def?.edge);
              void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
                getCurrentWindow().close(),
              );
            }}
            className="btn-ghost ml-auto px-2.5 py-1 text-[12px]"
          >
            Dock it back
          </button>
        </header>
        <div className="min-h-0 flex-1 p-1">
          <div className="panel h-full overflow-hidden rounded-sheet">
            {Body ? (
              <Body workdir={workdir} />
            ) : (
              <div className="grid h-full place-items-center p-6">
                <p className="text-[12.5px] text-faint">
                  This build has no “{panel}” panel.
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function OverlayShell() {
  const theme = useSettings((state) => state.config.theme);

  useEffect(() => {
    const root = document.documentElement;
    // The overlay follows the app's theme; it is a pane on the desktop, so it
    // never paints a background of its own.
    root.classList.toggle("dark", theme === "dark");
    root.style.background = "transparent";
    document.body.style.background = "transparent";
  }, [theme]);

  return <AskOverlay />;
}

export default function App() {
  const params =
    typeof window !== "undefined"
      ? new URLSearchParams(window.location.search)
      : new URLSearchParams();

  if (params.has("computer")) {
    return <ComputerPill />;
  }
  if (params.has("ask")) {
    return <OverlayShell />;
  }
  // A torn-off panel: the same bundle, one panel, its own window.
  const panel = params.get("panel");
  if (panel) {
    return <PanelWindow panel={panel} />;
  }
  return <MainShell />;
}
