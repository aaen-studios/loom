import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { relativeTime } from "../lib/format";
import { parentOf } from "../lib/fileTreeOperations";
import { useEditor } from "../stores/editor";
import { useGit } from "../stores/git";
import type { GitFile } from "../types";
import {
  BranchPlusIcon,
  CheckIcon,
  ChevronDownIcon,
  GitCommitIcon,
  GitDiffIcon,
  GitPullIcon,
  GitPushIcon,
  RefreshIcon,
  SparkIcon,
  StopIcon,
} from "./icons";
import { Kbd } from "./ui";

/**
 * Git, as a panel.
 *
 * ## The shape
 *
 * A commit box at the top with the AI button beside it, then the branch and
 * remote row, then the two file lists — staged and unstaged — then history. That
 * order is deliberate: it is the order the work happens in, and the commit box is
 * the thing you come back to most, so it never scrolls out of reach.
 *
 * ## What this panel deliberately does not do
 *
 * **Render diffs.** Clicking a changed file opens a `diff:` tab in the editor,
 * which draws it with Monaco's own `DiffEditor` from two file versions. That is
 * the whole reason there is no patch parser anywhere in this feature, and it is
 * also better: two versions is exact, where a patch is a description that can be
 * subtly wrong about a file with no trailing newline.
 *
 * **Decide anything about the index.** Every action calls the backend and adopts
 * the status that comes back, rather than modelling what staging does to a
 * two-sided file. See `stores/git.ts` for why a speculative version is wrong in
 * the dangerous direction.
 */
