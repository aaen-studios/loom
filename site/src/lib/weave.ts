/**
 * The weave: every figure on the site, as geometry.
 *
 * ---------------------------------------------------------------------------
 * Why this is one pure module rather than a pile of components
 * ---------------------------------------------------------------------------
 *
 * The page's whole visual language is a thread field, and a thread field is *maths* — a few sine terms,
 * a seeded random, a falloff function. Putting that maths in components would make it untestable and,
 * worse, untunable: the thing you want to change about a generative figure is a number, and the thing
 * you want to know about it is whether it still lands inside its own box.
 *
 * So this file has no React in it at all. It takes a size, a seed and a phase, and returns paths, nodes
 * and edges as plain data. `weave.test.ts` checks the properties that matter — that it is deterministic,
 * that it stays in bounds, that the pointer only affects what is near it, that nothing is ever `NaN` —
 * and `components/weave/figure.tsx` is then a thin renderer with nothing to get wrong.
 *
 * That separation also buys the one thing a generative site usually cannot have: **the server and the
 * client draw exactly the same first frame**, because they run the same function with the same
 * arguments. There is no hydration mismatch and no flash, which is the failure mode that makes
 * generative artwork on a landing page feel cheap.
 *
 * ---------------------------------------------------------------------------
 * The four figures
 * ---------------------------------------------------------------------------
 *
 *   - `field`   — a warp under tension with a weft thrown across it. The hero.
 *   - `lattice` — a jittered grid of joints joined to their nearest neighbours. Parts holding parts.
 *   - `rings`   — two sources of concentric distortion deflecting each other. Two systems meeting.
 *   - `bundle`  — strands converging to a single point. Many possibilities narrowing to one.
 *
 * All four are abstract, and that is a decision rather than a taste. A screenshot of a docking layout
 * at the size a web page can afford is illegible; a labelled diagram of one is a picture of software,
 * which is what every page in this category already has; an animated mock-up is the pattern this site
 * has abandoned three times. A generative figure says something none of those can — that the thing is
 * made of parts holding each other under tension — at any size, with no asset.
 */

/** A point in the figure's own coordinate space, which is always the viewBox. */
export interface Point {
  x: number;
  y: number;
}

/** Where the reader's cursor is, normalised to the figure's box, and how hard it pulls. */
export interface Pointer {
  /** 0–1 across the box. */
  x: number;
  /** 0–1 down the box. */
  y: number;
  /**
   * Intensity, 0–1. Not a distance.
   *
   * The displacement that a strength of 1 produces is decided by *the figure*, because it is bounded by
   * that figure's own pitch — and only the function that built the figure knows what its pitch is. See
   * the note on `pull` below for the bug that made this the right shape for the parameter.
   */
  strength: number;
}

export type FigureKind = "field" | "lattice" | "rings" | "bundle";

export interface FigureOptions {
  width: number;
  height: number;
  /** Changing the seed changes the whole figure and nothing else. */
  seed: number;
  /** Animation position. `0` and `1` are the same frame, so a loop wraps without a jump. */
  phase?: number;
  pointer?: Pointer | null;
  /**
   * A multiplier on how many elements the figure draws. `1` is the default; the hero runs slightly over
   * it and the decorative strips in the last two movements run well over it.
   */
  detail?: number;
}

