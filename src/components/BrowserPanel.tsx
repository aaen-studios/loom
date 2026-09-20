import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { BlockingStatus } from "../types";
import { cn } from "../lib/cn";
import { createSlotReporter, resolveOmnibox } from "../lib/browserSlot";
import { ipc } from "../lib/ipc";
import { useTauriEvent } from "../lib/listen";
import { isTauri } from "../lib/tauri";
import { useBrowser } from "../stores/browser";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import {
  ChevronDownIcon,
  GlobeIcon,
  PlusIcon,
  RefreshIcon,
  ShieldIcon,
  TrashIcon,
} from "./icons";

/**
 * The built-in browser.
 *
 * ## Where the page is
 *
 * Not in this component, and not in a window either. Each tab is a **child
 * webview** of the window this panel is rendered in — see
 * `src-tauri/src/browser.rs` for why that matters — and this panel's job is to
 * say *where* it should be. The `slot` div below is that rectangle.
 *
 * The coordinates are the ones this panel already measures in: a
 * `getBoundingClientRect` in logical pixels, which is what the shell positions
 * child webviews with. Nothing is converted, so there is no window position or
 * scale factor that can go stale.
 *
 * ## The one rule the chrome obeys
 *
 * **Nothing here may be drawn over the slot.** A child webview paints above its
 * window's HTML, so an overlay opening across the page would be hidden behind
 * it. Every overlay in this panel therefore opens *away* from the slot — the
 * rail and the blocking summary sit below it, the omnibox above it — and that is
 * a constraint rather than a style preference.
 */