export function GitPanel({ workdir }: { workdir: string | null }) {
  const status = useGit((state) => state.status);
  const branches = useGit((state) => state.branches);
  const commits = useGit((state) => state.commits);
  const loading = useGit((state) => state.loading);
  const available = useGit((state) => state.available);
  const op = useGit((state) => state.op);
  const notice = useGit((state) => state.notice);
  const error = useGit((state) => state.error);
  const message = useGit((state) => state.message);
  const setMessage = useGit((state) => state.setMessage);
  const load = useGit((state) => state.load);
  const refresh = useGit((state) => state.refresh);
  const refreshHistory = useGit((state) => state.refreshHistory);
  const stage = useGit((state) => state.stage);
  const unstage = useGit((state) => state.unstage);
  const discard = useGit((state) => state.discard);
  const commit = useGit((state) => state.commit);
  const checkout = useGit((state) => state.checkout);
  const createBranch = useGit((state) => state.createBranch);
  const fetch = useGit((state) => state.fetch);
  const pull = useGit((state) => state.pull);
  const push = useGit((state) => state.push);
  const generate = useGit((state) => state.generateMessage);
  const clearNotice = useGit((state) => state.clearNotice);
  const clearError = useGit((state) => state.clearError);

  const [tab, setTab] = useState<"changes" | "history">("changes");
  const [confirmDiscard, setConfirmDiscard] = useState<string[] | null>(null);
  const [branchMenu, setBranchMenu] = useState(false);
  const [newBranch, setNewBranch] = useState<string | null>(null);
  const messageRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    void load(workdir);
  }, [load, workdir]);

  const staged = useMemo(
    () => status.files.filter((file) => file.staged),
    [status.files],
  );
  const unstaged = useMemo(
    () => status.files.filter((file) => file.unstaged || file.untracked),
    [status.files],
  );

  if (!workdir) {
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[240px] text-center text-[12.5px] leading-5 text-faint">
          This chat has no workspace folder. Pick one from the workspace chip and
          its git state appears here.
        </p>
      </div>
    );
  }

  if (available === false) {
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[260px] text-center text-[12.5px] leading-5 text-faint">
          Loom could not find <span className="mono">git</span> on this machine.
          Install it, or make sure it is on your PATH, and reopen this panel.
        </p>
      </div>
    );
  }

  if (!loading && !status.isRepo) {
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[240px] text-center text-[12.5px] leading-5 text-faint">
          This folder is not a git repository. Run{" "}
          <span className="mono text-soft">git init</span> in the terminal panel to
          start tracking it.
        </p>
      </div>
    );
  }

  const busy = op !== null;
  const canCommit = staged.length > 0 && message.trim().length > 0 && !busy;
  const conflicted = status.files.some((file) => file.conflicted);

  /** The generate button. Async, so the panel says what it is doing. */
  const generateMessage = () => {
    // The box is the thing the result lands in, so it takes focus first: the
    // generated text is a draft to edit, and having to click into the box after
    // generating is a step that exists for no reason.
    messageRef.current?.focus();
    void generate();
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* The commit box. Pinned, never scrolled away — it is what you come back
          to, and a commit box you have to scroll to is one you use less. */}
      <div className="shrink-0 border-b border-[var(--glass-border)] px-2 pt-2 pb-2">
        <div className="relative">
          <textarea
            ref={messageRef}
            value={message}
            onChange={(event) => setMessage(event.currentTarget.value)}
            onKeyDown={(event) => {
              // Ctrl+Enter commits, matching the send key the composer uses for
              // a message. Plain Enter is a newline: a commit message's second
              // line is its body, so Enter has a job here.
              if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                if (canCommit) void commit();
              }
            }}
            rows={3}
            placeholder="Commit message…"
            aria-label="Commit message"
            spellCheck={false}
            className="w-full resize-none rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2 py-1.5 text-[12.5px] leading-5 text-[var(--ink)] placeholder:text-[var(--ink-faint)] focus:border-[var(--accent)]"
          />
          <button
            type="button"
            onClick={generateMessage}
            disabled={busy || status.files.length === 0}
            aria-label="Write the message with AI"
            title="Write the message with AI"
            className={cn(
              "absolute top-1.5 right-1.5 grid h-6 w-6 place-items-center rounded-control transition",
              op === "generate"
                ? "text-[var(--accent)]"
                : "text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--accent)]",
              "disabled:opacity-40",
            )}
          >
            {op === "generate" ? (
              <span className="loom-spin">
                <RefreshIcon size={13} />
              </span>
            ) : (
              <SparkIcon size={14} />
            )}
          </button>
        </div>

        <div className="mt-1.5 flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => void commit()}
            disabled={!canCommit}
            title={
              conflicted
                ? "Resolve the conflicts first"
                : staged.length === 0
                  ? "Stage a file first"
                  : "Commit (Ctrl+Enter)"
            }
            className={cn(
              "flex min-w-0 flex-1 items-center justify-center gap-1.5 rounded-control px-2 py-1.5 text-[12.5px] font-medium transition",
              canCommit
                ? "bg-[var(--control-bg)] text-[var(--control-ink)] hover:opacity-90"
                : "bg-[var(--ink-ghost)] text-faint",
            )}
          >
            {op === "commit" ? (
              <>
                <span className="loom-spin">
                  <RefreshIcon size={12} />
                </span>
                Committing…
              </>
            ) : (
              <>
                <GitCommitIcon size={13} />
                Commit {staged.length > 0 && `(${staged.length})`}
              </>
            )}
          </button>
        </div>

        {conflicted && (
          <p className="mt-1.5 text-[11.5px] leading-4 text-[var(--danger)]">
            This repository has unresolved conflicts. Open the files, resolve them,
            and stage the result.
          </p>
        )}

        {status.operation && (
          <p className="mt-1.5 text-[11.5px] leading-4 text-[var(--accent)]">
            A {status.operation} is in progress.
          </p>
        )}
      </div>

      {/* Branch, and the remote actions. */}
      <div className="shrink-0 border-b border-[var(--glass-border)] px-2 py-1.5">
        <div className="flex items-center gap-1">
          <div className="relative min-w-0 flex-1">
            <button
              type="button"
              onClick={() => {
                setBranchMenu((value) => !value);
                void refreshHistory();
              }}
              aria-expanded={branchMenu}
              title="Switch branch"
              className="flex w-full min-w-0 items-center gap-1 rounded-row px-1.5 py-1 text-left hover:bg-[var(--hover-bg)]"
            >
              <span className="min-w-0 flex-1 truncate text-[12.5px] text-soft">
                {status.detached
                  ? "detached HEAD"
                  : (status.branch ?? "no branch")}
              </span>
              {/* Ahead/behind, which is the one number that says whether a push
                  or a pull is worth doing. */}
              {status.ahead > 0 && (
                <span className="shrink-0 text-[10.5px] text-[var(--accent)]">
                  ↑{status.ahead}
                </span>
              )}
              {status.behind > 0 && (
                <span className="shrink-0 text-[10.5px] text-soft">
                  ↓{status.behind}
                </span>
              )}
              <span className={cn("shrink-0 text-faint", branchMenu && "rotate-180")}>
                <ChevronDownIcon size={11} />
              </span>
            </button>

            {branchMenu && (
              <>
                <button
                  type="button"
                  aria-label="Close the branch menu"
                  onClick={() => setBranchMenu(false)}
                  className="fixed inset-0 z-40 cursor-default"
                />
                <div className="panel-strong absolute top-full left-0 z-50 mt-1 max-h-[320px] w-[220px] overflow-y-auto rounded-sheet p-1">
                  <p className="px-2 pt-1 pb-0.5 text-[10.5px] font-semibold tracking-[0.08em] text-faint uppercase">
                    Branches
                  </p>
                  {branches.map((branch) => (
                    <button
                      key={branch.name}
                      type="button"
                      disabled={branch.current || busy}
                      onClick={() => {
                        setBranchMenu(false);
                        void checkout(branch.name);
                      }}
                      className={cn(
                        "flex w-full items-center gap-1.5 rounded-row px-2 py-1 text-left text-[12.5px]",
                        branch.current
                          ? "cursor-default text-faint"
                          : "text-soft hover:bg-[var(--hover-bg)]",
                      )}
                    >
                      <span className="min-w-0 flex-1 truncate">{branch.name}</span>
                      {branch.current && <CheckIcon size={12} className="shrink-0" />}
                    </button>
                  ))}
                  {newBranch === null ? (
                    <button
                      type="button"
                      onClick={() => setNewBranch("")}
                      className="flex w-full items-center gap-1.5 rounded-row px-2 py-1 text-left text-[12.5px] text-soft hover:bg-[var(--hover-bg)]"
                    >
                      <BranchPlusIcon size={13} className="text-faint" />
                      New branch…
                    </button>
                  ) : (
                    <input
                      autoFocus
                      value={newBranch}
                      onChange={(event) => setNewBranch(event.currentTarget.value)}
                      placeholder="branch-name"
                      aria-label="New branch name"
                      onKeyDown={(event) => {
                        if (event.key === "Enter" && newBranch.trim()) {
                          setBranchMenu(false);
                          void createBranch(newBranch.trim());
                          setNewBranch(null);
                        } else if (event.key === "Escape") {
                          setNewBranch(null);
                        }
                      }}
                      className="w-full rounded-row border border-[var(--accent)] bg-[var(--hover-bg)] px-2 py-1 font-mono text-[11.5px] outline-none"
                    />
                  )}
                </div>
              </>
            )}
          </div>

          <RemoteButton
            label="Fetch"
            icon={<RefreshIcon size={13} />}
            busy={op === "fetch"}
            disabled={busy}
            onClick={() => void fetch()}
          />
          <RemoteButton
            label="Pull"
            icon={
              op === "pull" ? (
                <span className="loom-spin">
                  <RefreshIcon size={13} />
                </span>
              ) : (
                <GitPullIcon size={13} />
              )
            }
            // Zero behind is not "nothing to do" — the counts are only known
            // after a fetch, so the button stays live and the fetch is what
            // makes the number meaningful.
            busy={false}
            disabled={busy}
            onClick={() => void pull()}
          />
          <RemoteButton
            label="Push"
            icon={
              op === "push" ? (
                <span className="loom-spin">
                  <RefreshIcon size={13} />
                </span>
              ) : (
                <GitPushIcon size={13} />
              )
            }
            busy={false}
            disabled={busy || status.detached}
            onClick={() => void push()}
          />
        </div>
      </div>

      {/* The two file lists. */}
      <div className="flex shrink-0 items-center gap-1 border-b border-[var(--glass-border)] px-2 py-1">
        <PanelTab active={tab === "changes"} onClick={() => setTab("changes")}>
          Changes
          {status.files.length > 0 && (
            <span className="ml-1 text-[10.5px] text-faint">
              {status.files.length}
            </span>
          )}
        </PanelTab>
        <PanelTab active={tab === "history"} onClick={() => setTab("history")}>
          History
        </PanelTab>
        <div className="flex-1" />
        <button
          type="button"
          aria-label="Refresh"
          title="Refresh"
          onClick={() => {
            void refresh();
            void refreshHistory();
          }}
          className="grid h-6 w-6 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
        >
          <span className={cn(loading && "loom-spin")}>
            <RefreshIcon size={12} />
          </span>
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2">
        {tab === "changes" ? (
          <>
            {status.files.length === 0 && !loading && (
              <p className="px-2 py-3 text-center text-[12px] text-faint">
                No changes. {status.ahead > 0 && "You have commits to push."}
              </p>
            )}

            {staged.length > 0 && (
              <FileGroup
                label="Staged"
                files={staged}
                action="unstage"
                busy={busy}
                onAction={(paths) => void unstage(paths)}
                onOpen={(file) => useEditor.getState().openDiff(file.path, true)}
              />
            )}

            {unstaged.length > 0 && (
              <FileGroup
                label="Changes"
                files={unstaged}
                action="stage"
                busy={busy}
                onAction={(paths) => void stage(paths)}
                onOpen={(file) => useEditor.getState().openDiff(file.path, false)}
                onDiscard={(path) => setConfirmDiscard([path])}
              />
            )}
          </>
        ) : (
          <History commits={commits} />
        )}
      </div>

      {notice && (
        <button
          type="button"
          onClick={clearNotice}
          className="shrink-0 border-t border-[var(--glass-border)] px-2 py-1.5 text-left text-[11.5px] leading-4 text-faint hover:text-soft"
        >
          {notice}
        </button>
      )}

      {error && (
        <div className="shrink-0 border-t border-[var(--danger)]/40 bg-[var(--danger)]/10 px-2 py-1.5">
          <p className="text-[11.5px] leading-4 break-words">{error}</p>
          <button
            type="button"
            onClick={clearError}
            className="mt-1 text-[11px] text-faint hover:text-[var(--ink)]"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Discard is the one action here that can lose work nothing restores, so
          it is the one action that always asks. Inside a repository that
          `delete_path` would treat as recoverable, an uncommitted edit is not
          recoverable at all — there is no commit holding it. */}
      {confirmDiscard && (
        <div className="absolute inset-0 z-50 grid place-items-center bg-black/30 p-4">
          <div className="panel-strong w-full max-w-[300px] rounded-sheet p-3">
            <p className="text-[13px] text-[var(--ink)]">
              Discard changes to{" "}
              <span className="mono text-[12px]">
                {confirmDiscard.length === 1
                  ? confirmDiscard[0]
                  : `${confirmDiscard.length} files`}
                ?
              </span>
            </p>
            <p className="mt-1 text-[11.5px] leading-4 text-faint">
              Uncommitted changes cannot be restored by Loom or by git. This is
              final.
            </p>
            <div className="mt-2.5 flex justify-end gap-1.5">
              <button
                type="button"
                onClick={() => setConfirmDiscard(null)}
                className="btn-ghost px-2.5 py-1 text-[12px]"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  const paths = confirmDiscard;
                  setConfirmDiscard(null);
                  void discard(paths);
                }}
                className="rounded-control bg-[var(--danger)] px-2.5 py-1 text-[12px] font-medium text-white"
              >
                Discard
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function PanelTab({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "rounded-row px-1.5 py-0.5 text-[12px] transition-colors",
        active
          ? "bg-[var(--hover-bg)] text-[var(--ink)]"
          : "text-faint hover:text-soft",
      )}
    >
      {children}
    </button>
  );
}

