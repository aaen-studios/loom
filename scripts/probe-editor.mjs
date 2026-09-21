// A probe for the one failure in the editor that is silent.
//
// Monaco does its tokenising, linting and TypeScript analysis in web workers. If
// a worker cannot be constructed — which is what happens when `worker-src` does
// not permit it, or when a blob URL is refused by `script-src 'self'` — Monaco
// **does not error**. It falls back to the main thread, keeps rendering, and
// merely loses language features and speed. Every visible check passes.
//
// This is the same shape of failure `probe-glass.mjs` was written for: a feature
// that renders nothing at all while every computed style looks correct. So it
// gets the same treatment — a check that reads a fact only a real worker can
// produce, rather than looking at the screen and deciding it seems fine.
//
// Run against a dev build with the DevTools protocol open:
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//   .\target\debug\loom.exe
//   node scripts/probe-editor.mjs 9333
//
// Exit code 0 means a worker started. 1 means Monaco is running on the main
// thread, which is the failure this exists to catch.

import { WebSocket } from "ws";

const port = process.argv[2] ?? "9333";
const endpoint = `http://127.0.0.1:${port}/json/list`;

/** The page target, which is the app's webview. */
async function findPage() {
  const response = await fetch(endpoint);
  const targets = await response.json();
  const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
  if (!page) throw new Error(`no page target on port ${port}`);
  return page;
}

/** A tiny CDP client: connect, evaluate, disconnect. */
async function evaluate(url, expression, { awaitPromise = false } = {}) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const id = 1;
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error("timed out"));
    }, 30_000);

    socket.on("open", () => {
      socket.send(
        JSON.stringify({
          id,
          method: "Runtime.evaluate",
          params: {
            expression,
            awaitPromise,
            returnByValue: true,
            // The store lives on `window.__loom`, which is only installed when
            // `loomDebug` is set — this probe does not need it, but a failure
            // message is more useful when it can name what it found.
            includeCommandLineAPI: true,
          },
        }),
      );
    });

    socket.on("message", (raw) => {
      const message = JSON.parse(raw.toString());
      if (message.id !== id) return;
      clearTimeout(timer);
      socket.close();
      if (message.error) {
        reject(new Error(JSON.stringify(message.error)));
        return;
      }
      if (message.result?.exceptionDetails) {
        reject(new Error(message.result.exceptionDetails.text ?? "evaluation threw"));
        return;
      }
      resolve(message.result?.result?.value);
    });

    socket.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
  });
}

/**
 * Whether a worker actually constructed.
 *
 * `MonacoEnvironment.getWorker` is called once per worker *kind*, and the object
 * it returns is a real `Worker` — whose constructor fails loudly if the CSP
 * refuses it. So the check is: monkey-patch the factory, open a file, and see
 * whether it was called and whether what came back was usable.
 *
 * The patch is installed *before* Monaco loads, which is why this probes the
 * factory rather than inspecting a live worker: `new Worker(...)` is the exact
 * line that either succeeds or silently degrades, so that is the line to watch.
 */
const INSTALL = `
(() => {
  window.__loomWorkerProbe = { called: [], errors: [] };
  const patch = () => {
    const env = window.MonacoEnvironment;
    if (!env || !env.getWorker || env.__probed) return false;
    const original = env.getWorker;
    env.getWorker = function (id, label) {
      window.__loomWorkerProbe.called.push(label);
      try {
        const worker = original.call(this, id, label);
        // A Worker that was refused is still an object; the only reliable
        // signal is whether it ever reaches "ready". Attaching an error
        // listener catches the CSP refusal, which fires asynchronously.
        if (worker && typeof worker.addEventListener === "function") {
          worker.addEventListener("error", (event) => {
            window.__loomWorkerProbe.errors.push(
              label + ": " + (event.message || "worker error"),
            );
          });
        }
        return worker;
      } catch (error) {
        window.__loomWorkerProbe.errors.push(label + ": " + error.message);
        throw error;
      }
    };
    env.__probed = true;
    return true;
  };
  // Monaco installs MonacoEnvironment on first import, so patch both now and
  // when it appears.
  if (!patch()) {
    const timer = setInterval(() => {
      if (patch()) clearInterval(timer);
    }, 50);
    window.__loomWorkerProbe.polling = true;
  }
  return true;
})()
`;

