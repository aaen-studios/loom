/**
 * The page's rectangle, in the units the shell positions child webviews in.
 *
 * This is the whole reason the browser uses child webviews rather than windows:
 * `getBoundingClientRect` on an element in the panel returns **logical pixels
 * relative to the window's client area**, and that is exactly what
 * `Window::add_child` and `Webview::set_bounds` take. So the conversion is a
 * copy, with no window position, no scale factor and nothing that can go stale
 * between a resize and the next frame.
 *
 * The window-per-tab version needed `outerPosition`, `scaleFactor`, and a
 * multiply — three things that had to agree, on a per-monitor basis, for the
 * page to land in the right place. It did not, and the page appeared nowhere
 * near its slot.
 */

import type { BrowserSlot } from "./ipc";
import { isTauri } from "./tauri";

/** The smallest a slot can be and still be a page rather than a sliver. */
export const MIN_SLOT = 48;

/**
 * The slot for a measured element, or `null` when there is nowhere to put a
 * page.
 *
 * `null` rather than a zero-sized rectangle for the degenerate cases — a
 * collapsed panel, a hidden one, a layout mid-flight — because "park the page"
 * and "draw it at 0,0" are very different instructions and only one is ever
 * meant.
 */
export function slotFromRect(rect: {
  left: number;
  top: number;
  width: number;
  height: number;
}): BrowserSlot | null {
  if (!isTauri) return null;
  if (!Number.isFinite(rect.left) || !Number.isFinite(rect.top)) return null;
  if (rect.width < MIN_SLOT || rect.height < MIN_SLOT) return null;
  return {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

/**
 * A slot's identity for de-duplication, or `null` when there is no page.
 *
 * Rounded to whole pixels: a `ResizeObserver` fires for changes that do not alter
 * the slot at all, and a half-pixel of layout jitter is not a move — moving a
 * webview to the rectangle it is already in is a frame of work for nothing.
 *
 * Exported because the rule is worth testing on its own, without a Tauri runtime
 * to produce real coordinates.
 */
export function slotKey(slot: BrowserSlot | null): string {
  if (!slot) return "park";
  return `${Math.round(slot.x)}:${Math.round(slot.y)}:${Math.round(slot.width)}:${Math.round(slot.height)}`;
}

/**
 * Reports a slot on every change, coalesced to one report per frame and
 * de-duplicated against the last one sent.
 *
 * A drag fires the observer many times per frame — the transcript reflowing, the
 * splitter moving, the window resizing — and each report crosses the IPC boundary
 * and moves a real webview. Coalescing is not an optimisation here; without it a
 * resize would queue hundreds of moves and the page would visibly lag the
 * pointer.
 */
export function createSlotReporter(send: (slot: BrowserSlot | null) => void): {
  report: (rect: { left: number; top: number; width: number; height: number }) => void;
  stop: () => void;
} {
  let frame: number | null = null;
  let pending: { left: number; top: number; width: number; height: number } | null = null;
  let last: string | null = null;
  let stopped = false;

  const flush = () => {
    frame = null;
    if (stopped || !pending) return;
    const rect = pending;
    pending = null;
    const slot = slotFromRect(rect);
    const key = slotKey(slot);
    if (key === last) return;
    last = key;
    send(slot);
  };

  return {
    report: (rect) => {
      if (stopped) return;
      pending = rect;
      if (frame === null) frame = window.requestAnimationFrame(flush);
    },
    stop: () => {
      stopped = true;
      if (frame !== null) window.cancelAnimationFrame(frame);
      frame = null;
    },
  };
}

/** Whether a URL is one the browser can be asked to open. */
export function isOpenableUrl(url: string): boolean {
  const text = url.trim().toLowerCase();
  return (
    text.startsWith("http://") ||
    text.startsWith("https://") ||
    // A bare host: `example.com`, `localhost:3000`. No space, so no query.
    /^[a-z0-9][a-z0-9.-]*(\.[a-z]{2,}|:\d+)(\/|$)/.test(text)
  );
}

/**
 * What the omnibox does with what was typed.
 *
 * A bare host becomes https, anything with a space is a search, and anything
 * that is already a URL is left alone — the same rule the tool layer applies,
 * duplicated here so the omnibox and `browser_open` cannot disagree about what a
 * typed string means.
 */
export function resolveOmnibox(value: string, engine: string): string {
  const text = value.trim();
  if (!text) return "";
  if (
    text.startsWith("http://") ||
    text.startsWith("https://") ||
    text.startsWith("about:") ||
    text.startsWith("file:")
  ) {
    return text;
  }
  if (text.startsWith("localhost") || text.startsWith("127.0.0.1")) {
    return `http://${text}`;
  }
  if (/\s/.test(text)) {
    return `${engine}${encodeURIComponent(text)}`;
  }
  return `https://${text}`;
}
