// Verifies saved prompts (slash menu) and the pinnable chats popup.
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
  const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result?.exceptionDetails) return `EXCEPTION: ${result.exceptionDetails.text}`;
  return result?.result?.value ?? "";
};
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");
await evaluate(`localStorage.setItem("loomDebug","1")`);

// A saved prompt, added the way the settings editor does.
console.log(
  "add prompt:",
  await evaluate(`(async () => {
    const { ipc } = await import("/src/lib/ipc.ts").catch(() => ({ ipc: null }));
    return "skipped: modules are bundled";
  })()`),
);

// Use the exposed store + a fetch against the invoked command instead.
console.log(
  "prompt saved:",
  await evaluate(`(async () => {
    const config = window.__loom.settings.getState().config;
    const response = await window.__TAURI_INTERNALS__.invoke("upsert_prompt", {
      prompt: { id: "", title: "Explain simply", body: "Explain this as if I am five." },
    });
    window.__loom.settings.getState().applyRemote(response);
    return (response.prompts ?? []).map((p) => p.title).join(", ");
  })()`),
);

// Open the composer's slash menu and look for it.
await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, "/");
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  ta.focus();
  return "slash";
})()`);
await sleep(700);
console.log(
  "slash menu entries:",
  await evaluate(`(() => {
    const menu = document.querySelector('.panel-strong.absolute.bottom-full');
    return menu ? menu.innerText.replace(/\\n+/g, " | ").slice(0, 160) : "menu not rendered";
  })()`),
);

// Selecting the prompt should fill the composer with its body.
console.log(
  "select prompt:",
  await evaluate(`(() => {
    const menu = document.querySelector('.panel-strong.absolute.bottom-full');
    if (!menu) return "no menu";
    const button = [...menu.querySelectorAll("button")].find((b) => b.textContent.includes("Explain simply"));
    if (!button) return "entry missing";
    button.click();
    return document.querySelector("textarea").value;
  })()`),
);

// Clear the composer again.
await evaluate(`(() => {
  const ta = document.querySelector('textarea');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
  setter.call(ta, "");
  ta.dispatchEvent(new Event('input', { bubbles: true }));
  return "cleared";
})()`);

// Pin the chats popup: it must survive a click on the backdrop.
await evaluate(`window.__loom.ui.getState().setSidebarOpen(true)`);
await sleep(600);
console.log("pin pressed:", await evaluate(`(() => {
  const button = [...document.querySelectorAll("aside button")].find((b) => (b.getAttribute("aria-label") || "").includes("Pin"));
  if (!button) return "pin button missing";
  button.click();
  return button.getAttribute("aria-label");
})()`));
await sleep(900);
await evaluate(`document.querySelector(".absolute.inset-0.cursor-default")?.click()`);
await sleep(400);
console.log(
  "popup still open after clicking the backdrop:",
  await evaluate(`!!document.querySelector("aside")`),
);
console.log(
  "pinned setting:",
  await evaluate(`window.__loom.settings.getState().config.interface.sidebarPinned`),
);
socket.close();
