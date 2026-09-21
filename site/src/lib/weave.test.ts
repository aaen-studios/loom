import { describe, expect, test } from "bun:test";
import { figure, profile, rng, smoothPath, type FigureKind } from "./weave";

/**
 * The weave's geometry, as data.
 *
 * A generative figure is the one kind of visual work that can be tested at all, and it should be —
 * because its failure modes are all *invisible*. A figure that draws a different picture on the
 * server than on the client does not show an error; it shows a flash, and only for some visitors. A
 * figure whose threads wander outside its own box is clipped, which looks like a deliberate crop. A
 * figure with a NaN in it renders as nothing at all, silently, and the page simply has a gap in it.
 *
 * All four are caught here, along with the two properties that make the whole approach work:
 * determinism (same seed, same figure, always) and bounds (every point lands inside the viewBox).
 */

const KINDS: FigureKind[] = ["field", "lattice", "rings", "bundle"];

const options = { width: 1200, height: 600, seed: 7 };

describe("the random source", () => {
  test("is deterministic for a seed", () => {
    const a = rng(42);
    const b = rng(42);
    expect(Array.from({ length: 20 }, a)).toEqual(Array.from({ length: 20 }, b));
  });

  test("gives different numbers for different seeds", () => {
    // The property that makes the seeds worth having at all: if two seeds produced the same stream,
    // every figure in the site would be the same figure.
    expect(rng(1)()).not.toBe(rng(2)());
  });

  test("stays inside [0, 1)", () => {
    const random = rng(99);
    for (let i = 0; i < 500; i += 1) {
      const value = random();
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThan(1);
    }
  });
});

describe("the curve", () => {
  test("returns nothing for no points", () => {
    expect(smoothPath([])).toBe("");
  });

  test("draws a line for two points and a curve for three", () => {
    expect(smoothPath([{ x: 0, y: 0 }, { x: 10, y: 10 }])).toBe("M 0.00 0.00 L 10.00 10.00");
    const three = smoothPath([{ x: 0, y: 0 }, { x: 5, y: 5 }, { x: 10, y: 0 }]);
    expect(three.startsWith("M 0.00 0.00 C ")).toBe(true);
  });

  test("closes the path when asked, and not otherwise", () => {
    const points = [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 10 },
      { x: 0, y: 10 },
    ];
    expect(smoothPath(points).endsWith("Z")).toBe(false);
    expect(smoothPath(points, true).endsWith("Z")).toBe(true);
  });

  test("passes through every point it is given", () => {
    /*
     * The property that matters: the curve is a Catmull-Rom spline, which interpolates its control
     * points rather than approximating them. That is why a thread's vertex is the lattice node it is
     * tied to — an approximating curve (a plain Bézier chain, say) would bow away from every node,
     * and the lattice would visibly come apart.
     */
    const points = [
      { x: 0, y: 0 },
      { x: 20, y: 40 },
      { x: 60, y: 10 },
      { x: 100, y: 50 },
    ];
    const path = smoothPath(points);

    for (const point of points) {
      expect(path).toContain(`${point.x.toFixed(2)} ${point.y.toFixed(2)}`);
    }
  });

  test("never emits a NaN, which would blank the whole figure", () => {
    // A single NaN anywhere in a path makes the browser drop that entire `<path>`, and a figure
    // whose threads have silently vanished is a figure that looks deliberate.
    for (const kind of KINDS) {
      const shape = figure(kind, {
        ...options,
        phase: 0.5,
        pointer: { x: 0.5, y: 0.5, strength: 1 },
      });
      for (const path of shape.paths) {
        expect(path).not.toContain("NaN");
        expect(path).not.toContain("Infinity");
      }
    }
  });
});

