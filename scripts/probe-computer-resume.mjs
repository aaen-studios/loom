// Proves the thing that was broken: that pressing Resume sticks.
//
// The old bug was a takeover latch nothing ever cleared. One real input event
// set it, and the bridge task re-read it every 150 ms, so Resume cleared the
// pause and the latch immediately re-created it — the pill flickered back to
// "Paused" within a frame, for the rest of the turn.
//
// This drives a real computer turn, fakes a takeover with the debug-only
// command (a human cannot be asked to click on cue), then asserts that a
// Resume is still in force a second later.
//
// Requires a debug build with the DevTools protocol enabled:
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//   .\target\debug\loom.exe
// Usage: node scripts/probe-computer-resume.mjs [port]
const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page" && (t.title ?? "").includes("Loom"));
if (!page) {
  console.error("no Loom page");
  process.exit(1);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (message) => {
  const payload = JSON.parse(message.data);
  if (payload.id && pending.has(payload.id)) {
    pending.get(payload.id)(payload.result ?? payload.error);
    pending.delete(payload.id);
  }
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result?.exceptionDetails) return `EXCEPTION: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "";
};

await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");
await evaluate(`localStorage.setItem("loomDebug","1")`);
await send("Page.enable");
await send("Page.reload", { ignoreCache: false });
await sleep(6000);

let failures = 0;
const check = (label, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"}  ${label}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures += 1;
};

// Arm the chip on the active chat, creating one if needed.
const armed = await evaluate(`(async () => {
  const chat = window.__loom?.chat;
  if (!chat) return "stores not exposed";
  let state = chat.getState();
  if (!state.activeId) await state.ensureSession();
  state = chat.getState();
  await state.setComputerAccess(true);
  return state.activeId ?? "none";
})()`);
check("a chat is armed for computer use", armed && !armed.startsWith("EXCEPTION") && armed !== "none", armed);
if (!armed || armed === "none" || armed.startsWith("EXCEPTION")) {
  socket.close();
  process.exit(1);
}

// The turn has to still be running when the takeover is faked, so it ends with
// a sleep rather than a click: `wait` is a computer tool, so the hooks are up,
// and ten seconds is a wide window.
const prompt =
  `Computer check. Call screenshot with target "region" and region ` +
  `{x:0,y:0,width:400,height:300}. Then call wait with seconds 10. ` +
  `Then reply "done". Exactly three steps.`;

await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, ${JSON.stringify(prompt)});
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
  return "sent";
})()`);

const paused = async () => {
  const raw = await evaluate(
    `JSON.stringify(window.__loom.chat.getState().computerPaused ?? {})`,
  );
  try {
    return Object.values(JSON.parse(raw)).some(Boolean);
  } catch {
    return null;
  }
};

// Wait for the screenshot to land: that is the moment the watch arms.
let armedWatch = false;
for (let tick = 0; tick < 15; tick += 1) {
  await sleep(1000);
  const driver = await evaluate(
    `String(window.__loom.chat.getState().computerDriver ?? "")`,
  );
  if (driver && driver !== "null" && driver !== "") {
    armedWatch = true;
    break;
  }
}
check("the write-ahead latch exists (a chat holds the computer)", armedWatch);
if (!armedWatch) {
  socket.close();
  process.exit(1);
}

// Fake a real key press. This is what a user's click does.
await evaluate(`window.__loom.ipc.debugTripComputerTakeover()`);
let sawPause = false;
for (let tick = 0; tick < 12; tick += 1) {
  await sleep(250);
  if (await paused()) {
    sawPause = true;
    break;
  }
}
check("a takeover pauses the turn", sawPause);

// The bug: this used to be undone within 150 ms, every time.
const resumed = await evaluate(`window.__loom.ipc.resumeComputer()`);
check("Resume is accepted", resumed === true, String(resumed));

let cameBack = false;
for (let tick = 0; tick < 8; tick += 1) {
  await sleep(250);
  if (await paused()) {
    cameBack = true;
    break;
  }
}
check("the pause does NOT come back after Resume", !cameBack, cameBack ? "re-paused" : "stayed resumed");

// And the pill's status agrees, which is what the user actually sees.
const status = await evaluate(`JSON.stringify(await window.__loom.ipc.computerStatus())`);
try {
  const parsed = JSON.parse(status);
  check("the pill reports the resumed state", parsed.state !== "paused", status);
  check(
    "the pill is told the auto-resume window",
    typeof parsed.resumeInSeconds === "number" && parsed.resumeInSeconds > 0,
    status,
  );
} catch {
  check("the pill status is readable", false, status);
}

// Stopping must end only this chat's turn, and say why.
const stopped = await evaluate(`(async () => {
  await window.__loom.ipc.stopComputer();
  return window.__loom.chat.getState().computerDriver ?? "null";
})()`);
check("Stop clears the driver", stopped === "null" || stopped === "", String(stopped));

console.log("");
console.log(failures === 0 ? "all checks passed" : `${failures} check(s) failed`);
socket.close();
process.exit(failures === 0 ? 0 : 1);
