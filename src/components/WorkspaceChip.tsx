import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { ipc } from "../lib/ipc";
import { useMenu, clipTop } from "../lib/menu";
import { AGENT_MODES, PERMISSION_MODES } from "../lib/modes";
import { isTauri } from "../lib/tauri";
import { useBrowser } from "../stores/browser";
import { useChat } from "../stores/chat";
import { useDock } from "../stores/dock";
import { currentModel, useProviders } from "../stores/providers";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import type { BrowserTier, Workspace } from "../types";
import { SearchField } from "./ui";

/**
 * Above this many saved folders, the popover grows a search box.
 *
 * A threshold rather than always-on: the field costs a row of vertical space,
 * and the common case is three or four folders you can take in at a glance.
 */
const SEARCH_THRESHOLD = 7;

/**
 * The empty tier list, shared.
 *
 * A `?? []` written inside the selector hands the store a brand-new array on
 * every read, and `useSettings` compares by identity: the chip re-rendered on
 * every unrelated store update, twice, because the dev badge reads the same
 * list. Hoisted, the empty case is one stable reference.
 */
const NO_TIERS: BrowserTier[] = [];
import {
  BrainIcon,
  CheckIcon,
  ChevronDownIcon,
  FolderIcon,
  GitBranchIcon,
  GlobeIcon,
  PlusIcon,
  SearchIcon,
  TrashIcon,
} from "./icons";
import { LiquidSurface } from "./LiquidSurface";

/**
 * Workspace chip: the folder this chat's tools may touch, plus the git branch
 * when it is a repository. Folders the user picks are kept in the saved
 * workspace list, so they are one click away in every later chat.
 */