/** Opens the editor and waits for it to say it is ready. */
const OPEN_EDITOR = `
(async () => {
  const debug = localStorage.getItem("loomDebug") === "1";
  if (!debug) {
    // The probe needs the stores to drive the panel. Setting the flag and
    // reloading is the documented way in — see scripts/drive-ui.mjs.
    localStorage.setItem("loomDebug", "1");
    return { reloaded: true };
  }
  const loom = window.__loom;
  if (!loom) return { error: "window.__loom is missing; is this a dev build?" };

  // A workspace folder, so the editor has something to open. The panel takes
  // its workdir from the *active* chat, so finding a session is not enough —
  // it has to be opened, or EditorPane never mounts, no model is created, and
  // no worker is ever asked for. That reads as a failed probe when the panel is
  // simply empty, which is the one wrong conclusion this script must not draw.
  const chat = loom.chat.getState();
  const active = chat.sessions.find((s) => s.id === chat.activeId);
  const session = active?.workdir ? active : chat.sessions.find((s) => s.workdir);
  if (!session) return { error: "no chat has a workspace folder set" };
  if (session.id !== chat.activeId) await chat.openSession(session.id);

  // Switching chats reloads the dock for the new folder, and that load lands
  // after \`openSession\` resolves. Opening the panel first would be undone when
  // the folder's stored layout arrives — the panel would vanish and this probe
  // would read that as a broken editor. So wait for the dock to be on the
  // folder the file is about to come from.
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if (loom.dock.getState().workdir === session.workdir) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }

  loom.dock.getState().openPanel("editor", "right");
  // The panel's effect points the editor store at the new workdir on the next
  // render; opening before that lands would read against the old folder.
  await new Promise((resolve) => setTimeout(resolve, 700));

  // A JS/TS file, specifically. The probe used to open \`package.json\`, and
  // JSON's language service never asks for a worker when a model is merely
  // created and rendered — only a JS/TS model does, on creation. So a healthy
  // Monaco failed this probe because of the file it was told to open, which is
  // the exact mis-diagnosis the script exists to prevent. If the workspace has
  // no source file at its root, say so rather than blame the workers.
  const entries = await loom.ipc.dirList(session.workdir, "");
  const source = (entries ?? []).find(
    (entry) => !entry.isDir && /\\.(ts|tsx|js|jsx|mjs|cjs)$/.test(entry.name),
  );
  if (!source) {
    return { error: "no JS/TS file in the workspace root, and only those start a worker" };
  }
  const path = source.path;
  await loom.editor.getState().open(path);

  // Give Monaco a moment to construct its workers. The TypeScript worker is the
  // slow one, and it is requested as soon as a JS/TS model exists.
  await new Promise((resolve) => setTimeout(resolve, 3000));
  return { path, tabs: loom.editor.getState().tabs.length };
})()
`;

const SLEEP = (ms) => `new Promise((resolve) => setTimeout(resolve, ${ms}))`;

async function main() {
  const page = await findPage();
  console.log(`probing ${page.url}`);

  const installed = await evaluate(page.webSocketDebuggerUrl, INSTALL);
  if (!installed) throw new Error("could not install the worker probe");

  const opened = await evaluate(page.webSocketDebuggerUrl, OPEN_EDITOR, {
    awaitPromise: true,
  });
  if (opened?.reloaded) {
    console.log("set loomDebug and reloading; run this probe again");
    await evaluate(page.webSocketDebuggerUrl, `location.reload()`);
    process.exit(2);
  }
  if (opened?.error) throw new Error(opened.error);

  console.log("editor opened:", JSON.stringify(opened));

  const result = await evaluate(
    page.webSocketDebuggerUrl,
    `JSON.stringify(window.__loomWorkerProbe || { called: [], errors: [], missing: true })`,
  );
  const probe = JSON.parse(result ?? "{}");

  console.log("worker labels requested:", probe.called ?? []);
  if (probe.errors?.length) {
    console.error("worker errors:", probe.errors);
  }

  // A `MonacoEnvironment` that was never installed means Monaco loaded without
  // it — which is the failure mode this whole probe exists for: no error, no
  // worker, and an editor that looks fine.
  if (!probe.called?.length) {
    console.error(
      "\nFAIL: no worker was constructed. Monaco is running on the main thread,\n" +
        "which means tokenisation, linting and TypeScript analysis are missing\n" +
        "without any visible error. Check `worker-src` in tauri.conf.json and\n" +
        "the `?worker` imports in lib/monaco.ts.",
    );
    process.exit(1);
  }

  if (probe.errors?.length) {
    console.error("\nFAIL: a worker was constructed but errored.");
    process.exit(1);
  }

  console.log("\nOK: a worker was constructed and did not error.");
  process.exit(0);
}

main().catch((error) => {
  console.error("probe failed:", error.message);
  process.exit(1);
});

// Referenced so the linter does not flag it as unused: the timing helper is
// useful when this probe is extended to wait on a condition.
void SLEEP;
