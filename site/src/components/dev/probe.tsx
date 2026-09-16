"use client";

import { useEffect } from "react";

/**
 * The layout probe.
 *
 * A development-only diagnostic that measures the rendered page and writes the
 * result into a `<pre id="loom-probe">`, so a headless browser can dump it as text.
 * `probe-layout.mjs` drives that, at several widths and both motion preferences.
 *
 * ---------------------------------------------------------------------------
 * Why this rather than a screenshot
 * ---------------------------------------------------------------------------
 *
 * Several of this site's defining ideas are *geometry*, not content: the warp
 * threads have to line up with the columns the content sits in, the weft's end-knots
 * have to sit on the outermost of those threads, and neither is visible in a build, a
 * typecheck, or a prerendered HTML dump. Both can also be wrong in a way that only
 * appears at a particular viewport width.
 *
 * Concretely, the weft's knots are placed with
 * `calc(max(0px, (100vw - 92rem) / 2) + <gutter>)`. If a CSS parser rejects that, the
 * declaration is invalid at computed-value time, `left` falls back to `auto`, and
 * both knots pile up at the left edge — roughly 1800px from the threads they are meant
 * to be crossing, on a wide monitor. No build step can tell you that. This can.
 *
 * A screenshot could too, but only if someone remembered to look at the right
 * viewport, and only by eye. This asserts numbers, which is the job `verify-*.mjs`
 * already does for the markup.
 *
 * Rendered only when `NODE_ENV === "development"`, so it is absent from the
 * production build entirely rather than merely inert inside it.
 */
