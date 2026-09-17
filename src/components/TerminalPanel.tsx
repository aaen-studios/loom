import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { create } from "zustand";
import { cn } from "../lib/cn";
import { folderName } from "../lib/workspaces";
import {
  attachTerminal,
  disposeTerminal,
  fitTerminal,
  restyleTerminal,
  searchAddon,
  terminalFor,
  watchTerminalTheme,
} from "../lib/terminals";
import { useDock } from "../stores/dock";
import { usePty } from "../stores/pty";
import { useSettings } from "../stores/settings";
import { useStripRendered } from "../dock/registry";
import { CloseIcon, PlusIcon, SearchIcon, TerminalIcon } from "./icons";

/**
 * The docked terminal.
 *
 * The React side is thin on purpose. Everything that matters — the terminal
 * object, its buffer, its scrollback — lives in `lib/terminals`, keyed by
 * session id, because a tab switch must not destroy a shell's history and a
 * `StrictMode` remount must not build a second one.
 *
 * Output does not flow through React state either: it arrives on `loom://pty`
 * and is written straight into xterm. A shell redrawing a progress bar does so
 * hundreds of times a second, and a `setState` per chunk would re-render the
 * tab strip for every frame of it.
 */

/** Which shell's find bar is open. View state, so it is not in a store. */
const useTerminalFind = create<{ open: boolean; set: (open: boolean) => void }>(
  (set) => ({ open: false, set: (open) => set({ open }) }),
);

/** The folder's shells, so a workspace switch never shows another project's. */
function useFolderSessions(workdir: string | null) {
  const sessions = usePty((state) => state.sessions);
  const activeId = usePty((state) => state.activeId);
  const mine = sessions.filter((session) => session.workdir === workdir);
  const showing = mine.find((session) => session.id === activeId) ?? mine[0] ?? null;
  return { mine, showing };
}

/**
 * The shells as a tab strip.
 *
 * Registered as the terminal's `tabStrip`, so when the terminal is alone in its
 * zone these tabs *are* the zone's tabs: one row instead of two saying almost
 * the same thing, and the `+` that adds a shell sits where a `+` for a panel
 * would be.
 */