export function WorkspaceChip({ align = "up" }: { align?: "up" | "down" }) {
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const setWorkdir = useChat((state) => state.setWorkdir);
  const ensureSession = useChat((state) => state.ensureSession);
  const workspaces = useSettings((state) => state.config.workspaces);
  const applyRemote = useSettings((state) => state.applyRemote);
  const setSettingsCategory = useUi((state) => state.setSettingsCategory);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const [branch, setBranch] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const { open, setOpen, close } = useMenu("workspace", containerRef);
  const [chunks, setChunks] = useState<number | null>(null);
  const [indexing, setIndexing] = useState(false);
  const [indexNote, setIndexNote] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [nameDraft, setNameDraft] = useState("");
  const [query, setQuery] = useState("");
  /**
   * Where the popover opens, and how tall it may be — measured, not fixed.
   *
   * This chip renders twice: in the title bar at the top of the window, and in
   * the composer's hero card. A menu that always opens one way is wrong in one
   * of those two places, which is why the side and the cap are both derived from
   * the space actually available rather than from the `align` hint alone.
   */
  const [drop, setDrop] = useState({ up: align === "up", maxHeight: 520 });

  const workdir = session?.workdir ?? null;

  useEffect(() => {
    void ipc
      .workspaceInfo(workdir)
      .then((info) => setBranch(info?.branch ?? null));
    if (workdir && session?.id) {
      void ipc.indexStatus(session.id).then((count) => setChunks(count ?? 0));
    }
  }, [workdir, session?.id]);

  // Using a folder is what "adding" it means: it must survive a restart, so
  // any workspace a chat points at is registered once seen.
  useEffect(() => {
    if (!workdir || workspaces.some((workspace) => workspace.path === workdir)) {
      return;
    }
    void ipc.addWorkspace(workdir).then((updated) => {
      if (updated) applyRemote(updated);
    });
  }, [workdir, workspaces, applyRemote]);

  /** Opens toward whichever side has more room, capped to the visible panel. */
  const toggle = () => {
    if (open) {
      close();
      return;
    }
    const rect = containerRef.current?.getBoundingClientRect();
    const gap = 12;
    const above = (rect?.top ?? 0) - clipTop(containerRef.current) - gap;
    const below = window.innerHeight - (rect?.bottom ?? 0) - gap;
    const up = above > below;
    setDrop({
      up,
      maxHeight: Math.max(220, Math.min(560, (up ? above : below) - 4)),
    });
    setOpen(true);
  };

  const choose = async (path: string | null) => {
    close();
    // From the empty state there is no chat yet; choosing a folder starts one.
    await ensureSession();
    await setWorkdir(path);
  };

  const addFolder = async () => {
    if (!isTauri) return;
    const picked = await openDialog({ directory: true, multiple: false });
    if (!picked || typeof picked !== "string") return;
    const updated = await ipc.addWorkspace(picked);
    if (updated) applyRemote(updated);
    await choose(picked);
  };

  const forget = async (workspace: Workspace) => {
    const updated = await ipc.removeWorkspace(workspace.path);
    if (updated) applyRemote(updated);
  };

  const commitRename = async (workspace: Workspace) => {
    const name = nameDraft.trim();
    setRenaming(null);
    if (!name || name === workspace.name) return;
    const updated = await ipc.renameWorkspace(workspace.path, name);
    if (updated) applyRemote(updated);
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

  // A short saved list needs no search box; a long one is unusable without it.
  // The threshold is what keeps the popover from growing a field it never needs.
  const needle = query.trim().toLowerCase();
  const shown =
    needle.length === 0
      ? workspaces
      : workspaces.filter(
          (workspace) =>
            workspace.name.toLowerCase().includes(needle) ||
            workspace.path.toLowerCase().includes(needle),
        );

  return (
    <div className="relative" ref={containerRef}>
      <button
        type="button"
        onClick={toggle}
        title={session?.workdir ?? "Choose a workspace folder for tools"}
        className={cn(
          "hover-surface flex h-8 max-w-[210px] items-center gap-1.5 rounded-full px-2.5 text-[12.5px]",
          folderName ? "text-soft" : "text-faint",
        )}
      >
        <FolderIcon size={15} />
        <span className="max-w-[110px] truncate">
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
        <LiquidSurface
        surface="popovers"
        layout="block"
        tint="var(--panel-bg-strong)"
          className={cn(
            "animate-fade-up absolute z-40 w-[340px] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-sheet p-1.5",
            drop.up ? "bottom-full left-0 mb-2" : "top-full left-0 mt-2",
          )}
          contentStyle={{ maxHeight: drop.maxHeight }}
        >
          {/* The search box only appears once the list is long enough to need
              it. A field over four folders is a row of chrome that never earns
              its place, and the popover is meant to be scanned at a glance. */}
          {workspaces.length > SEARCH_THRESHOLD && (
            <div className="px-0.5 pt-0.5 pb-1.5">
              <SearchField
                value={query}
                onChange={setQuery}
                placeholder="Find a folder…"
              />
            </div>
          )}

          {shown.length > 0 && (
            <p className="px-2 pt-1 pb-1 text-[11px] font-semibold tracking-[0.08em] text-faint uppercase">
              Workspaces
            </p>
          )}

          {workspaces.length > SEARCH_THRESHOLD && shown.length === 0 && (
            <p className="px-2 py-2 text-[12.5px] text-faint">
              No folder matches “{query.trim()}”.
            </p>
          )}

          {shown.map((workspace) => {
            const current = workspace.path === workdir;
            const isRenaming = renaming === workspace.path;
            return (
              <div
                key={workspace.path}
                className={cn(
                  "group relative flex items-center rounded-row",
                  current ? "bg-[var(--hover-bg)]" : "hover:bg-[var(--hover-bg)]",
                )}
              >
                {isRenaming ? (
                  <input
                    autoFocus
                    value={nameDraft}
                    onChange={(event) => setNameDraft(event.currentTarget.value)}
                    onBlur={() => void commitRename(workspace)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void commitRename(workspace);
                      if (event.key === "Escape") setRenaming(null);
                    }}
                    className="m-1 min-w-0 flex-1 rounded-control border border-[var(--accent)] bg-transparent px-2 py-1 text-[13px]"
                  />
                ) : (
                  <button
                    type="button"
                    onClick={() => void choose(workspace.path)}
                    onDoubleClick={() => {
                      setRenaming(workspace.path);
                      setNameDraft(workspace.name);
                    }}
                    title={`${workspace.path} — double-click to rename`}
                    className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left"
                  >
                    <FolderIcon size={15} className="shrink-0 text-faint" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13px] text-soft">
                        {workspace.name}
                      </span>
                      <span className="block truncate text-[11px] text-faint">
                        {workspace.path}
                      </span>
                    </span>
                    {current && (
                      <CheckIcon size={14} className="shrink-0 text-[var(--accent)]" />
                    )}
                  </button>
                )}

                {!isRenaming && (
                  <button
                    type="button"
                    title="Forget this workspace"
                    aria-label="Forget workspace"
                    onClick={() => void forget(workspace)}
                    className="mr-1 hidden h-7 w-7 shrink-0 place-items-center rounded-control text-faint hover:text-[var(--danger)] group-hover:grid"
                  >
                    <TrashIcon size={14} />
                  </button>
                )}
              </div>
            );
          })}

          <button
            type="button"
            onClick={() => void addFolder()}
            className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
          >
            <PlusIcon size={15} />
            Add folder…
          </button>

          {session?.workdir && (
            <button
              type="button"
              onClick={() => void choose(null)}
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
              <button
                type="button"
                onClick={() => {
                  close();
                  setSettingsCategory("memory");
                  setSettingsOpen(true);
                }}
                className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
              >
                <BrainIcon size={15} className="shrink-0 text-faint" />
                Memory…
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
            Tools can only read inside this folder.
          </p>
        </LiquidSurface>
      )}
    </div>
  );
}

