import { useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { ipc } from "../lib/ipc";
import { sortEntries, suggestedNameFor } from "../lib/fileTreeOperations";
import { useEditor, useTree } from "../stores/editor";
import { statusFor, useGit } from "../stores/git";
import type { GitStatus, TreeEntry } from "../types";
import {
  ChevronDownIcon,
  FileIcon,
  FolderIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  TrashIcon,
} from "./icons";

/**
 * The workspace, as a tree.
 *
 * ## Why it expands rather than listing everything
 *
 * The composer's `@` picker walks every path up front, because it needs to
 * *search* the tree. This needs to *show* it, and those want opposite things: a
 * picker cannot answer "which file?" without having seen them all, while a tree
 * that can be expanded only ever draws what is on screen. So a collapsed folder
 * costs one row and nothing else, and a repository with a hundred thousand files
 * opens instantly.
 *
 * That also means the listing is per folder and cached in `useTree`, so
 * re-opening a folder you have already looked at is free — and `invalidate`
 * drops just that folder and its parent when something changes inside it.
 *
 * ## What it shows about git
 *
 * Each row carries the file's state as a single letter, the same vocabulary
 * every editor uses: `M` modified, `A` added, `D` deleted, `U` untracked, `C`
 * conflicted. That is worth the pixels because it answers the question you have
 * while editing — "is this committed?" — without switching panels.
 */
export function FileTree({ workdir }: { workdir: string | null }) {
  const listings = useTree((state) => state.listings);
  const loading = useTree((state) => state.loading);
  const load = useTree((state) => state.load);
  const invalidate = useTree((state) => state.invalidate);
  const expanded = useEditor((state) => state.expanded);
  const toggleFolder = useEditor((state) => state.toggleFolder);
  const open = useEditor((state) => state.open);
  const selected = useEditor((state) => state.selected);
  const setSelected = useEditor((state) => state.setSelected);
  const treeError = useEditor((state) => state.treeError);
  const setTreeError = useEditor((state) => state.setTreeError);
  const status = useGit((state) => state.status);
  const refresh = useGit((state) => state.refresh);

  const [query, setQuery] = useState("");
  const [creating, setCreating] = useState<{ parent: string; isDir: boolean } | null>(null);
  const [draftName, setDraftName] = useState("");
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  // The root listing, loaded once per folder.
  useEffect(() => {
    void load(workdir, "");
  }, [load, workdir]);

  // Folders that are expanded but have no listing yet — after a reload, or when
  // something expanded the tree programmatically to reveal a file.
  useEffect(() => {
    for (const folder of expanded) {
      if (!listings[folder]) void load(workdir, folder);
    }
  }, [expanded, listings, load, workdir]);

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

  const root = listings[""] ?? [];

  const create = async () => {
    if (!creating) return;
    const name = draftName.trim() || suggestedNameFor(listings[creating.parent] ?? [], creating.isDir);
    const path = creating.parent ? `${creating.parent}/${name}` : name;
    try {
      await ipc.fileCreate(workdir, path, creating.isDir);
      invalidate(creating.parent);
      invalidate(path);
      await load(workdir, creating.parent, true);
      await refresh();
      setCreating(null);
      setDraftName("");
      // A new file is almost always a file you want to type into, so it opens.
      // A new folder is not, so it does not.
      if (!creating.isDir) void open(path);
    } catch (cause) {
      setTreeError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const remove = async (path: string) => {
    try {
      await ipc.fileDelete(workdir, path);
      invalidate(path);
      const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
      await load(workdir, parent, true);
      await refresh();
      setConfirmDelete(null);
    } catch (cause) {
      setTreeError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-1 px-1.5 pt-1.5 pb-1">
        <div className="relative min-w-0 flex-1">
          <SearchIcon
            size={13}
            className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-faint"
          />
          <input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Filter open folders…"
            aria-label="Filter the file tree"
            className="w-full rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] py-1 pr-2 pl-7 text-[12px] text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)]"
          />
        </div>
        <button
          type="button"
          aria-label="New file"
          title="New file"
          onClick={() => {
            setCreating({ parent: "", isDir: false });
            setDraftName("");
          }}
          className="grid h-7 w-7 shrink-0 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
        >
          <PlusIcon size={14} />
        </button>
        <button
          type="button"
          aria-label="New folder"
          title="New folder"
          onClick={() => {
            setCreating({ parent: "", isDir: true });
            setDraftName("");
          }}
          className="grid h-7 w-7 shrink-0 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
        >
          <FolderIcon size={14} />
        </button>
        <button
          type="button"
          aria-label="Refresh"
          title="Refresh"
          onClick={() => {
            // A force reload of the root, then git. Both, because a file that
            // appeared from a terminal is invisible to each of them alone.
            invalidate("");
            void load(workdir, "", true);
            void refresh();
          }}
          className="grid h-7 w-7 shrink-0 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
        >
          <RefreshIcon size={14} />
        </button>
      </div>

      {treeError && (
        <div className="mx-1.5 mb-1 flex items-start gap-1.5 rounded-row border border-[var(--danger)]/40 bg-[var(--danger)]/10 px-2 py-1.5">
          <span className="min-w-0 flex-1 text-[11.5px] leading-4 break-words">
            {treeError}
          </span>
          <button
            type="button"
            onClick={() => setTreeError(null)}
            className="shrink-0 text-[11px] text-faint hover:text-[var(--ink)]"
          >
            Dismiss
          </button>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2">
        {creating && creating.parent === "" && (
          <NameField
            value={draftName}
            onChange={setDraftName}
            onCommit={() => void create()}
            onCancel={() => setCreating(null)}
            isDir={creating.isDir}
          />
        )}

        {root.length === 0 && !loading.includes("") && (
          <p className="px-2 py-2 text-[12px] text-faint">
            This folder is empty.
          </p>
        )}

        {root.map((entry) => (
          <TreeRow
            key={entry.path}
            entry={entry}
            depth={0}
            query={query}
            listings={listings}
            expanded={expanded}
            selected={selected}
            status={status}
            creating={creating}
            draftName={draftName}
            confirmDelete={confirmDelete}
            onDraftName={setDraftName}
            onToggle={toggleFolder}
            onSelect={(path) => {
              setSelected(path);
              void open(path);
            }}
            onStartCreate={(parent, isDir) => {
              setCreating({ parent, isDir });
              setDraftName("");
            }}
            onCommitCreate={() => void create()}
            onCancelCreate={() => setCreating(null)}
            onAskDelete={setConfirmDelete}
            onConfirmDelete={(path) => void remove(path)}
          />
        ))}
      </div>
    </div>
  );
}

/** The inline input for a new entry, in whichever folder asked for it. */
function NameField({
  value,
  onChange,
  onCommit,
  onCancel,
  isDir,
}: {
  value: string;
  onChange: (value: string) => void;
  onCommit: () => void;
  onCancel: () => void;
  isDir: boolean;
}) {
  const [ref, setRef] = useState<HTMLInputElement | null>(null);
  useEffect(() => {
    ref?.focus();
  }, [ref]);

  return (
    <div className="flex items-center gap-1.5 px-2 py-0.5">
      {isDir ? (
        <FolderIcon size={13} className="shrink-0 text-faint" />
      ) : (
        <FileIcon size={13} className="shrink-0 text-faint" />
      )}
      <input
        ref={setRef}
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
        placeholder={isDir ? "folder-name" : "file-name"}
        aria-label={isDir ? "New folder name" : "New file name"}
        onKeyDown={(event) => {
          // Enter commits and Escape abandons, which is the contract every
          // inline rename field in every editor uses. Without Escape there is no
          // way out but creating something.
          if (event.key === "Enter") {
            event.preventDefault();
            onCommit();
          } else if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
        // Committing on blur is deliberately *not* done: a stray click would
        // create a file named "untitled" that the user then has to delete.
        className="min-w-0 flex-1 rounded-row border border-[var(--accent)] bg-[var(--hover-bg)] px-1.5 py-0.5 font-mono text-[11.5px] text-[var(--ink)] outline-none"
      />
    </div>
  );
}

interface RowProps {
  entry: TreeEntry;
  depth: number;
  query: string;
  listings: Record<string, TreeEntry[]>;
  expanded: string[];
  selected: string | null;
  status: GitStatus;
  creating: { parent: string; isDir: boolean } | null;
  draftName: string;
  confirmDelete: string | null;
  onDraftName: (value: string) => void;
  onToggle: (path: string) => void;
  onSelect: (path: string) => void;
  onStartCreate: (parent: string, isDir: boolean) => void;
  onCommitCreate: () => void;
  onCancelCreate: () => void;
  onAskDelete: (path: string | null) => void;
  onConfirmDelete: (path: string) => void;
}

function TreeRow(props: RowProps) {
  const { entry, depth, listings, expanded, selected, status } = props;
  const isOpen = expanded.includes(entry.path);
  const changed = statusFor(status, entry.path);

  // A file row's filter check. A folder that is closed cannot be filtered into
  // without walking it, which is the cost this tree exists to avoid — so the
  // filter hides rows and never expands anything.
  if (props.query.trim()) {
    const needle = props.query.trim().toLowerCase();
    const matches = entry.name.toLowerCase().includes(needle);
    const children = listings[entry.path] ?? [];
    const childMatches =
      entry.isDir &&
      isOpen &&
      children.some((child) => child.name.toLowerCase().includes(needle));
    if (!matches && !childMatches) return null;
  }

  return (
    <>
      <div
        className={cn(
          "group flex items-center gap-1 rounded-row pr-1",
          selected === entry.path && !entry.isDir
            ? "bg-[var(--hover-bg)]"
            : "hover:bg-[var(--hover-bg)]",
        )}
        style={{ paddingLeft: depth * 11 + 4 }}
      >
        {entry.isDir ? (
          <button
            type="button"
            onClick={() => props.onToggle(entry.path)}
            aria-expanded={isOpen}
            className="flex min-w-0 flex-1 items-center gap-1 py-0.5 text-left"
          >
            <span
              className={cn(
                "shrink-0 text-faint transition-transform",
                isOpen && "rotate-180",
              )}
            >
              <ChevronDownIcon size={11} />
            </span>
            <FolderIcon size={13} className="shrink-0 text-faint" />
            <span className="min-w-0 flex-1 truncate text-[12.5px] text-soft">
              {entry.name}
            </span>
          </button>
        ) : (
          <button
            type="button"
            onClick={() => props.onSelect(entry.path)}
            className="flex min-w-0 flex-1 items-center gap-1 py-0.5 pl-[15px] text-left"
            title={entry.path}
          >
            <FileIcon size={13} className="shrink-0 text-faint" />
            <span className="min-w-0 flex-1 truncate text-[12.5px] text-soft">
              {entry.name}
            </span>
          </button>
        )}

        {/* One letter, in git's own vocabulary. */}
        {changed && (
          <span
            className={cn(
              "shrink-0 font-mono text-[10.5px]",
              changed.conflicted
                ? "text-[var(--danger)]"
                : changed.untracked
                  ? "text-emerald-400"
                  : "text-[var(--accent)]",
            )}
            title={
              changed.conflicted
                ? "Conflicted"
                : changed.untracked
                  ? "Untracked"
                  : changed.staged
                    ? "Staged"
                    : "Modified"
            }
          >
            {gitLetter(changed)}
          </span>
        )}

        {/* Row actions, on hover only. A tree where every row carries two
            buttons is a tree you cannot read. */}
        <span className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          {entry.isDir && (
            <>
              <button
                type="button"
                aria-label={`New file in ${entry.name}`}
                title="New file"
                onClick={() => {
                  if (!isOpen) props.onToggle(entry.path);
                  props.onStartCreate(entry.path, false);
                }}
                className="grid h-5 w-5 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
              >
                <PlusIcon size={11} />
              </button>
              <button
                type="button"
                aria-label={`New folder in ${entry.name}`}
                title="New folder"
                onClick={() => {
                  if (!isOpen) props.onToggle(entry.path);
                  props.onStartCreate(entry.path, true);
                }}
                className="grid h-5 w-5 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
              >
                <FolderIcon size={11} />
              </button>
            </>
          )}
          {props.confirmDelete === entry.path ? (
            <button
              type="button"
              onClick={() => props.onConfirmDelete(entry.path)}
              className="rounded-control px-1 text-[10px] font-medium text-[var(--danger)]"
            >
              really?
            </button>
          ) : (
            <button
              type="button"
              aria-label={`Delete ${entry.name}`}
              title="Delete"
              onClick={() => props.onAskDelete(entry.path)}
              className="grid h-5 w-5 place-items-center rounded-control text-faint hover:text-[var(--danger)]"
            >
              <TrashIcon size={11} />
            </button>
          )}
        </span>
      </div>

      {entry.isDir && isOpen && (
        <>
          {props.creating?.parent === entry.path && (
            <div style={{ paddingLeft: (depth + 1) * 11 + 4 }}>
              <NameField
                value={props.draftName}
                onChange={props.onDraftName}
                onCommit={props.onCommitCreate}
                onCancel={props.onCancelCreate}
                isDir={props.creating.isDir}
              />
            </div>
          )}
          {sortEntries(listings[entry.path] ?? []).map((child) => (
            <TreeRow key={child.path} {...props} entry={child} depth={depth + 1} />
          ))}
        </>
      )}
    </>
  );
}

/** The single letter for a file's state. */
function gitLetter(file: {
  conflicted: boolean;
  untracked: boolean;
  staged: boolean;
}): string {
  if (file.conflicted) return "C";
  if (file.untracked) return "U";
  if (file.staged) return "A";
  return "M";
}
