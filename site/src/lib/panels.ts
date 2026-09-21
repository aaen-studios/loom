/**
 * The panels, in one place.
 *
 * Eight of them, and the list is read by three things: the capability grid in the second movement,
 * the count in the footer, and `verify-pages.mjs`. A ninth panel in the application arrives on this
 * page or the next verification run says so.
 *
 * The `role` is one line, and it is a line rather than a paragraph on purpose. A capability grid is
 * scannable or it is nothing: a reader looking for "does it have a terminal" wants to find the word
 * *Terminal* and stop, not read a sentence that mentions it in the second clause.
 */
export interface Panel {
  id: string;
  /** The application's own name for the panel. Short, and capitalised as the tab is. */
  name: string;
  /** One line: what the panel is for. */
  role: string;
}

export const PANELS: readonly Panel[] = [
  {
    id: "terminal",
    name: "Terminal",
    role: "A real pty — colours, resize, arrow keys, full-screen programs, and all sixteen ANSI colours defined per theme.",
  },
  {
    id: "editor",
    name: "Editor",
    role: "Monaco, with changed files opened as exact diffs rather than as patch text.",
  },
  {
    id: "git",
    name: "Git",
    role: "Driven through your own git binary, so credentials, hooks, aliases and signing all keep working.",
  },
  {
    id: "files",
    name: "Files",
    role: "The workspace tree, with changed-file marks that match the git panel beside it.",
  },
  {
    id: "chat",
    name: "Chat",
    role: "The conversation itself: reasoning, tool calls and the reply, in the order they happened.",
  },
  {
    id: "chats",
    name: "Chats",
    role: "Every conversation, grouped by the folder it belongs to.",
  },
  {
    id: "runs",
    name: "Runs",
    role: "Commands that outlived their turn, still streaming to a log, each with a stop button.",
  },
  {
    id: "goal",
    name: "Goal",
    role: "The objective and the live task list the model keeps checked off as it goes.",
  },
] as const;

/** Panels by id, for a lookup that cannot come back `undefined` twice. */
export const PANEL_BY_ID: Record<string, Panel> = Object.fromEntries(
  PANELS.map((panel) => [panel.id, panel]),
);
