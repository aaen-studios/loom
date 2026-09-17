// Settings moved from the foot of the chats list to the left of the chats pill.
//
// Four things have to be true at once, and three of them are about absence —
// which is exactly the kind of change that looks right in a diff and wrong on
// screen:
//
//   1. a Settings button exists in the title bar
//   2. it sits to the LEFT of the chats pill
//   3. the Voice mode button is gone
//   4. the chats list has no settings row left at its foot
//
// Plus: clicking the new button actually opens the drawer, since a button that
// renders is not the same as a button that works.
const port = process.argv[2] ?? "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let page = null;
for (let i = 0; i < 40 && !page; i += 1) {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    page = targets.find((t) => t.type === "page" && t.url.includes("1420")) ?? null;
  } catch {
    /* not up */
  }
  if (!page) await sleep(500);
}
if (!page) {
  console.error("no app page");
  process.exit(1);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
const errors = [];
socket.addEventListener("message", (m) => {
  const p = JSON.parse(m.data);
  if (p.id && pending.has(p.id)) {
    pending.get(p.id)(p.result ?? p.error);
    pending.delete(p.id);
    return;
  }
  if (p.method === "Runtime.exceptionThrown") {
    errors.push(p.params.exceptionDetails.text);
  }
  if (p.method === "Runtime.consoleAPICalled" && p.params.type === "error") {
    errors.push(p.params.args.map((a) => a.value ?? a.description).join(" "));
  }
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression, awaitPromise = true) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise });
  if (r?.exceptionDetails) return `EVAL ERROR: ${r.exceptionDetails.text}`;
  return r?.result?.value;
};

await new Promise((r) => socket.addEventListener("open", r));
await send("Runtime.enable");
await send("Page.enable");

// Fresh load: a module that failed to transform mid-edit leaves a broken graph
// no later fix reaches, and this would then measure stale code.
await send("Page.reload", { ignoreCache: true });
await sleep(4500);
for (let i = 0; i < 40; i += 1) {
  if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
  await sleep(500);
}

const report = await evaluate(`(() => {
  const header = document.querySelector("header");
  const buttons = [...header.querySelectorAll("button")].map((b) => ({
    label: b.getAttribute("aria-label"),
    x: Math.round(b.getBoundingClientRect().x),
    visible: b.getBoundingClientRect().width > 0,
  }));
  const settings = buttons.find((b) => b.label === "Settings");
  const chats = buttons.find((b) => b.label === "Chats");
  const voice = buttons.find((b) => b.label === "Voice mode");

  // Every settings affordance outside the title bar, so a leftover entry point
  // in the chats list shows up rather than being assumed away.
  const settingsElsewhere = [...document.querySelectorAll("button")]
    .filter((b) => !header.contains(b))
    .filter((b) => /^settings/i.test((b.getAttribute("aria-label") ?? "") + (b.textContent ?? "").trim()))
    .map((b) => (b.textContent ?? "").trim().slice(0, 40));

  const word = (node) => (node.textContent ?? "").trim();
  const notInHeader = [...document.querySelectorAll("*")]
    .filter((el) => !header.contains(el) && el.children.length === 0 && word(el) === "Settings");

  return JSON.stringify({
    titleBar: {
      settingsPresent: !!settings,
      voicePresent: !!voice,
      settingsX: settings?.x ?? null,
      chatsX: chats?.x ?? null,
      settingsLeftOfChats:
        settings && chats ? settings.x < chats.x : null,
      labels: buttons.map((b) => b.label),
    },
    settingsTextOutsideHeader: notInHeader.map(
      (el) => el.closest("aside") ? "inside <aside> (the chats list)" : "elsewhere",
    ),
    settingsButtonsOutsideHeader: settingsElsewhere,
  }, null, 2);
})()`, false);

console.log("=== where settings lives ===");
console.log(report);

