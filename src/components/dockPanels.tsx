import { useChat } from "../stores/chat";
import { FileTree } from "./FileTree";
import { GoalPanel } from "./GoalPanel";
import { Sidebar } from "./Sidebar";
import { TasksPanel } from "./TasksPanel";

/**
 * The panels, as the dock sees them.
 *
 * Each of these is a thin adapter: the real surface is an existing component,
 * and the only thing happening here is giving it a shape that fills a zone
 * rather than assuming it owns the window. Keeping the adapters in one file is
 * deliberate — this is the complete list of what had to change to make an
 * overlay into a panel, which is the sort of thing that is worth being able to
 * read in one screen.
 */

/** Runs: the same panel that used to be a popup, now living in the dock. */
export function RunsPanel() {
  return <TasksPanel />;
}

/** Chats: the sessions list, as the dock's left panel. */
export function SessionsPanel() {
  return <Sidebar />;
}

/**
 * The goal and task list.
 *
 * Not in any default layout on purpose: the composer keeps it, where it has
 * always been. This is the panel for a chat with a long task list, where
 * watching it beside the transcript beats watching it inside it.
 */
export function GoalDockPanel() {
  const activeId = useChat((state) => state.activeId);
  const goal = useChat((state) =>
    state.activeId ? (state.goals[state.activeId] ?? null) : null,
  );
  // Select a boolean, never the array itself. This previously read
  // `state.todos[state.activeId] ?? []`, which builds a fresh array on every
  // store read; useSyncExternalStore compares snapshots by reference, so it saw
  // "changed" every time, re-rendered, read again, and spun until React threw
  // "Maximum update depth exceeded". GoalPanel and Composer dodge this with a
  // hoisted NO_TODOS; only emptiness is needed here, so a boolean is simpler
  // and cannot regress.
  const hasTodos = useChat((state) =>
    state.activeId ? (state.todos[state.activeId]?.length ?? 0) > 0 : false,
  );

  if (!activeId || (!goal && !hasTodos)) {
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[240px] text-center text-[12.5px] leading-5 text-faint">
          {activeId
            ? "This chat has no goal or task list yet. Ask for a plan and it will appear here."
            : "Open a chat to see its goal and tasks."}
        </p>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-2">
      <GoalPanel />
    </div>
  );
}

/**
 * Files in the workspace, as a tree you can open files from.
 *
 * This used to be a flat list of every path that copied one to the clipboard on
 * click, and its own comment said why there was no more: "the backend exposes a
 * file list, not a change set, and deriving a diff from a list of paths would be
 * inventing one." That is still true of a *list*. It stopped being the whole
 * story when the backend grew `dir_list`, `file_read` and `git_diff` — there is
 * now a change set to show, and a file to open rather than a path to copy.
 *
 * The composer's `@` picker keeps the flat list, and should: it needs to *search*
 * every path, which is the opposite of what a tree that expands on demand needs.
 * Two callers, two shapes, one backend.
 */
export function FilesPanel({ workdir }: { workdir: string | null }) {
  return <FileTree workdir={workdir} />;
}

/**
 * The browser: Loom's own, in a dock zone.
 *
 * The real component lives in `BrowserPanel.tsx` and is re-exported here rather
 * than moved, because this file is the complete list of what had to change to
 * turn an overlay into a panel — and the browser is the entry that proves the
 * list is *only* adapters now. Its slot existed before the feature did, so
 * docking, tearing off and dragging between edges were already exercised by the
 * time there was anything to put in them.
 */
export { BrowserPanel } from "./BrowserPanel";

/**
 * Git, and the editor that shows what it changed.
 *
 * Re-exported rather than wrapped, unlike the adapters above: these two were
 * written *as* panels, so there is no shape to convert. `FilesPanel` was the
 * one component that had to change — it was a flat list of paths that copied on
 * click, and it is now `FileTree`, an expandable tree that opens files.
 */
export { GitPanel } from "./GitPanel";
export { EditorPanel, EditorTabs } from "./EditorPanel";