export interface Figure {
  kind: FigureKind;
  /** Always `0 0 width height`, so a renderer can scale it without knowing anything. */
  viewBox: string;
  /** Open paths, in draw order. Threads, wefts and rings. */
  paths: string[];
  /**
   * A brightness multiplier per path, parallel to `paths`.
   *
   * ---------------------------------------------------------------------------
   * Why the artwork carries its own shading
   * ---------------------------------------------------------------------------
   *
   * The first version drew every thread at identical weight, and a screenshot is what showed how wrong
   * that was: forty identical strands read as *plotted output* — a graph, or a technical drawing —
   * rather than as thread. Cloth is not uniform. Threads differ in tension, in thickness, in how much
   * light they catch, and a warp with none of that variation looks manufactured in the way a wireframe
   * looks manufactured.
   *
   * So a figure states its own shading, as data, for the same reason it states its own geometry: the
   * renderer cannot know which strand should be brighter. Only the generator knows.
   *
   * Optional, because a `lattice` or a `rings` figure has no use for it — its edges and nodes are
   * already differentiated by kind.
   */
  shades?: number[];
  /**
   * A stroke width per path, parallel to `paths`.
   *
   * The weft needs it and nothing else does. A weft is *thrown across* the warp, so it passes over every
   * thread it crosses and is drawn heavier than any of them. At hairline weight it is invisible —
   * which is what happened on the first attempt at drawing a weave, where the cross-threads were
   * generated correctly, at full opacity, in the right place, and the figure still read as a comb.
   */
  weights?: number[];
  /** Marks: lattice joints, ring sources. */
  nodes: Point[];
  /** Connections between nodes, as index-free point pairs so a renderer needs no lookup. */
  edges: Array<[Point, Point]>;
}

/* ===========================================================================
   The primitives
=========================================================================== */

/**
 * A seeded random number generator — `mulberry32`.
 *
 * Not `Math.random`, and that is the whole point: every figure has to be identical on the server, on the
 * client, and on every visitor's machine, or the site draws a different picture for each person and none
 * of them matches the share card. A seed in, the same figure out, forever.
 */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Two decimals is under a tenth of a pixel at any size this is drawn at, and half the bytes. */
const n = (value: number): string => value.toFixed(2);

/**
 * A smooth path through a set of points, as cubic Béziers.
 *
 * Catmull-Rom converted to Bézier, which is the standard way to draw a curve that passes *through* its
 * own control points rather than near them. That distinction matters here: a thread is its points, and a
 * spline that merely approximates them would pull the strands off the joints they are tied to in the
 * lattice figure — and in the field figure it would round off the ends, so the threads would no longer
 * start and finish on the figure's edges.
 *
 * The endpoint handling clamps for an open path and wraps for a closed one, so an open thread does not
 * loop back on itself at the top of the figure.
 */
export function smoothPath(points: Point[], closed = false): string {
  if (points.length === 0) return "";
  if (points.length === 1) return `M ${n(points[0].x)} ${n(points[0].y)}`;
  if (points.length === 2) {
    return `M ${n(points[0].x)} ${n(points[0].y)} L ${n(points[1].x)} ${n(points[1].y)}`;
  }

  const count = points.length;
  const at = (index: number): Point => {
    if (closed) return points[(index + count) % count];
    return points[Math.max(0, Math.min(count - 1, index))];
  };

  let d = `M ${n(points[0].x)} ${n(points[0].y)}`;
  const last = closed ? count : count - 1;

  for (let i = 0; i < last; i += 1) {
    const p0 = at(i - 1);
    const p1 = at(i);
    const p2 = at(i + 1);
    const p3 = at(i + 2);

    // A sixth of the chord either side is the Catmull-Rom tangent, and it is what makes the curve
    // *continuous* rather than merely smooth: the incoming and outgoing tangents at every point are
    // derived from the same three neighbours, so they agree and there is no visible kink.
    const c1 = { x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6 };
    const c2 = { x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6 };

    d += ` C ${n(c1.x)} ${n(c1.y)} ${n(c2.x)} ${n(c2.y)} ${n(p2.x)} ${n(p2.y)}`;
  }

  if (closed) d += " Z";
  return d;
}

/**
 * How much a point is pulled by the pointer, as a two-dimensional displacement.
 *
 * ---------------------------------------------------------------------------
 * Why this takes a `max` rather than a scale factor, and why that was a bug
 * ---------------------------------------------------------------------------
 *
 * The first version multiplied the pointer's `strength` by a scale passed in by the caller — and the
 * caller passed `amplitude * 2.4`. With `strength` set to 26 at the call site, the displacement came to
 * roughly eleven hundred units in a sixteen-hundred-unit box: on the first mouse move the threads would
 * have been flung clean off the screen.
 *
 * Nothing caught it. It is invisible to a unit test that never passes a pointer, and it is invisible to
 * a screenshot, because a headless browser never moves a cursor — so the *only* interactive thing on
 * this page was untested in the one way that mattered.
 *
 * The fix is to make `strength` a plain 0–1 intensity and let the caller state the maximum displacement
 * **in the figure's own units**. That is the right shape for the parameter as well as the safe one: a
 * thread field's maximum deflection is bounded by its *pitch* — the distance to the next thread — and
 * only the function that built the field knows what that is.
 *
 * The falloff is a Gaussian, which is the right shape rather than the convenient one: it is smooth
 * everywhere, so there is no visible edge where the influence stops. A linear falloff draws a circle you
 * can see, and a hard cutoff draws a boundary.
 */
