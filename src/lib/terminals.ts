import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { ipc } from "./ipc";
import { currentXtermTheme } from "./xtermTheme";
import type { TerminalConfig } from "../types";

/**
 * The live xterm instances, held outside React on purpose.
 *
 * A `Terminal` is a large object with its own DOM, its own scrollback and its
 * own input handling. Putting one in a store would re-render whatever subscribes
 * on every prompt redraw, and putting it in component state would destroy it on
 * every tab switch — losing the scrollback, which is the thing a terminal is
 * for. So each shell's terminal is created once, owns a `<div>`, and that div is
 * moved into whichever React node is currently showing it.
 *
 * That is also what makes React's `StrictMode` double-mount harmless: the effect
 * reattaches the same terminal rather than building a second one, so no output
 * is lost between the unmount and the remount.
 *
 * Disposal is deliberate and rare — only when a tab is actually closed, via
 * `disposeTerminal`. Nothing unmounting is a reason to throw away a shell.
 */

interface Live {
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  /** The element xterm rendered into; moved between React nodes. */
  element: HTMLDivElement;
  /** Removes the pty-input and resize subscriptions. */
  detachInput: () => void;
}

const live = new Map<string, Live>();

/**
 * xterm's options for this app.
 *
 * `allowTransparency` is on because the theme's background is deliberately
 * transparent: the tint and the blur belong to the panel around the terminal
 * (`.terminal-surface`), not to the canvas. That keeps one declaration for the
 * surface, and — the reason it matters — keeps xterm away from the alpha
 * channel it cannot parse. See `lib/xtermTheme.ts`.
 */
export function buildOptions(
  config: TerminalConfig,
): ConstructorParameters<typeof Terminal>[0] {
  return {
    fontFamily: `"${config.fontFamily}", ui-monospace, Consolas, monospace`,
    fontSize: config.fontSize,
    lineHeight: config.lineHeight / 100,
    letterSpacing: 0,
    cursorBlink: true,
    cursorStyle: "bar",
    // A terminal without a selection colour is one you cannot copy out of
    // confidently, and the default is a washed-out grey on every background.
    rightClickSelectsWord: true,
    allowTransparency: true,
    convertEol: false,
    scrollback: 10000,
    macOptionIsMeta: true,
    drawBoldTextInBrightColors: true,
    theme: currentXtermTheme(),
  };
}

/**
 * The terminal for a session, created on first use.
 *
 * `rows`/`cols` seed the geometry so the very first paint is already the right
 * size: a shell that starts at 80 columns and is told otherwise a moment later
 * prints its first prompt at the wrong width.
 */
export function terminalFor(
  id: string,
  config: TerminalConfig,
  geometry?: { rows: number; cols: number },
): Live {
  const existing = live.get(id);
  if (existing) return existing;

  // Positioned absolutely inside its holder so that two terminals can never
  // stack and double the scroll height — which is exactly what happened before:
  // a second session appended its element under the first, the holder grew to
  // 200%, and the panel showed a stray scrollbar beside a mistyped command.
  const element = document.createElement("div");
  element.className = "absolute inset-0";
  element.style.padding = "6px 8px";

  const term = new Terminal({
    ...buildOptions(config),
    ...(geometry ? { rows: geometry.rows, cols: geometry.cols } : {}),
  });
  const fit = new FitAddon();
  const search = new SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(
    new WebLinksAddon((event, uri) => {
      // Ctrl-click opens, matching the terminal convention, because a plain
      // click on a path in build output should not launch a browser.
      if (event.ctrlKey || event.metaKey) {
        void import("@tauri-apps/plugin-opener").then((opener) =>
          opener.openUrl(uri),
        );
      }
    }),
  );
  term.open(element);

  // Typed keys go to the shell, byte for byte: xterm's `onData` already
  // produces the exact escape sequences a tty expects, which is why the pty
  // write takes a string rather than trying to interpret keystrokes here.
  const input = term.onData((data) => {
    void ipc.ptyWrite(id, data);
  });
  // Bracketed paste arrives as one `onData` burst; xterm sends the markers.
  const resize = term.onResize(({ rows, cols }) => {
    void ipc.ptyResize(id, rows, cols);
  });

  const entry: Live = {
    term,
    fit,
    search,
    element,
    detachInput: () => {
      input.dispose();
      resize.dispose();
    },
  };
  live.set(id, entry);

  // Anything the shell said before this terminal existed.
  const queued = pending.get(id);
  if (queued) {
    pending.delete(id);
    for (const bytes of queued) term.write(bytes);
  }

  return entry;
}