export function BrowserPanel() {
  const tabs = useBrowser((state) => state.tabs);
  const downloads = useBrowser((state) => state.downloads);
  const loaded = useBrowser((state) => state.loaded);
  const host = useBrowser((state) => state.host);
  const railOpen = useBrowser((state) => state.railOpen);
  const activity = useBrowser((state) => state.activity);
  const load = useBrowser((state) => state.load);
  const open = useBrowser((state) => state.open);
  const close = useBrowser((state) => state.close);
  const focus = useBrowser((state) => state.focus);
  const navigate = useBrowser((state) => state.navigate);
  const reportSlot = useBrowser((state) => state.reportSlot);
  const setRailOpen = useBrowser((state) => state.setRailOpen);
  const note = useBrowser((state) => state.note);

  const sessionId = useChat((state) => state.activeId);
  const blockingEnabled = useSettings(
    (state) => state.config.browser?.blocking?.enabled ?? false,
  );

  const slotRef = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [blocking, setBlocking] = useState<BlockingStatus | null>(null);

  /** Only this host's tabs, and which of them it is showing. */
  const mine = useMemo(
    () => (host ? tabs.filter((tab) => tab.host === host) : tabs),
    [tabs, host],
  );
  const activeId = useMemo(
    () => mine.find((tab) => tab.active)?.id ?? mine[0]?.id ?? null,
    [mine],
  );
  const active = useMemo(
    () => mine.find((tab) => tab.id === activeId) ?? null,
    [mine, activeId],
  );

  useEffect(() => {
    void load();
  }, [load]);

  // What the blocker is doing, so the panel can say so rather than leaving a
  // user to guess whether an ad is missing because of Loom.
  useEffect(() => {
    if (!blockingEnabled) {
      setBlocking(null);
      return;
    }
    void ipc.browserBlockingStatus().then(setBlocking);
  }, [blockingEnabled, tabs.length]);

  /* ---------------------------------------------------------------------
     The slot.

     One reporter, driven by everything that can move it: the element's own size
     (`ResizeObserver`), the window's (`onResized`), and this panel's scrolling.
     The last is the easiest to miss and the most likely to be hit, because the
     rail at the bottom changes height and pushes the slot around.
     --------------------------------------------------------------------- */
  useEffect(() => {
    if (!host) return;
    const reporter = createSlotReporter((slot) => reportSlot(activeId, slot));

    const measure = () => {
      const node = slotRef.current;
      if (!node) return;
      const rect = node.getBoundingClientRect();
      reporter.report({
        left: rect.left,
        top: rect.top,
        width: rect.width,
        height: rect.height,
      });
    };

    const observer = new ResizeObserver(measure);
    if (slotRef.current) observer.observe(slotRef.current);
    // Also the panel itself: the slot's own box can stay the same while its
    // *position* moves, which a `ResizeObserver` on it would not see.
    const panel = slotRef.current?.closest("[data-browser-panel]");
    if (panel) observer.observe(panel);
    measure();

    let disposeResize: (() => void) | undefined;
    void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
      void getCurrentWindow()
        .onResized(measure)
        .then((unlisten) => {
          disposeResize = unlisten;
        })
        .catch(() => {});
    });

    const scroller = slotRef.current?.closest("[data-browser-scroll]");
    scroller?.addEventListener("scroll", measure);

    return () => {
      observer.disconnect();
      scroller?.removeEventListener("scroll", measure);
      disposeResize?.();
      reporter.stop();
      // Park every tab: the panel is going away, and a page left drawn over the
      // transcript with no panel under it is worse than one that is not drawn.
      // Nothing is destroyed — a parked tab keeps running.
      reportSlot(null, null);
    };
  }, [host, activeId, reportSlot, railOpen, blocking]);

  /* ---------------------------------------------------------------------
     What the model is doing, from the engine's own event stream.

     A second subscription on `loom://event` rather than reading the chat store:
     the store holds tool calls for the transcript, and this wants a different
     projection — a short feed naming the tab and the URL.
     --------------------------------------------------------------------- */
  useTauriEvent<{ type: string; name?: string; arguments?: string; sessionId?: string }>(
    "loom://event",
    (event) => {
      if (event.type !== "toolCallStarted" || !event.name?.startsWith("browser_")) return;
      let url = "";
      let summary = event.name.replace("browser_", "").replace(/_/g, " ");
      try {
        const args = JSON.parse(event.arguments ?? "{}") as Record<string, unknown>;
        url = typeof args.url === "string" ? args.url : "";
        const target = args.target ?? args.from ?? args.selector;
        if (url) summary = `${summary} ${url}`;
        else if (typeof target === "string") summary = `${summary} ${target}`;
      } catch {
        // Arguments that are not JSON are already reported to the model; there
        // is nothing useful to add to the rail.
      }
      note({ actor: "Loom", summary, url, sessionId: event.sessionId ?? null });
    },
  );

  const go = useCallback(
    (value: string) => {
      const resolved = resolveOmnibox(value, "https://duckduckgo.com/?q=");
      if (!resolved) return;
      if (active) void navigate(active.id, "goto", resolved);
      else void open(resolved, "normal", sessionId ?? undefined);
    },
    [active, navigate, open, sessionId],
  );

  /* --------------------------------------------------------------------- */

  if (!loaded) {
    return (
      <div className="grid h-full place-items-center p-6">
        <p className="text-[12.5px] text-faint">Opening the browser…</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-browser-panel>
      {/* Tab strip. Everything here is above the slot, and the rail is below it,
          so no part of this chrome ever overlaps the page. */}
      <div className="flex shrink-0 items-center gap-0.5 overflow-x-auto px-1.5 pt-1.5">
        {mine.map((tab) => (
          <div key={tab.id} className="group flex shrink-0 items-center">
            <button
              type="button"
              onClick={() => void focus(tab.id, sessionId)}
              title={`${tab.title || tab.url}\n${tab.url}`}
              className={cn(
                "flex max-w-[190px] items-center gap-1.5 rounded-row py-1 pr-1 pl-2 text-[12px] transition",
                tab.id === activeId
                  ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                  : "text-faint hover:bg-[var(--hover-bg)]",
              )}
            >
              {tab.loading ? (
                <span className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-[var(--accent)]" />
              ) : (
                <GlobeIcon
                  size={12}
                  className={cn(
                    "shrink-0",
                    tab.profile === "ghost" ? "text-[var(--accent)]" : "text-faint",
                  )}
                />
              )}
              <span className="min-w-0 truncate">
                {tab.profile === "ghost" && (
                  <span className="mr-1 text-[10px] tracking-wide text-[var(--accent)] uppercase">
                    ghost
                  </span>
                )}
                {tab.title || tab.url || "New tab"}
              </span>
              {/* A tab the model is working in is marked, because the whole point
                  of sharing a browser is being able to see that. */}
              {tab.driving && (
                <span
                  className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--accent)]"
                  title="A chat is working in this tab"
                />
              )}
            </button>
            <button
              type="button"
              onClick={() => void close(tab.id)}
              aria-label="Close tab"
              className="ml-0.5 rounded-full px-1 text-faint opacity-0 transition group-hover:opacity-100 hover:text-[var(--danger)]"
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={() => void open("about:blank", "normal", sessionId ?? undefined)}
          aria-label="New tab"
          title="New tab"
          className="shrink-0 rounded-full px-1.5 py-1 text-faint hover:text-[var(--ink)]"
        >
          <PlusIcon size={13} />
        </button>
      </div>

      {/* Toolbar, above the slot. */}
      <div className="flex shrink-0 items-center gap-1 px-1.5 py-1.5">
        <button
          type="button"
          disabled={!active}
          onClick={() => active && void navigate(active.id, "back")}
          title="Back"
          className="rounded-full px-2 py-1 text-[12px] text-faint hover:text-[var(--ink)] disabled:opacity-30"
        >
          ‹
        </button>
        <button
          type="button"
          disabled={!active}
          onClick={() => active && void navigate(active.id, "forward")}
          title="Forward"
          className="rounded-full px-2 py-1 text-[12px] text-faint hover:text-[var(--ink)] disabled:opacity-30"
        >
          ›
        </button>
        <button
          type="button"
          disabled={!active}
          onClick={() => active && void navigate(active.id, active.loading ? "stop" : "reload")}
          title={active?.loading ? "Stop" : "Reload"}
          className="rounded-full px-2 py-1 text-faint hover:text-[var(--ink)] disabled:opacity-30"
        >
          {active?.loading ? (
            <span className="block text-[12px] leading-none">×</span>
          ) : (
            <RefreshIcon size={13} />
          )}
        </button>
        <input
          value={draft ?? active?.url ?? ""}
          onChange={(event) => setDraft(event.target.value)}
          onFocus={() => setDraft((current) => current ?? active?.url ?? "")}
          onBlur={() => setDraft(null)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              go((draft ?? active?.url ?? "").trim());
              (event.target as HTMLInputElement).blur();
            }
            if (event.key === "Escape") {
              setDraft(null);
              (event.target as HTMLInputElement).blur();
            }
          }}
          placeholder="Search or enter a URL"
          spellCheck={false}
          className="min-w-0 flex-1 rounded-control border border-[var(--glass-border)] bg-[var(--ink-ghost)] px-2.5 py-1 font-mono text-[11.5px] text-[var(--ink)] outline-none focus:border-[var(--accent)]"
        />
        {blocking?.installed && (
          <span
            title={
              `${blocking.rules} filter rules from ${blocking.sources.length} list(s)\n` +
              `${blocking.blocked} requests blocked this session`
            }
            className="flex shrink-0 items-center gap-1 rounded-capsule border border-[var(--glass-border)] px-2 py-1 text-[11px] text-faint"
          >
            <ShieldIcon size={12} className="text-[var(--accent)]" />
            {blocking.blocked}
          </span>
        )}
      </div>

      {/* The slot. The page is a child webview of this window drawn exactly over
          this rectangle, so anything placed *inside* it would be hidden. */}
      <div data-browser-scroll className="relative min-h-0 flex-1 overflow-hidden px-1.5">
        <div
          ref={slotRef}
          className={cn(
            "h-full w-full",
            // Only drawn when there is no page over it, so the frame is the empty
            // state rather than a border under a live page.
            !active && "rounded-row border border-dashed border-[var(--glass-border)]",
          )}
        >
          {!active && (
            <NewTab onGo={(url) => void open(url, "normal", sessionId ?? undefined)} />
          )}
          {active && !isTauri && (
            <div className="grid h-full place-items-center p-6 text-center">
              <p className="max-w-[280px] text-[12px] leading-5 text-faint">
                The built-in browser needs the Loom app, not a plain browser tab.
              </p>
            </div>
          )}
        </div>
      </div>

      {/* The activity rail, below the slot. Every URL the model touched, and what
          it did — which is what makes "the chip is the consent" trustworthy
          rather than merely convenient. */}
      <div className="shrink-0 border-t border-[var(--glass-border)]">
        <button
          type="button"
          onClick={() => setRailOpen(!railOpen)}
          className="flex w-full items-center gap-1.5 px-2 py-1 text-left text-[11px] text-faint hover:text-[var(--ink)]"
        >
          <ChevronDownIcon
            size={11}
            className={cn("transition-transform", !railOpen && "-rotate-90")}
          />
          <span>
            {activity.length === 0
              ? "No browser activity yet"
              : `${activity.length} browser action${activity.length === 1 ? "" : "s"}`}
          </span>
          {blockingEnabled && !blocking?.installed && (
            <span className="ml-auto text-[10.5px] text-[var(--accent)]">
              loading filters…
            </span>
          )}
          {blocking?.installed && (
            <span className="ml-auto text-[10.5px] text-faint">
              {blocking.rules.toLocaleString()} rules
            </span>
          )}
        </button>
        {railOpen && (activity.length > 0 || downloads.length > 0) && (
          <ul className="max-h-[132px] overflow-y-auto px-2 pb-1">
            {downloads.slice(0, 3).map((download) => (
              <li
                key={`${download.at}-${download.path}`}
                className="flex items-baseline gap-2 py-0.5 text-[11.5px]"
              >
                <span className="shrink-0 text-soft">download</span>
                <span
                  className={cn(
                    "min-w-0 flex-1 truncate",
                    download.ok ? "text-faint" : "text-[var(--danger)]",
                  )}
                  title={download.path}
                >
                  {download.ok ? download.path : `failed — ${download.url}`}
                </span>
              </li>
            ))}
            {activity
              .slice()
              .reverse()
              .slice(0, 40)
              .map((entry) => (
                <li key={entry.id} className="flex items-baseline gap-2 py-0.5 text-[11.5px]">
                  <span className="shrink-0 font-mono text-[10.5px] text-faint">
                    {new Date(entry.at).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </span>
                  <span
                    className={cn(
                      "shrink-0",
                      entry.actor === "you" ? "text-soft" : "text-[var(--accent)]",
                    )}
                  >
                    {entry.actor}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-faint" title={entry.url}>
                    {entry.summary}
                  </span>
                </li>
              ))}
          </ul>
        )}
        {railOpen && activity.length > 0 && (
          <button
            type="button"
            onClick={() => useBrowser.getState().clearActivity()}
            className="flex w-full items-center gap-1.5 px-2 pb-1.5 text-[10.5px] text-faint hover:text-[var(--danger)]"
          >
            <TrashIcon size={10} />
            Clear
          </button>
        )}
      </div>
    </div>
  );
}

/** The page an empty tab shows. Rendered by React, so it costs no webview. */
function NewTab({ onGo }: { onGo: (url: string) => void }) {
  const [value, setValue] = useState("");
  const submit = () => {
    const resolved = resolveOmnibox(value, "https://duckduckgo.com/?q=");
    if (resolved) onGo(resolved);
  };
  return (
    <div className="grid h-full place-items-center p-6">
      <div className="w-full max-w-[320px] text-center">
        <GlobeIcon size={22} className="mx-auto text-faint" />
        <p className="mt-2 text-[13px] text-soft">No page open</p>
        <p className="mt-1 text-[11.5px] leading-5 text-faint">
          Loom's own browser, sharing the tabs the assistant uses. Logins carry
          over, a private tab keeps nothing, and ads are blocked at the network.
        </p>
        <input
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") submit();
          }}
          placeholder="example.com"
          spellCheck={false}
          className="mt-3 w-full rounded-control border border-[var(--glass-border)] bg-[var(--ink-ghost)] px-2.5 py-1.5 text-center font-mono text-[11.5px] text-[var(--ink)] outline-none focus:border-[var(--accent)]"
        />
      </div>
    </div>
  );
}
