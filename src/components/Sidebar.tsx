import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { relativeTime } from "../lib/format";
import { ipc } from "../lib/ipc";
import { useMenu } from "../lib/menu";
import { isTauri } from "../lib/tauri";
import { activityLabel } from "../lib/sessionStatus";
import {
  arrangeGroups,
  isGroupCollapsed,
  moveInList,
  orderChats,
  toggleCollapsedGroup,
  visibleRows,
} from "../lib/sidebarOrder";
import { sortSessions, workspaceLabel, type WorkspaceGroup } from "../lib/workspaces";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import type { InterfaceConfig, Session, SidebarGrouping, SidebarSort } from "../types";
import {
  CheckIcon,
  ChevronDownIcon,
  DownloadIcon,
  FolderIcon,
  GripIcon,
  LoomMark,
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
 * A model id as a row label: the last path segment, without a routing suffix.
 *
 * The stored form is `deepseek/deepseek-v4.1-flash:free`, which is three times
 * too long for a 300px column and made every row's second line the widest thing
 * in the panel. The provider prefix is redundant — the chat's own provider is in
 * its header — and `:free` / `:nitro` describe billing rather than capability.
 * What remains is the part that actually distinguishes one chat's model from
 * another's.
 */
function shortModel(modelId: string | null): string | null {
  if (!modelId) return null;
  const last = modelId.split("/").pop() ?? modelId;
  return last.replace(/:(free|nitro|beta|preview|extended)$/i, "") || null;
}

/**
 * The chats list, as the dock's left panel — the app's navigation column.
 *
 * It used to be a card floating over the transcript, and that is the reason so
 * much of this file is machinery for a list: an `open` flag to summon it, a pin
 * to stop it retracting, a click-away scrim, a check against the dock so the two
 * copies never rendered at once, and a `setOpen(false)` on every row click so
 * choosing a chat dismissed the thing you were choosing from. All of that
 * existed to manage an overlay. It is one panel in the dock now, so the zone
 * owns visibility, the zone's own header owns closing it, and selecting a chat
 * leaves the list where it is — which is what you want when the list is a
 * column beside the chat rather than a card on top of it.
 *
 * Grouped mode is ordered by hand. A workspace you drag keeps its place, a
 * chat you drag keeps its, and anything you have never dragged still sorts
 * newest-first — so a chat you just started lands on top of a group you
 * arranged last week. The rules themselves live in `lib/sidebarOrder.ts`; this
 * file is only the gesture and the rendering.
 */
export function Sidebar() {
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const sessions = useChat((state) => state.sessions);
  const activeId = useChat((state) => state.activeId);
  const openSession = useChat((state) => state.openSession);
  const newSession = useChat((state) => state.newSession);
  const deleteSession = useChat((state) => state.deleteSession);
  const applySessionOrder = useChat((state) => state.applySessionOrder);

  const [query, setQuery] = useState("");
  /**
   * Groups opened for this visit only, because the chat you are in lives in
   * one of them. Deliberately *not* merged into the saved list: opening the
   * list should not rewrite a fold you chose, or the choice would never
   * outlive the next glance at the sidebar.
   */
  const [revealed, setRevealed] = useState<string[]>([]);
  /** Groups showing every row rather than the first handful. */
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [groupDrag, setGroupDrag] = useState<string | null>(null);
  const [groupOver, setGroupOver] = useState<string | null>(null);
  const [chatDrag, setChatDrag] = useState<{ id: string; group: string } | null>(null);
  const [chatOver, setChatOver] = useState<string | null>(null);
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
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

  // The list must show where you are, even if that group was collapsed in a
  // previous visit — and, now that the fold persists in config, even if it was
  // collapsed in a previous *run*. Runs on mount and whenever the chat changes,
  // because a panel is always present rather than opened.
  useEffect(() => {
    const group = sessions.find((session) => session.id === activeId)?.workdir ?? "";
    setRevealed((current) =>
      current.includes(group) ? current : [...current, group],
    );
  }, [activeId, sessions]);

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? sessions.filter(
        (session) =>
          session.title.toLowerCase().includes(needle) ||
          (session.modelId ?? "").toLowerCase().includes(needle),
      )
    : sessions;

  // Grouped mode is the user's own order; flat mode is still the Sort menu's,
  // which is why `sortSessions` is applied only there.
  const grouped = config.interface.sidebarGrouping === "workspace";
  const sorted = sortSessions(visible, config.interface.sidebarSort);
  const groups: WorkspaceGroup[] = grouped
    ? arrangeGroups(
        visible,
        config.workspaces,
        config.interface.sidebarWorkspaceOrder,
      )
    : [
        {
          key: "",
          name: "",
          workdir: null,
          sessions: sorted,
          newestAt: 0,
          addedAt: 0,
        },
      ];
  // Headers only earn their space when grouped and there is something to
  // separate; a search always expands every group.
  const showHeaders = grouped && (groups.length > 1 || Boolean(groups[0]?.workdir));
  const activeWorkdir =
    sessions.find((session) => session.id === activeId)?.workdir ?? null;
  const activeLabel = activeWorkdir
    ? workspaceLabel(activeWorkdir, config.workspaces)
    : null;

  /** Every workspace group's path, in the order it is drawn. */
  const groupOrder = groups
    .map((group) => group.workdir)
    .filter((path): path is string => Boolean(path));

  /**
   * Writes the whole group order, not just the moved entry. The first drag is
   * what places every folder that exists at that moment, so only a folder
   * added afterwards is unlisted — and only that one floats to the top.
   */
  const dropGroup = (targetPath: string) => {
    setGroupDrag(null);
    setGroupOver(null);
    if (!groupDrag || groupDrag === targetPath) return;
    const next = moveInList(groupOrder, groupDrag, targetPath);
    if (next === groupOrder) return;
    void saveInterface({ sidebarWorkspaceOrder: next });
  };

  /**
   * Reorders the chats of one group. Dragging into a different workspace would
   * mean moving the chat between folders, which is the workspace chip's job —
   * so a cross-group drop is ignored rather than guessed at.
   */
  const dropChat = (targetId: string, targetGroup: string) => {
    setChatDrag(null);
    setChatOver(null);
    if (!chatDrag) return;
    if (chatDrag.group !== targetGroup || chatDrag.id === targetId) return;
    const group = groups.find((entry) => entry.key === targetGroup);
    if (!group) return;
    const ids = orderChats(group.sessions).map((session) => session.id);
    const next = moveInList(ids, chatDrag.id, targetId);
    if (next === ids) return;
    // Optimistic: the list must not jump back to its computed order while the
    // write is in flight.
    applySessionOrder(next);
    if (isTauri) void ipc.reorderSessions(next);
  };

  // Selecting a chat no longer dismisses anything. There was a `setOpen(false)`
  // here, which made sense while this was a popup floating over the transcript:
  // picking a row meant you wanted the chat back. A panel is a column beside the
  // chat, so closing it on every click would make the list unusable — you could
  // never open a second chat without re-summoning it.
  const startChat = (workdir: string | null) => {
    void newSession(null, workdir);
  };

  const openRow = (id: string) => {
    void openSession(id);
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
  };

  return (
    // No surface of its own. The zone it lives in is already a `panel`, and
    // drawing a second `panel-strong` inside it put two glass edges a few pixels
    // apart — a seam around the list, plus 8px of a 300px column spent on
    // padding that belonged to the zone. The zone owns the surface; this is
    // only its contents.
    <div className="flex h-full min-h-0 flex-col">
      <aside
        ref={asideRef}
        className="flex h-full w-full flex-col overflow-hidden"
      >
        {/* No title row. The dock's zone header already says "Chats" a few
            pixels above this, so the panel used to print the same word twice —
            with 44px of a navigation column spent on a heading that was already
            on screen. This panel starts with its controls instead. */}
        <div className="flex items-center gap-1.5 px-2.5 pt-2.5">
          <div className="min-w-0 flex-1">
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

          <div ref={arrangeRef} className="relative shrink-0">
            {/* No Pin and no Close. Pin meant "do not dismiss on click-away",
                which cannot arise in a column; Close meant "hide the overlay",
                and the zone's own collapse is that affordance now. */}
            <IconButton
              label="Arrange chats"
              aria-expanded={arrangeOpen}
              active={arrangeOpen}
              onClick={() => setArrangeOpen(!arrangeOpen)}
            >
              <SortIcon size={15} />
            </IconButton>

            {arrangeOpen && (
              <>
                {/* A click-catcher, so clicking anywhere else closes the menu.
                    Without it the only way out of an open arrange menu was to
                    press its own button again, which meant reaching for a
                    control in order to dismiss one. */}
                <button
                  type="button"
                  aria-label="Close arrange menu"
                  onClick={() => setArrangeOpen(false)}
                  className="fixed inset-0 z-30 cursor-default"
                />
                <div className="panel-strong absolute top-full right-0 z-40 mt-2 w-[210px] rounded-sheet p-1.5">
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
                  {grouped ? (
                    // Offering these here would be a lie: in grouped mode the
                    // order is whatever you dragged it to.
                    <p className="px-2 pb-1 text-[11.5px] leading-4 text-faint">
                      Grouped order is by hand — drag a workspace or a chat to
                      move it. Switch to Flat list for these.
                    </p>
                  ) : (
                    SORT_OPTIONS.map((option) => (
                      <ArrangeRow
                        key={option.id}
                        label={option.label}
                        active={config.interface.sidebarSort === option.id}
                        onClick={() => void saveInterface({ sidebarSort: option.id })}
                      />
                    ))
                  )}
                </div>
              </>
            )}
          </div>
        </div>

        {/* A quiet outlined button, not the inverted `btn-primary` it was.
            That version filled the width with `--control-bg` — a near-solid
            near-black block at the top of a translucent column, which made the
            single most routine action in the app the loudest thing on screen
            and outweighed the chat titles it sits above. The accent plus is
            what carries the "this creates something" meaning now. */}
        <div className="px-2.5 pt-1.5">
          <button
            type="button"
            onClick={() => startChat(activeWorkdir)}
            title={
              activeWorkdir
                ? `New chat in ${activeLabel ?? activeWorkdir}`
                : "New chat"
            }
            className="hover-surface flex w-full items-center gap-2 rounded-row border border-[var(--glass-border)] px-2.5 py-1.5 text-[12.5px] text-soft"
          >
            <PlusIcon size={14} className="shrink-0 text-[var(--accent)]" />
            <span className="shrink-0">New chat</span>
            {/* Which folder the chat lands in, said once rather than in a
                tooltip. It is the difference between a chat that can use tools
                and one that cannot, so it belongs on the button. */}
            {activeLabel && (
              <span className="min-w-0 flex-1 truncate text-right text-[11px] text-faint">
                {activeLabel}
              </span>
            )}
          </button>
        </div>

        <div
          className="mt-2 min-h-0 flex-1 overflow-y-auto px-1.5 pb-1.5"
          // A drag that ends over empty space rather than a row: drop the
          // gesture rather than leaving the indicator stuck.
          onDragEnd={() => {
            setChatDrag(null);
            setChatOver(null);
            setGroupDrag(null);
            setGroupOver(null);
          }}
        >
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
            const isCollapsed =
              !needle &&
              isGroupCollapsed(
                config.interface.sidebarCollapsedGroups,
                revealed,
                group.key,
              );
            const ordered = orderChats(group.sessions);
            // A search shows everything: the cap is for browsing, not looking
            // for a specific chat.
            const { shown, hidden } = visibleRows(
              ordered,
              activeId,
              Boolean(needle) || Boolean(expandedGroups[group.key]),
            );
            // "No workspace" is pinned and cannot be dragged; in flat mode
            // there are no groups to order.
            const canDragGroup = grouped && Boolean(group.workdir);
            const isGroupTarget = groupOver === group.key && groupDrag !== group.key;

            return (
              <div
                key={group.key || "__none"}
                // The drop target is the *whole group*, not just its header.
                //
                // This is what was actually broken. The header is a ~24px
                // strip at the top of the group, and it carried the only
                // `dragover`/`drop` handlers — so a drag that started fine and
                // looked right did nothing at all unless the pointer happened
                // to be released on that one thin line. Releasing anywhere over
                // the group's own chats, which is where you naturally let go,
                // silently cancelled the drag. A gesture that appears to work
                // and then discards your input reads as "dragging is broken",
                // which is exactly the report this fixes.
                //
                // A group drag and a chat drag are mutually exclusive, because
                // `groupDrag` is only ever set for the former, so the chat
                // rows' own handlers below still win for a chat.
                onDragOver={(event) => {
                  if (!canDragGroup || !groupDrag) return;
                  event.preventDefault();
                  setGroupOver(group.key);
                }}
                onDragLeave={(event) => {
                  // Containment, not a bare clear: dragging across a group's
                  // own children fires `dragleave` at every element boundary,
                  // so clearing unconditionally made the drop indicator blink
                  // on and off all the way down the list.
                  if (event.currentTarget.contains(event.relatedTarget as Node | null)) {
                    return;
                  }
                  setGroupOver((current) => (current === group.key ? null : current));
                }}
                onDrop={(event) => {
                  if (!canDragGroup || !groupDrag) return;
                  event.preventDefault();
                  dropGroup(group.key);
                }}
                className={cn(
                  isGroupTarget && "rounded-row ring-1 ring-[var(--accent)]",
                  groupDrag === group.key && "opacity-40",
                )}
              >
                {showHeaders && (
                  <div
                    draggable={canDragGroup}
                    onDragStart={(event) => {
                      if (!canDragGroup) return;
                      setGroupDrag(group.key);
                      event.dataTransfer.effectAllowed = "move";
                    }}
                    onDragEnd={() => {
                      setGroupDrag(null);
                      setGroupOver(null);
                    }}
                    className={cn(
                      // A hairline under the sticky band, so a header that has
                      // rows scrolling beneath it looks deliberate rather than
                      // like the list is bleeding through.
                      "group/head sticky top-0 z-10 -mx-1.5 flex select-none items-center gap-1 border-b border-[var(--glass-border)] bg-[var(--panel-bg-strong)] px-3 pt-2.5 pb-1.5",
                      canDragGroup && "cursor-grab",
                    )}
                    title={
                      group.workdir
                        ? `${group.workdir}${canDragGroup ? " — drag to reorder" : ""}`
                        : "Chats with no workspace"
                    }
                  >
                    {/* The handle. It has to be in the flow rather than
                        absolute: a header's own padding is only 12px, so an
                        overlay would sit on top of the chevron. The width is
                        reserved whether or not it is hovered, so revealing it
                        cannot shift the title sideways. */}
                    {canDragGroup && (
                      <span
                        aria-hidden="true"
                        className="grid w-3 shrink-0 place-items-center text-faint opacity-0 transition-opacity group-hover/head:opacity-100"
                      >
                        <GripIcon size={12} />
                      </span>
                    )}
                    <button
                      type="button"
                      aria-expanded={!isCollapsed}
                      onClick={() => {
                        // Toggling clears this group's reveal as well, so the
                        // click is what decides from here on rather than the
                        // auto-reveal fighting it back open.
                        setRevealed((current) =>
                          current.filter((entry) => entry !== group.key),
                        );
                        void saveInterface({
                          sidebarCollapsedGroups: toggleCollapsedGroup(
                            config.interface.sidebarCollapsedGroups,
                            group.key,
                          ),
                        });
                      }}
                      // `grab`, not `pointer`: this element covers the whole
                      // header, so the cursor here is the only thing that says
                      // the row can be dragged at all. It is also the collapse
                      // toggle, but a click still works.
                      className={cn(
                        "flex min-w-0 flex-1 items-center gap-1.5 text-left",
                        canDragGroup ? "cursor-grab" : "cursor-pointer",
                      )}
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
                    {shown.map((session) => (
                      <SessionRow
                        key={session.id}
                        session={session}
                        active={session.id === activeId}
                        workspaceTag={
                          grouped
                            ? null
                            : workspaceLabel(session.workdir, config.workspaces)
                        }
                        draggable={grouped}
                        dragging={chatDrag?.id === session.id}
                        dropTarget={
                          chatOver === session.id &&
                          chatDrag !== null &&
                          chatDrag.id !== session.id &&
                          chatDrag.group === group.key
                        }
                        onDragStart={() => setChatDrag({ id: session.id, group: group.key })}
                        onDragEnd={() => {
                          setChatDrag(null);
                          setChatOver(null);
                        }}
                        onDragOver={() => setChatOver(session.id)}
                        onDragLeave={() =>
                          setChatOver((current) =>
                            current === session.id ? null : current,
                          )
                        }
                        onDrop={() => dropChat(session.id, group.key)}
                        onOpen={openRow}
                        onExport={(id) => void exportSession(id)}
                        onDelete={(id) => void deleteSession(id)}
                      />
                    ))}

                    {group.sessions.length === 0 && (
                      <p className="px-2.5 py-2 text-[12px] text-faint">
                        No chats yet
                      </p>
                    )}

                    {hidden > 0 && (
                      <button
                        type="button"
                        onClick={() =>
                          setExpandedGroups((current) => ({
                            ...current,
                            [group.key]: true,
                          }))
                        }
                        className="hover-surface mx-1 my-0.5 flex items-center gap-1.5 rounded-row px-2.5 py-1.5 text-[12px] text-faint"
                      >
                        <ChevronDownIcon size={12} className="shrink-0" />
                        Show {hidden} more
                      </button>
                    )}

                    {expandedGroups[group.key] && group.sessions.length > 5 && (
                      <button
                        type="button"
                        onClick={() =>
                          setExpandedGroups((current) => ({
                            ...current,
                            [group.key]: false,
                          }))
                        }
                        className="hover-surface mx-1 my-0.5 flex items-center gap-1.5 rounded-row px-2.5 py-1.5 text-[12px] text-faint"
                      >
                        <ChevronDownIcon size={12} className="shrink-0 -rotate-180" />
                        Show less
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <div className="border-t border-[var(--glass-border)] p-2">
          <button
            type="button"
            // Settings opens beside the chats rather than over them, so the
            // list stays open and clickable while you are in there.
            onClick={() => setSettingsOpen(true)}
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
 *
 * The whole row is the drag source rather than a handle, which is the pattern
 * every file list uses and needs no extra glyph. Nothing is lost to it: a
 * click is a click, and double-click rename survives because a drag needs
 * movement the browser will not mistake for one.
 */
function SessionRow({
  session,
  active,
  workspaceTag,
  draggable,
  dragging,
  dropTarget,
  onDragStart,
  onDragEnd,
  onDragOver,
  onDragLeave,
  onDrop,
  onOpen,
  onExport,
  onDelete,
}: {
  session: Session;
  active: boolean;
  workspaceTag: string | null;
  draggable: boolean;
  dragging: boolean;
  dropTarget: boolean;
  onDragStart: () => void;
  onDragEnd: () => void;
  onDragOver: () => void;
  onDragLeave: () => void;
  onDrop: () => void;
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
  const model = shortModel(session.modelId);
  const title = [
    failed ? error : activity ? `${activity}${model ? ` · ${model}` : ""}` : null,
    draggable ? "Drag to reorder" : null,
    "Double-click to rename",
  ]
    .filter(Boolean)
    .join(" — ");

  return (
    <div
      className={cn(
        // `select-none`: without it, a drag that crosses the title starts a
        // text selection instead, and the two gestures fight over the same
        // mousedown. The rename input below re-enables selection for itself.
        "group relative flex select-none items-center rounded-row",
        dragging
          ? "opacity-40"
          : active
            ? "bg-[var(--active-bg)]"
            : "hover:bg-[var(--hover-bg)]",
        dropTarget && "ring-1 ring-[var(--accent)]",
        // While renaming, the row must not be a drag source: selecting text in
        // the input is the gesture you actually want there.
        draggable && !renaming && "cursor-grab",
      )}
      draggable={draggable && !renaming}
      onDragStart={(event) => {
        if (!draggable || renaming) return;
        onDragStart();
        event.dataTransfer.effectAllowed = "move";
      }}
      onDragEnd={onDragEnd}
      onDragOver={(event) => {
        if (!draggable) return;
        event.preventDefault();
        onDragOver();
      }}
      onDragLeave={(event) => {
        // Same containment rule as a group's: a row is full of spans, and each
        // boundary crossing would otherwise clear the indicator.
        if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
        onDragLeave();
      }}
      onDrop={(event) => {
        if (!draggable) return;
        event.preventDefault();
        onDrop();
      }}
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
          className="m-1 min-w-0 flex-1 select-text rounded-control border border-[var(--accent)] bg-transparent px-2 py-1 text-[13px]"
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
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-2.5 py-2 text-left"
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