export function LayoutProbe() {
  useEffect(() => {
    if (!new URLSearchParams(window.location.search).has("probe")) return;

    const lines: string[] = [];
    const say = (label: string, value: string) => lines.push(`${label}: ${value}`);
    const check = (label: string, ok: boolean) =>
      lines.push(`CHECK ${label}: ${ok ? "PASS" : "FAIL"}`);
    const round = (value: number) => Math.round(value * 100) / 100;

    const root = document.documentElement;
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    say("viewport", `${viewport.width}×${viewport.height}`);

    /**
     * The motion preference the browser is reporting.
     *
     * Reported because it changes what the rest of this probe means: under `reduce`
     * the scene renders its settled frame, so a check written against the animated
     * path would be testing nothing.
     */
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    say("prefers-reduced-motion", reduced ? "reduce" : "no-preference");

    /**
     * Resolves a CSS expression to pixels.
     *
     * `getComputedStyle(root).getPropertyValue("--warp-edge")` returns the
     * *substituted token stream* — literally `calc(max(0px, …) + …)` — not a number,
     * because custom properties are computed lazily. So the value is resolved by
     * handing it to a throwaway element and measuring where it lands. This is the
     * only honest way to find out what the browser actually did with it.
     */
    const resolve = (expression: string): number => {
      const probe = document.createElement("div");
      probe.style.cssText = `position:absolute;top:-9999px;left:${expression};width:0;height:0;`;
      document.body.appendChild(probe);
      const x = probe.getBoundingClientRect().left;
      probe.remove();
      return round(x);
    };

    // --- the frame --------------------------------------------------------
    const header = document.querySelector<HTMLElement>("header");
    const hero = document.querySelector<HTMLElement>("[data-pass]");
    say("header bottom", header ? `${round(header.getBoundingClientRect().bottom)}px` : "—");
    say("first pass top", hero ? `${round(hero.getBoundingClientRect().top)}px` : "—");

    // --- the warp ---------------------------------------------------------
    const threads = Array.from(document.querySelectorAll<HTMLElement>(".warp-line"));
    say("warp threads rendered", String(threads.length));

    const threadX = threads.map((thread) => round(thread.getBoundingClientRect().left));
    if (threadX.length > 1) {
      say("first thread x", String(threadX[0]));
      say("last thread x", String(threadX[threadX.length - 1]));

      // The columns must be evenly pitched, or the threads are not a grid and nothing
      // placed in them lands where it was told to.
      const pitches = threadX.slice(1).map((x, index) => x - threadX[index]);
      const spread = Math.max(...pitches) - Math.min(...pitches);
      say("column pitch", `${pitches[0].toFixed(2)} (spread ${spread.toFixed(2)})`);
      check("the warp is an even grid", spread < 1);
    }

    // --- the weft ---------------------------------------------------------
    const weft = document.querySelector<HTMLElement>(".weft");
    say("weft present", weft ? "yes" : "no");
    say("weft top", weft ? `${round(weft.getBoundingClientRect().top)}px` : "—");

    const declaredEdge = getComputedStyle(root).getPropertyValue("--warp-edge").trim();
    say("--warp-edge declared", declaredEdge.replace(/\s+/g, " ") || "(missing)");
    say("--weft-y resolved", `${round(resolve("var(--weft-y)"))}px`);

    if (declaredEdge && threadX.length > 0) {
      const edge = resolve("var(--warp-edge)");
      say("--warp-edge resolved", `${edge}px`);
      const drift = Math.abs(edge - threadX[0]);
      say("knot vs first thread", `${drift.toFixed(2)}px apart`);
      // A knot pinned at the gutter instead would be hundreds of pixels out on a
      // viewport wider than the cloth, and a few pixels out on a narrow one — so the
      // tolerance has to be tight enough to catch the small version too.
      check("the knots sit on the outer threads", drift < 1.5);
    }

    // The line has to have been positioned. Its initial value is `-2px`, and the
    // failure this catches is the shuttle never running at all — which is what happens
    // under a motion preference if the component bails out instead of just not
    // gliding.
    const weftY = resolve("var(--weft-y)");
    check("the shuttle positioned the weft", weftY > 0);

    // --- the scene --------------------------------------------------------
    const cloth = document.querySelector<HTMLElement>("[data-scene-phase]");
    if (cloth) {
      say("scene phase", cloth.dataset.scenePhase ?? "—");
      say("scene clock running", cloth.dataset.sceneRunning ?? "—");
      say("cloth passes rendered", String(document.querySelectorAll(".cloth-pass").length));
      // Under `reduce` the settled frame is correct and expected, so this only asserts
      // the animated path.
      if (!reduced) {
        check(
          "the clock advanced past the empty state",
          cloth.dataset.scenePhase !== "empty",
        );
      }
    }

    // --- the demonstration is reachable -----------------------------------
    const firstPanel = document.querySelector<HTMLElement>(".panel");
    if (firstPanel) {
      say(
        "first .panel",
        `${firstPanel.tagName.toLowerCase()}.${firstPanel.className.split(" ").slice(0, 2).join(".")}`,
      );
      say("  top", `${round(firstPanel.getBoundingClientRect().top)}px`);
      // The page's whole argument is that it shows the work rather than claiming it,
      // so the demonstration starting below the fold would undercut it.
      check(
        "the demonstration is visible without scrolling",
        firstPanel.getBoundingClientRect().top < viewport.height,
      );
    }

    // --- the app's glass --------------------------------------------------
    // Three declarations from the same `@utility panel` rule. Reading all three is
    // deliberate: headless Chrome with GPU compositing disabled can report
    // `backdrop-filter: none` while the rule is applying perfectly, and a single
    // reading would look like a broken build. If the background and border come back
    // from the token sheet, the rule is reaching the element.
    if (firstPanel) {
      const style = getComputedStyle(firstPanel);
      const blur = style.backdropFilter || style.getPropertyValue("backdrop-filter");
      say("  background", style.backgroundColor);
      say("  border", style.borderTopColor);
      say("  backdrop-filter", blur || "(none)");
      check(
        "the app's glass rule reaches the element",
        style.backgroundColor.startsWith("rgba"),
      );
      check("the glass blurs what is behind it", blur.includes("blur"));
    }

    // --- overflow ---------------------------------------------------------
    // One element wider than its container is enough to give the whole page a
    // horizontal scrollbar, and on a page built from twelve hairlines a stray 1px
    // border is the usual culprit.
    const overflow = root.scrollWidth - root.clientWidth;
    say("horizontal overflow", `${overflow}px`);
    check("there is no sideways scroll", overflow <= 1);

    // --- the strip --------------------------------------------------------
    const details = document.querySelector<HTMLElement>("header details");
    const inline = document.querySelector<HTMLElement>(".draft-strip");
    const visible = (element: HTMLElement | null) =>
      element ? element.getBoundingClientRect().width > 0 : false;
    say("small-screen menu visible", visible(details) ? "yes" : "no");
    say("inline strip visible", visible(inline) ? "yes" : "no");
    // Exactly one of the two, never both and never neither: both would duplicate
    // every destination for anyone moving through the page by link.
    check(
      "exactly one navigation is reachable",
      visible(details) !== visible(inline),
    );

    // --- what the page is made of ----------------------------------------
    say("passes on the page", String(document.querySelectorAll("[data-pass]").length));
    say("compact density leaked in", document.querySelector(".density-compact") ? "yes" : "no");
    // The app-only surface that must never reach this stylesheet.
    check("the quick-ask overlay stayed out", !document.querySelector(".ask-rail"));

    const pre = document.createElement("pre");
    pre.id = "loom-probe";
    pre.textContent = lines.join("\n");
    document.body.appendChild(pre);

    // Also mirrored into the title: `--dump-dom` fires on the load event, and an
    // effect that ran after it would otherwise leave no trace at all.
    document.title = `probe:${lines.filter((line) => line.includes("FAIL")).length}`;
  }, []);

  return null;
}
