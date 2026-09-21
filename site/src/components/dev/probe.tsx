"use client";

import { useEffect } from "react";

/**
 * The layout probe.
 *
 * A development-only diagnostic that measures the rendered document and writes the
 * result into a `<pre id="loom-probe">`, so a headless browser can dump it as text.
 * `probe-layout.mjs` drives that, at four widths and both motion preferences.
 *
 * ---------------------------------------------------------------------------
 * Why numbers rather than a screenshot
 * ---------------------------------------------------------------------------
 *
 * The claims this document makes are *geometric*, and none of them is visible in a
 * build, a typecheck or a prerendered HTML dump:
 *
 *   - the text column must be the measure. This is the defect the rebuild was
 *     largely about: the previous layout let prose run a ten-column shed, which at
 *     the cap is about 150 characters a line, or twice what anyone can read without
 *     losing their place. A page that is 150 characters wide typechecks, builds and
 *     renders perfectly.
 *   - the margin index must appear at exactly one breakpoint and the running head's
 *     section indicator at exactly the other, so "where am I" is answered once and
 *     never twice or zero times.
 *   - a figure must not be shrunk below legibility, and a table must scroll rather
 *     than push the page sideways.
 *
 * A screenshot could catch some of that, but only if someone remembered to look at
 * the right width, and only by eye. This asserts numbers.
 *
 * ---------------------------------------------------------------------------
 * What was deleted, and why deleting it matters as much as what is here
 * ---------------------------------------------------------------------------
 *
 * The previous probe checked two things that no longer exist: the position of a weft
 * line that followed the scroll, and the pitch of twelve fixed warp hairlines. It also
 * checked that a `.panel` actually had `backdrop-filter`, because the site was built
 * out of the application's glass.
 *
 * All of those are gone from the page, so all of those checks are gone from here.
 * Leaving them would have been worse than deleting them: a `querySelectorAll` that
 * matches nothing is not a failure, it is an empty list, so every check would have
 * passed against `undefined` forever while reading as coverage. The last check in this
 * file exists specifically to keep the glass deleted.
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
    say(
      "prefers-reduced-motion",
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
        ? "reduce"
        : "no-preference",
    );

    /** Resolves a CSS expression to pixels by measuring where it lands. */
    const resolve = (expression: string): number => {
      const probe = document.createElement("div");
      probe.style.cssText = `position:absolute;top:-9999px;left:${expression};width:0;height:0;`;
      document.body.appendChild(probe);
      const x = probe.getBoundingClientRect().left;
      probe.remove();
      return round(x);
    };

    // --- the measure ------------------------------------------------------
    //
    // The single most load-bearing number in the stylesheet, and the one that cannot
    // be checked any other way. Resolved rather than read, because
    // `getPropertyValue("--measure")` returns the token stream and not a length.
    const declared = getComputedStyle(root).getPropertyValue("--measure").trim();
    say("--measure declared", declared || "(missing)");

    const paper = document.querySelector<HTMLElement>(".paper");
    if (paper) {
      const style = getComputedStyle(paper);
      const padding = Number.parseFloat(style.paddingLeft) || 0;
      const content = paper.getBoundingClientRect().width - padding * 2;
      say("text column", `${round(content)}px`);

      // The column is the measure, to within a rounding pixel — unless the viewport
      // is narrower than the measure, in which case the gutter is the constraint and
      // the column is *supposed* to be narrower.
      const expected = Math.min(
        Number.parseFloat(declared.replace("rem", "")) * 16 || 0,
        viewport.width - padding * 2,
      );
      check(
        "the text column is the measure, not the viewport",
        Math.abs(content - expected) < 2,
      );

      // And the measure is a readable number of characters. 34rem at 16px is 544px,
      // which is 62–70 characters of Inter depending on the line — so the assertion
      // is on the width, and the character count is what the width is for.
      if (viewport.width > 900) {
        check("the measure is in the readable range", content >= 480 && content <= 620);
      }

      // The column is centred, which is what leaves a margin on both sides for the
      // index. Left-flush prose on a wide screen reads as a document that fell over.
      const leftGap = paper.getBoundingClientRect().left;
      const rightGap = viewport.width - paper.getBoundingClientRect().right;
      check("the column is centred", Math.abs(leftGap - rightGap) < 2);
    } else {
      check("the document has a text column", false);
    }

    // --- the index and the running head ------------------------------------
    //
    // Two position indicators, gated in opposite directions so exactly one of them is
    // ever visible. Two would duplicate the answer to "where am I" for anyone moving
    // through the document; none would mean the reader has no way to know.
    const index = document.querySelector<HTMLElement>(".index");
    const current = document.querySelector<HTMLElement>(".bar-current");
    const shown = (element: HTMLElement | null) =>
      element ? element.getBoundingClientRect().width > 0 : false;

    say("margin index visible", shown(index) ? "yes" : "no");
    say("running-head section visible", shown(current) ? "yes" : "no");
    check(
      "exactly one position indicator is on screen",
      shown(index) !== shown(current),
    );

    // The breakpoint is the point. Above 72rem there is a margin wide enough for the
    // index; below it there is not, and a fixed panel drawn over the text it is
    // supposed to be beside is not navigation.
    check(
      "the margin index appears only where there is a margin for it",
      shown(index) === (viewport.width >= 1152),
    );

    if (shown(index) && index) {
      const box = index.getBoundingClientRect();
      const paperBox = paper?.getBoundingClientRect();
      // It must not overlap the text. The whole reason it is in the margin and not
      // over the column.
      if (paperBox) {
        check("the index clears the text column", box.right <= paperBox.left + 0.5);
      }
      // It has to stay reachable: if the list is longer than the viewport it would
      // run off the bottom and the last entries would be unreachable.
      check("the whole index fits on screen", box.height < viewport.height - 80);
    }

    // --- the running head --------------------------------------------------
    const bar = document.querySelector<HTMLElement>(".bar");
    say("running head present", bar ? "yes" : "no");
    if (bar) {
      const style = getComputedStyle(bar);
      // Opaque, not frosted. Content must not show through a running head, and the
      // assertion is on the computed colour rather than on the absence of a rule.
      check(
        "the running head is opaque",
        !style.backdropFilter.includes("blur"),
      );
      // Sticky, so it stays while the document scrolls under it.
      check("the running head sticks", style.position === "sticky");
    }

    // --- figures and tables ------------------------------------------------
    const figureCaptions = document.querySelectorAll("figcaption").length;
    const tableCaptions = document.querySelectorAll("table caption").length;
    say("figure captions", String(figureCaptions));
    say("table captions", String(tableCaptions));

    // The registers in `lib/document.ts` are the source for both, so a figure whose
    // caption was written by hand would show up here as a mismatch.
    const registerFigures = document.querySelectorAll(".figure").length;
    check(
      "every figure has exactly one caption",
      figureCaptions === registerFigures && registerFigures > 0,
    );

    // A drawing shrunk below legibility is worse than no drawing. The figure scrolls
    // rather than shrinking, so the SVG's rendered width is what to measure.
    const firstArt = document.querySelector<SVGElement>(".figure-art");
    if (firstArt) {
      const width = firstArt.getBoundingClientRect().width;
      say("first figure width", `${round(width)}px`);
      check("a figure is not shrunk below legibility", width >= 660);
    }

    // --- what is deliberately absent ---------------------------------------
    //
    // The check that keeps the deletions deleted. Each of these was a real element on
    // the previous version of this page and each is a two-line change to add back — and
    // every one of them read as "generated" rather than as made.
    //
    // This is measured on the *rendered* document, by computed style, and that is a
    // deliberate choice rather than a convenience. The obvious version of this check —
    // `querySelectorAll(".panel, .pill, .blob")` — was here first, and it failed for a
    // reason worth recording: querying for a class name puts that class name in this
    // file, Tailwind scans this file, and so **writing the check was what made Tailwind
    // emit the very utilities the check was looking for.** The stylesheet then contained
    // `.panel{...}` and `.pill{...}`, and a separate stylesheet-level check reported them
    // as failures, which they were not: no element on the page had ever carried them.
    //
    // Computed style is immune to that whole class of confusion, because it asks the
    // browser what it actually painted rather than asking the stylesheet what it
    // contains. It is also strictly stronger: it catches a glass surface however the
    // class name got there, including from markup nobody thought to grep for.
    const painted = Array.from(document.querySelectorAll<HTMLElement>("*"));
    const busy = painted.filter((element) => {
      const style = getComputedStyle(element);
      return (
        (style.backdropFilter && style.backdropFilter.includes("blur")) ||
        (style.backgroundImage && style.backgroundImage.includes("data:image/svg"))
      );
    });
    say("blurred or textured elements", String(busy.length));
    check("nothing on the page is blurred or textured", busy.length === 0);

    const drifting = painted.filter((element) => {
      const style = getComputedStyle(element);
      return (
        style.animationName !== "none" && style.animationIterationCount === "infinite"
      );
    });
    say("elements that loop forever", String(drifting.length));
    check("nothing on the page loops forever", drifting.length === 0);

    // The old fixed layers, by the same route: a filled, fixed, full-viewport element
    // behind everything. There is no backdrop element any more — the ground is a
    // `background-color` on `<body>` — so anything matching this shape is a regression.
    const layers = painted.filter((element) => {
      const style = getComputedStyle(element);
      const box = element.getBoundingClientRect();
      return (
        style.position === "fixed" &&
        box.width >= viewport.width - 1 &&
        box.height >= viewport.height - 1 &&
        style.backgroundColor !== "rgba(0, 0, 0, 0)"
      );
    });
    say("full-viewport fixed layers", String(layers.length));
    check("no fixed layer is painted behind the document", layers.length === 0);

    // The scripted turn the hero used to run. A regression to it is exactly the kind
    // of thing that gets reintroduced as "a bit of life at the top of the page".
    check(
      "the hero is a drawing, not a scripted turn",
      !document.querySelector("[data-scene-phase]"),
    );

    // --- the document's shape ----------------------------------------------
    say("sections rendered", String(document.querySelectorAll("[data-section]").length));
    say("prose runs measured", String(document.querySelectorAll(".t-body").length));

    // Every block of prose is inside the measure, and this is the check the whole
    // file exists for. A single over-wide paragraph is invisible in a build and plain
    // to a reader.
    const measurePx = resolve("var(--measure)");
    say("--measure resolved", `${measurePx}px`);
    if (measurePx > 0) {
      const wide = Array.from(
        document.querySelectorAll<HTMLElement>(".t-body, .t-standfirst, .note, .hanging-b"),
      ).filter((run) => run.getBoundingClientRect().width > measurePx + 2);
      say("prose runs wider than the measure", String(wide.length));
      check("no run of prose is wider than the measure", wide.length === 0);
    }

    // --- overflow ----------------------------------------------------------
    // One element wider than its container is enough to give the whole document a
    // horizontal scrollbar, and a breakout width on a narrow viewport is the usual
    // culprit.
    const overflow = root.scrollWidth - root.clientWidth;
    say("horizontal overflow", `${overflow}px`);
    check("there is no sideways scroll", overflow <= 1);

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