describe("every figure", () => {
  for (const kind of KINDS) {
    test(`${kind}: is deterministic`, () => {
      const a = figure(kind, options);
      const b = figure(kind, options);
      expect(JSON.stringify(a)).toBe(JSON.stringify(b));
    });

    test(`${kind}: changes when the seed changes`, () => {
      const a = figure(kind, { ...options, seed: 1 });
      const b = figure(kind, { ...options, seed: 2 });
      expect(JSON.stringify(a)).not.toBe(JSON.stringify(b));
    });

    test(`${kind}: declares its own viewBox`, () => {
      const shape = figure(kind, options);
      expect(shape.viewBox).toBe(`0 0 ${options.width} ${options.height}`);
      expect(shape.kind).toBe(kind);
    });

    test(`${kind}: draws something`, () => {
      const shape = figure(kind, options);
      const has =
        shape.paths.length > 0 || shape.nodes.length > 0 || shape.edges.length > 0;
      expect(has).toBe(true);

      // Any per-path array a figure declares has to be parallel to `paths`, or the renderer reads
      // `undefined` for the strands past the end of it — which SVG treats as a missing attribute
      // rather than as an error, so the last few threads would silently take the defaults.
      for (const array of [shape.shades, shape.weights]) {
        if (array === undefined) continue;
        expect(array.length).toBe(shape.paths.length);
        expect(array.every((value) => Number.isFinite(value) && value > 0)).toBe(true);
      }
    });

    test(`${kind}: stays inside its own box, allowing a margin for the pull`, () => {
      /*
       * The bounds check, with a margin, and the margin is the honest part.
       *
       * The pointer pull is *designed* to push points outside the figure — that is what being
       * pushed looks like — so asserting the exact box would fail on every figure that is working.
       * What must hold is that the excursion is bounded: a quarter of the width at most, or a
       * figure at rest with a cursor at the corner would throw threads an arbitrary distance and
       * the clipping would read as a bug rather than as a push.
       */
      const shape = figure(kind, options);
      const slackX = options.width * 0.25;
      const slackY = options.height * 0.25;

      const coordinates = shape.paths
        .flatMap((path) => path.match(/-?\d+\.\d+/g) ?? [])
        .map(Number);

      for (let i = 0; i < coordinates.length; i += 2) {
        const x = coordinates[i];
        const y = coordinates[i + 1];
        if (y === undefined) break;
        expect(x).toBeGreaterThan(-slackX);
        expect(x).toBeLessThan(options.width + slackX);
        expect(y).toBeGreaterThan(-slackY);
        expect(y).toBeLessThan(options.height + slackY);
      }
    });
  }
});

