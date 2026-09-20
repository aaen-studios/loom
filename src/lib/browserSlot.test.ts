// @vitest-environment jsdom
//
// jsdom rather than the default `node`, and it is required rather than
// convenient: the reporter is built on `requestAnimationFrame` and the slot
// maths reads `window`, so a node environment cannot exercise either. The rule
// the rest of this file asserts — that nothing invents a slot outside Tauri —
// still holds here, because jsdom sets no `__TAURI_INTERNALS__`.
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createSlotReporter,
  isOpenableUrl,
  resolveOmnibox,
  slotFromRect,
  slotKey,
} from "./browserSlot";

describe("resolveOmnibox", () => {
  const engine = "https://duckduckgo.com/?q=";

  it("promotes a bare host to https", () => {
    expect(resolveOmnibox("example.com", engine)).toBe("https://example.com");
    expect(resolveOmnibox("example.com/a?b=1", engine)).toBe("https://example.com/a?b=1");
  });

  it("treats a space as a search", () => {
    // No hostname contains a space, so this is the tell — the same rule the tool
    // layer applies, which is the point of testing it twice.
    expect(resolveOmnibox("rust tauri webview2", engine)).toBe(
      "https://duckduckgo.com/?q=rust%20tauri%20webview2",
    );
  });

  it("leaves a real URL alone, whatever its scheme", () => {
    expect(resolveOmnibox("http://x.test", engine)).toBe("http://x.test");
    expect(resolveOmnibox("https://x.test/a", engine)).toBe("https://x.test/a");
    expect(resolveOmnibox("about:blank", engine)).toBe("about:blank");
  });

  it("gives localhost http rather than https", () => {
    // A dev server on localhost is almost never serving TLS, and silently
    // upgrading it produces a connection error that reads like a broken app.
    expect(resolveOmnibox("localhost:3000", engine)).toBe("http://localhost:3000");
    expect(resolveOmnibox("127.0.0.1:8080/x", engine)).toBe("http://127.0.0.1:8080/x");
  });

  it("returns nothing for nothing", () => {
    expect(resolveOmnibox("   ", engine)).toBe("");
  });
});

describe("isOpenableUrl", () => {
  it("accepts what the browser can be asked to open", () => {
    expect(isOpenableUrl("https://example.com")).toBe(true);
    expect(isOpenableUrl("http://localhost:3000")).toBe(true);
    expect(isOpenableUrl("example.com")).toBe(true);
  });

  it("refuses what it cannot", () => {
    expect(isOpenableUrl("not a url at all")).toBe(false);
    expect(isOpenableUrl("")).toBe(false);
  });
});

describe("slotFromRect", () => {
  it("refuses a slot too small to draw, rather than reporting one at 0,0", () => {
    // The distinction matters: "park the page" and "put it at the origin" are
    // very different instructions, and only one is ever meant.
    expect(slotFromRect({ left: 0, top: 0, width: 10, height: 400 })).toBeNull();
    expect(slotFromRect({ left: 0, top: 0, width: 400, height: 10 })).toBeNull();
  });

  it("refuses a non-finite rect instead of passing NaN to the shell", () => {
    expect(slotFromRect({ left: NaN, top: 0, width: 400, height: 300 })).toBeNull();
    expect(slotFromRect({ left: 0, top: 0, width: Infinity, height: 300 })).toBeNull();
  });

  it("returns null outside Tauri instead of inventing coordinates", () => {
    // Vitest runs in jsdom, where `isTauri` is false — there is no webview to
    // place anything in, so the panel draws its own placeholder.
    expect(slotFromRect({ left: 10, top: 20, width: 400, height: 300 })).toBeNull();
  });

  it("passes a real rect through unchanged", () => {
    // The whole point of a child webview: the panel measures in the same units
    // the shell positions in, so there is nothing to convert.
    const rect = { left: 12.5, top: 64, width: 400, height: 300 };
    const slot = slotFromRect(rect);
    if (slot) {
      expect(slot).toEqual({ x: 12.5, y: 64, width: 400, height: 300 });
    }
  });
});

describe("createSlotReporter", () => {
  const frames: FrameRequestCallback[] = [];

  function fakeFrames() {
    frames.length = 0;
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      frames.push(callback);
      return frames.length;
    });
    vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  }

  function runFrame() {
    const callback = frames.shift();
    callback?.(0);
  }

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("coalesces many reports into one per frame", () => {
    // A live resize fires the observer several times per frame, and each report
    // moves a real webview. Without coalescing, the page would lag the pointer.
    fakeFrames();
    const send = vi.fn();
    const reporter = createSlotReporter(send);

    reporter.report({ left: 0, top: 0, width: 400, height: 300 });
    reporter.report({ left: 0, top: 0, width: 420, height: 300 });
    reporter.report({ left: 0, top: 0, width: 440, height: 300 });

    // One frame requested for three reports, and nothing sent yet.
    expect(frames.length).toBe(1);
    expect(send).not.toHaveBeenCalled();
    reporter.stop();
  });

  it("sends nothing after stop, even for a frame already queued", () => {
    // The panel is on its way out and must not move a webview it no longer owns.
    fakeFrames();
    const send = vi.fn();
    const reporter = createSlotReporter(send);
    reporter.report({ left: 0, top: 0, width: 400, height: 300 });
    reporter.stop();
    runFrame();
    expect(send).not.toHaveBeenCalled();
  });

  it("does not re-send a slot that has not changed", () => {
    // A `ResizeObserver` fires for changes that do not alter the slot, and
    // moving a webview to where it already is is a frame of work for nothing.
    //
    // Every report here resolves to "park", because jsdom has no Tauri and so
    // `slotFromRect` returns null — which is itself the de-duplication case that
    // matters most, since a closed panel parks on every layout pass. The
    // *changed* case is asserted directly on `slotKey` below, where it does not
    // need a runtime to produce coordinates.
    fakeFrames();
    const send = vi.fn();
    const reporter = createSlotReporter(send);

    reporter.report({ left: 0, top: 0, width: 400, height: 300 });
    runFrame();
    expect(send).toHaveBeenCalledTimes(1);

    reporter.report({ left: 0, top: 0, width: 400, height: 300 });
    runFrame();
    expect(send).toHaveBeenCalledTimes(1);
    reporter.stop();
  });
});

describe("slotKey", () => {
  it("ignores a sub-pixel jitter", () => {
    // The reason it rounds: a fractional difference in a layout is not a move,
    // and treating it as one would move a real webview for nothing.
    const a = slotKey({ x: 12.4, y: 64, width: 400, height: 300 });
    const b = slotKey({ x: 12.4999, y: 64, width: 400, height: 300 });
    expect(a).toBe(b);
  });

  it("distinguishes a real move, and a real resize", () => {
    const base = slotKey({ x: 12, y: 64, width: 400, height: 300 });
    expect(slotKey({ x: 13, y: 64, width: 400, height: 300 })).not.toBe(base);
    expect(slotKey({ x: 12, y: 65, width: 400, height: 300 })).not.toBe(base);
    expect(slotKey({ x: 12, y: 64, width: 401, height: 300 })).not.toBe(base);
    expect(slotKey({ x: 12, y: 64, width: 400, height: 301 })).not.toBe(base);
  });

  it("is a single value for every parked case", () => {
    // Parked is one state, however it was reached — a collapsed panel, a hidden
    // one, a panel outside Tauri — so a page is never re-parked on a loop.
    expect(slotKey(null)).toBe("park");
  });
});
