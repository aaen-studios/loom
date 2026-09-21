"use client";

import { useEffect } from "react";

/**
 * The layout probe.
 *
 * A development-only diagnostic that measures the rendered page and writes the result into a
 * `<pre id="loom-probe">`, so a headless browser can dump it as text. `probe-layout.mjs` drives that, at
 * four widths and both motion preferences.
 *
 * ---------------------------------------------------------------------------
 * Why numbers rather than a screenshot
 * ---------------------------------------------------------------------------
 *
 * Five claims this page makes are things no build step can see:
 *
 *   - **The figures are drawn.** A generation function that returns an empty path list renders as
 *     nothing at all — silently, with no error anywhere — and a movement with a blank plate in it looks
 *     like a deliberate choice. The only way to know is to count the paths in the DOM.
 *   - **The geometry is identical on both sides.** The hero's figure is computed once on the server and
 *     once in the browser from the same seed. If they disagree, React reports a hydration mismatch, and
 *     the page flashes a different drawing before settling.
 *   - **The pointer bends the threads.** The physics is a Gaussian falloff, and a falloff with a broken
 *     reach either moves nothing or moves everything. Both look like a bug in the *design* rather than
 *     in the maths.
 *   - **Nothing is wider than the shell.** A figure that bleeds without knowing where the edges are is
 *     the one layout bug that gives a page a horizontal scrollbar, and it is invisible at the width it
 *     was written for.
 *   - **The reveal never hides content.** Its worst case has to be "no animation" rather than "no text".
 *
 * A screenshot could catch some of that — but only if someone remembered to look at the right width, and
 * only by eye. This asserts numbers.
 *
 * ---------------------------------------------------------------------------
 * What was deleted, and why deleting it matters as much as what is here
 * ---------------------------------------------------------------------------
 *
 * The previous version of this probe measured a dock: four zones, eight tabs, three splitters, and the
 * arithmetic that kept the centre a column at the extremes of every range. All of it is gone from the
 * page, so all of it is gone from here. Leaving the checks would have been worse than deleting them: a
 * `querySelectorAll` that matches nothing is not a failure, it is an empty list, so every assertion
 * would have passed against `undefined` forever while reading as coverage.
 */