/**
 * The chat's behaviour, in one chip: agent mode, tool permissions, and
 * computer use. They are all the same question — what may the model do here —
 * so they live in one menu instead of three competing pills.
 */
export function ModeChip({ compact = false }: { compact?: boolean }) {
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const setAgentMode = useChat((state) => state.setAgentMode);
  const setPermissionMode = useChat((state) => state.setPermissionMode);
  const setComputerAccess = useChat((state) => state.setComputerAccess);
  const setBrowserAccess = useChat((state) => state.setBrowserAccess);
  const paused = useChat((state) =>
    session ? Boolean(state.computerPaused[session.id]) : false,
  );
  const globalAgentMode = useSettings((state) => state.config.chat.agentMode);
  const globalMode = useSettings((state) => state.config.chat.permissionMode);
  const models = useProviders((state) => state.models);
  const defaults = useSettings((state) => state.config.chat);
  const containerRef = useRef<HTMLDivElement>(null);
  const { open, setOpen } = useMenu("mode", containerRef);
  const [drop, setDrop] = useState({ up: true, maxHeight: 620 });

  const agent = session?.agentMode ?? globalAgentMode;
  const permission = session?.permissionMode ?? globalMode;
  const selectedMode = AGENT_MODES.find((mode) => mode.id === agent);
  const selectedPermission = PERMISSION_MODES.find((mode) => mode.id === permission);
  const computerOn = session?.computerAccess ?? false;
  const browserOn = session?.browserAccess ?? false;
  // The dev tier holds `browser_evaluate` and the cookie writer. Saying which
  // is on beats the model discovering it one refused call at a time — and
  // Chat mode refuses the browser outright rather than narrowing it, so there
  // is no badge to show there.
  const browserTiers = useSettings(
    (state) => state.config.browser?.tiers ?? NO_TIERS,
  );
  const browserDev = browserTiers.includes("dev");
  // The chat actually driving the machine: every armed chat has a chip, but
  // only one of them owns the mouse, so only that one gets a Stop button.
  const driver = useChat((state) => state.computerDriver);
  const driving = driver != null && (!session || driver === session.id);
  // Plan and Review refuse every mutating tool, computer ones included, so an
  // armed chip there shows screenshots only. Saying so beats the model
  // discovering it a tool call at a time. Chat refuses the computer tools
  // outright rather than narrowing them, so it shows no badge at all: there is
  // nothing it could look with either.
  const lookOnly = agent === "plan" || agent === "review";
  const model = currentModel(models, session, defaults);
  // A model that cannot see images makes every screenshot worthless; say so
  // rather than letting the turn burn tokens on blind clicks.
  const blind =
    model != null &&
    model.spec.inputModalities.length > 0 &&
    !model.spec.inputModalities.includes("image");

  const tone =
    selectedPermission?.tone === "danger"
      ? "danger"
      : selectedMode?.tone === "accent" || selectedPermission?.tone === "accent"
        ? "accent"
        : "neutral";

  /** Opens toward whichever side has more room, capped to the visible panel. */
  const toggle = () => {
    if (open) {
      setOpen(false);
      return;
    }
    // Who holds the computer is engine state, not something the UI can derive
    // from its own events alone (a chat armed while another drives, a turn that
    // ended while this window was closed). Refreshing on open is cheap and
    // keeps the Stop button honest.
    void useChat.getState().refreshComputerDriver();
    const rect = containerRef.current?.getBoundingClientRect();
    const gap = 12;
    const above = (rect?.top ?? 0) - clipTop(containerRef.current) - gap;
    const below = window.innerHeight - (rect?.bottom ?? 0) - gap;
    const up = above > below;
    setDrop({
      up,
      maxHeight: Math.max(240, Math.min(620, (up ? above : below) - 4)),
    });
    setOpen(true);
  };

  return (
    <div className="relative shrink-0" ref={containerRef}>
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        title={`Agent mode: ${selectedMode?.help ?? ""} · Permissions: ${selectedPermission?.help ?? ""}`}
        className={cn(
          "flex items-center gap-1.5 whitespace-nowrap rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12.5px] transition",
          tone === "danger"
            ? "text-[var(--danger)]"
            : tone === "accent"
              ? "text-[var(--accent)]"
              : "text-soft",
        )}
      >
        {agent === "plan" ? (
          <ModeIcon />
        ) : agent === "review" ? (
          <SearchIcon size={13} />
        ) : agent === "chat" ? (
          <ChatIcon />
        ) : (
          <BuildIcon />
        )}
        {/* Two labels and a separator are about 90px, which is the single
            biggest thing standing between this row and fitting a narrow
            composer. The glyph and the tone already carry the meaning — a
            danger-toned chip means Ask, an accent one means auto — and the
            `title` above spells both out. */}
        {!compact && (
          <>
            {selectedMode?.label ?? "Build"}
            <span className="text-faint">·</span>
            {selectedPermission?.label ?? "Ask"}
          </>
        )}
        {(computerOn || paused) && agent !== "chat" && (
          <MonitorIcon
            size={12}
            className={paused ? "text-[var(--danger)]" : "text-[var(--accent)]"}
          />
        )}
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <LiquidSurface
          surface="popovers"
          layout="block"
          className={cn(
            "animate-fade-up absolute left-0 z-40 w-[336px] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-sheet",
            drop.up ? "bottom-full mb-2" : "top-full mt-2",
          )}
          contentStyle={{ maxHeight: drop.maxHeight }}
          contentClassName="p-1.5"
          tint="var(--panel-bg-strong)"
        >
          <p className="px-2 pt-1.5 pb-1 text-[10px] font-semibold tracking-[0.1em] text-faint uppercase">
            Mode
          </p>
          <div className="grid grid-cols-2 gap-1 px-0.5">
            {AGENT_MODES.map((mode) => {
              const active = agent === mode.id;
              return (
                <button
                  key={mode.id}
                  type="button"
                  onClick={() => {
                    void setAgentMode(mode.id === globalAgentMode ? null : mode.id);
                  }}
                  className={cn(
                    "flex flex-col items-start gap-0.5 rounded-row border px-2.5 py-2 text-left transition",
                    active
                      ? "border-[var(--glass-border-strong)] bg-[var(--hover-bg)]"
                      : "border-transparent hover:bg-[var(--hover-bg)]",
                  )}
                >
                  <span className="flex items-center gap-1.5">
                    {mode.id === "plan" ? (
                      <ModeIcon />
                    ) : mode.id === "review" ? (
                      <SearchIcon size={13} />
                    ) : mode.id === "chat" ? (
                      <ChatIcon />
                    ) : (
                      <BuildIcon />
                    )}
                    <span
                      className={cn(
                        "text-[13px]",
                        active ? "text-[var(--ink)]" : "text-soft",
                      )}
                    >
                      {mode.label}
                    </span>
                  </span>
                  <span className="text-[10.5px] leading-4 text-faint">
                    {AGENT_NOTES[mode.id]}
                  </span>
                </button>
              );
            })}
          </div>

          <div className="mx-1.5 my-1.5 h-px bg-[var(--glass-border)]" />

          <p className="px-2 pb-1 text-[10px] font-semibold tracking-[0.1em] text-faint uppercase">
            Approvals
          </p>
          {PERMISSION_MODES.map((mode) => {
            const active = permission === mode.id;
            return (
              <button
                key={mode.id}
                type="button"
                onClick={() => {
                  void setPermissionMode(mode.id === globalMode ? null : mode.id);
                }}
                className={cn(
                  "flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left transition",
                  active ? "bg-[var(--hover-bg)]" : "hover:bg-[var(--hover-bg)]",
                )}
              >
                <span
                  className={cn(
                    "grid h-3.5 w-3.5 shrink-0 place-items-center rounded-full border",
                    active
                      ? "border-[var(--accent)]"
                      : "border-[var(--glass-border-strong)]",
                  )}
                >
                  {active && (
                    <span className="h-1.5 w-1.5 rounded-full bg-[var(--accent)]" />
                  )}
                </span>
                <span
                  className={cn(
                    "shrink-0 text-[12.5px]",
                    active ? "text-[var(--ink)]" : "text-soft",
                  )}
                >
                  {mode.label}
                </span>
                <span className="min-w-0 flex-1 truncate text-[11px] text-faint">
                  {PERMISSION_NOTES[mode.id]}
                </span>
                {mode.id === "atelier" && (
                  <span className="shrink-0 text-[9.5px] font-semibold tracking-[0.08em] text-[var(--accent)] uppercase">
                    self-edit
                  </span>
                )}
              </button>
            );
          })}

          <div className="mx-1.5 my-1.5 h-px bg-[var(--glass-border)]" />

          <p className="px-2 pb-1 text-[10px] font-semibold tracking-[0.1em] text-faint uppercase">
            Computer
          </p>
          <div
            className={cn(
              "flex items-center gap-2.5 rounded-row px-2 py-1.5",
              paused && "bg-[var(--danger)]/10",
            )}
          >
            <MonitorIcon
              className={cn(
                "shrink-0",
                paused
                  ? "text-[var(--danger)]"
                  : computerOn
                    ? "text-[var(--accent)]"
                    : "text-faint",
              )}
            />
            <div className="min-w-0 flex-1">
              <p className="text-[12.5px] text-soft">Let Loom drive this PC</p>
              <p className="truncate text-[11px] text-faint">
                {paused
                  ? "Paused while you use the machine"
                  : computerOn && lookOnly
                    ? "Screenshots only in this mode"
                    : computerOn
                      ? "Screen, mouse and keyboard"
                      : blind
                        ? "Needs a vision model"
                        : "Off for this chat"}
              </p>
            </div>
            {paused ? (
              <div className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  onClick={() => void ipc.resumeComputer()}
                  className="rounded-capsule bg-[var(--control-bg)] px-2.5 py-1 text-[11.5px] font-medium text-[var(--control-ink)]"
                >
                  Resume
                </button>
                <button
                  type="button"
                  onClick={() => void ipc.stopComputer()}
                  className="rounded-capsule border border-[var(--glass-border)] px-2.5 py-1 text-[11.5px] text-soft hover:text-[var(--danger)]"
                >
                  Stop
                </button>
              </div>
            ) : (
              <>
                {computerOn && driving && (
                  <button
                    type="button"
                    onClick={() => void ipc.stopComputer()}
                    className="shrink-0 rounded-capsule px-1.5 py-1 text-[11.5px] text-faint transition hover:text-[var(--danger)]"
                  >
                    Stop
                  </button>
                )}
                <Switch
                  on={computerOn}
                  disabled={blind}
                  label="Computer use"
                  onToggle={() => void setComputerAccess(!computerOn)}
                />
              </>
            )}
          </div>

          <div className="mx-1.5 my-1.5 h-px bg-[var(--glass-border)]" />

          <p className="px-2 pb-1 text-[10px] font-semibold tracking-[0.1em] text-faint uppercase">
            Browser
          </p>
          <div className="flex items-center gap-2.5 rounded-row px-2 py-1.5">
            <GlobeIcon
              size={13}
              className={cn(
                "shrink-0",
                browserOn ? "text-[var(--accent)]" : "text-faint",
              )}
            />
            <div className="min-w-0 flex-1">
              <p className="text-[12.5px] text-soft">Let Loom use the browser</p>
              <p className="truncate text-[11px] text-faint">
                {browserOn
                  ? lookOnly
                    ? "Reading pages only in this mode"
                    : browserDev
                      ? "Pages, tabs, cookies and JavaScript"
                      : "Pages, tabs and clicks"
                  : "Off for this chat"}
              </p>
            </div>
            <button
              type="button"
              onClick={() => {
                // Off is a revocation, not a note for later: the engine stops
                // the turn, so there is no Stop button to offer beside it.
                void setBrowserAccess(!browserOn);
                // Opening the panel is what makes the chip useful — an armed
                // browser with nowhere to show a page is a switch that does
                // nothing visible.
                if (!browserOn) openBrowserPanel();
              }}
              aria-label="Browser use"
              role="switch"
              aria-checked={browserOn}
              className={cn(
                "relative h-[18px] w-8 shrink-0 rounded-full border transition",
                browserOn
                  ? "border-transparent bg-[var(--accent)]"
                  : "border-[var(--glass-border-strong)] bg-[var(--ink-ghost)]",
              )}
            >
              <span
                className={cn(
                  "absolute top-1/2 h-3 w-3 -translate-y-1/2 rounded-full bg-white shadow-sm transition-all",
                  browserOn ? "left-[16px]" : "left-[2px]",
                )}
              />
            </button>
          </div>
        </LiquidSurface>
      )}
    </div>
  );
}

