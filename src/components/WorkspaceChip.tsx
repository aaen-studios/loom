import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { ipc } from "../lib/ipc";
import { isTauri } from "../lib/tauri";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { ChevronDownIcon, FolderIcon, GitBranchIcon } from "./icons";

/**
 * Workspace chip: the folder this chat's tools may touch, plus the git branch
 * when it is a repository.
 */
export function WorkspaceChip() {
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const setWorkdir = useChat((state) => state.setWorkdir);
  const globalMode = useSettings((state) => state.config.chat.permissionMode);
  const [branch, setBranch] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [chunks, setChunks] = useState<number | null>(null);
  const [indexing, setIndexing] = useState(false);
  const [indexNote, setIndexNote] = useState<string | null>(null);

  const workdir = session?.workdir ?? null;

  useEffect(() => {
    void ipc
      .workspaceInfo(workdir)
      .then((info) => setBranch(info?.branch ?? null));
    if (workdir && session?.id) {
      void ipc.indexStatus(session.id).then((count) => setChunks(count ?? 0));
    } 
  }, [workdir, session?.id]);

  const pick = async () => {
    if (!isTauri) return;
    setOpen(false);
    const picked = await openDialog({ directory: true, multiple: false });
    if (!picked || typeof picked !== "string") return;
    await setWorkdir(picked);
  };

  const indexNow = async () => {
    if (!session) return;
    setIndexing(true);
    setIndexNote("Reading and embedding files…");
    try {
      const count = await ipc.indexWorkspace(session.id);
      setChunks(count ?? 0);
      setIndexNote(`Indexed ${count} chunks.`);
    } catch (error) {
      setIndexNote(error instanceof Error ? error.message : String(error));
    } finally {
      setIndexing(false);
    }
  };

  const folderName = workdir ? workdir.split(/[\\/]/).filter(Boolean).pop() : null;

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        title={session?.workdir ?? "Choose a workspace folder for tools"}
        className={cn(
          "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px]",
          folderName
            ? "border-[var(--glass-border)] text-soft"
            : "border-[var(--glass-border)] text-faint",
        )}
      >
        <FolderIcon size={14} />
        <span className="max-w-[140px] truncate">
          {folderName ?? "Workspace"}
        </span>
        {branch && (
          <>
            <span className="text-faint">·</span>
            <GitBranchIcon size={13} />
            <span className="max-w-[90px] truncate">{branch}</span>
          </>
        )}
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[300px] overflow-hidden rounded-sheet p-1.5">
          <button
            type="button"
            onClick={() => void pick()}
            className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
          >
            <FolderIcon size={15} />
            {folderName ? "Change folder…" : "Choose folder…"}
          </button>
          {session?.workdir && (
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                void setWorkdir(null);
              }}
              className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
            >
              Clear workspace
            </button>
          )}

          {session?.workdir && (
            <>
              <button
                type="button"
                disabled={indexing}
                onClick={() => void indexNow()}
                className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft disabled:opacity-50"
              >
                {indexing ? "Indexing…" : "Index workspace"}
              </button>
              {chunks !== null && (
                <button
                  type="button"
                  onClick={() => {
                    void ipc.clearIndex(session.id).then(() => {
                      setChunks(0);
                      setIndexNote("Index cleared.");
                    });
                  }}
                  className="hover-surface flex w-full items-center justify-between gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
                >
                  <span>Clear index</span>
                  <span className="text-[11.5px] text-faint">
                    {chunks} chunk{chunks === 1 ? "" : "s"}
                  </span>
                </button>
              )}
            </>
          )}

          {indexNote && (
            <p className="px-2 py-1.5 text-[11.5px] leading-5 text-faint">
              {indexNote}
            </p>
          )}

          <p className="px-2 py-1.5 text-[11.5px] leading-5 text-faint">
            Tools can only read inside this folder. Permission mode:{" "}
            <span className="text-soft">
              {session?.permissionMode ?? globalMode}
            </span>
          </p>
        </div>
      )}
    </div>
  );
}

const MODES = [
  { id: "ask", label: "Ask", help: "Confirm every tool call" },
  { id: "auto-read-only", label: "Auto read", help: "Read-only tools run silently" },
  { id: "auto-all", label: "Auto all", help: "Run every tool without asking" },
] as const;

/** Per-chat tool permission mode (falls back to the global default). */
export function PermissionChip() {
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const setPermissionMode = useChat((state) => state.setPermissionMode);
  const globalMode = useSettings((state) => state.config.chat.permissionMode);
  const [open, setOpen] = useState(false);

  const current = session?.permissionMode ?? null;
  const effective = current ?? globalMode;
  const label = MODES.find((mode) => mode.id === effective)?.label ?? "Ask";

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        title={`Tool permissions: ${MODES.find((m) => m.id === effective)?.help ?? ""}`}
        className={cn(
          "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[12.5px]",
          effective === "auto-all"
            ? "border-[var(--danger)]/40 text-[var(--danger)]"
            : "border-[var(--glass-border)] text-soft",
        )}
      >
        <ShieldIcon />
        {label}
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[280px] overflow-hidden rounded-sheet p-1.5">
          {MODES.map((mode) => (
            <button
              key={mode.id}
              type="button"
              onClick={() => {
                setOpen(false);
                void setPermissionMode(
                  mode.id === globalMode ? null : (mode.id as typeof effective),
                );
              }}
              className={cn(
                "hover-surface flex w-full flex-col items-start rounded-row px-2 py-1.5 text-left",
                effective === mode.id ? "text-[var(--ink)]" : "text-soft",
              )}
            >
              <span className="text-[13px]">{mode.label}</span>
              <span className="text-[11.5px] text-faint">{mode.help}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ShieldIcon() {
  return (
    <svg
      width={13}
      height={13}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 3.5l7 2.5v6c0 4.2-3 7.4-7 8.5-4-1.1-7-4.3-7-8.5V6z" />
    </svg>
  );
}