function pull(
  x: number,
  y: number,
  pointer: Pointer | null | undefined,
  width: number,
  height: number,
  max: number,
): { x: number; y: number } {
  if (!pointer || pointer.strength === 0) return { x: 0, y: 0 };

  const dx = x - pointer.x * width;
  const dy = y - pointer.y * height;
  const distance = Math.hypot(dx, dy);

  /*
   * At the exact centre of the pull there is no direction to push in, and dividing by the distance would
   * produce an infinity and then a `NaN` — and a single `NaN` anywhere in a path makes the browser drop
   * that entire `<path>`, silently. A figure whose threads have vanished looks like a deliberate choice.
   */
  if (distance < 0.001) return { x: 0, y: 0 };

  const reach = width * 0.2;
  const falloff = Math.exp(-(distance * distance) / (reach * reach));
  const amount = (pointer.strength * max * falloff) / distance;

  return { x: dx * amount, y: dy * amount };
}

/* ===========================================================================
   The figures
=========================================================================== */

/**
 * A warp under tension, with a weft thrown across it.
 *
 * ---------------------------------------------------------------------------
 * This figure was rebuilt four times, and each rebuild was the same lesson
 * ---------------------------------------------------------------------------
 *
 * Every version before this one was tuned by *proportion* — thread count, amplitude as a percentage of
 * the width, the ratio of bow to ripple — and each of those tunings produced a different wrong thing:
 *
 *   - Twenty-two threads with per-thread phases was **water**: a handful of wavy lines crossing at
 *     random, like a river seen from above.
 *   - Fifty-six threads was a **wash**: too fine to read as texture, too faint to read as graphic,
 *     landing in the awkward middle. It looked like paper grain.
 *   - Thirty-eight threads with a *shared* bow moved as one sheet, which was the breakthrough — and was
 *     immediately a **moiré curtain**, because a hundred threads moving in perfect unison is a pattern
 *     rather than a material.
 *
 * The lesson is that none of those were the variable. Three things make a warp, and all three are
 * structural rather than proportional:
 *
 *   1. **A shared bow with a lag.** Every thread bows together — a curtain bows as a *sheet*, and forty
 *      threads with forty private phases are forty private objects, which the eye is quite right to
 *      read as grain. But sharing the phase *exactly* is the opposite mistake: that is a pattern. A real
 *      fold *travels*, so neighbours are related by a small lag. See `lag` below.
 *
 *   2. **A weft.** A warp alone is a comb. Warp and weft together is cloth, which is what the word means,
 *      and it is what makes the figure legible as *woven* rather than as a set of vertical lines.
 *
 *   3. **Few, definite threads.** Once (1) and (2) were right, the count that read best was the *low*
 *      one. Twenty-six at this size puts the pitch at about sixty units, where each thread is a
 *      deliberate mark and each crossing is unmissable. This is a *drawing* of cloth, not a simulation
 *      of it.
 *
 * `amplitude` is derived from the pitch rather than from the width, and that is the load-bearing line of
 * the whole function — see the note beside it.
 */
