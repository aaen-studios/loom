import { useEffect } from "react";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";

/** True when the event came from somewhere the user is typing. */
function isTyping(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  const tag = element.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    element.isContentEditable
  );
}

/**
 * Global keyboard shortcuts.
 *
 * - Ctrl+N       new chat
 * - Ctrl+K       chats popup (focuses its search when the list is visible)
 * - Ctrl+,       settings
 * - Ctrl+End     jump to the newest text
 * - ?            the shortcut sheet
 *
 * Per-message navigation lives in the transcript itself.
 */
export function useShortcuts(): void {
  const newSession = useChat((state) => state.newSession);
  const setSidebarOpen = useUi((state) => state.setSidebarOpen);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const setShortcutsOpen = useUi((state) => state.setShortcutsOpen);
  const settingsOpen = useUi((state) => state.settingsOpen);
  const shortcutsOpen = useUi((state) => state.shortcutsOpen);
  const theme = useSettings((state) => state.config.theme);
  const setTheme = useSettings((state) => state.setTheme);

  useEffect(() => {
    const focusSidebarSearch = () => {
      const input = document.getElementById(
        "sidebar-search",
      ) as HTMLInputElement | null;
      if (input) {
        input.focus();
        input.select();
        return true;
      }
      return false;
    };

    const onKey = (event: KeyboardEvent) => {
      const modified = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();

      if (modified) {
        if (key === "n") {
          event.preventDefault();
          void newSession();
          setSidebarOpen(false);
        } else if (key === "k") {
          event.preventDefault();
          // Visible already (open or pinned): jump straight to the search.
          if (!focusSidebarSearch()) {
            setSidebarOpen(true);
            requestAnimationFrame(() => focusSidebarSearch());
          }
        } else if (key === ",") {
          event.preventDefault();
          setSettingsOpen(true);
        } else if (event.key === "End") {
          event.preventDefault();
          document
            .querySelector<HTMLElement>("[data-latest]")
            ?.scrollIntoView({ behavior: "smooth", block: "end" });
        } else if (event.shiftKey && key === "d") {
          // A quick way to flip themes while designing.
          event.preventDefault();
          setTheme(theme === "dark" ? "light" : "dark");
        }
        return;
      }

      if (event.key === "?" && !isTyping(event.target)) {
        event.preventDefault();
        setShortcutsOpen(!shortcutsOpen);
        return;
      }

      if (event.key === "Escape" && (settingsOpen || shortcutsOpen)) {
        setSettingsOpen(false);
        setShortcutsOpen(false);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    newSession,
    setSidebarOpen,
    setSettingsOpen,
    setShortcutsOpen,
    settingsOpen,
    shortcutsOpen,
    theme,
    setTheme,
  ]);
}