// Does it work? A rendered button is not a working button, and this one has an
// `active` state driven by the store it also writes to.
const clicked = await evaluate(`(() => {
  const header = document.querySelector("header");
  const button = [...header.querySelectorAll("button")].find(
    (b) => b.getAttribute("aria-label") === "Settings",
  );
  if (!button) return "no button";
  button.click();
  return "clicked";
})()`, false);
await sleep(900);

const after = await evaluate(`(() => {
  const header = document.querySelector("header");
  const button = [...header.querySelectorAll("button")].find(
    (b) => b.getAttribute("aria-label") === "Settings",
  );
  const headings = [...document.querySelectorAll("h3")].map((h) => h.textContent.trim());
  return JSON.stringify({
    ariaPressed: button?.getAttribute("aria-pressed"),
    drawerHeadings: headings.slice(0, 6),
    // The drawer is the only thing that renders a "General" heading.
    drawerOpen: headings.includes("General") || headings.length > 0,
  }, null, 2);
})()`, false);
console.log("\n=== after clicking it ===");
console.log(after);

// Close it the way a user does — Escape, handled by `useShortcuts` — rather
// than through the store. The first version reached for `window.__loom`, which
// only exists when `loomDebug` is set in localStorage; this probe never sets it,
// so the optional chain silently did nothing, the drawer stayed open, and the
// button correctly reported itself active for a drawer that was still there.
// That read as "the pressed state never clears", which would have been a real
// bug in `active={settingsOpen}`. It was not one.
await evaluate(
  `(() => {
     window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
     return true;
   })()`,
  false,
);
await sleep(600);

const closed = await evaluate(`(() => {
  const header = document.querySelector("header");
  const button = [...header.querySelectorAll("button")].find(
    (b) => b.getAttribute("aria-label") === "Settings",
  );
  const headings = [...document.querySelectorAll("h3")].map((h) => h.textContent.trim());
  return JSON.stringify({
    ariaPressedWhenClosed: button?.getAttribute("aria-pressed"),
    drawerStillOpen: headings.length > 0,
  }, null, 2);
})()`, false);
console.log("\n=== after closing it with Escape ===");
console.log(closed);

console.log(`\n=== console errors (${errors.length}) ===`);
for (const e of errors) console.log("  " + e);

/* ----------------------------------------------------------------- verdict */

console.log("\n=== VERDICT ===");
let parsed = null;
try {
  parsed = JSON.parse(report);
} catch {
  /* reported above */
}
let afterParsed = null;
try {
  afterParsed = JSON.parse(after);
} catch {
  /* reported above */
}
let closedParsed = null;
try {
  closedParsed = JSON.parse(closed);
} catch {
  /* reported above */
}

const checks = [
  ["a Settings button exists in the title bar", parsed?.titleBar.settingsPresent === true],
  ["it sits left of the chats pill", parsed?.titleBar.settingsLeftOfChats === true],
  ["the Voice mode button is gone", parsed?.titleBar.voicePresent === false],
  [
    "no settings row is left in the chats list",
    (parsed?.settingsTextOutsideHeader ?? []).length === 0,
  ],
  ["clicking it opens the drawer", afterParsed?.drawerOpen === true],
  [
    "the button reports itself active while the drawer is open",
    afterParsed?.ariaPressed === "true",
  ],
  ["Escape closes the drawer", closedParsed?.drawerStillOpen === false],
  [
    "and the button stops reporting itself active",
    closedParsed?.ariaPressedWhenClosed === "false",
  ],
  ["no console errors", errors.length === 0],
];
let failed = 0;
for (const [label, ok] of checks) {
  if (!ok) failed += 1;
  console.log(`  ${ok ? "PASS" : "FAIL"}  ${label}`);
}
if (parsed) {
  console.log(`\n  title-bar buttons: ${parsed.titleBar.labels.filter(Boolean).join(", ")}`);
}

socket.close();
process.exit(failed === 0 ? 0 : 1);
