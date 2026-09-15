import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AskOverlay } from "./components/AskOverlay";
import { Background } from "./components/Background";
import { ChatCanvas } from "./components/ChatCanvas";
import { SettingsPanel } from "./components/SettingsPanel";
import { ShortcutsSheet } from "./components/ShortcutsSheet";
import { SidebarPopup } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { UpdateToast } from "./components/UpdateToast";
import { useEngineEvents, useShellEvents } from "./lib/events";
import { useShortcuts } from "./lib/shortcuts";
import { isTauri } from "./lib/tauri";
import { useChat } from "./stores/chat";
import { useProviders } from "./stores/providers";
import { useSettings } from "./stores/settings";

function MainShell() {
  const theme = useSettings((state) => state.config.theme);
  const load = useSettings((state) => state.load);
  const loadProviders = useProviders((state) => state.load);
  const loadSessions = useChat((state) => state.loadSessions);

  useEngineEvents();
  useShellEvents();
  useShortcuts();

  useEffect(() => {
    void load();
    void loadProviders();
    void loadSessions();
  }, [load, loadProviders, loadSessions]);

  // The quick-ask overlay can create chats while this window is hidden.
  useEffect(() => {
    if (!isTauri) return;
    let dispose: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload }) => {
        if (payload) void loadSessions();
      })
      .then((unlisten) => {
        dispose = unlisten;
      });
    return () => dispose?.();
  }, [loadSessions]);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", theme === "dark");
    // Match the window backdrop so there is no white flash before paint.
    root.style.background = theme === "dark" ? "#070a12" : "#eef1f7";
  }, [theme]);

  return (
    <div className="relative h-full w-full overflow-hidden">
      <Background />

      <div className="relative z-10 flex h-full flex-col">
        <TitleBar />
        <main className="flex min-h-0 flex-1">
          <SidebarPopup />
          <ChatCanvas />
        </main>
      </div>

      <SettingsPanel />
      <ShortcutsSheet />
      <UpdateToast />
    </div>
  );
}

function OverlayShell() {
  useEffect(() => {
    const root = document.documentElement;
    root.classList.add("dark");
    root.style.background = "transparent";
    document.body.style.background = "transparent";
  }, []);

  return <AskOverlay />;
}

export default function App() {
  const isOverlay =
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).has("ask");

  return isOverlay ? <OverlayShell /> : <MainShell />;
}
