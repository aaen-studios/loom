import { useEffect, useMemo, useState } from "react";
import { folderName } from "../lib/workspaces";
import { useChat } from "../stores/chat";
import { useWorkspaceFiles } from "../stores/workspaceFiles";
import { GoalPanel } from "./GoalPanel";
import { Sidebar } from "./Sidebar";
import { TasksPanel } from "./TasksPanel";
import { EmptyState, SearchField } from "./ui";

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
 * Files in the workspace.
 *
 * Reuses the composer's cached file list rather than walking the tree again, so
 * opening this panel costs nothing when the `@` picker has already run — and a
 * second walk would be a second thing that can disagree with the first.
 *
 * No diff view yet: the backend exposes a file list, not a change set, and
 * deriving a diff from a list of paths would be inventing one.
 */
export function FilesPanel({ workdir }: { workdir: string | null }) {
  const files = useWorkspaceFiles((state) => state.files);
  const load = useWorkspaceFiles((state) => state.load);
  const loading = useWorkspaceFiles((state) => state.loading);
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    void load(workdir);
  }, [load, workdir]);

  const needle = query.trim().toLowerCase();
  const shown = useMemo(() => {
    if (!needle) return files;
    return files.filter((file) => file.toLowerCase().includes(needle));
  }, [files, needle]);

  if (!workdir) {
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[240px] text-center text-[12.5px] leading-5 text-faint">
          This chat has no workspace folder. Pick one from the workspace chip and
          its files appear here.
        </p>
      </div>
    );
  }

  const copy = (path: string) => {
    void navigator.clipboard?.writeText(path);
    setCopied(path);
    window.setTimeout(
      () => setCopied((current) => (current === path ? null : current)),
      1200,
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 px-2 pt-2 pb-1.5">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder={`Search ${folderName(workdir)}…`}
        />
      </div>

      {loading && files.length === 0 && (
        <p className="px-3 py-2 text-[12.5px] text-faint">Reading the workspace…</p>
      )}

      {!loading && files.length === 0 && (
        <div className="p-3">
          <EmptyState
            title="No files indexed"
            hint="Loom could not read this folder, or it is empty."
          />
        </div>
      )}

      {files.length > 0 && shown.length === 0 && (
        <p className="px-3 py-2 text-[12.5px] text-faint">No matches.</p>
      )}

      <ul className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
        {shown.slice(0, 800).map((file) => (
          <li key={file}>
            <button
              type="button"
              onClick={() => copy(file)}
              title={`Copy path — ${file}`}
              className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1 text-left"
            >
              <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-soft">
                {file}
              </span>
              {copied === file && (
                <span className="shrink-0 text-[10.5px] text-[var(--accent)]">
                  copied
                </span>
              )}
            </button>
          </li>
        ))}
      </ul>

      {shown.length > 800 && (
        <p className="shrink-0 border-t border-[var(--glass-border)] px-3 py-1.5 text-[11px] text-faint">
          Showing the first 800 of {shown.length}. Narrow the search to see the rest.
        </p>
      )}
    </div>
  );
}

/**
 * The browser, as a placeholder.
 *
 * Registered so the docking system already carries a panel it knows nothing
 * about. When the real one lands it is a component swap in `registry.tsx` and
 * nothing else.
 */
export function BrowserPanel() {
  return (
    <div className="grid h-full place-items-center p-6">
      <div className="max-w-[280px] text-center">
        <p className="text-[13px] text-soft">The browser panel is not built yet.</p>
        <p className="mt-1.5 text-[12px] leading-5 text-faint">
          Its slot exists so that when it arrives it docks, tabs and tears off
          like every panel beside it — with no change to the layout.
        </p>
      </div>
    </div>
  );
}