function RemoteButton({
  label,
  icon,
  busy,
  disabled,
  onClick,
}: {
  label: string;
  icon: React.ReactNode;
  busy: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={label}
      aria-label={label}
      className={cn(
        "grid h-6 w-6 shrink-0 place-items-center rounded-control transition",
        busy ? "text-[var(--accent)]" : "text-faint hover:text-[var(--ink)]",
        "disabled:opacity-40",
      )}
    >
      {icon}
    </button>
  );
}

/** One of the two file lists, with a stage/unstage-all header. */
function FileGroup({
  label,
  files,
  action,
  busy,
  onAction,
  onOpen,
  onDiscard,
}: {
  label: string;
  files: GitFile[];
  action: "stage" | "unstage";
  busy: boolean;
  onAction: (paths: string[]) => void;
  onOpen: (file: GitFile) => void;
  onDiscard?: (path: string) => void;
}) {
  return (
    <div className="mb-1">
      <div className="flex items-center gap-1 px-1.5 pt-1.5 pb-0.5">
        <span className="text-[10.5px] font-semibold tracking-[0.08em] text-faint uppercase">
          {label}
        </span>
        <span className="text-[10.5px] text-faint">{files.length}</span>
        <div className="flex-1" />
        <button
          type="button"
          disabled={busy}
          onClick={() => onAction(files.map((file) => file.path))}
          title={action === "stage" ? "Stage everything" : "Unstage everything"}
          className="rounded-control px-1 text-[10.5px] text-faint hover:text-[var(--accent)] disabled:opacity-40"
        >
          {action === "stage" ? "stage all" : "unstage all"}
        </button>
      </div>

      {files.map((file) => (
        <div
          key={file.path}
          className="group flex items-center gap-1 rounded-row pr-1 hover:bg-[var(--hover-bg)]"
        >
          <button
            type="button"
            onClick={() => onOpen(file)}
            title={file.from ? `${file.from} → ${file.path}` : file.path}
            className="flex min-w-0 flex-1 items-center gap-1.5 py-1 pl-1.5 text-left"
          >
            <GitDiffIcon size={12} className="shrink-0 text-faint" />
            <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-soft">
              {file.path}
            </span>
            {/* Only the *directory* is shown, never the whole path again: the
                filename is already the label, and repeating it doubles the row's
                width for nothing. */}
            {parentOf(file.path) && (
              <span className="shrink-0 truncate text-[10.5px] text-faint">
                {parentOf(file.path)}
              </span>
            )}
            <span
              className={cn(
                "shrink-0 font-mono text-[10.5px]",
                file.conflicted
                  ? "text-[var(--danger)]"
                  : file.untracked
                    ? "text-emerald-400"
                    : "text-[var(--accent)]",
              )}
            >
              {file.conflicted ? "C" : file.untracked ? "U" : file.staged ? "M" : "M"}
            </span>
          </button>

          <span className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
            {onDiscard && (
              <button
                type="button"
                disabled={busy}
                onClick={() => onDiscard(file.path)}
                aria-label={`Discard changes to ${file.path}`}
                title="Discard changes"
                className="grid h-5 w-5 place-items-center rounded-control text-faint hover:text-[var(--danger)] disabled:opacity-40"
              >
                <StopIcon size={11} />
              </button>
            )}
            <button
              type="button"
              disabled={busy}
              onClick={() => onAction([file.path])}
              aria-label={
                action === "stage" ? `Stage ${file.path}` : `Unstage ${file.path}`
              }
              title={action === "stage" ? "Stage" : "Unstage"}
              className="grid h-5 w-5 place-items-center rounded-control text-faint hover:text-[var(--accent)] disabled:opacity-40"
            >
              {action === "stage" ? "+" : "−"}
            </button>
          </span>
        </div>
      ))}
    </div>
  );
}

/** Recent commits. `git log` has no diff here: the panel shows history, and a
 *  commit's diff is a click away in a terminal if it is wanted. */
function History({ commits }: { commits: import("../types").GitCommit[] }) {
  if (commits.length === 0) {
    return (
      <p className="px-2 py-3 text-center text-[12px] text-faint">
        No commits yet.
      </p>
    );
  }

  return (
    <>
      {commits.map((commit) => (
        <div
          key={commit.id}
          className="flex items-baseline gap-1.5 rounded-row px-1.5 py-1 hover:bg-[var(--hover-bg)]"
          title={`${commit.id} — ${commit.author}`}
        >
          <span className="shrink-0 font-mono text-[10.5px] text-faint">
            {commit.id}
          </span>
          <span className="min-w-0 flex-1 truncate text-[12px] text-soft">
            {commit.subject}
          </span>
          <span className="shrink-0 text-[10px] text-faint">
            {relativeTime(commit.at * 1000)}
          </span>
        </div>
      ))}
      <p className="px-1.5 pt-1.5 text-[10.5px] leading-4 text-faint">
        <Kbd>Ctrl+Enter</Kbd> commits
      </p>
    </>
  );
}
