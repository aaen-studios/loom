import type { ReactNode } from "react";

/**
 * The page's primitives.
 *
 * Small on purpose. A page needs a movement, a paragraph, a note, a specification block, a table, a
 * statement and a card — and having exactly those, used everywhere, is what keeps six movements looking
 * like one document rather than six people's work.
 *
 * All server components. None of them needs state, and adding `"use client"` here would be worse than
 * pointless — it would make every primitive a client boundary, so a page that is currently rendered
 * entirely on the server would ship its whole component tree to the browser to be re-created. The two
 * interactive things on the page are the hero's weave and `Reveal`, and both are their own client
 * components imported where they are needed.
 */

import { movementById } from "@/lib/movements";

/**
 * A movement.
 *
 * ---------------------------------------------------------------------------
 * Why a movement and not a section
 * ---------------------------------------------------------------------------
 *
 * The page does not explain the product feature by feature. It states six things, each with a
 * generative figure and a short argument, and the figure is doing the persuading rather than
 * illustrating a claim.
 *
 * So the furniture is deliberately minimal: an index, a word, a heading, and a rule. There is no
 * card, no border, no shadow, and no sidebar of contents — because the visual weight on this page
 * belongs to the figures, and every piece of chrome around them is a piece of chrome competing with
 * them.
 *
 * The number and the word come from `lib/movements.ts` rather than being written at the call site.
 * The first version of this page wrote the furniture out by hand in three files — eight copies of
 * `<span>01</span><span>Label</span>` — which meant the numbers were a fact stated eight times in
 * three places, none of which knew about the others. Inserting a movement between two of them would
 * have renumbered nothing and the page would have gone on claiming an order it no longer had, with
 * nothing to notice: a mono `03` beside the wrong heading looks exactly like a correct one.
 */
export function Movement({
  id,
  heading,
  lead,
  children,
}: {
  /** Must match an id in `lib/movements.ts`. */
  id: string;
  heading: string;
  lead?: string;
  children: ReactNode;
}) {
  const movement = movementById(id);

  return (
    <section id={movement.id} className="movement">
      <div className="shell">
        <header className="movement-head">
          <p className="movement-index">
            {/* Decorative: the heading below says everything the number says, in words. */}
            <span className="t-index" aria-hidden="true">
              {String(movement.n).padStart(2, "0")}
            </span>
            <span className="t-label">{movement.label}</span>
          </p>
          <h2 className="t-h2 measure">{heading}</h2>
          {lead && <p className="t-lede measure">{lead}</p>}
        </header>
        <div className="mt-12">{children}</div>
      </div>
    </section>
  );
}

/** A paragraph, capped at the measure. */
export function P({ children }: { children: ReactNode }) {
  return <p className="t-body measure mt-5">{children}</p>;
}

/** A closing remark, set apart by a hairline rather than by a box. */
export function Note({ children }: { children: ReactNode }) {
  return (
    <p className="t-small measure mt-7 border-t border-[var(--glass-border)] pt-5">{children}</p>
  );
}

/** Inline code, for paths and commands only. The restraint is the point. */
export function Code({ children }: { children: ReactNode }) {
  return <code className="code">{children}</code>;
}

/**
 * A pull quote: the one place a movement stops and says a single thing.
 *
 * Set in the display face and tracked tight, on the same measure as the prose so it reads as part of the
 * argument rather than as an interruption of it.
 */
export function Statement({ children }: { children: ReactNode }) {
  return <p className="t-statement measure mt-10">{children}</p>;
}

/** A shell session the reader is meant to run. */
export function Shell({ children }: { children: ReactNode }) {
  return <pre className="shell mt-6">{children}</pre>;
}

/**
 * Specification lines: a term, its value, a hairline between each.
 *
 * The `div` wrapper is what HTML permits inside a `dl` to keep a `dt` and its `dd` together when they
 * are not adjacent — which matters here because the rows are grid items, and a bare pair would be
 * laid out as two separate cells instead of as one.
 */
export function Spec({ rows }: { rows: readonly { term: string; def: ReactNode }[] }) {
  return (
    <dl className="spec mt-9 max-w-[var(--measure)]">
      {rows.map((row) => (
        <div className="spec-row" key={row.term}>
          <dt className="spec-term">{row.term}</dt>
          <dd className="spec-def">{row.def}</dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * Numbers, in a row.
 *
 * The only place the page counts anything, and the numbers are the ones the repository actually
 * produces. A row of figures set large in the accent with a mono label under each — because a stat
 * row is the one typographic device that earns its size.
 */
export function Figures({
  items,
}: {
  items: readonly { value: string; label: string }[];
}) {
  return (
    <dl className="mt-10 grid grid-cols-2 gap-x-8 gap-y-8 sm:grid-cols-4">
      {items.map((item) => (
        <div key={item.label}>
          <dt className="t-figure num">{item.value}</dt>
          <dd className="t-label mt-2">{item.label}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A card: one panel, one mode. The app's own surface, at a card's size. */
export function Card({ children }: { children: ReactNode }) {
  return <div className="card">{children}</div>;
}

/**
 * A table in the booktabs style: a rule above, one under the header, one below, and nothing vertical.
 *
 * Removing the vertical rules and giving the rows room is most of what separates a printed table from
 * a spreadsheet pasted into a page, and the middle rule is what lets the eye find the header again
 * after a long row without a shaded band to do it with.
 */
export function Table({
  caption,
  head,
  children,
}: {
  caption: string;
  head: readonly string[];
  children: ReactNode;
}) {
  return (
    <div className="table-scroll mt-9">
      <table className="booktabs">
        <caption>{caption}</caption>
        <thead>
          <tr>
            {head.map((label) => (
              <th key={label} scope="col">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}
