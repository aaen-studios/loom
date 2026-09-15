import { useEffect } from "react";
import { Background } from "./components/Background";
import { ChatCanvas } from "./components/ChatCanvas";
import { SettingsPanel } from "./components/SettingsPanel";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { useSettings } from "./stores/settings";

export default function App() {
  const theme = useSettings((state) => state.config.theme);
  const load = useSettings((state) => state.load);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", theme === "dark");
    // Match the window backdrop so there is no white flash before paint.
    root.style.background = theme === "dark" ? "#0b0d12" : "#eef1f7";
  }, [theme]);

  return (
    <div className="relative flex h-full w-full overflow-hidden">
      <Background />
      <TitleBar />
      <div className="relative flex min-w-0 flex-1 pt-14">
        <Sidebar />
        <ChatCanvas />
      </div>
      <SettingsPanel />
    </div>
  );
}
