import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { isTauri } from "./tauri";

/**
 * A Tauri event subscription that survives StrictMode.
 *
 * The obvious shape is wrong, and it was wrong in seventeen places:
 *
 * ```ts
 * let dispose: (() => void) | undefined;
 * void listen("loom://x", handler).then((unlisten) => { dispose = unlisten });
 * return () => dispose?.();
 * ```
 *
 * `listen()` returns a **promise**, and under `React.StrictMode` — which this
 * app runs in — React invokes the effect, cleans it up, then invokes it again,
 * all synchronously. So the sequence is:
 *
 * 1. effect runs; `listen()` is pending; `dispose` is still `undefined`
 * 2. cleanup runs immediately; `dispose?.()` does nothing, because there is
 *    nothing to dispose *yet*
 * 3. effect runs again and attaches a **second** listener
 *
 * Every channel therefore ended up with two live handlers. The symptom was
 * everywhere and nowhere: terminal output was written into xterm twice, so
 * typing `ls` rendered `ll` then `ss`; and every engine event was applied to
 * the chat store twice, so sessions and task state advanced in pairs.
 *
 * The fix is to remember that the effect has gone, and unsubscribe when the
 * promise finally resolves rather than only if it already has.
 */
export function useTauriEvent<T>(
  name: string,
  handler: (payload: T) => void,
): void {
  // The handler is held in a ref and refreshed every render, so the
  // subscription depends only on the channel name. Without this, a handler
  // closed over changing state would tear the listener down and rebuild it on
  // every render — which is how you get a listener that is *sometimes* double.
  const latest = useRef(handler);
  latest.current = handler;

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;

    void listen<T>(name, (event) => latest.current(event.payload))
      .then((dispose) => {
        // The effect may already have been cleaned up while this was in
        // flight. Unsubscribing *here* is the whole point of the helper.
        if (cancelled) dispose();
        else unlisten = dispose;
      })
      .catch((error) => {
        console.error(`[loom] could not listen on ${name}:`, error);
      });

    return () => {
      cancelled = true;
      unlisten?.();
      unlisten = undefined;
    };
  }, [name]);
}

/**
 * The same guarantee for code that is not a React effect.
 *
 * A store that subscribes once for the process lifetime, or a non-component
 * module, still has the pending-promise race: a `dispose()` called before the
 * promise resolves would be a no-op and leak the listener. This returns a
 * disposer that is correct whenever it is called.
 */
export function subscribeTauri<T>(
  name: string,
  handler: (payload: T) => void,
): () => void {
  if (!isTauri) return () => {};
  let cancelled = false;
  let unlisten: UnlistenFn | undefined;

  void listen<T>(name, (event) => handler(event.payload))
    .then((dispose) => {
      if (cancelled) dispose();
      else unlisten = dispose;
    })
    .catch((error) => {
      console.error(`[loom] could not listen on ${name}:`, error);
    });

  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = undefined;
  };
}

/**
 * Wraps any promise-returning subscription so its disposer is race-safe.
 *
 * For the few listeners that come from an API other than `loom://` — a window
 * or webview handle, say — which have the same promise shape and therefore the
 * same leak.
 */
export function raceSafe(start: () => Promise<UnlistenFn>): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | undefined;

  void start()
    .then((dispose) => {
      if (cancelled) dispose();
      else unlisten = dispose;
    })
    .catch(() => {
      // A window that has closed, or a webview that is not there. Nothing to
      // report: the caller has no useful recovery.
    });

  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = undefined;
  };
}