/**
 * Opens the browser panel, in the active chat's folder.
 *
 * Through the dock store rather than a bespoke window call, so the panel obeys
 * the same rules as every other: it goes to its own edge, it is draggable, it
 * can be torn off, and its arrangement is remembered per folder.
 */
function openBrowserPanel() {
  const dock = useDock.getState();
  dock.openPanel("browser", "right");
  void useBrowser.getState().load();
}

const AGENT_NOTES: Record<string, string> = {
  plan: "Inspect, propose, don't touch",
  build: "Change files and run commands",
  review: "Find issues, propose fixes",
  chat: "Answer only, search at most",
};

const PERMISSION_NOTES: Record<string, string> = {
  ask: "Confirm each call",
  "auto-read-only": "Reads run silently",
  "auto-all": "No confirmations",
  atelier: "The model may edit Loom itself",
};

/** The one switch in the app: computer use, the standing consent. */
function Switch({
  on,
  disabled,
  label,
  onToggle,
}: {
  on: boolean;
  disabled?: boolean;
  label: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={onToggle}
      className={cn(
        "relative h-[18px] w-8 shrink-0 rounded-full border transition",
        on
          ? "border-transparent bg-[var(--accent)]"
          : "border-[var(--glass-border-strong)] bg-[var(--ink-ghost)]",
        disabled && "opacity-40",
      )}
    >
      <span
        className={cn(
          "absolute top-1/2 h-3 w-3 -translate-y-1/2 rounded-full bg-white shadow-sm transition-all",
          on ? "left-[16px]" : "left-[2px]",
        )}
      />
    </button>
  );
}