export function TerminalTabs({
  workdir,
  standalone,
}: {
  workdir: string | null;
  /**
   * Set when the strip is the *panel's* own rather than the zone's. Inside the
   * dock, an empty terminal panel would immediately be refilled by its own
   * start-a-shell effect — so the `×` would look broken — and closing the panel
   * is the honest outcome. In a torn-off window there is no panel to close, so
   * the window goes away instead.
   */
  standalone?: boolean;
}) {
  const { mine, showing } = useFolderSessions(workdir);
  const setActive = usePty((state) => state.setActive);
  const closeSession = usePty((state) => state.close);
  const open = usePty((state) => state.open);
  const closePanel = useDock((state) => state.closePanel);
  const zone = useDock(
    (state) => state.layout.zones.find((entry) => entry.panels.includes("terminal")) ?? null,
  );
  const find = useTerminalFind((state) => state.open);
  const setFind = useTerminalFind((state) => state.set);
  const [menuOpen, setMenuOpen] = useState(false);
  const profiles = usePty((state) => state.profiles);

  const closeTab = (id: string) => {
    disposeTerminal(id);
    void closeSession(id);
    if (mine.length > 1) return;
    if (zone) closePanel(zone.id, "terminal");
    else if (standalone) {
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
        getCurrentWindow().close(),
      );
    }
  };

  return (
    <div className="flex min-w-0 flex-1 items-center gap-0.5">
      <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
        {mine.map((session) => (
          <div key={session.id} className="group relative flex shrink-0 items-center">
            <button
              type="button"
              onClick={() => setActive(session.id)}
              title={
                session.alive
                  ? `${session.name}${session.workdir ? ` — ${session.workdir}` : ""}`
                  : `${session.name} — this shell has ended`
              }
              className={cn(
                "flex max-w-[190px] items-center gap-1.5 rounded-row py-1 pr-5 pl-2 text-[12px]",
                session.id === showing?.id
                  ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                  : "text-faint hover:bg-[var(--hover-bg)] hover:text-soft",
              )}
            >
              {/* A live dot rather than a close button: which shell is running is
                  the thing you want at a glance, and a dot costs a 4px square
                  where a control costs a hit target. */}
              <span
                aria-hidden="true"
                className={cn(
                  "h-1.5 w-1.5 shrink-0 rounded-full",
                  session.alive ? "bg-emerald-400" : "bg-[var(--ink-faint)]",
                )}
              />
              <span className="truncate">{session.name}</span>
            </button>
            <button
              type="button"
              aria-label={`Close ${session.name}`}
              onClick={() => closeTab(session.id)}
              className="absolute right-0.5 grid h-5 w-5 place-items-center rounded-control text-faint opacity-0 transition-opacity hover:text-[var(--ink)] group-hover:opacity-100"
            >
              <CloseIcon size={11} />
            </button>
          </div>
        ))}
      </div>

      <button
        type="button"
        aria-label="Find in terminal"
        aria-pressed={find}
        title="Find in terminal"
        onClick={() => setFind(!find)}
        className={cn(
          "grid h-7 w-7 shrink-0 place-items-center rounded-control",
          find
            ? "bg-[var(--hover-bg)] text-[var(--ink)]"
            : "text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]",
        )}
      >
        <SearchIcon size={14} />
      </button>

      <button
        type="button"
        aria-label="New shell"
        title="New shell"
        onClick={() => void open({ workdir, forceNew: true })}
        className="grid h-7 w-7 shrink-0 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
      >
        <PlusIcon size={15} />
      </button>

      <div className="relative shrink-0">
        <button
          type="button"
          aria-label="Choose a shell"
          title="Start a specific shell"
          onClick={() => setMenuOpen((value) => !value)}
          className="grid h-7 w-7 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
        >
          <TerminalIcon size={15} />
        </button>
        {menuOpen && (
          <>
            <button
              type="button"
              aria-label="Close the shell menu"
              onClick={() => setMenuOpen(false)}
              className="fixed inset-0 z-40 cursor-default"
            />
            <div className="panel-strong absolute right-0 z-50 mt-1 w-[210px] overflow-hidden rounded-sheet p-1">
              <p className="px-2 pt-1.5 pb-1 text-[11px] font-semibold tracking-[0.08em] text-faint uppercase">
                New shell
              </p>
              {profiles.length === 0 && (
                <p className="px-2 py-1.5 text-[12.5px] text-faint">Looking for shells…</p>
              )}
              {profiles.map((profile) => (
                <button
                  key={profile.id}
                  type="button"
                  onClick={() => {
                    setMenuOpen(false);
                    void open({ workdir, profile: profile.id, name: profile.name });
                  }}
                  className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px] text-soft"
                >
                  <span className="min-w-0 flex-1 truncate">{profile.name}</span>
                  {profile.default && (
                    <span className="shrink-0 text-[10.5px] text-faint">default</span>
                  )}
                </button>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/** The terminal body: the xterm mount, and the find bar when it is open. */
export function TerminalPanel({ workdir }: { workdir: string | null }) {
  const { mine, showing } = useFolderSessions(workdir);
  const opening = usePty((state) => state.opening);
  const error = usePty((state) => state.error);
  const loadProfiles = usePty((state) => state.loadProfiles);
  const open = usePty((state) => state.open);
  const config = useSettings((state) => state.config.terminal);
  const find = useTerminalFind((state) => state.open);
  const setFind = useTerminalFind((state) => state.set);
  // True when the zone header has already drawn this panel's shell tabs.
  const stripRendered = useStripRendered();

  const holderRef = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  // One shell per folder, started lazily on the first reveal. Opening a dock
  // must not spawn a process for a user who is only rearranging the layout.
  // `open` is single-flight per folder, so a StrictMode double-invoke joins the
  // first call rather than starting a second shell.
  useEffect(() => {
    if (showing || opening || mine.length > 0) return;
    void open({ workdir });
  }, [showing, opening, mine.length, workdir, open]);

  const starting = !showing && !error;

  // Create (or re-adopt) the terminal and mount it.
  useLayoutEffect(() => {
    if (!showing?.id) return;
    const box = holderRef.current;
    terminalFor(showing.id, config, {
      rows: Math.max(2, Math.floor((box?.clientHeight ?? 400) / 18)),
      cols: Math.max(2, Math.floor((box?.clientWidth ?? 800) / 8)),
    });
    attachTerminal(showing.id, box);
    // Fitting has to wait for layout, or the first fit measures a zero box and
    // xterm reflows a fresh buffer into one column.
    const frame = requestAnimationFrame(() => fitTerminal(showing.id));
    return () => cancelAnimationFrame(frame);
  }, [showing?.id, config]);

  // Follow the dock's own size. A ResizeObserver rather than a window listener,
  // because the splitter moving does not resize the window — and because the
  // observer coalesces a drag's many size changes into one fit per frame.
  useEffect(() => {
    const node = holderRef.current;
    if (!node || !showing?.id) return;
    const observer = new ResizeObserver(() => fitTerminal(showing.id));
    observer.observe(node);
    return () => observer.disconnect();
  }, [showing?.id]);

  useEffect(() => {
    if (find) requestAnimationFrame(() => searchRef.current?.focus());
    else setQuery("");
  }, [find]);

  const search = showing ? searchAddon(showing.id) : null;
  const focusTerminal = () => holderRef.current?.querySelector("textarea")?.focus?.();

  const runSearch = (next: string) => {
    setQuery(next);
    if (next) search?.findNext(next, { incremental: true, caseSensitive: false });
  };

  /** Sends Ctrl+C, which every shell reads as "clear the current line". */
  const clearScreen = () => {
    if (!showing) return;
    void import("../lib/ipc").then(({ ipc }) => ipc.ptyWrite(showing.id, "\f"));
    focusTerminal();
  };

  const folder = workdir ? folderName(workdir) : null;

  return (
    <div className="terminal-surface relative flex h-full min-h-0 flex-col">
      {/* A torn-off window has no zone header, so the shells need their own row
          there. Inside the dock the zone is drawing exactly this. */}
      {!stripRendered && (
        <div className="flex h-9 shrink-0 items-center gap-0.5 border-b border-[var(--glass-border)] px-1">
          <TerminalTabs workdir={workdir} standalone />
        </div>
      )}

      {find && (
        <div className="flex shrink-0 items-center gap-1 border-b border-[var(--glass-border)] px-2 py-1">
          <SearchIcon size={13} className="shrink-0 text-faint" />
          <input
            ref={searchRef}
            value={query}
            onChange={(event) => runSearch(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setFind(false);
                focusTerminal();
              }
              if (event.key === "Enter") {
                // Shift flips direction, matching every other find bar.
                if (event.shiftKey) search?.findPrevious(query);
                else search?.findNext(query);
              }
            }}
            placeholder="Find in terminal"
            className="min-w-0 flex-1 bg-transparent px-1 py-0.5 text-[12px] text-[var(--ink)] outline-none placeholder:text-[var(--ink-faint)]"
          />
          <button
            type="button"
            onClick={() => search?.findPrevious(query)}
            className="rounded-control px-1.5 py-0.5 text-[11.5px] text-soft hover:bg-[var(--hover-bg)]"
          >
            Prev
          </button>
          <button
            type="button"
            onClick={() => search?.findNext(query)}
            className="rounded-control px-1.5 py-0.5 text-[11.5px] text-soft hover:bg-[var(--hover-bg)]"
          >
            Next
          </button>
        </div>
      )}

      {/* The xterm mount. `relative` and a single absolutely-positioned child,
          so a terminal can never stack under another and double the height. */}
      <div className="relative min-h-0 flex-1">
        <div
          ref={holderRef}
          onClick={focusTerminal}
          // A click in the terminal is the user aiming the keyboard at the
          // shell, so this is one of the few places that should steal focus.
          className="absolute inset-0 overflow-hidden"
        />
        {starting && (
          <div className="pointer-events-none absolute inset-0 grid place-items-center">
            <div className="flex items-center gap-2 text-[12.5px] text-[var(--ink-faint)]">
              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-[var(--accent)]" />
              Starting a shell…
            </div>
          </div>
        )}
        {error && (
          <div className="absolute inset-0 grid place-items-center p-6">
            <div className="max-w-[300px] text-center">
              <p className="text-[12.5px] leading-5 text-[var(--danger)]">{error}</p>
              <button
                type="button"
                onClick={() => void open({ workdir })}
                className="btn-ghost mt-3 px-3 py-1 text-[12.5px]"
              >
                Try again
              </button>
            </div>
          </div>
        )}
      </div>

      {/* A status row, pinned to the bottom. It is the one piece of chrome a
          terminal is genuinely missing without: which shell you are in, which
          folder it opened in, and a way to clear the screen without remembering
          a keystroke. Kept to 24px so it never competes with the buffer. */}
      <div className="flex h-6 shrink-0 items-center gap-2 border-t border-[var(--glass-border)] px-2 text-[11px] text-[var(--ink-faint)]">
        <span
          aria-hidden="true"
          className={cn(
            "h-1.5 w-1.5 shrink-0 rounded-full",
            showing?.alive ? "bg-emerald-400" : "bg-[var(--ink-ghost)]",
          )}
        />
        <span className="shrink-0 truncate">{showing?.name ?? "No shell"}</span>
        {folder && (
          <>
            <span aria-hidden="true" className="text-[var(--ink-ghost)]">
              ·
            </span>
            <span className="min-w-0 truncate" title={workdir ?? undefined}>
              {folder}
            </span>
          </>
        )}
        <span className="min-w-0 flex-1" />
        {mine.length > 1 && (
          <span className="shrink-0 tabular-nums">{mine.length} shells</span>
        )}
        <button
          type="button"
          onClick={clearScreen}
          disabled={!showing}
          title="Clear the screen (Ctrl+L)"
          className={cn(
            "shrink-0 rounded-control px-1.5 py-px",
            showing
              ? "hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
              : "cursor-default opacity-50",
          )}
        >
          Clear
        </button>
      </div>
    </div>
  );
}

/**
 * Restyles every live terminal when the appearance changes.
 *
 * Mounted by the dock rather than by the panel, because a terminal whose panel
 * is closed still has to pick up a font or palette change. A font or line
 * height change is a settings change; a *colour* change is a class on `<html>`,
 * which `watchTerminalTheme` handles — separate because they have separate
 * triggers, and one of them belongs to the document rather than to this store.
 */
export function useTerminalAppearance(): void {
  const config = useSettings((state) => state.config.terminal);
  const sessions = usePty((state) => state.sessions);

  useEffect(() => {
    for (const session of sessions) restyleTerminal(session.id, config);
  }, [sessions, config]);

  useEffect(() => watchTerminalTheme(), []);
}
