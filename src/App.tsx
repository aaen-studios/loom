import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AskOverlay } from "./components/AskOverlay";
import { Background } from "./components/Background";
import { ChatCanvas } from "./components/ChatCanvas";
import { ComputerPill } from "./components/ComputerPill";
import { SettingsPanel } from "./components/SettingsPanel";
import { ShortcutsSheet } from "./components/ShortcutsSheet";
import { SidebarPopup } from "./components/Sidebar";
import { TasksPanel } from "./components/TasksPanel";
import { TitleBar } from "./components/TitleBar";
import { UpdateToast } from "./components/UpdateToast";
import { useEngineEvents, useShellEvents } from "./lib/events";
import { useShortcuts } from "./lib/shortcuts";
import { isTauri } from "./lib/tauri";
import { useChat } from "./stores/chat";
import { useProviders } from "./stores/providers";
import { useSettings } from "./stores/settings";
import { useUsage } from "./stores/usage";

function MainShell() {
  const theme = useSettings((state) => state.config.theme);
  const load = useSettings((state) => state.load);
  const loadProviders = useProviders((state) => state.load);
  const loadSessions = useChat((state) => state.loadSessions);
  const startUsage = useUsage((state) => state.start);

  useEngineEvents();
  useShellEvents();
  useShortcuts();

  useEffect(() => {
    void load();
    void loadProviders();
    void loadSessions();
    startUsage();
  }, [load, loadProviders, loadSessions, startUsage]);

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
      <TasksPanel />
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
  const params =
    typeof window !== "undefined"
      ? new URLSearchParams(window.location.search)
      : new URLSearchParams();

  if (params.has("computer")) {
    return <ComputerPill />;
  }
  return params.has("ask") ? <OverlayShell /> : <MainShell />;
}