function BuildIcon() {
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
      <path d="M13.5 6.5l4 4" />
      <path d="M15.2 4.8l1.3-1.3a1.3 1.3 0 0 1 1.8 0l2.2 2.2a1.3 1.3 0 0 1 0 1.8l-1.3 1.3" />
      <path d="M13.9 6.5l3.6 3.6-7.9 7.9a2 2 0 0 1-1.4.6H6.4v-1.8a2 2 0 0 1 .6-1.4z" />
    </svg>
  );
}

function MonitorIcon({
  size = 13,
  className,
}: {
  size?: number;
  className?: string;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
    >
      <rect x="3" y="4.5" width="18" height="12" rx="2" />
      <path d="M9 20h6M12 16.5V20" />
    </svg>
  );
}

/** Per-chat agent mode (falls back to the global default). */
function ModeIcon() {
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
      <path d="M9 4.5l6 1.5 6-1.5v13l-6 1.5-6-1.5-6 1.5v-13z" />
      <path d="M9 4.5v13M15 6v13" />
    </svg>
  );
}

/**
 * Pure chat: an empty speech bubble with a spark. Deliberately the lightest of
 * the four glyphs — the mode is the one with nothing behind it but words.
 */
function ChatIcon() {
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
      <path d="M20 12.5a7 7 0 0 1-7 7H8l-4 3v-4.6A7 7 0 0 1 3 12.5v-1a7 7 0 0 1 7-7h3a7 7 0 0 1 7 7z" />
      <path d="M12 7.6l.85 2.05L15 10.5l-2.15.85L12 13.4l-.85-2.05L9 10.5l2.15-.85z" />
    </svg>
  );
}