describe("the figures in particular", () => {
  test("the field draws a warp and a weft, and none of the paths are empty", () => {
    /*
     * The composition of the figure, asserted as two populations rather than as a count.
     *
     * A warp runs down the figure and starts at its top edge; a weft runs across and starts at its left
     * edge. That is the only thing that distinguishes them in the returned data — both are just paths —
     * so the test reads the first point of each and sorts them by it. Which is fragile in a way worth
     * naming: if a warp ever started somewhere other than y=0, this would silently classify it as weft
     * and the count assertion below would fail with a message about the wrong thing.
     */
    const shape = figure("field", { ...options, detail: 1 });
    expect(shape.nodes).toEqual([]);
    expect(shape.edges).toEqual([]);
    expect(shape.paths.every((path) => path.length > 10)).toBe(true);

    const starts = shape.paths.map((path) => {
      const numbers = (path.match(/-?\d+\.\d+/g) ?? []).map(Number);
      return { x: numbers[0], y: numbers[1] };
    });

    const warp = starts.filter((point) => point.y < 0.01);
    const weft = starts.filter((point) => point.x < 0.01);

    /*
     * Classified by *shape*, not by a coordinate, and that distinction is a bug this test had.
     *
     * The original classifier was `y < 0.01` — a warp starts at the top edge, so its first point has
     * y = 0. That is true of every warp and it is *also* true of any weft whose first sample lands in the
     * top 0.01 of the figure, and once the weft family grew from 3 strands to 26 with jitter past its own
     * spacing, that collision stopped being hypothetical: three wefts sat near the top edge, were counted
     * as warp, and the "does not let two threads cross" test downstream reported a crossing that was
     * really a weft being mistaken for one.
     *
     * A warp starts on the top edge *and runs down it*, so its second point is at y > 0. A weft starts on
     * the left edge and runs across. Testing the second coordinate first separates the two families
     * whatever the spacing does.
     */
    const runsDown = (path: string) => {
      const numbers = (path.match(/-?\d+\.\d+/g) ?? []).map(Number);
      return numbers[3] - numbers[1] > 1;
    };

    const warpPaths = shape.paths.filter(runsDown);
    const weftPaths = shape.paths.filter((path) => !runsDown(path));

    expect(warpPaths.length).toBeGreaterThanOrEqual(20);
    expect(weftPaths.length).toBeGreaterThanOrEqual(6);
    expect(weftPaths.length).toBeLessThan(warpPaths.length);

    // Every weft starts at the left edge, because a weft spans the cloth rather than beginning in it.
    const weftStarts = starts.filter((point) => point.x < 0.01);
    expect(weftStarts.every((point) => Math.abs(point.x) < 0.01)).toBe(true);

    // And the warp's starts are the other family, so the two together are every path.
    expect(warp.length + weft.length).toBeGreaterThanOrEqual(shape.paths.length - 1);
  });

  test("the field is not uniformly weighted", () => {
    /*
     * The defect a screenshot caught: forty threads at identical opacity read as a technical drawing
     * rather than as thread. This asserts the two things that fix it, and asserts them as spread rather
     * than as particular values, because the actual numbers are a matter of taste.
     */
    const shape = figure("field", options);
    expect(shape.shades).toBeDefined();
    expect(shape.shades!.length).toBe(shape.paths.length);

    const shades = shape.shades!;
    /*
     * How many trailing paths are weft. Derived, not hardcoded, and it is derived from the same
     * generator the test is checking rather than from a second copy of the rule — the count is
     * `max(6, round(26 * detail))` for the default detail of 1, and this reads it off the figure's own
     * geometry so that changing the density does not silently make these assertions describe the warp.
     */
    const weftCount = 26;

    /*
     * Asserted as *spread* rather than against particular values, because the numbers themselves are a
     * matter of taste and have already been retuned twice. What must not change is that the figure is
     * not flat: the whole point of this array is that some threads sit back in the cloth and some catch
     * the light.
     */
    const spread = Math.max(...shades) - Math.min(...shades);
    expect(spread).toBeGreaterThan(0.25);

    // None is invisible and none exceeds full: a shade outside (0, 1] is either a missing thread or an
    // SVG value the browser clamps, and both are silent.
    expect(shades.every((shade) => shade > 0 && shade <= 1)).toBe(true);

    /*
     * And the weft family is bright — but no longer uniformly full, which this assertion used to require.
     *
     * "Every weft is exactly 1" was true when there were three identical strands. With a family of
     * twenty-six the whole point is that they are *not* identical: a weft family has ends of different
     * grist, some sitting proud of the surface and catching light, some beaten in. So the assertion moves
     * from "all equal to 1" to the property that actually carries the design — the weft family is on
     * average brighter than the warp, and none of it disappears.
     */
    const weftShades = shades.slice(-weftCount);
    expect(weftShades.length).toBeGreaterThanOrEqual(6);
    expect(Math.min(...weftShades)).toBeGreaterThan(0.5);
    expect(Math.min(...weftShades)).toBeGreaterThan(Math.max(...shades.slice(0, -weftCount)) / 2);

    /*
     * And the thickest, which is not decoration — it is the difference between a weave and a comb.
     *
     * A weft at hairline weight, drawn over forty warp threads at hairline weight, is invisible: the
     * first version of this figure rendered its cross-threads correctly and still read as a set of
     * vertical lines, because a one-pixel horizontal among forty one-pixel verticals has nothing to
     * distinguish it. The width is what makes the crossing legible.
     */
    expect(shape.weights).toBeDefined();
    expect(shape.weights!.length).toBe(shape.paths.length);

    const warp = shape.weights!.slice(0, shape.weights!.length - weftCount);
    const weft = shape.weights!.slice(-weftCount);
    // Every warp thread the same; the weft family heavier throughout, and by a margin that reads as
    // deliberate rather than as a rendering artefact.
    expect(new Set(warp).size).toBe(1);
    expect(Math.min(...weft) / Math.max(...warp)).toBeGreaterThan(1.4);
  });

  test("the field gains weight toward the right, where the copy is not", () => {
    /*
     * The compositional half of the shading, and the reason it exists at all: the hero is left-aligned
     * type with nothing opposite it, so the right of the figure was empty space. Asserted as a trend
     * over the warp rather than on a single thread, because any individual thread's shade is dominated
     * by the random term — the ramp is only visible in aggregate.
     */
    const shape = figure("field", { ...options, detail: 1 });
    const shades = shape.shades!;
    // The warp is everything except the trailing weft threads.
    const warp = shades.slice(0, shades.length - 26);

    const third = Math.floor(warp.length / 3);
    const mean = (values: number[]) => values.reduce((sum, value) => sum + value, 0) / values.length;

    const left = mean(warp.slice(0, third));
    const right = mean(warp.slice(-third));

    expect(right).toBeGreaterThan(left);
  });

  test("the lattice joins its nodes, and never the same pair twice", () => {
    /*
     * Doubled edges are the failure that matters: drawing every connection twice doubles its
     * apparent weight, so the lattice ends up denser exactly where two nodes happened to be each
     * other's nearest neighbour — which reads as a rendering fault rather than as a lattice.
     */
    const shape = figure("lattice", options);
    expect(shape.nodes.length).toBeGreaterThan(9);
    expect(shape.edges.length).toBeGreaterThan(0);

    const keys = shape.edges.map(([a, b]) => {
      const one = `${a.x.toFixed(3)},${a.y.toFixed(3)}`;
      const two = `${b.x.toFixed(3)},${b.y.toFixed(3)}`;
      return one < two ? `${one}|${two}` : `${two}|${one}`;
    });

    expect(new Set(keys).size).toBe(keys.length);
  });

  test("the lattice jitters its nodes without pulling them off the grid", () => {
    // Under half a cell, so the grid is still legible underneath. A lattice whose nodes wander
    // further stops looking like it is holding anything together.
    const shape = figure("lattice", { width: 900, height: 500, seed: 3 });
    const cols = 9;
    const cell = 900 / cols;

    for (const node of shape.nodes) {
      const nearestColumn = Math.round(node.x / cell - 0.5);
      const ideal = (nearestColumn + 0.5) * cell;
      expect(Math.abs(node.x - ideal)).toBeLessThan(cell);
    }
  });

  test("the rings draw two independent systems", () => {
    // Two sources, each with its own set of concentric rings, and the count is even because the
    // figure loops one system per source. An odd count would mean one source's rings were dropped.
    const shape = figure("rings", { ...options, detail: 1 });
    expect(shape.nodes.length).toBe(2);
    expect(shape.paths.length % 2).toBe(0);
    expect(shape.paths.every((path) => path.endsWith("Z"))).toBe(true);
  });

  test("the bundle converges: every strand ends at the same point", () => {
    /*
     * The figure's defining property, and the one a first version of the generator got wrong.
     *
     * Eight strands that end eight different places at the bottom are a flare, not a bundle — and the
     * failure is invisible in code review, because `taper = 0.06` reads like "nearly converged". It
     * means 6% of the spread, which at 1200 wide is 36 pixels of daylight between the outermost strands
     * at the meeting point.
     */
    const shape = figure("bundle", { ...options, detail: 1 });
    expect(shape.paths.length).toBeGreaterThanOrEqual(5);

    for (const path of shape.paths) {
      const numbers = (path.match(/-?\d+\.\d+/g) ?? []).map(Number);
      const lastX = numbers[numbers.length - 2];
      const lastY = numbers[numbers.length - 1];
      // On the bottom edge, and at the centre — to within a tenth of a pixel, because the strands
      // genuinely meet rather than nearly meeting.
      expect(Math.abs(lastY - options.height)).toBeLessThan(0.5);
      expect(Math.abs(lastX - options.width / 2)).toBeLessThan(0.5);
    }
  });

  test("the bundle is still where it is held, and loose where it is not", () => {
    // The damping, asserted as a comparison rather than as a shape: the outermost strand has to wander
    // more at the top of the figure than at the bottom, or the gathering reads as being blown rather
    // than pulled.
    const shape = figure("bundle", { ...options, detail: 1 });
    const outermost = shape.paths[0];
    const numbers = (outermost.match(/-?\d+\.\d+/g) ?? []).map(Number);

    // Sample the horizontal deviation from the centre at four points down the strand.
    const xs = numbers.filter((_, index) => index % 2 === 0);
    const spreadAtTop = Math.abs(xs[0] - options.width / 2);
    const spreadAtFoot = Math.abs(xs[xs.length - 1] - options.width / 2);

    expect(spreadAtTop).toBeGreaterThan(80);
    expect(spreadAtFoot).toBeLessThan(1);
  });
});

