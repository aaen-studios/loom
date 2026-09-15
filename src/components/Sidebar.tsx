import { useEffect, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { relativeTime } from "../lib/format";
import { ipc } from "../lib/ipc";
import { isTauri } from "../lib/tauri";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import { DownloadIcon, LoomMark, PlusIcon, SettingsIcon, TrashIcon } from "./icons";

/**
 * Sessions live in a popup, not a permanent panel: the canvas stays full width
 * and centered, and this floats over it when summoned from the titlebar.
 */
export function SidebarPopup() {
  const open = useUi((state) => state.sidebarOpen);
  const setOpen = useUi((state) => state.setSidebarOpen);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const sessions = useChat((state) => state.sessions);
  const activeId = useChat((state) => state.activeId);
  const busy = useChat((state) => state.busy);
  const openSession = useChat((state) => state.openSession);
  const newSession = useChat((state) => state.newSession);
  const deleteSession = useChat((state) => state.deleteSession);
  const renameSession = useChat((state) => state.renameSession);

  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const config = useSettings((state) => state.config);
  const applyRemote = useSettings((state) => state.applyRemote);
  const pinned = config.interface.sidebarPinned;

  /** Pinning keeps the popup open until it is closed explicitly. */
  const togglePin = async () => {
    const updated = await ipc.setInterfaceSettings({
      ...config.interface,
      sidebarPinned: !pinned,
    });
    if (updated) applyRemote(updated);
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  if (!open) return null;

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? sessions.filter(
        (session) =>
          session.title.toLowerCase().includes(needle) ||
          (session.modelId ?? "").toLowerCase().includes(needle),
      )
    : sessions;

  const commitRename = async (id: string) => {
    const title = draft.trim();
    setRenaming(null);
    if (title) await renameSession(id, title);
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
    setOpen(false);
  };

  return (
    <div className="absolute inset-0 z-30 animate-fade-in">
      {/* Pinned means the popup behaves like a panel: no click-away, no scrim. */}
      {!pinned && (
        <button
          type="button"
          aria-label="Close chats"
          onClick={() => setOpen(false)}
          className="absolute inset-0 cursor-default"
        />
      )}

      <aside className="panel-strong animate-fade-up absolute bottom-3 left-3 top-16 flex w-[300px] flex-col overflow-hidden rounded-sheet">
        <div className="flex items-center gap-2 px-3 py-2.5 text-soft">
          <LoomMark size={16} />
          <span className="text-[13px] font-semibold tracking-[0.01em]">Chats</span>
          <button
            type="button"
            title={pinned ? "Unpin (closes when you click away)" : "Pin open"}
            aria-label={pinned ? "Unpin chats" : "Pin chats open"}
            onClick={() => void togglePin()}
            className={cn(
              "ml-auto grid h-7 w-7 place-items-center rounded-control hover:bg-[var(--hover-bg)]",
              pinned ? "text-[var(--accent)]" : "text-faint hover:text-[var(--ink)]",
            )}
          >
            {pinned ? "Pinned" : "Pin"}
          </button>
          <button
            type="button"
            aria-label="Close"
            onClick={() => setOpen(false)}
            className="grid h-7 w-7 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
          >
            ✕
          </button>
        </div>

        <div className="px-2.5">
          <button
            type="button"
            onClick={() => {
              void newSession();
              setOpen(false);
            }}
            className="hover-surface flex w-full items-center gap-2 rounded-row border border-[var(--glass-border)] px-3 py-2 text-left text-[13.5px] font-medium text-soft"
          >
            <PlusIcon size={16} />
            New chat
          </button>
        </div>

        <div className="px-2.5 pt-2">
          <input
            value={query}
            placeholder="Search chats…"
            onChange={(event) => setQuery(event.currentTarget.value)}
            className="w-full rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2.5 py-1.5 text-[12.5px] placeholder:text-[var(--ink-faint)]"
          />
        </div>

        <div className="mt-2 min-h-0 flex-1 overflow-y-auto px-1.5 pb-1.5">
          {visible.length === 0 && (
            <p className="px-2 pt-2 text-[12.5px] leading-5 text-faint">
              {sessions.length === 0 ? "No chats yet." : "No matches."}
            </p>
          )}

          {visible.map((session) => {
            const isActive = session.id === activeId;
            const isBusy = busy[session.id];
            const isRenaming = renaming === session.id;
            return (
              <div
                key={session.id}
                className={cn(
                  "group relative flex items-center rounded-row",
                  isActive ? "bg-[var(--hover-bg)]" : "hover:bg-[var(--hover-bg)]",
                )}
              >
                {isRenaming ? (
                  <input
                    autoFocus
                    value={draft}
                    onChange={(event) => setDraft(event.currentTarget.value)}
                    onBlur={() => void commitRename(session.id)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void commitRename(session.id);
                      if (event.key === "Escape") setRenaming(null);
                    }}
                    className="m-1 min-w-0 flex-1 rounded-control border border-[var(--accent)] bg-transparent px-2 py-1 text-[13px]"
                  />
                ) : (
                  <button
                    type="button"
                    onClick={() => {
                      void openSession(session.id);
                      if (!pinned) setOpen(false);
                    }}
                    onDoubleClick={() => {
                      setRenaming(session.id);
                      setDraft(session.title);
                    }}
                    title="Double-click to rename"
                    className="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2 text-left"
                  >
                    {isBusy && (
                      <span className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-[var(--accent)]" />
                    )}
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13.5px]">
                        {session.title || "New chat"}
                      </span>
                      <span className="block text-[11.5px] text-faint">
                        {relativeTime(session.updatedAt)}
                        {session.modelId
                          ? ` · ${session.modelId.split("/").pop()}`
                          : ""}
                      </span>
                    </span>
                  </button>
                )}

                {!isRenaming && (
                  <span className="mr-1.5 hidden shrink-0 items-center gap-0.5 group-hover:flex">
                    <button
                      type="button"
                      title="Export as markdown"
                      aria-label="Export chat"
                      onClick={() => void exportSession(session.id)}
                      className="grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
                    >
                      <DownloadIcon size={15} />
                    </button>
                    <button
                      type="button"
                      title="Delete chat"
                      aria-label="Delete chat"
                      onClick={() => void deleteSession(session.id)}
                      className="grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--danger)]"
                    >
                      <TrashIcon size={15} />
                    </button>
                  </span>
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
            className="hover-surface flex w-full items-center gap-2 rounded-row px-2.5 py-2 text-left text-[13.5px] text-soft"
          >
            <SettingsIcon size={16} />
            Settings
          </button>
        </div>
      </aside>
    </div>
  );
}
