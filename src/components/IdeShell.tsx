import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { ChatCanvas } from "./ChatCanvas";
import { EditorTabs } from "./EditorPanel";
import { FileTree } from "./FileTree";
import { GitPanel } from "./GitPanel";
import { ResizeHandle } from "./ui";
import { useChat } from "../stores/chat";
import { useEditor } from "../stores/editor";
import { useGit } from "../stores/git";
import { useSettings } from "../stores/settings";
import { ChatIcon, FolderIcon, GitBranchIcon, PanelLeftIcon } from "./icons";

/**
 * IDE mode: the whole surface, with the chat as a column.
 *
 * ## Why this is a swap of the dock's centre child
 *
 * `App.tsx` renders `<DockHost>{ideMode ? <IdeShell/> : <ChatCanvas/>}</DockHost>`.
 * Everything the dock does — the zones, the splitters, the tab stacks, tear-off,
 * `Ctrl+`` — therefore keeps working unchanged, and a torn-off terminal or a
 * bottom Runs panel behaves identically in both modes. A second application shell
 * would have had to reimplement all of it, and would then have drifted.
 *
 * ## Why the chat is a column rather than gone
 *
 * Because the model is the reason this application exists. An editor that removes
 * the composer makes you leave it to ask a question, which is the opposite of what
 * a chat-first editor should be. So the chat narrows to a column you can widen,
 * and collapses entirely if you want the screen — but it is a deliberate choice
 * either way, not a mode boundary.
 *
 * ## Why the sidebar holds the *same* components
 *
 * `FileTree` and `GitPanel` are the same components the dock renders. That is the
 * point: there is one file tree and one git panel, and this is a second place to
 * put them. A bespoke IDE file list would drift from the dock's within a week.
 */
export function IdeShell() {
  const config = useSettings((state) => state.config);
  const setInterface = useSettings((state) => state.setInterface);

  /**
   * The folder this chat is showing — from the **chat**, not from the editor.
   *
   * This was `useEditor((state) => state.workdir)` and that was a real bug with
   * a confusing symptom: the sidebar said "this chat has no workspace folder"
   * for a chat that plainly had one. The editor store's `workdir` is set by
   * `EditorPanel` — the *dock panel* — and in IDE mode that component is never
   * mounted, so the field stayed null while everything else in the app knew the
   * folder perfectly well.
   *
   * The chat's session is the source of truth for which folder is open, exactly
   * as `DockHost` treats it. The editor store is a projection of that, so this
   * pushes the value across rather than reading it back.
   */
  const workdir = useChat(
    (state) =>
      state.sessions.find((item) => item.id === state.activeId)?.workdir ?? null,
  );
  const setEditorWorkdir = useEditor((state) => state.setWorkdir);

  useEffect(() => {
    setEditorWorkdir(workdir);
  }, [setEditorWorkdir, workdir]);

  const [sidebar, setSidebar] = useState<"files" | "git">("files");
  const [chatOpen, setChatOpen] = useState(true);

  const sidebarOpen = config.interface.ideSidebarOpen;
  const sidebarWidth = config.interface.ideSidebarWidth;
  const chatWidth = config.interface.ideChatWidth;

  // The git panel loads its status when the folder is known, which in IDE mode
  // can happen before the panel is ever shown — so it is refreshed here rather
  // than only on mount of the git tab.
  const loadGit = useGit((state) => state.load);
  useEffect(() => {
    void loadGit(workdir);
  }, [loadGit, workdir]);

  return (
    <div className="relative flex h-full min-w-0 flex-1">
      {/* The sidebar: Files and Git, side by side with the editor. */}
      {sidebarOpen && (
        <aside
          style={{ width: sidebarWidth }}
          className="panel relative z-10 m-1 mr-0 flex min-h-0 shrink-0 flex-col overflow-hidden rounded-sheet"
        >
          <div className="glass-thin flex h-9 shrink-0 items-center gap-0.5 border-b border-[var(--glass-border)] px-1">
            <SidebarTab
              active={sidebar === "files"}
              onClick={() => setSidebar("files")}
              icon={<FolderIcon size={13} />}
              label="Files"
            />
            <SidebarTab
              active={sidebar === "git"}
              onClick={() => setSidebar("git")}
              icon={<GitBranchIcon size={13} />}
              label="Git"
            />
            <div className="flex-1" />
            <button
              type="button"
              aria-label="Hide the sidebar"
              title="Hide the sidebar"
              onClick={() => setInterface({ ideSidebarOpen: false })}
              className="grid h-7 w-7 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
            >
              <PanelLeftIcon size={14} />
            </button>
          </div>
          <div className="min-h-0 flex-1">
            {sidebar === "files" ? <FileTree workdir={workdir} /> : <GitPanel workdir={workdir} />}
          </div>
          <ResizeHandle
            value={sidebarWidth}
            min={200}
            max={520}
            axis="x"
            sign={1}
            onChange={(width) => setInterface({ ideSidebarWidth: width })}
            label="Resize the sidebar"
          />
        </aside>
      )}

      {/* The editor: tabs, then the pane. */}
      <div className="relative z-10 m-1 flex min-h-0 min-w-0 flex-1 flex-col">
        {!sidebarOpen && (
          <button
            type="button"
            onClick={() => setInterface({ ideSidebarOpen: true })}
            title="Show the sidebar"
            aria-label="Show the sidebar"
            className="absolute top-1.5 left-1.5 z-20 grid h-7 w-7 place-items-center rounded-control bg-[var(--panel-bg-strong)] text-faint hover:text-[var(--ink)]"
          >
            <PanelLeftIcon size={14} />
          </button>
        )}
        <div className="panel flex min-h-0 flex-1 flex-col overflow-hidden rounded-sheet">
          <div className="glass-thin flex h-9 shrink-0 items-center gap-0.5 border-b border-[var(--glass-border)] px-1">
            <EditorTabs />
          </div>
          {/* `flex flex-col`, and both of those words are load-bearing.
              This was `<div className="min-h-0 flex-1">` — a *block* container —
              and `EditorPane`'s root is `flex-1`, which does nothing inside a
              block parent. So the Monaco holder collapsed to zero height: the
              editor mounted, loaded the file, and had no room to draw a single
              pixel of it. The visible symptom was a tab strip, a status bar
              showing the right filename, and a blank pane below — which reads
              like "Monaco did not render" rather than "Monaco was given no
              height", and is why this is worth a paragraph.

              A `flex-1` child needs a flex parent all the way down. That chain
              is now: this div → EditorPane's root → the holder. */}
          <div className="flex min-h-0 flex-1 flex-col">
            <EditorBody />
          </div>
        </div>
      </div>

      {/* The chat column. */}
      {chatOpen ? (
        <div
          style={{ width: chatWidth }}
          className="relative m-1 ml-0 flex min-h-0 shrink-0 flex-col"
        >
          <div className="panel flex min-h-0 flex-1 flex-col overflow-hidden rounded-sheet">
            <div className="glass-thin flex h-9 shrink-0 items-center gap-1.5 border-b border-[var(--glass-border)] px-2">
              <ChatIcon size={13} className="text-faint" />
              <span className="text-[12px] text-soft">Chat</span>
              <div className="flex-1" />
              <button
                type="button"
                onClick={() => setChatOpen(false)}
                title="Hide the chat column"
                aria-label="Hide the chat column"
                className="rounded-control px-1.5 py-0.5 text-[10.5px] text-faint hover:text-[var(--ink)]"
              >
                hide
              </button>
            </div>
            <div className="min-h-0 flex-1 overflow-hidden">
              {/* The chat, unchanged. It renders its own hero/docked composer and
                  its own transcript; all that differs is the width it is given,
                  which is why nothing in `ChatCanvas` needed a prop for this. */}
              <ChatCanvas />
            </div>
          </div>
          <ResizeHandle
            value={chatWidth}
            min={280}
            max={720}
            axis="x"
            sign={-1}
            onChange={(width) => setInterface({ ideChatWidth: width })}
            label="Resize the chat column"
          />
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setChatOpen(true)}
          title="Show the chat column"
          aria-label="Show the chat column"
          className="panel absolute top-2 right-2 z-20 grid h-8 w-8 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
        >
          <ChatIcon size={15} />
        </button>
      )}
    </div>
  );
}