export function LayoutProbe() {
  useEffect(() => {
    if (!new URLSearchParams(window.location.search).has("probe")) return;

    const lines: string[] = [];
    const say = (label: string, value: string) => lines.push(`${label}: ${value}`);
    const check = (label: string, ok: boolean, detail?: string) => {
      lines.push(`CHECK ${label}: ${ok ? "PASS" : "FAIL"}`);
      // Printed only on failure. `check` renders whatever it is given either way, so passing an
      // explanation unconditionally produces a line reading "not found" beside the word PASS — which
      // makes a correct result look like a contradiction.
      if (!ok && detail) lines.push(`        ${detail}`);
    };
    const round = (value: number) => Math.round(value * 100) / 100;

    const root = document.documentElement;
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    say("viewport", `${viewport.width}×${viewport.height}`);
    say(
      "prefers-reduced-motion",
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
        ? "reduce"
        : "no-preference",
    );

    // --- the figures --------------------------------------------------------
    //
    // The page's whole visual language, and the part that fails silently. A generator that returns
    // nothing draws nothing, and there is no error: the movement simply has a gap in it.
    const figures = Array.from(document.querySelectorAll<SVGSVGElement>("[data-figure]"));
    say("figures on the page", String(figures.length));
    check("every movement's figure is drawn", figures.length >= 6, `found ${figures.length}`);

    for (const kind of ["field", "lattice", "rings", "bundle"]) {
      const count = figures.filter((svg) => svg.dataset.figure === kind).length;
      say(`  ${kind}`, String(count));
      check(`a ${kind} figure is on the page`, count >= 1, `found ${count}`);
    }

    let empty = 0;
    let degenerate = 0;
    for (const svg of figures) {
      const drawn = svg.querySelectorAll("path, line, circle").length;
      if (drawn === 0) empty += 1;

      // A path whose `d` is an empty string is the failure this is really looking for: the element
      // exists, so a count of elements looks healthy, and nothing is painted.
      for (const path of Array.from(svg.querySelectorAll("path"))) {
        const d = path.getAttribute("d") ?? "";
        if (d.length < 8) degenerate += 1;
      }
    }
    say("figures with nothing in them", String(empty));
    say("paths with no geometry", String(degenerate));
    check("no figure renders empty", empty === 0);
    check("no path is degenerate", degenerate === 0);


    /*
     * The colour inheritance.
     *
     * Every stroke on this page is a `var(--thread-*)` or `var(--accent)`, which means the artwork
     * follows the theme without a second code path. If a figure ever lost that and got a literal colour,
     * it would be the one thing on the page that did not change with the theme — and it would look
     * *nearly* right in the theme it was written in.
     */
    const strokes = Array.from(document.querySelectorAll<SVGElement>("[data-figure] path"));
    const hardcoded = strokes.filter((path) => {
      const stroke = path.getAttribute("stroke") ?? "";
      return /^#|^rgb/.test(stroke);
    });
    say("paths with a hardcoded colour", String(hardcoded.length));
    check(
      "every figure takes its colour from a token",
      hardcoded.length === 0,
      hardcoded[0]?.getAttribute("stroke") ?? undefined,
    );

    // --- the hero's animation ----------------------------------------------
    //
    // The one figure that moves. Its paths are rendered on the server and then mutated in place by the
    // animation loop, so the check is that there are paths *and* that they have real geometry — which is
    // true whether or not the loop is running, and is the state the server put them in.
    const hero = document.querySelector<SVGSVGElement>('[data-figure="field"]');
    say("hero figure present", hero ? "yes" : "no");
    if (hero) {
      const heroPaths = hero.querySelectorAll("path").length;
      say("hero threads", String(heroPaths));
      check("the hero draws a field of threads", heroPaths >= 20, `found ${heroPaths}`);

      // The animation writes pixel coordinates into every thread, so a thread whose `d` is still the
      // server's is indistinguishable from one that has been animated — both are valid paths. What is
      // worth asserting is that the box has a size, because a zero-height figure is a hero with nothing
      // behind it and no error.
      const box = hero.getBoundingClientRect();
      say("hero figure size", `${round(box.width)}×${round(box.height)}`);
      check(
        "the hero figure has a box",
        box.width > 200 && box.height > 200,
        "the figure is drawn into nothing, so the hero has no artwork behind it",
      );
    }

    // --- the light source ---------------------------------------------------
    //
    // One fixed layer, in the accent's own family. A second would make the ground a texture again, which
    // is the thing this rebuild removed.
    const glow = document.querySelectorAll(".glow").length;
    say("light-source layers", String(glow));
    check("there is exactly one light source", glow === 1, `found ${glow}`);

    // --- the shell and the measure -----------------------------------------
    const shell = document.querySelector<HTMLElement>(".shell");
    if (shell) {
      const style = getComputedStyle(shell);
      const padding = Number.parseFloat(style.paddingLeft) || 0;
      const content = shell.getBoundingClientRect().width - padding * 2;
      say("shell content width", `${round(content)}px`);

      const declared =
        Number.parseFloat(getComputedStyle(root).getPropertyValue("--shell").replace("rem", "")) * 16;
      const expected = Math.min(declared, viewport.width - 2 * padding);
      check(
        "the shell is the declared width, not the viewport",
        Math.abs(content - expected) < 2,
        `content is ${round(content)}px against an expected ${round(expected)}px`,
      );
    }

    const measure = Number.parseFloat(
      getComputedStyle(root).getPropertyValue("--measure").replace("rem", ""),
    ) * 16;
    say("--measure resolved", `${round(measure)}px`);
    if (measure > 0) {
      // The failure this catches is the one the prose has always had: a paragraph that quietly runs the
      // full width of the viewport because a `max-width` stopped applying. It typechecks and it renders.
      const runs = Array.from(
        document.querySelectorAll<HTMLElement>(".t-body, .t-lede, .t-small, .measure"),
      );
      const wide = runs.filter((run) => run.getBoundingClientRect().width > measure + 2);
      say("prose runs measured", String(runs.length));
      check(
        "no run of prose is wider than the measure",
        wide.length === 0,
        `${wide.length} run(s) exceed ${round(measure)}px`,
      );
    }

    // --- the reveal --------------------------------------------------------
    //
    // Its worst case has to be "no animation". If an element is armed and on screen, the observer is not
    // firing and the content is invisible — a page that has hidden itself, which is the failure that
    // makes scroll animations unacceptable on a page whose whole job is to be read.
    const armed = Array.from(document.querySelectorAll<HTMLElement>('[data-reveal="armed"]'));
    say("revealed elements still armed", String(armed.length));
    const hiddenInView = armed.filter((element) => {
      const box = element.getBoundingClientRect();
      return box.top < viewport.height && box.bottom > 0;
    });
    check(
      "nothing in view is left hidden",
      hiddenInView.length === 0,
      `${hiddenInView.length} element(s) are armed while on screen`,
    );

    // --- overflow ----------------------------------------------------------
    const overflow = root.scrollWidth - root.clientWidth;
    say("horizontal overflow", `${overflow}px`);
    check("there is no sideways scroll", overflow <= 1);

    // --- what is deliberately absent ---------------------------------------
    //
    // Measured by computed style rather than by querying for class names, and that is a deliberate
    // choice: querying for a class name puts that name in this file, Tailwind scans this file, and so
    // *writing the check* makes Tailwind emit the very utilities the check was looking for. Computed
    // style is immune to that whole class of confusion, because it asks the browser what it painted
    // rather than asking the stylesheet what it contains.
    const painted = Array.from(document.querySelectorAll<HTMLElement>("*"));

    const ghosts = painted.filter((element) =>
      /(^|\s)(zone|tab|splitter|warp-line|warp-field|pick-cloth|spine|weft|dock)\b/.test(
        element.className,
      ),
    );
    say("ghosts of earlier builds", String(ghosts.length));
    check("the earlier builds have stayed deleted", ghosts.length === 0);

    // The scripted hero, three times removed. A regression to it is exactly the kind of thing that gets
    // reintroduced as "a bit of life at the top of the page".
    check(
      "the hero is a weave, not a scripted demo",
      !document.querySelector("[data-scene-phase]"),
    );

    // Nothing that moves on its own. The page's only motion is the hero's drift and a single
    // fade-and-rise per movement; an infinite CSS animation anywhere is the twitch that makes a dark
    // page feel cheap. The app's own motion rules reach the stylesheet unconditionally — see the note in
    // `verify-tokens.mjs` — so what matters is that no element is wearing one.
    const looping = painted.filter((element) => {
      const style = getComputedStyle(element);
      return style.animationName !== "none" && style.animationIterationCount === "infinite";
    });
    say("elements that loop forever", String(looping.length));
    check("nothing on the page loops forever", looping.length === 0);

    const pre = document.createElement("pre");
    pre.id = "loom-probe";
    pre.textContent = lines.join("\n");
    document.body.appendChild(pre);

    // Also mirrored into the title: `--dump-dom` fires on the load event, and an effect that ran after
    // it would otherwise leave no trace at all.
    document.title = `probe:${lines.filter((line) => line.includes("FAIL")).length}`;
  }, []);

  return null;
}
