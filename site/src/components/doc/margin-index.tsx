"use client";

import { CONTENTS } from "@/lib/document";
import { useCurrentSection } from "./use-current-section";

/**
 * The contents list, fixed in the left margin.
 *
 * This is the only fixed element on the page, and it earns its place the way a
 * printed running index does: it answers "where am I, and what else is here"
 * without a scroll back to the top. A 34rem column inside a 1440px viewport leaves
 * a wide margin, and a margin with the contents in it is worth more than a centred
 * column with nothing beside it.
 *
 * ---------------------------------------------------------------------------
 * What this replaced
 * ---------------------------------------------------------------------------
 *
 * A "spine": one thread down the left margin with a knot per pick of a seven-pass
 * weaving draft, lit by `aria-current`. It was the same element in the same place
 * carrying the same information, and it was worse for one reason — the knots were a
 * notation, not a navigation. You could not read the spine; you had to know what a
 * pick was, and there were seven of them with no words.
 *
 * The contents list is the same device with the words put back. It is longer, it
 * is not decorative, and it is legible to someone who has never heard of a loom.
 *
 * The position marker is `aria-current` and nothing else — no second state, no
 * shadow copy of "where am I" in the markup. It is also `aria-hidden="false"` by
 * omission and announced as the current page link by a screen reader, which is
 * exactly what it is.
 *
 * `display: none` below 72rem, which is where the arithmetic in `globals.css`
 * first leaves a positive margin for it — 34rem of text plus a 14rem index and two
 * gutters. Below that the running head carries the section instead.
 */
export function MarginIndex() {
  const current = useCurrentSection();

  return (
    <nav className="index" aria-label="Contents">
      <p className="index-head label">Contents</p>
      <ol className="index-list">
        {CONTENTS.map((entry) => (
          <li key={entry.id}>
            <a
              href={`#${entry.id}`}
              className="index-link"
              aria-current={current?.number === entry.number ? "true" : undefined}
            >
              <span className="index-num">{entry.number}</span>
              <span>{entry.title}</span>
            </a>
          </li>
        ))}
      </ol>
    </nav>
  );
}
