import type { ReactNode } from "react";
import type { Entry } from "@/lib/document";

/**
 * A section of the manual.
 *
 * The furniture is deliberately thin: a rule across the measure, the section
 * number, the part name as a rubric, the heading, then the body. That is what a
 * printed specification does, and it is enough to make a long document navigable
 * by eye — where the previous version needed twelve hairlines, a floating weft
 * line and a glowing knot to achieve less.
 *
 * `data-section` is the anchor and the number, and `data-section-title` the
 * heading, because two other components read them off the DOM: the running head,
 * which names the current section on a narrow viewport, and the margin index,
 * which marks your position in the contents. Putting the title in an attribute
 * rather than reading the heading's `textContent` keeps the number out of it.
 *
 * The heading is `h2` and the document's `h1` is on the title page, so the outline
 * is `h1 → h2 → h3` throughout. `verify-a11y.mjs` fails on a skipped level.
 */
export function Section({ entry, children }: { entry: Entry; children: ReactNode }) {
  return (
    <section
      id={entry.id}
      data-section={entry.number}
      data-section-title={entry.title}
      className="section"
    >
      <div className="section-rule" />
      <header className="section-head">
        {/* The part name sits above the heading and quiet, so the eye lands on the
            plain words. A reader who does not know what a selvedge is never has to
            find out in order to read the heading. */}
        <p className="rubric">{entry.rubric}</p>
        <h2 className="t-section mt-3">
          <span className="sec-num num">{entry.number}</span>
          {entry.title}
        </h2>
      </header>
      <div className="section-body">{children}</div>
    </section>
  );
}

/** A subheading inside a section. Always `h3`, always below an `h2`. */
export function Sub({ children }: { children: ReactNode }) {
  return <h3 className="t-sub mt-10">{children}</h3>;
}

/**
 * The one control in the document.
 *
 * The app's own primary button, unchanged. Everything else on the page is a link
 * in a sentence, which is why this reads as the single thing to do rather than as
 * one of a row of options.
 */
export function Action({ children }: { children: ReactNode }) {
  return <div className="action">{children}</div>;
}
