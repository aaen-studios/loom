// Asks the running webview what is actually on screen: every element whose
// text is "awd", and anything small and circular.
const port = process.argv[2] ?? "9333";
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page" && (t.title ?? "").includes("Loom"));
if (!page) {
  console.error("no Loom page", targets.map((t) => t.title));
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
await new Promise((resolve) => socket.addEventListener("open", resolve));
await send("Runtime.enable");

const expression = `(() => {
  const describe = (el) => {
    const r = el.getBoundingClientRect();
    const s = getComputedStyle(el);
    return {
      tag: el.tagName.toLowerCase(),
      cls: (el.className || "").toString().slice(0, 120),
      text: (el.textContent || "").trim().slice(0, 40),
      rect: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)],
      radius: s.borderRadius,
      bg: s.backgroundColor,
      position: s.position,
      z: s.zIndex,
      visible: s.visibility + "/" + s.display + "/" + s.opacity,
      parent: el.parentElement ? el.parentElement.className.toString().slice(0, 60) : null,
    };
  };
  const all = [...document.querySelectorAll("*")];
  return JSON.stringify({
    awd: all.filter((el) => el.children.length === 0 && (el.textContent || "").trim() === "awd").map(describe),
    circles: all
      .filter((el) => {
        const s = getComputedStyle(el);
        const r = el.getBoundingClientRect();
        return (
          r.width > 4 && r.width < 60 && Math.abs(r.width - r.height) < 3 &&
          (s.borderRadius.includes("50%") || parseFloat(s.borderRadius) >= r.width / 2 - 1) &&
          s.visibility !== "hidden"
        );
      })
      .map(describe)
      .slice(0, 8),
    sidebarOpen: !!document.querySelector('aside'),
    bodyChildren: document.body.children.length,
  }, null, 1);
})()`;

const result = await send("Runtime.evaluate", { expression, returnByValue: true });
console.log(result?.result?.value ?? JSON.stringify(result));
socket.close();