function field(o: FigureOptions): Figure {
  const { width, height, seed } = o;
  const phase = o.phase ?? 0;
  const detail = o.detail ?? 1;

  const count = Math.max(6, Math.round(26 * detail));
  const samples = 48;

  /** The distance between two threads, and the figure's fundamental unit. */
  const pitch = width / count;

  /**
   * A third of the pitch, multiplied per thread by up to 1.1 — so the true worst case is 0.37 of the gap
   * on each side, and two neighbours bending toward each other at once still leave a third of the gap
   * between them.
   *
   * The reason this is written as a fraction of the *pitch* and not of the width is the point of the
   * line: the maximum a thread may travel is bounded by the distance to its neighbour, so density and
   * amplitude can never be combined into an unsafe pair. Written as a fraction of the width, they are two
   * independent knobs that happen to be safe at the numbers they were chosen with and unsafe at the next
   * ones — and "the threads cross each other" is the failure that stops a warp being a warp.
   */
  const amplitude = pitch * 0.34;

  const paths: string[] = [];
  const shades: number[] = [];
  /** A warp thread is a hairline; a weft is heavier. See `weights` in the `Figure` interface. */
  const weights: number[] = [];

  /*
   * The bow, shared by every thread in the field.
   *
   * Drawn once from the figure's seed, above the loop. The per-thread random below decides how far each
   * thread travels and how it lags — never the frequency, and never the sign.
   */
  const shared = rng(seed + 104729);
  const bowSlow = 0.5 + shared() * 0.35;
  const bowPhase = shared() * Math.PI * 2;

  for (let i = 0; i < count; i += 1) {
    const random = rng(seed + i * 7919);
    const base = ((i + 0.5) / count) * width;

    /*
     * Where this thread sits across the cloth, 0–1, and the lag that follows from it.
     *
     * The lag is a smooth ramp from one edge of the field to the other — a fold running diagonally,
     * which is what a warp strung on a beam and pulled at one end actually does — plus a little jitter so
     * the ramp is not itself visible as a ramp.
     *
     * About four tenths of a cycle edge to edge. Narrower and it reads as noise on top of a shared wave,
     * which is the moiré curtain this replaced; much wider and the two ends of the figure stop being one
     * sheet, because they are moving in opposite directions at the same moment.
     */
    const across = count === 1 ? 0.5 : i / (count - 1);
    const lag = (across - 0.5) * 2.6 + (random() - 0.5) * 0.5;

    /** How far this thread travels. It is what makes two neighbours visibly different threads. */
    const reach = 0.5 + random() * 0.6;
    /** This thread's own faint ripple, at a fraction of a full cycle. */
    const ripple = 0.6 + random() * 1.4;
    const ripplePhase = random() * Math.PI * 2;

    const points: Point[] = [];

    for (let s = 0; s <= samples; s += 1) {
      const t = s / samples;
      const y = t * height;

      let x =
        base +
        amplitude *
          reach *
          (Math.sin(t * Math.PI * 2 * bowSlow + bowPhase + lag + phase * Math.PI * 2) +
            /*
             * The ripple is deliberately small — a tenth of the shared bow.
             *
             * It exists only so that no two threads are exact copies of each other. Raised, it is what
             * turns a sheet into wood grain: at five times this weight, with the phase per thread, it was
             * the *entire* previous version of this figure, and it looked like a contour map.
             */
            0.1 * Math.sin(t * Math.PI * 2 * ripple + ripplePhase - phase * Math.PI * 1.3));

      /*
       * The pointer, and only on the horizontal axis.
       *
       * A thread is *fixed at both ends* — tied to the beam at the top and to the cloth at the bottom —
       * so it can bend sideways under a load and it cannot slide. That is why this takes only the x
       * component where the lattice takes both.
       *
       * The pull is scaled by `sin(πt)`, which is zero at both ends and one in the middle: a thread that
       * moved as much at its pinned ends as at its centre would look like it was being dragged rather
       * than pushed. And the maximum is bounded by the pitch, so a pushed thread bends toward its
       * neighbour and the two can never cross.
       */
      const looseness = Math.sin(Math.PI * t);
      x += pull(x, y, o.pointer, width, height, pitch * 0.5 * looseness).x;

      points.push({ x, y });
    }

    paths.push(smoothPath(points));

    /*
     * This thread's brightness — the second thing a screenshot caught.
     *
     * Two terms. The random one is the material: some threads are tight and bright, some sit back in the
     * cloth. The `across` one is doing compositional work: it makes the field build toward the right edge,
     * which is *where the copy is not*.
     *
     * The hero is left-aligned type with nothing opposite it, so the right half of the figure was five
     * hundred pixels of uniform line and read as empty. A field that gains weight as it travels right
     * gives the composition a second mass without moving the type or adding an element — the figure is
     * simply heavier where the page is quiet.
     */
    shades.push(Math.min(1, 0.4 + 0.28 * random() + 0.44 * across));
    weights.push(1);
  }

  /*
   * The weft: threads thrown across the warp.
   *
   * Three, at irregular heights, and both of those numbers matter. A warp alone is a comb; warp and weft
   * together is cloth. But *many* evenly-spaced crossings are a grid — the whole difference between
   * "this is woven" and "this is graph paper" is in how few of them there are and how unevenly they sit.
   *
   * They bow less than the warp, because a weft is pulled taut between two selvedges at the sides and so
   * is nearly straight. And they take the pointer on the *vertical* axis, for the mirror-image reason the
   * warp takes it on the horizontal: a weft is pinned at both ends too, just at the sides rather than top
   * and bottom.
   */
  const weftCount = Math.max(2, Math.round(3 * detail));

  for (let w = 0; w < weftCount; w += 1) {
    const random = rng(seed + 90001 + w * 6151);

    // Irregular by a fifth of the figure's height, which is enough to break the rhythm and not enough to
    // make the spacing look accidental.
    const base = ((w + 0.5) / weftCount + (random() - 0.5) * 0.2) * height;
    const bow = 0.25 + random() * 0.5;
    const bowPhase = random() * Math.PI * 2;

    const points: Point[] = [];
    const steps = 64;

    for (let s = 0; s <= steps; s += 1) {
      const t = s / steps;
      const x = t * width;
      const y =
        base +
        Math.sin(t * Math.PI * 2 * bow + bowPhase + phase * Math.PI * 2) * height * 0.02 +
        // Bounded by the figure's height over the weft count, so two wefts can never swap places.
        pull(x, base, o.pointer, width, height, (height / weftCount) * 0.3 * Math.sin(Math.PI * t)).y;

      points.push({ x, y });
    }

    paths.push(smoothPath(points));
    /*
     * Full brightness and half again the width of any warp thread.
     *
     * This is not decoration — it is the difference between a weave and a comb. A weft drawn at hairline
     * weight over twenty-six hairline warp threads has nothing to distinguish it, so the figure reads as
     * vertical lines with some faint horizontal ones behind them. Being *thrown across*, a real weft
     * passes over the warp: brighter, and heavier.
     */
    shades.push(1);
    weights.push(2);
  }

  return { kind: "field", viewBox: `0 0 ${width} ${height}`, paths, shades, weights, nodes: [], edges: [] };
}

