import type { ReactNode } from "react";
import type { Pass } from "@/lib/weave/passes";

/**
 * A shed: the gap content sits in.
 *
 * On a loom, a shed is opened by lifting some warp threads and leaving others
 * down, and the shuttle passes through the gap. Here it is a span of columns in
 * the twelve-column grid — so a section does not sit "in a container", it sits in
 * a *measured gap* between specific threads, and the threads either side of it are
 * the ones that were lifted to let it through.
 *
 * Rendered as an inline `grid-column` rather than a set of Tailwind classes
 * because the span is data, not a design decision: it comes from the draft in
 * `lib/weave/passes.ts`, and a `col-span-8` in the markup would be a second copy
 * of it.
 */
export function Shed({
  from = 2,
  span = 10,
  className,
  children,
}: {
  from?: number;
  span?: number;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={className}
      style={{ gridColumn: `${from} / span ${span}` }}
    >
      {children}
    </div>
  );
}

/**
 * One pass of the weft: a section of the page.
 *
 * The heading is built from the draft rather than written at the call site, so the
 * pass number, the structural element and the anchor cannot drift out of step with
 * `PASSES` — and `data-pass` is what the shuttle measures, so a section that is not
 * a `Pass` is a section the weft does not know about.
 *
 * The heading is `h2` and the page's `h1` lives in the hero, so the outline is
 * `h1 → h2 → h3` throughout. `verify-a11y.mjs` fails on a skipped level.
 */
export function Pick({
  pass,
  title,
  lead,
  bodyFrom,
  bodySpan,
  children,
}: {
  pass: Pass;
  title: string;
  lead?: string;
  /** Override the shed the *body* sits in, to let one pass break the measure. */
  bodyFrom?: number;
  bodySpan?: number;
  children: ReactNode;
}) {
  return (
    <section
      id={pass.id}
      data-pass={pass.pick}
      // Clears the sticky header, or an in-page link lands with its own heading
      // hidden behind the bar.
      className="scroll-mt-24 py-16 sm:py-20"
    >
      <div className="warp-grid">
        <Shed from={pass.shed.from} span={pass.shed.span}>
          <header className="max-w-2xl">
            <p className="text-faint flex items-center gap-2.5 text-[11.5px] font-medium tracking-[0.16em] uppercase">
              <span className="knot-lit knot" />
              <span>
                Pass {String(pass.pick).padStart(2, "0")} · {pass.element}
              </span>
            </p>
            <h2 className="mt-3 text-[26px] leading-tight font-medium tracking-tight text-balance sm:text-[32px]">
              {title}
            </h2>
            {lead && <p className="text-soft mt-4 text-[15px] leading-6">{lead}</p>}
          </header>
        </Shed>

        {/* The body is a second grid item, not a child of the header, so a pass
            that wants the full measure can have it. Only one pass uses that (`Count`,
            whose whole point is how many threads there are), because a page where
            every section breaks its own measure has no measure at all. */}
        <Shed from={bodyFrom ?? pass.shed.from} span={bodySpan ?? pass.shed.span} className="mt-10">
          {children}
        </Shed>
      </div>
    </section>
  );
}

/**
 * One thread of the warp standing on its own: an icon knot at the top and a claim
 * hanging below it.
 *
 * Deliberately a *column* rather than a card. A card is a box with a border, which
 * says "this is a discrete object"; a thread is a line held under tension with
 * something tied to it, which says "this is part of a set that holds together".
 * That distinction is the whole reason this page is laid out the way it is, and
 * after a few of them the eye reads the group as a warp rather than as a grid.
 */
export function End({
  index,
  title,
  children,
}: {
  index: number;
  title: string;
  children: ReactNode;
}) {
  return (
    <article className="relative pl-5">
      {/* The thread itself, running the height of the claim. */}
      <span aria-hidden="true" className="rail absolute top-1 bottom-0 left-0" />
      <span
        aria-hidden="true"
        className="knot-lit knot absolute top-1.5 left-[-2px]"
      />
      <p className="text-faint font-mono text-[10.5px] tabular-nums">
        {String(index).padStart(2, "0")}
      </p>
      <h3 className="mt-1 text-[14.5px] font-medium">{title}</h3>
      <p className="text-soft mt-2 text-[13.5px] leading-[1.6]">{children}</p>
    </article>
  );
}

/**
 * An inline code chip, matching the app's `:not(pre) > code` treatment.
 *
 * The `.loom-markdown` rules that style that in the product are deliberately
 * outside the shared token regions — the site renders prose, not the app's
 * markdown pipeline — so these few classes stand in for them, which
 * `verify-tokens.mjs` is what enforces.
 */
export function Code({ children }: { children: ReactNode }) {
  return (
    <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
      {children}
    </code>
  );
}
