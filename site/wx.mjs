import { figure } from "./src/lib/weave.ts";

const opt = { width: 1200, height: 600, seed: 7 };

function warpMiddles(f) {
  return f.paths
    .map((p) => (p.match(/-?\d+\.\d+/g) ?? []).map(Number))
    .filter((nums) => nums[1] < 0.01)
    .map((nums) => {
      const xs = nums.filter((_, k) => k % 2 === 0);
      return xs[Math.floor(xs.length / 2)];
    });
}

function warpBows(f) {
  return f.paths
    .map((p) => (p.match(/-?\d+\.\d+/g) ?? []).map(Number))
    .filter((nums) => nums[1] < 0.01)
    .map((nums) => {
      const xs = nums.filter((_, k) => k % 2 === 0);
      const base = (xs[0] + xs[xs.length - 1]) / 2;
      return Math.max(...xs.map((x) => Math.abs(x - base)));
    });
}

for (const [name, o] of [
  ["rest  ", opt],
  ["pushed", { ...opt, pointer: { x: 0.5, y: 0.5, strength: 1 } }],
]) {
  const f = figure("field", o);
  const m = warpMiddles(f);
  const bows = warpBows(f);
  const pitch = opt.width / (f.paths.length - 26);
  console.log(name, "paths", f.paths.length, "warp", m.length, "pitch~", pitch.toFixed(2));
  console.log("   bow min/mean/max", Math.min(...bows).toFixed(2), (bows.reduce((a, b) => a + b, 0) / bows.length).toFixed(2), Math.max(...bows).toFixed(2));
  let worst = Infinity;
  let at = -1;
  for (let k = 1; k < m.length; k++) {
    const gap = m[k] - m[k - 1];
    if (gap < worst) {
      worst = gap;
      at = k;
    }
  }
  console.log("   tightest neighbour gap", worst.toFixed(2), "at", at, "of", m.length);
  console.log("   min adjacent gap ratio to pitch", (worst / pitch).toFixed(3));
  const inv = [];
  for (let k = 1; k < m.length; k++) if (!(m[k] > m[k - 1])) inv.push([k, m[k - 1].toFixed(2), m[k].toFixed(2)]);
  console.log("   inversions:", inv.length, inv.slice(0, 5));
}
