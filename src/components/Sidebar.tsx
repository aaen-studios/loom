import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { relativeTime } from "../lib/format";
import { ipc } from "../lib/ipc";
import { useMenu } from "../lib/menu";
import { isTauri } from "../lib/tauri";
import { activityLabel } from "../lib/sessionStatus";
import { groupSessions, sortSessions, workspaceLabel } from "../lib/workspaces";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import type { InterfaceConfig, Session, SidebarGrouping, SidebarSort } from "../types";
import {
  CheckIcon,
  ChevronDownIcon,
  CloseIcon,
  DownloadIcon,
  FolderIcon,
  LoomMark,
  PinIcon,
  PlusIcon,
  SettingsIcon,
  SortIcon,
  TrashIcon,
} from "./icons";
import { EmptyState, IconButton, Kbd, SearchField } from "./ui";

const GROUPING_OPTIONS: { id: SidebarGrouping; label: string }[] = [
  { id: "workspace", label: "By workspace" },
  { id: "none", label: "Flat list" },
];

const SORT_OPTIONS: { id: SidebarSort; label: string }[] = [
  { id: "recent", label: "Recent first" },
  { id: "oldest", label: "Oldest first" },
  { id: "title", label: "Title A–Z" },
];

/**
 * Sessions live in a card, not a permanent panel: the canvas stays full width
 * and centered, and this floats over it when summoned from the titlebar.
 * Pinning only stops it retracting — same card, same place, until you unpin
 * it or close it.
 */
