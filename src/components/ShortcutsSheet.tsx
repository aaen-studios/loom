import { cn } from "../lib/cn";
import { useUi } from "../stores/ui";
import { CloseIcon } from "./icons";
import { LiquidSurface } from "./LiquidSurface";

interface Shortcut {
  keys: string;
  action: string;
}

const SHORTCUTS: { group: string; items: Shortcut[] }[] = [
  {
    group: "Panels",
    items: [
      { keys: "Ctrl+`", action: "The dock, on or off" },
      { keys: "Ctrl+`", action: "Close every panel once the dock is showing" },
      { keys: "Drag a tab", action: "Reorder panels, or drag one to another edge" },
      { keys: "Drag outside", action: "Give a panel its own window" },
    ],
  },
  {
    group: "Chats",
    items: [
      { keys: "Ctrl+N", action: "New chat" },
      { keys: "Ctrl+K", action: "The chats list, with its search focused" },
      { keys: "Ctrl+,", action: "Settings" },
      { keys: "Ctrl+Shift+Space", action: "Quick-ask overlay from anywhere" },
      { keys: "Ctrl+Alt+Esc", action: "Stop the computer turn, from anywhere" },
    ],
  },
  {
    group: "Reading",
    items: [
      { keys: "Ctrl+End", action: "Jump to the newest text" },
      { keys: "Alt+↑ / Alt+↓", action: "Select the previous / next message" },
      { keys: "C", action: "Copy the selected message" },
      { keys: "E", action: "Edit the selected message (yours)" },
      { keys: "R", action: "Regenerate the selected reply" },
      { keys: "Shift+Delete", action: "Delete the selected message" },
    ],
  },
  {
    group: "Voice",
    items: [
      { keys: "Ctrl+Shift+V", action: "Voice mode: speak and listen" },
      { keys: "Esc", action: "Leave voice mode" },
    ],
  },
  {
    group: "Composer",
    items: [
      { keys: "Enter", action: "Send (or Ctrl+Enter, your choice in Settings)" },
      { keys: "Shift+Enter", action: "New line" },
      { keys: "Enter (while replying)", action: "Queue the message" },
      { keys: "/", action: "Commands, skills and saved prompts" },
      { keys: "#", action: "Refer to another chat (the id is added on send)" },
      { keys: "@", action: "Refer to a file in this workspace" },
      { keys: "Esc", action: "Stop the running reply, deselect" },
      { keys: "?", action: "This list" },
    ],
  },
  {
    group: "Editor",
    items: [
      // There is deliberately no `Ctrl+I` here. Monaco claims a large share of
      // the `Ctrl+<letter>` space for itself — `Ctrl+I` is italic in several
      // grammars, `Ctrl+P` is its own quick-open, `Ctrl+F` its find — and a
      // global binding that silently loses to an editor-local one is worse than
      // no binding at all, because it works everywhere except where you want it.
      // So IDE mode is a button in the title bar and a row in Settings, and the
      // keys below are the ones the editor does not already own.
      { keys: "Ctrl+S", action: "Save the file (it also saves as you pause)" },
      { keys: "Ctrl+Z / Ctrl+Shift+Z", action: "Undo / redo in the editor" },
      { keys: "Ctrl+F", action: "Find in the open file" },
      { keys: "Ctrl+D", action: "Select the next occurrence" },
      { keys: "Alt+↑ / Alt+↓", action: "Move the current line" },
      { keys: "Middle-click a tab", action: "Close it" },
      { keys: "Right-click a tab", action: "Close, or close the others" },
    ],
  },
  {
    group: "Git",
    items: [
      // Ctrl+K is the app's chats list, so the commit key is Ctrl+Enter — the
      // same chord that queues a message when the composer has focus, which is
      // the closest thing to a habit to hang this on.
      { keys: "Ctrl+Enter", action: "Commit what is staged (in the commit box)" },
      { keys: "Click a changed file", action: "Open its diff beside the panel" },
      { keys: "Drag a splitter", action: "Resize the git or editor column" },
      { keys: "Double-click a splitter", action: "Reset it to its default width" },
    ],
  },
];

/** Overlay listing every shortcut; opened with `?` or from Settings. */
export function ShortcutsSheet() {
  const open = useUi((state) => state.shortcutsOpen);
  const setOpen = useUi((state) => state.setShortcutsOpen);

  if (!open) return null;

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center p-6">
      <button
        type="button"
        aria-label="Close shortcuts"
        onClick={() => setOpen(false)}
        className="absolute inset-0 cursor-default bg-black/20"
      />
      <LiquidSurface
        surface="overlays"
        layout="column"
        tint="var(--panel-bg-strong)" className="animate-fade-up relative flex max-h-full w-[460px] flex-col overflow-hidden rounded-sheet">
        <div className="flex items-center justify-between px-4 py-3">
          <h2 className="text-[14.5px] font-semibold">Keyboard shortcuts</h2>
          <button
            type="button"
            aria-label="Close shortcuts"
            onClick={() => setOpen(false)}
            className="hover-surface grid h-8 w-8 place-items-center rounded-control text-soft"
          >
            <CloseIcon size={16} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-4">
          {SHORTCUTS.map((section) => (
            <div key={section.group} className="mb-4 last:mb-0">
              <h3 className="mb-1.5 text-[11.5px] font-semibold tracking-[0.08em] text-faint uppercase">
                {section.group}
              </h3>
              <div className="space-y-1">
                {section.items.map((item) => (
                  <div key={item.keys} className="flex items-baseline justify-between gap-4">
                    <span className="min-w-0 text-[13px] text-soft">{item.action}</span>
                    <kbd
                      className={cn(
                        "shrink-0 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2 py-0.5",
                        "font-mono text-[11.5px] text-[var(--ink)]",
                      )}
                    >
                      {item.keys}
                    </kbd>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </LiquidSurface>
    </div>
  );
}
