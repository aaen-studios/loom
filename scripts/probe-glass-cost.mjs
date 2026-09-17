// What the refracting glass costs per frame, measured against a baseline that
// means something.
//
// ## Why the earlier numbers were useless
//
// The first version compared "effect on" against "no glass at all", which is not
// the question. Loom *already* runs `backdrop-filter` over a background that
// never stops drifting; what matters is what the **SVG filter graph adds on top
// of the glass Loom already had**. And the second version reported 20.8ms for
// every state including "all glass removed", which is a plateau, not a
// measurement — the numbers agreed to within 0.2ms across a 5x change in work,
// which is the signature of frame pacing rather than of cost.
//
// So this one:
//
//   * **Freezes the animations.** Otherwise the page's own 52-second drift is
//     inside every sample, and the variance between states is the animation
//     rather than the glass.
//   * **Measures one surface at a time**, by turning the filter off on the
//     others. That is how the pills and the composer get separate numbers, which
//     is what decides whether the composer is the one to turn off.
//   * **Reports a floor**, with every glass property removed from the refracting
//     surfaces, so "cheap" and "cheaper than nothing" are distinguishable.
//
//   node scripts/probe-glass-cost.mjs 9333
import { writeFileSync } from "node:fs";

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
  console.error("no app page on port", port);
  process.exit(1);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (m) => {
  const p = JSON.parse(m.data);
  if (p.id && pending.has(p.id)) {
    pending.get(p.id)(p.result ?? p.error);
    pending.delete(p.id);
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

console.log("reloading for a clean module graph...");
await send("Page.reload", { ignoreCache: true });
await sleep(4000);
for (let i = 0; i < 40; i += 1) {
  if ((await evaluate('document.querySelectorAll(".lg-stage").length', false)) > 0) break;
  await sleep(500);
}

// Freeze everything. The drift is a compositor transform, so it is nearly free
// to run — but it makes every sample non-comparable, and comparability is the
// only thing a timing probe has.
await evaluate(
  `(() => {
     const el = document.createElement("style");
     el.id = "lg-freeze";
     el.textContent = "*, *::before, *::after { animation-play-state: paused !important; }";
     document.head.appendChild(el);
     return true;
   })()`,
  false,
);
await sleep(1000);

const counts = await evaluate(`(() => {
  const stages = [...document.querySelectorAll(".lg-stage")];
  stages.forEach((stage) => {
    stage.dataset.lgSurface = stage.closest("header") ? "pill" : "composer";
  });
  return JSON.stringify({
    total: stages.length,
    pills: document.querySelectorAll('[data-lg-surface="pill"]').length,
    composers: document.querySelectorAll('[data-lg-surface="composer"]').length,
  });
})()`, false);
console.log("surfaces:", counts, "\n");

/** Frame timing over N frames, discarding the first 25 for the settle. */
const TIME = (frames) => `new Promise((resolve) => {
  const samples = []; let last = performance.now(); let n = 0;
  const step = (now) => {
    samples.push(now - last); last = now;
    if (++n < ${frames}) requestAnimationFrame(step);
    else {
      const s = samples.slice(25).sort((a, b) => a - b);
      const at = (p) => +s[Math.floor(s.length * p)].toFixed(2);
      resolve(JSON.stringify({
        p50: at(0.5), p95: at(0.95), mean: +(s.reduce((a, v) => a + v, 0) / s.length).toFixed(2),
        over16: s.filter((v) => v > 16.9).length, of: s.length,
      }));
    }
  };
  requestAnimationFrame(step);
})`;

/**
 * Install the CSS for one state.
 *
 * `scope` narrows which surfaces are affected, so a single surface can be timed
 * with the others held at their baseline rather than removed entirely.
 */
async function state({ filter = "on", backdrop = "on", scope = "all" } = {}) {
  await evaluate(`(() => {
    document.getElementById("lg-cost")?.remove();
    const rules = [];
    const target = ${JSON.stringify(scope)} === "all"
      ? ".lg-shell .glass__warp"
      : '.lg-stage[data-lg-surface="' + ${JSON.stringify(scope)} + '"] .lg-shell .glass__warp';
    // The complement, so the other surfaces are pinned at baseline instead of
    // being left in whatever state the last cell of the matrix left them.
    const other = ${JSON.stringify(scope)} === "all"
      ? null
      : '.lg-stage:not([data-lg-surface="' + ${JSON.stringify(scope)} + '"]) .lg-shell .glass__warp';
    if (${JSON.stringify(filter)} === "off") rules.push(target + " { filter: none !important; }");
    if (${JSON.stringify(backdrop)} === "off") {
      rules.push(target + " { backdrop-filter: none !important; -webkit-backdrop-filter: none !important; }");
    }
    if (other) {
      rules.push(other + " { filter: none !important; backdrop-filter: none !important; -webkit-backdrop-filter: none !important; }");
    }
    if (!rules.length) return true;
    const el = document.createElement("style");
    el.id = "lg-cost";
    el.textContent = rules.join("\\n");
    document.head.appendChild(el);
    return true;
  })()`, false);
  await sleep(1100);
}

const rows = [];
async function measure(label, options) {
  await state(options);
  const raw = await evaluate(TIME(200));
  let parsed = null;
  try {
    parsed = JSON.parse(raw);
  } catch {
    /* keep the raw text */
  }
  rows.push({ label, options, raw, ...(parsed ?? {}) });
  console.log(`${label.padEnd(44)} ${raw}`);
}

console.log("=== one surface at a time, the others pinned at baseline ===");
await measure("everything on", {});
await measure("pills: filter off", { scope: "pill", filter: "off", backdrop: "on" });
await measure("pills: filter + backdrop off", { scope: "pill", filter: "off", backdrop: "off" });
await measure("composer: filter off", { scope: "composer", filter: "off", backdrop: "on" });
await measure("composer: filter + backdrop off", { scope: "composer", filter: "off", backdrop: "off" });

console.log("\n=== every refracting surface, together ===");
await measure("all: filter + backdrop off (floor)", { filter: "off", backdrop: "off" });

// Put the app back exactly as found.
await evaluate(`document.getElementById("lg-cost")?.remove()`, false);
await evaluate(`document.getElementById("lg-freeze")?.remove()`, false);

console.log("\n=== what the displacement costs, over the glass Loom already had ===");
const find = (label) => rows.find((r) => r.label === label);
const pairs = [
  ["pills (4)", "pills: filter + backdrop off", "pills: filter off"],
  ["composer", "composer: filter + backdrop off", "composer: filter off"],
];
for (const [name, withoutBackdrop, withBackdrop] of pairs) {
  const a = find(withoutBackdrop);
  const b = find(withBackdrop);
  if (!a || !b) continue;
  // `withBackdrop` has the filter OFF and the backdrop ON; `withoutBackdrop` has
  // both off. So the difference is the backdrop-filter alone, and the filter's
  // own cost is `everything on` minus `filter off`.
  console.log(
    `  ${name.padEnd(12)} backdrop-filter alone: p50 ${b.p50}ms   without any glass: p50 ${a.p50}ms`,
  );
}

const all = find("everything on");
const pillsFilterOff = find("pills: filter off");
const composerFilterOff = find("composer: filter off");
if (all) {
  console.log(`\n  all surfaces, as shipped:            p50 ${all.p50}ms  p95 ${all.p95}ms  frames >16.9ms: ${all.over16}/${all.of}`);
}
if (pillsFilterOff && all) {
  console.log(`  the same with the pills' filter off: p50 ${pillsFilterOff.p50}ms  p95 ${pillsFilterOff.p95}ms  frames >16.9ms: ${pillsFilterOff.over16}/${pillsFilterOff.of}`);
}
if (composerFilterOff && all) {
  console.log(`  the same with the composer's off:    p50 ${composerFilterOff.p50}ms  p95 ${composerFilterOff.p95}ms  frames >16.9ms: ${composerFilterOff.over16}/${composerFilterOff.of}`);
}

console.log(
  "\n  Read the p95 and the `over16` count rather than the p50: a p50 that\n" +
    "  lands exactly on 16.7 or 20.8 is a frame boundary, which means the page\n" +
    "  is pacing rather than working, and a difference of 0.1ms there is noise.",
);

writeFileSync("target/cost-report.json", JSON.stringify(rows, null, 2));
console.log("\nwrote target/cost-report.json");
socket.close();