function SidebarTab({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "flex items-center gap-1.5 rounded-row px-2 py-1 text-[12px] transition-colors",
        active
          ? "bg-[var(--hover-bg)] text-[var(--ink)]"
          : "text-faint hover:text-soft",
      )}
    >
      {icon}
      {label}
    </button>
  );
}

/**
 * The editor pane for the active tab.
 *
 * `EditorPane` is imported through the module rather than statically, so the
 * ~2MB of Monaco is not in the entry bundle — a user who never opens IDE mode and
 * never opens the Files panel never downloads it. The editor panel in the dock
 * does the same thing by being a lazy component; this is the same idea at the
 * call site.
 */
function EditorBody() {
  const activePath = useEditor((state) => state.activePath);
  const tabs = useEditor((state) => state.tabs);
  const active = tabs.find((tab) => tab.path === activePath) ?? null;

  const [DiffView, setDiffView] = useState<React.ComponentType<{
    tab: import("../stores/editor").EditorTab;
  }> | null>(null);
  const [EditorPane, setEditorPane] = useState<React.ComponentType<{
    tab: import("../stores/editor").EditorTab;
  }> | null>(null);
  const loaded = useRef(false);

  useEffect(() => {
    if (loaded.current) return;
    loaded.current = true;
    void import("./DiffView").then((module) => setDiffView(() => module.DiffView));
    void import("./EditorPanel").then((module) =>
      setEditorPane(() => module.EditorPane),
    );
  }, []);

  if (!active) {
    return (
      <div className="grid h-full place-items-center p-6">
        <p className="max-w-[280px] text-center text-[12.5px] leading-5 text-faint">
          No file open. Pick one from the tree beside this, or click a changed file
          in the Git tab to see its diff.
        </p>
      </div>
    );
  }

  const isDiff = activePath?.startsWith("diff:") ?? false;

  if (isDiff) {
    if (!DiffView) {
      return <p className="p-4 text-[12px] text-faint">Loading the diff view…</p>;
    }
    return <DiffView tab={active} />;
  }

  if (!EditorPane) {
    return <p className="p-4 text-[12px] text-faint">Loading the editor…</p>;
  }
  return <EditorPane tab={active} />;
}