export function SidebarPopup() {
  const open = useUi((state) => state.sidebarOpen);
  const setOpen = useUi((state) => state.setSidebarOpen);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const sessions = useChat((state) => state.sessions);
  const activeId = useChat((state) => state.activeId);
  const openSession = useChat((state) => state.openSession);
  const newSession = useChat((state) => state.newSession);
  const deleteSession = useChat((state) => state.deleteSession);

  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const pinned = config.interface.sidebarPinned;
  const asideRef = useRef<HTMLElement>(null);
  const arrangeRef = useRef<HTMLDivElement>(null);
  const { open: arrangeOpen, setOpen: setArrangeOpen } = useMenu(
    "sidebar-arrange",
    arrangeRef,
  );

  const saveInterface = async (patch: Partial<InterfaceConfig>) => {
    const updated = await ipc.setInterfaceSettings({ ...config.interface, ...patch });
    if (updated) applyRemote(updated);
  };

  /** Pinning keeps the popup open until it is closed explicitly. */
  const togglePin = async () => {
    await saveInterface({ sidebarPinned: !pinned });
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  // Opening the list must show where you are, even if that group was collapsed
  // in a previous visit.
  useEffect(() => {
    if (!open) return;
    const group = sessions.find((session) => session.id === activeId)?.workdir ?? "";
    setCollapsed((current) =>
      current[group] ? { ...current, [group]: false } : current,
    );
  }, [open, activeId, sessions]);

  if (!open) return null;

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? sessions.filter(
        (session) =>
          session.title.toLowerCase().includes(needle) ||
          (session.modelId ?? "").toLowerCase().includes(needle),
      )
    : sessions;

  // History is grouped by workspace unless the user asked for one flat list.
  // Headers only earn their space when grouped and there is something to
  // separate; a search always expands every group.
  const grouped = config.interface.sidebarGrouping === "workspace";
  const sorted = sortSessions(visible, config.interface.sidebarSort);
  const groups = grouped
    ? groupSessions(sorted, config.workspaces)
    : [{ key: "", name: "", workdir: null, sessions: sorted }];
  const showHeaders = grouped && (groups.length > 1 || Boolean(groups[0]?.workdir));
  const activeWorkdir =
    sessions.find((session) => session.id === activeId)?.workdir ?? null;
  const activeLabel = activeWorkdir
    ? workspaceLabel(activeWorkdir, config.workspaces)
    : null;

  const startChat = (workdir: string | null) => {
    void newSession(null, workdir);
    if (!pinned) setOpen(false);
  };

  const openRow = (id: string) => {
    void openSession(id);
    if (!pinned) setOpen(false);
  };

  const exportSession = async (id: string) => {
    if (!isTauri) return;
    const session = sessions.find((item) => item.id === id);
    const suggested = `${(session?.title || "loom-chat")
      .replace(/[^\w\s-]/g, "")
      .trim()
      .replace(/\s+/g, "-")
      .toLowerCase() || "loom-chat"}.md`;

    const path = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!path) return;
    await ipc.exportSession(id, path);
    if (!pinned) setOpen(false);
  };

  return (
    // The card floats over the transcript in both states; only the scrim
    // depends on pinning. The layer itself ignores the pointer so the chat
    // behind stays interactive while pinned.
    <div className="pointer-events-none absolute inset-0 z-30 animate-fade-in">
      {/* Pinned means no click-away: only unpin or close retires the card. */}
      {!pinned && (
        <button
          type="button"
          aria-label="Close chats"
          onClick={() => setOpen(false)}
          className="pointer-events-auto absolute inset-0 cursor-default"
        />
      )}

      <aside
        ref={asideRef}
        className="panel-strong pointer-events-auto animate-fade-up absolute bottom-3 left-3 top-16 flex w-[320px] flex-col overflow-hidden rounded-sheet"
      >
        <div className="flex items-center gap-2 px-3.5 pt-3 pb-1">
          <LoomMark size={15} className="text-[var(--accent)]" />
          <span className="text-[13px] font-semibold tracking-[0.01em]">Chats</span>
          {sessions.length > 0 && (
            <span className="text-[11px] text-faint">{sessions.length}</span>
          )}

          <div ref={arrangeRef} className="relative ml-auto flex items-center gap-0.5">
            <IconButton
              label="Arrange chats"
              aria-expanded={arrangeOpen}
              active={arrangeOpen}
              onClick={() => setArrangeOpen(!arrangeOpen)}
            >
              <SortIcon size={15} />
            </IconButton>
            <IconButton
              label={pinned ? "Unpin chats" : "Pin chats open"}
              active={pinned}
              onClick={() => void togglePin()}
            >
              <PinIcon size={15} />
            </IconButton>
            <IconButton label="Close" onClick={() => setOpen(false)}>
              <CloseIcon size={15} />
            </IconButton>

            {arrangeOpen && (
              <div className="panel-strong absolute top-full right-0 z-40 mt-2 w-[200px] rounded-sheet p-1.5">
                <p className="px-2 pt-1 pb-0.5 text-[11px] font-semibold tracking-[0.08em] text-faint uppercase">
                  Group
                </p>
                {GROUPING_OPTIONS.map((option) => (
                  <ArrangeRow
                    key={option.id}
                    label={option.label}
                    active={config.interface.sidebarGrouping === option.id}
                    onClick={() => void saveInterface({ sidebarGrouping: option.id })}
                  />
                ))}

                <p className="px-2 pt-2 pb-0.5 text-[11px] font-semibold tracking-[0.08em] text-faint uppercase">
                  Sort
                </p>
                {SORT_OPTIONS.map((option) => (
                  <ArrangeRow
                    key={option.id}
                    label={option.label}
                    active={config.interface.sidebarSort === option.id}
                    onClick={() => void saveInterface({ sidebarSort: option.id })}
                  />
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="px-2.5 pt-2">
          <button
            type="button"
            onClick={() => startChat(activeWorkdir)}
            title={activeWorkdir ? "New chat in the current workspace" : "New chat"}
            className="btn-primary w-full px-3 py-2 text-[13px]"
          >
            <PlusIcon size={15} />
            New chat
            {activeLabel && (
              <span className="min-w-0 truncate text-[11.5px] font-normal opacity-75">
                {activeLabel}
              </span>
            )}
          </button>
        </div>

        <div className="px-2.5 pt-2">
          <SearchField
            id="sidebar-search"
            value={query}
            onChange={setQuery}
            placeholder="Search chats…"
            onKeyDown={(event) => {
              if (event.key === "Escape" && query) {
                event.stopPropagation();
                setQuery("");
              }
            }}
          />
        </div>

        <div className="mt-2 min-h-0 flex-1 overflow-y-auto px-1.5 pb-1.5">
          {visible.length === 0 &&
            (sessions.length === 0 ? (
              <EmptyState
                icon={<LoomMark size={26} />}
                title="No chats yet"
                hint="Start one and it will show up here, grouped by workspace."
                action={
                  <button
                    type="button"
                    onClick={() => startChat(activeWorkdir)}
                    className="btn-primary px-3 py-1.5 text-[12.5px]"
                  >
                    <PlusIcon size={14} />
                    New chat
                  </button>
                }
              />
            ) : (
              <EmptyState
                title={`No matches for “${query.trim()}”`}
                hint="Try a different word, or clear the search."
                action={
                  <button
                    type="button"
                    onClick={() => setQuery("")}
                    className="btn-ghost px-3 py-1.5 text-[12.5px]"
                  >
                    Clear search
                  </button>
                }
              />
            ))}

          {groups.map((group) => {
            const isCollapsed = !needle && collapsed[group.key];
            return (
              <div key={group.key || "__none"}>
                {showHeaders && (
                  <div className="sticky top-0 z-10 -mx-1.5 flex items-center gap-1 bg-[var(--panel-bg-strong)] px-3 pt-2.5 pb-1">
                    <button
                      type="button"
                      title={group.workdir ?? "Chats with no workspace"}
                      onClick={() =>
                        setCollapsed((current) => ({
                          ...current,
                          [group.key]: !current[group.key],
                        }))
                      }
                      className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                    >
                      <ChevronDownIcon
                        size={12}
                        className={cn(
                          "shrink-0 text-faint transition-transform",
                          isCollapsed && "-rotate-90",
                        )}
                      />
                      {group.workdir && (
                        <FolderIcon size={13} className="shrink-0 text-faint" />
                      )}
                      <span className="truncate text-[12px] font-medium text-soft">
                        {group.name}
                      </span>
                      <span className="shrink-0 text-[11px] text-faint">
                        {group.sessions.length}
                      </span>
                    </button>
                    {group.workdir && (
                      <IconButton
                        label={`New chat in ${group.name}`}
                        onClick={() => startChat(group.workdir)}
                      >
                        <PlusIcon size={13} />
                      </IconButton>
                    )}
                  </div>
                )}

                {!isCollapsed && (
                  // The thread rail: one warp line per workspace, with the
                  // open chat knotted onto it.
                  <div className="ml-[13px] border-l border-[var(--ink-ghost)]">
                    {group.sessions.map((session) => (
                      <SessionRow
                        key={session.id}
                        session={session}
                        active={session.id === activeId}
                        workspaceTag={
                          grouped
                            ? null
                            : workspaceLabel(session.workdir, config.workspaces)
                        }
                        onOpen={openRow}
                        onExport={(id) => void exportSession(id)}
                        onDelete={(id) => void deleteSession(id)}
                      />
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <div className="border-t border-[var(--glass-border)] p-2">
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              setSettingsOpen(true);
            }}
            className="hover-surface flex w-full items-center gap-2 rounded-row px-2.5 py-2 text-left text-[13px] text-soft"
          >
            <SettingsIcon size={15} />
            <span className="flex-1">Settings</span>
            <Kbd>Ctrl+,</Kbd>
          </button>
        </div>
      </aside>
    </div>
  );
}

/**
 * One chat row: what the chat is called, what it is doing (running, failed,
 * unread, waiting on you), and the actions revealed on hover.
 */
function SessionRow({
  session,
  active,
  workspaceTag,
  onOpen,
  onExport,
  onDelete,
}: {
  session: Session;
  active: boolean;
  workspaceTag: string | null;
  onOpen: (id: string) => void;
  onExport: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const busy = useChat((state) => state.busy[session.id] ?? false);
  const question = useChat((state) => state.questions[session.id]);
  const permission = useChat((state) => state.permissions[session.id]);
  const error = useChat((state) => state.errors[session.id]);
  const unread = useChat((state) => state.unread[session.id] ?? false);
  const renameSession = useChat((state) => state.renameSession);
  // A string selector: streaming deltas recompute the label, but only a row
  // whose label actually changed re-renders.
  const activity = useChat((state) => {
    if (!state.busy[session.id]) return null;
    const live = state.live[session.id];
    return activityLabel(live, live ? state.liveTools[live.messageId] : undefined);
  });
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState("");

  const commitRename = async () => {
    const title = draft.trim();
    setRenaming(false);
    if (title) await renameSession(session.id, title);
  };

  // A prompt waiting in another chat is invisible in the transcript, so the
  // row says so until you open it.
  const waiting = question ? "Question" : permission ? "Approval" : null;
  const failed = !busy && Boolean(error);
  const model = session.modelId ? session.modelId.split("/").pop() : null;
  const title = [
    failed ? error : activity ? `${activity}${model ? ` · ${model}` : ""}` : null,
    "Double-click to rename",
  ]
    .filter(Boolean)
    .join(" — ");

  return (
    <div
      className={cn(
        "group relative flex items-center rounded-row",
        active ? "bg-[var(--active-bg)]" : "hover:bg-[var(--hover-bg)]",
      )}
      aria-current={active ? "true" : undefined}
    >
      {active && (
        <span
          aria-hidden="true"
          className="absolute top-2 bottom-2 -left-[2px] w-[3px] rounded-full bg-[var(--accent)]"
        />
      )}

      {renaming ? (
        <input
          autoFocus
          value={draft}
          onChange={(event) => setDraft(event.currentTarget.value)}
          onBlur={() => void commitRename()}
          onKeyDown={(event) => {
            if (event.key === "Enter") void commitRename();
            if (event.key === "Escape") setRenaming(false);
          }}
          className="m-1 min-w-0 flex-1 rounded-control border border-[var(--accent)] bg-transparent px-2 py-1 text-[13px]"
        />
      ) : (
        <button
          type="button"
          onClick={() => onOpen(session.id)}
          onDoubleClick={() => {
            setRenaming(true);
            setDraft(session.title);
          }}
          title={title}
          className="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2 text-left"
        >
          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-1.5">
              <span className="min-w-0 flex-1 truncate text-[13.5px]">
                {session.title || "New chat"}
              </span>
              {(busy || failed) && (
                <span
                  title={failed ? error : "Running"}
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    failed ? "bg-[var(--danger)]" : "animate-pulse bg-[var(--accent)]",
                  )}
                />
              )}
              {unread && (
                <span
                  title="Replied while you were away"
                  className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--accent)]"
                />
              )}
              {waiting && (
                <span
                  title={
                    waiting === "Question"
                      ? "The model is waiting for your answer in this chat"
                      : "A tool call in this chat needs your approval"
                  }
                  className="shrink-0 rounded-capsule border border-[var(--accent)]/50 px-1.5 py-px text-[10px] font-medium text-[var(--accent)]"
                >
                  {waiting}
                </span>
              )}
            </span>
            <span className="mt-0.5 flex items-center gap-1.5 text-[11.5px] text-faint">
              {workspaceTag && (
                <span className="flex min-w-0 max-w-[110px] items-center gap-1">
                  <FolderIcon size={10} className="shrink-0" />
                  <span className="truncate">{workspaceTag}</span>
                </span>
              )}
              {failed ? (
                <span className="truncate text-[var(--danger)]" title={error}>
                  Failed · {relativeTime(session.updatedAt)}
                </span>
              ) : activity ? (
                <span className="truncate text-[var(--accent)]">{activity}</span>
              ) : (
                <span className="truncate">
                  {relativeTime(session.updatedAt)}
                  {model ? ` · ${model}` : ""}
                </span>
              )}
            </span>
          </span>
        </button>
      )}

      {!renaming && (
        <span className="pointer-events-none absolute top-1/2 right-1 flex -translate-y-1/2 items-center gap-0.5 rounded-control border border-[var(--glass-border)] bg-[var(--panel-bg-strong)] p-0.5 opacity-0 transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100">
          <IconButton label="Export chat" onClick={() => onExport(session.id)}>
            <DownloadIcon size={14} />
          </IconButton>
          <IconButton
            label="Delete chat"
            tone="danger"
            onClick={() => onDelete(session.id)}
          >
            <TrashIcon size={14} />
          </IconButton>
        </span>
      )}
    </div>
  );
}

/** One choice in the sidebar's arrange panel. */
function ArrangeRow({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px]",
        active ? "text-[var(--ink)]" : "text-soft",
      )}
    >
      <span className="min-w-0 flex-1">{label}</span>
      {active && <CheckIcon size={14} className="shrink-0 text-[var(--accent)]" />}
    </button>
  );
}
