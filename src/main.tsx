import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { applyCachedAppearance } from "./stores/settings";
import "streamdown/styles.css";
// The terminal's default typeface, bundled rather than assumed: a shell is the
// one surface where the font is load-bearing, and a machine without a good mono
// would fall back to whatever the browser picks.
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/700.css";
// Required, not cosmetic: xterm's own sheet is what positions the render layer,
// the helper textarea and the scroll viewport. Without it the terminal paints
// as a jumbled run of inline text.
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

/**
 * Last-resort reporter: if the bundle fails before React can mount, the window
 * would otherwise just be blank. This paints the failure instead.
 */
function reportFatal(message: string, detail?: string) {
  const root = document.getElementById("root");
  if (!root) return;

  // Leave React's own output alone; only step in when nothing rendered.
  if (root.childElementCount > 0 && !root.dataset.fatal) return;
  root.dataset.fatal = "1";
  root.innerHTML = `
    <div style="font-family:Inter,system-ui,sans-serif;height:100%;display:flex;align-items:center;justify-content:center;background:#0a0d16;color:#eef1f8;padding:24px">
      <div style="max-width:640px;border:1px solid rgba(255,255,255,.12);background:rgba(255,255,255,.05);border-radius:18px;padding:20px">
        <h1 style="font-size:16px;margin:0 0 6px">Loom could not start</h1>
        <p style="font-size:13px;opacity:.7;margin:0 0 12px">Your chats are safe in ~/.loom.</p>
        <pre style="font-family:ui-monospace,Consolas,monospace;font-size:11.5px;white-space:pre-wrap;background:rgba(0,0,0,.4);padding:12px;border-radius:10px;max-height:240px;overflow:auto">${message}</pre>
        ${detail ? `<pre style="font-family:ui-monospace,Consolas,monospace;font-size:11px;white-space:pre-wrap;opacity:.7;max-height:160px;overflow:auto">${detail}</pre>` : ""}
        <button onclick="location.reload()" style="margin-top:12px;border:0;border-radius:12px;background:#f4f6fc;color:#0b0e15;padding:8px 14px;font-size:13px;cursor:pointer">Reload</button>
      </div>
    </div>`;
}

window.addEventListener("error", (event) => {
  reportFatal(event.message, event.error?.stack);
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason as { message?: string; stack?: string } | string;
  reportFatal(
    typeof reason === "string" ? reason : reason?.message ?? "Unhandled promise rejection",
    typeof reason === "string" ? undefined : reason?.stack,
  );
});

// Diagnostics: `localStorage.setItem("loomDebug","1")` exposes the stores on
// window so the DevTools protocol can inspect real state.
/* Diagnostics: the app exposes its stores as `window.__loom` when
   `loomDebug` is set. */
if (localStorage.getItem("loomDebug") === "1") {
  void Promise.all([
    import("./stores/chat"),
    import("./stores/providers"),
    import("./stores/settings"),
    import("./stores/ui"),
    import("./lib/ipc"),
  ]).then(([chat, providers, settings, ui, ipc]) => {
    (window as unknown as Record<string, unknown>).__loom = {
      chat: chat.useChat,
      providers: providers.useProviders,
      settings: settings.useSettings,
      ui: ui.useUi,
      // The probe scripts drive the real IPC surface, which is the only way
      // to reach the debug-only commands.
      ipc: ipc.ipc,
    };
  });
}

// Before React mounts, and before the config arrives: the theme class and the
// window backdrop come from the synchronous appearance cache, so the first
// frame is already the right colour rather than light-then-dark. This is the
// earliest point a CSP that forbids inline scripts allows.
applyCachedAppearance();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