/**
 * A lattice: joints and the connections holding them.
 *
 * Jittered off a regular grid, then joined to the two nearest neighbours — which is the smallest amount
 * of structure that reads as a *system* rather than as a scatter plot. The jitter is under a quarter of
 * a cell, so the grid stays legible underneath: a lattice whose joints have wandered stops looking like
 * it is holding anything together, which is the opposite of the point.
 */
function lattice(o: FigureOptions): Figure {
  const { width, height, seed } = o;
  const detail = o.detail ?? 1;

  const cols = Math.max(3, Math.round(9 * detail));
  const rows = Math.max(3, Math.round(5 * detail));
  const cellWidth = width / cols;
  const cellHeight = height / rows;

  const random = rng(seed);
  const joints: Point[] = [];

  for (let j = 0; j < rows; j += 1) {
    for (let i = 0; i < cols; i += 1) {
      const x = ((i + 0.5) / cols) * width + (random() - 0.5) * cellWidth * 0.46;
      const y = ((j + 0.5) / rows) * height + (random() - 0.5) * cellHeight * 0.46;
      joints.push({ x, y });
    }
  }

  /*
   * The pointer displaces the joints it is near, so a lattice can be pushed at.
   *
   * Both axes, unlike the field's threads, and the reason is the same reason stated the other way round:
   * a thread is fixed at both ends so it can only bend sideways, and a joint is fixed to nothing, so a
   * force that moved it horizontally only would look like a shear rather than like something pushed.
   *
   * Bounded by half the smaller cell, so a joint can be shoved around without swapping places with a
   * neighbour. An inversion would rewire which nodes are joined — the connections are computed from the
   * *resulting* positions — and the lattice would visibly tie itself in a knot.
   */
  const limit = Math.min(cellWidth, cellHeight) * 0.5;
  const nodes = joints.map((joint) => {
    const push = pull(joint.x, joint.y, o.pointer, width, height, limit);
    return { x: joint.x + push.x, y: joint.y + push.y };
  });

  /*
   * Two neighbours each, then deduplicated.
   *
   * The deduplication is not tidiness. Without it every edge is generated twice — once as A's nearest
   * neighbour and once as B's — which doubles the apparent weight of exactly those connections where two
   * nodes happen to be each other's nearest, so the lattice develops thicker links in scattered places
   * and reads as a rendering fault rather than as a lattice. The key is ordered so A→B and B→A collapse
   * to the same string.
   */
  const seen = new Set<string>();
  const edges: Array<[Point, Point]> = [];

  for (let i = 0; i < nodes.length; i += 1) {
    const nearest = nodes
      .map((other, index) => ({
        index,
        distance: Math.hypot(other.x - nodes[i].x, other.y - nodes[i].y),
      }))
      .filter((entry) => entry.index !== i)
      .sort((a, b) => a.distance - b.distance)
      .slice(0, 2);

    for (const { index } of nearest) {
      const key = i < index ? `${i}:${index}` : `${index}:${i}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push([nodes[i], nodes[index]]);
    }
  }

  return { kind: "lattice", viewBox: `0 0 ${width} ${height}`, paths: [], nodes, edges };
}

/**
 * Two sources, interfering.
 *
 * Concentric rings around each of two centres, each ring displaced by a wave in its own angle and pushed
 * away by the *other* source. That second term is the whole figure: without it two ring systems simply
 * overlap, and with it they visibly deflect each other — which is what interference looks like, and is
 * the thing this figure exists to say. Two systems meeting, and neither unchanged by it.
 */
function rings(o: FigureOptions): Figure {
  const { width, height, seed } = o;
  const phase = o.phase ?? 0;
  const detail = o.detail ?? 1;

  const random = rng(seed);
  const sources: Point[] = [
    { x: width * (0.28 + random() * 0.06), y: height * (0.6 + random() * 0.06) },
    { x: width * (0.7 + random() * 0.06), y: height * (0.38 + random() * 0.06) },
  ];

  const count = Math.max(4, Math.round(7 * detail));
  const samples = 64;
  const unit = Math.min(width, height);
  const paths: string[] = [];

  for (let s = 0; s < sources.length; s += 1) {
    const centre = sources[s];
    const other = sources[(s + 1) % sources.length];

    for (let k = 1; k <= count; k += 1) {
      const base = (k / count) * unit * 0.52;
      const points: Point[] = [];

      for (let a = 0; a < samples; a += 1) {
        const angle = (a / samples) * Math.PI * 2;

        // The ring's own wobble, slow enough that the ring stays a ring.
        let radius = base * (1 + 0.05 * Math.sin(angle * 3 + phase * Math.PI * 2 + k * 0.7));

        // And the deflection from the other source, strongest where the two are closest. The floor on
        // the denominator is what stops a ring from exploding when it passes near the other centre.
        const x = centre.x + Math.cos(angle) * radius;
        const y = centre.y + Math.sin(angle) * radius;
        const gap = Math.hypot(x - other.x, y - other.y);
        radius += (unit * 0.035 * radius) / Math.max(gap, unit * 0.08);

        points.push({ x: centre.x + Math.cos(angle) * radius, y: centre.y + Math.sin(angle) * radius });
      }

      paths.push(smoothPath(points, true));
    }
  }

  return { kind: "rings", viewBox: `0 0 ${width} ${height}`, paths, nodes: sources, edges: [] };
}

/**
 * Strands converging to a point.
 *
 * The taper follows a smoothstep rather than a straight line, and that is the figure's entire character:
 * a linear taper is a triangle, and a smoothstep one is a bundle being *gathered* — the strands hold their
 * spacing for the first third, give way quickly in the middle, and arrive together.
 *
 * ---------------------------------------------------------------------------
 * Two details, both of which the first version got wrong
 * ---------------------------------------------------------------------------
 *
 *   - **The strands genuinely meet.** `GATHER` is *zero*, not "nearly zero". It was 0.06 to begin with,
 *     which reads like a tight taper and is — as a fraction of a 600-unit spread — thirty-six units, so
 *     the outermost strands met two and a half pixels apart. That is a bundle converging to a *line*,
 *     which is a different shape and not the one this figure is for. A test asserting convergence to
 *     within half a pixel found it.
 *
 *   - **The wobble damps as the strands gather.** The lateral sine is multiplied by the same easing term
 *     that narrows the bundle, so a strand is lively where it is loose and still where it is held. Without
 *     the damping the strands arrive at the point still oscillating, which looks like they are being blown
 *     rather than pulled — and, worse, the sine's phase at the foot of the figure depends on the seed, so
 *     the strands would scatter around the meeting point by a distance that changes per figure.
 */
function bundle(o: FigureOptions): Figure {
  const { width, height, seed } = o;
  const phase = o.phase ?? 0;
  const detail = o.detail ?? 1;

  const count = Math.max(5, Math.round(16 * detail));
  const samples = 34;
  const spread = width * 0.5;
  /** How much of the spread is left at the foot. Zero, so every strand's last sample is one point. */
  const GATHER = 0;

  const paths: string[] = [];

  for (let i = 0; i < count; i += 1) {
    const random = rng(seed + i * 104729);
    const offset = count === 1 ? 0 : i / (count - 1) - 0.5;

    const wobble = 0.5 + random() * 1.4;
    const seedPhase = random() * Math.PI * 2;
    const points: Point[] = [];

    for (let s = 0; s <= samples; s += 1) {
      const t = s / samples;
      const eased = t * t * (3 - 2 * t);

      const widthAt = spread * (1 - eased) + spread * GATHER * eased;
      const x =
        width / 2 +
        offset * 2 * widthAt +
        Math.sin(t * Math.PI * 2 * wobble + seedPhase + phase * Math.PI * 2) *
          width *
          0.012 *
          (1 - eased);

      points.push({ x, y: t * height });
    }

    paths.push(smoothPath(points));
  }

  return { kind: "bundle", viewBox: `0 0 ${width} ${height}`, paths, nodes: [], edges: [] };
}

/**
 * The figure, by kind.
 *
 * A switch rather than a lookup table, because the four generators take the same options and return the
 * same shape — so this is the one place that has to know which is which, and a `Record` would add an
 * indirection without adding a guarantee.
 */
export function figure(kind: FigureKind, options: FigureOptions): Figure {
  switch (kind) {
    case "field":
      return field(options);
    case "lattice":
      return lattice(options);
    case "rings":
      return rings(options);
    case "bundle":
      return bundle(options);
  }
}

/**
 * A one-dimensional profile, for renderers that cannot draw a path.
 *
 * The Open Graph card is drawn by Satori, which implements enough of SVG to be unpredictable and enough
 * of flexbox to be reliable — so the share card draws this as a row of thin bars rather than as curves.
 * It is the same sum of sines the field is built from, sampled down one axis, which is what keeps the
 * card recognisably the same artwork as the page.
 *
 * Normalised to 0–1, so a caller can scale it to whatever it is drawing without knowing what amplitude
 * the terms happened to sum to.
 */
export function profile(seed: number, count: number, phase = 0): number[] {
  const random = rng(seed);

  const terms = Array.from({ length: 4 }, () => ({
    frequency: 0.5 + random() * 2.4,
    offset: random() * Math.PI * 2,
    weight: 0.4 + random(),
  }));

  const total = terms.reduce((sum, term) => sum + term.weight, 0) || 1;

  return Array.from({ length: count }, (_, i) => {
    const t = count === 1 ? 0 : i / (count - 1);
    const sum = terms.reduce(
      (running, term) =>
        running +
        term.weight *
          Math.sin(t * Math.PI * 2 * term.frequency + term.offset + phase * Math.PI * 2),
      0,
    );
    return (sum / total + 1) / 2;
  });
}