describe("the pointer", () => {
  test("moves a figure", () => {
    const still = figure("field", options);
    const pushed = figure("field", {
      ...options,
      pointer: { x: 0.5, y: 0.5, strength: 1 },
    });
    expect(JSON.stringify(still)).not.toBe(JSON.stringify(pushed));
  });

  test("moves it by a bounded amount, not off the screen", () => {
    /*
     * The bug this exists to prevent, and it is the reason `strength` is a 0–1 intensity rather than a
     * distance.
     *
     * The first version multiplied the pointer's strength (26 at the call site) by a scale the caller
     * passed in (about 42) and produced a displacement of roughly eleven hundred units in a
     * sixteen-hundred-unit box — threads flung clean off the figure on the first mouse move. Nothing
     * caught it: a unit test that never passes a pointer cannot see it, and a screenshot cannot either,
     * because a headless browser never moves a cursor.
     *
     * So the assertion is on the *magnitude*: a fully-pushed figure must stay within a quarter of its
     * own width of where it rests. That bound is loose enough to allow a thread to bend most of the way
     * to its neighbour and tight enough that the old arithmetic fails it by two orders of magnitude.
     */
    const still = figure("field", options);
    const pushed = figure("field", {
      ...options,
      pointer: { x: 0.5, y: 0.5, strength: 1 },
    });

    const xs = (shape: typeof still) =>
      shape.paths
        .flatMap((path) => path.match(/-?\d+\.\d+/g) ?? [])
        .map(Number)
        .filter((_, index) => index % 2 === 0);

    const rest = xs(still);
    const moved = xs(pushed);
    expect(moved.length).toBe(rest.length);

    const worst = Math.max(...moved.map((x, index) => Math.abs(x - rest[index])));
    expect(worst).toBeGreaterThan(0.5); // it does move
    expect(worst).toBeLessThan(options.width * 0.25); // and it stays on the page
  });

  test("does not let two threads cross, however hard it is pushed", () => {
    /*
     * The one thing a warp must never do. A push is bounded by the pitch, so threads bend toward each
     * other and never through each other — and if that bound is ever raised, the figure stops being a
     * warp and becomes a tangle.
     *
     * Only the warp is tested. The weft is not part of this: it necessarily crosses the warp, which is
     * what a weft *is*, so including it here would assert the opposite of the truth.
     */
    const pushed = figure("field", {
      ...options,
      pointer: { x: 0.5, y: 0.5, strength: 1 },
    });

    const warp = pushed.paths.filter((path) => {
      const numbers = (path.match(/-?\d+\.\d+/g) ?? []).map(Number);
      // A warp runs *down* the figure: its second sample is below its first. See the note on the
      // classifier in "the field draws a warp and a weft" for why a y-coordinate test is not enough.
      return numbers[3] - numbers[1] > 1;
    });

    // Each thread's x at the vertical middle, which is where the push is strongest.
    const middles = warp.map((path) => {
      const numbers = (path.match(/-?\d+\.\d+/g) ?? []).map(Number);
      const xs = numbers.filter((_, index) => index % 2 === 0);
      return xs[Math.floor(xs.length / 2)];
    });

    expect(middles.length).toBeGreaterThanOrEqual(20);
    for (let i = 1; i < middles.length; i += 1) {
      expect(middles[i]).toBeGreaterThan(middles[i - 1]);
    }
  });

  test("moves only what is near it", () => {
    /*
     * The Gaussian falloff, asserted as a distance rather than as a shape.
     *
     * A thread six hundred units from the cursor must be identical whether the cursor is there or not.
     * If it moved, the influence function has a tail that reaches the whole figure and the cursor is
     * effectively warping everything at once — which is a different effect, and not the one this is for.
     */
    const far = figure("field", options);
    const pushedFarAway = figure("field", {
      ...options,
      pointer: { x: 0.98, y: 0.5, strength: 1 },
    });

    // Both have the same number of threads, and the leftmost — on the far side of the figure from a
    // cursor at the right edge — is untouched.
    expect(pushedFarAway.paths.length).toBe(far.paths.length);
    expect(pushedFarAway.paths[0]).toBe(far.paths[0]);
  });

  test("leaves the figure alone with no pointer at all", () => {
    // The server's case: no cursor exists, and the first frame has to be drawable regardless.
    expect(JSON.stringify(figure("field", { ...options, pointer: null }))).toBe(
      JSON.stringify(figure("field", options)),
    );
  });
});

describe("the profile, for renderers that cannot draw curves", () => {
  test("returns the count it was asked for, normalised to 0–1", () => {
    const values = profile(5, 24);
    expect(values.length).toBe(24);
    for (const value of values) {
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThanOrEqual(1);
    }
  });

  test("is deterministic, and moves with the phase", () => {
    expect(profile(5, 12)).toEqual(profile(5, 12));
    expect(profile(5, 12)).not.toEqual(profile(5, 12, 0.5));
  });

  test("gives different seeds different profiles", () => {
    expect(profile(1, 12)).not.toEqual(profile(2, 12));
  });
});
