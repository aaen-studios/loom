import { useEffect } from "react";
import { useChat } from "../stores/chat";
import { useUi } from "../stores/ui";

/**
 * Global keyboard shortcuts:
 * - Ctrl+N       new chat
 * - Ctrl+K       chats popup
 * - Ctrl+,       settings
 */
export function useShortcuts(): void {
  const newSession = useChat((state) => state.newSession);
  const setSidebarOpen = useUi((state) => state.setSidebarOpen);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const sidebarOpen = useUi((state) => state.sidebarOpen);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      const key = event.key.toLowerCase();

      if (key === "n") {
        event.preventDefault();
        void newSession();
        setSidebarOpen(false);
      } else if (key === "k") {
        event.preventDefault();
        setSidebarOpen(!sidebarOpen);
      } else if (key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [newSession, setSidebarOpen, setSettingsOpen, sidebarOpen]);
}
