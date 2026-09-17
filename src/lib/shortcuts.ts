import { useEffect } from "react";
import { useChat } from "../stores/chat";
import { useDock } from "../stores/dock";
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
 * - Ctrl+K       the chats list (focuses its search when it is on screen)
 * - Ctrl+,       settings
 * - Ctrl+`       the dock, on or off
 * - Ctrl+End     jump to the newest text
 * - Ctrl+Shift+V voice mode, on or off
 * - ?            the shortcut sheet
 *
 * Per-message navigation lives in the transcript itself.
 */
export function useShortcuts(): void {
  const newSession = useChat((state) => state.newSession);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const setShortcutsOpen = useUi((state) => state.setShortcutsOpen);
  const setVoiceOpen = useUi((state) => state.setVoiceOpen);
  const settingsOpen = useUi((state) => state.settingsOpen);
  const shortcutsOpen = useUi((state) => state.shortcutsOpen);
  const voiceOpen = useUi((state) => state.voiceOpen);
  const theme = useSettings((state) => state.config.theme);
  const setTheme = useSettings((state) => state.setTheme);
  const toggleDock = useDock((state) => state.toggleDock);

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
          // The chats list is navigation now, not an overlay, so starting a chat
          // leaves it alone. It used to close, because a popup sitting on top of
          // the new chat was in the way.
          void newSession();
        } else if (key === "k") {
          event.preventDefault();
          // On screen already: jump straight to the search. Otherwise open the
          // chats panel first and focus a frame later — the input does not exist
          // until it has rendered, which is why the focus is deferred either way.
          if (!focusSidebarSearch()) {
            useDock.getState().openPanel("sessions", "left");
            requestAnimationFrame(() => focusSidebarSearch());
          }
        } else if (key === ",") {
          event.preventDefault();
          setSettingsOpen(true);
        } else if (event.key === "`" || key === "backquote") {
          // The dock, from anywhere — including from inside the terminal.
          //
          // Deliberately here, in the modified branch and with no `isTyping`
          // guard, which is the opposite of what the other shortcuts do. xterm
          // keeps a hidden textarea for input, so `isTyping` reads every
          // keystroke in the terminal as "the user is typing" — a guard would
          // swallow this exactly where it is most wanted, and `Ctrl+backtick`
          // is not a text-editing habit anyone has.
          event.preventDefault();
          toggleDock();
        } else if (event.key === "End") {
          event.preventDefault();
          document
            .querySelector<HTMLElement>("[data-latest]")
            ?.scrollIntoView({ behavior: "smooth", block: "end" });
        } else if (event.shiftKey && key === "d") {
          // A quick way to flip themes while designing.
          event.preventDefault();
          setTheme(theme === "dark" ? "light" : "dark");
        } else if (event.shiftKey && key === "v" && !isTyping(event.target)) {
          // Voice mode, on the same `Ctrl+Shift+<letter>` pattern as the theme
          // flip. A toggle rather than an open, so the same keys get you out —
          // and it closes Settings first, because two full-screen surfaces at
          // once is one too many.
          //
          // The `isTyping` guard is not decoration: `Ctrl+Shift+V` is
          // "paste as plain text" in most editors, so without it a paste in
          // the composer would throw a full-screen surface over what was just
          // pasted. None of the *other* modified shortcuts need this because
          // none of them collide with a text-editing habit.
          event.preventDefault();
          setSettingsOpen(false);
          setShortcutsOpen(false);
          setVoiceOpen(!voiceOpen);
        }
        return;
      }

      if (event.key === "?" && !isTyping(event.target)) {
        event.preventDefault();
        setShortcutsOpen(!shortcutsOpen);
        return;
      }

      // Voice mode is absent here on purpose: it handles its own Escape, in
      // its own listener. Closing it from two places would also mean two
      // reasons to check its state on every keystroke in the app.
      if (event.key === "Escape" && (settingsOpen || shortcutsOpen)) {
        setSettingsOpen(false);
        setShortcutsOpen(false);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    newSession,
    setSettingsOpen,
    setShortcutsOpen,
    setVoiceOpen,
    settingsOpen,
    shortcutsOpen,
    voiceOpen,
    theme,
    setTheme,
    toggleDock,
  ]);
}