/**
 * Moves a session's terminal into a node.
 *
 * Safe to call repeatedly: `appendChild` on a node that is already the parent
 * is a move, so a StrictMode remount or a tab switch reattaches the same
 * terminal with its scrollback intact.
 *
 * Any *other* terminal already mounted here is removed first. Switching tabs
 * used to leave the previous session's element in place underneath, which
 * doubled the holder's scroll height and put a scrollbar beside a command that
 * had not typed itself twice — two symptoms, one cause.
 */
export function attachTerminal(id: string, container: HTMLElement | null): Live | null {
  const entry = live.get(id);
  if (!entry || !container) return entry ?? null;

  for (const child of Array.from(container.children)) {
    if (child !== entry.element) child.remove();
  }
  if (entry.element.parentElement !== container) {
    container.appendChild(entry.element);
  }
  return entry;
}

/**
 * Reflows the terminal to its container and tells the shell.
 *
 * Returns silently when there is no room. A hidden dock measures as zero, and
 * fitting to zero columns makes xterm reflow its scrollback into a one-column
 * column that cannot be undone — so a zero-size fit must never happen.
 *
 * Called on every splitter frame, which is deliberate: the glyphs follow the
 * divider so the terminal never looks stale behind it. The `ResizeObserver`
 * coalesces those calls to one per frame, and the shell is told at most once
 * per distinct geometry because `FitAddon` only fires `onResize` when the row
 * or column count actually changes.
 */
export function fitTerminal(id: string): void {
  const entry = live.get(id);
  if (!entry) return;
  const { clientWidth, clientHeight } = entry.element.parentElement ?? entry.element;
  if (clientWidth < 40 || clientHeight < 24) return;
  try {
    entry.fit.fit();
  } catch {
    // xterm throws if the element is detached mid-fit; nothing to do.
  }
}

/** Re-reads the theme. Called when light/dark flips or a token changes. */
export function rethemeTerminals(): void {
  const theme = currentXtermTheme();
  for (const entry of live.values()) {
    entry.term.options.theme = theme;
  }
}

/**
 * Keeps every live terminal in step with the theme.
 *
 * A `MutationObserver` on the root's class list rather than a subscription to
 * the settings store, because the theme is a *class on `<html>`* and that is the
 * thing the colours are scoped to. Watching the class means a terminal retints
 * whether the flip came from Settings, from a keyboard shortcut, or from
 * anything added later — and it keeps working in a torn-off window, which has
 * the same document but not the same store instance.
 *
 * Returns a disposer. Safe to call more than once; a second observer on the same
 * document would just do the same work twice.
 */
let themeObserver: MutationObserver | null = null;

export function watchTerminalTheme(): () => void {
  if (typeof document === "undefined" || themeObserver) return () => {};
  themeObserver = new MutationObserver(() => rethemeTerminals());
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class"],
  });
  return () => {
    themeObserver?.disconnect();
    themeObserver = null;
  };
}

/**
 * Renders pty bytes.
 *
 * The bytes are written straight through. They are not text — a chunk can end
 * mid-escape-sequence or mid-codepoint — and xterm's parser is the only thing
 * that should decide what they mean.
 *
 * Bytes that arrive before a terminal exists are buffered rather than dropped.
 * That window is real: the dock opens, the shell starts printing its banner, and
 * the effect that creates the terminal has not run yet. Losing the first prompt
 * of a session is the kind of bug that looks like the shell is broken.
 */
export function writeToTerminal(id: string, bytes: Uint8Array): void {
  const entry = live.get(id);
  if (!entry) {
    const queued = pending.get(id) ?? [];
    // Bounded: a shell that spews before the dock ever shows it must not
    // become unbounded memory. Beyond this the oldest output is dropped, which
    // is what a real terminal's scrollback does anyway.
    queued.push(bytes);
    if (queued.length > 256) queued.shift();
    pending.set(id, queued);
    return;
  }
  entry.term.write(bytes);
}

/** Output that arrived before its terminal existed. */
const pending = new Map<string, Uint8Array[]>();

/** Updates appearance in place, so a Settings change does not restart a shell. */
export function restyleTerminal(id: string, config: TerminalConfig): void {
  const entry = live.get(id);
  if (!entry) return;
  entry.term.options.fontFamily = `"${config.fontFamily}", ui-monospace, Consolas, monospace`;
  entry.term.options.fontSize = config.fontSize;
  entry.term.options.lineHeight = config.lineHeight / 100;
  entry.term.options.theme = currentXtermTheme();
  // The cell size changed, so the geometry did too.
  fitTerminal(id);
}

export function searchAddon(id: string): SearchAddon | null {
  return live.get(id)?.search ?? null;
}

/** Ends a terminal for good. Only a closed tab should call this. */
export function disposeTerminal(id: string): void {
  const entry = live.get(id);
  if (!entry) return;
  live.delete(id);
  pending.delete(id);
  entry.detachInput();
  entry.term.dispose();
  entry.element.remove();
}
