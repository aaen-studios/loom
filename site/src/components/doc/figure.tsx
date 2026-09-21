import type { ReactNode } from "react";
import { figureTitle } from "@/lib/document";

/**
 * A drawn plate, with its caption and legend.
 *
 * ---------------------------------------------------------------------------
 * What this replaced
 * ---------------------------------------------------------------------------
 *
 * The previous version's hero played a scripted turn: a prompt typing itself into
 * a composer that looked pressable and was not, a task list ticking over, a
 * fabricated token count underneath, on a twenty-second loop. It was built
 * carefully — the beat sheet was a pure module with its own test suite — and it was
 * still the most recognisable landing-page pattern in software. A visitor has seen
 * it a hundred times and has learned to skip it, which is the opposite of what the
 * top of a page is for.
 *
 * Then it was replaced by a diagram built out of dashed `div`s, which was honest
 * and not good. This is the third answer, and it is the one that works: a *drawn*
 * figure. SVG, on a fixed grid, with leader lines and numbered callouts and a
 * legend — the convention a technical manual has used for a hundred and fifty
 * years, and for good reasons.
 *
 *   - **Hairlines stay hairlines.** At any scale, on any display. A border on a
 *     `div` is a fraction of a pixel at one width and a slab at another.
 *   - **A leader line cannot drift.** `(160, 204) → (108, 204)` is the same
 *     relationship at every viewport, where a callout positioned with a margin is
 *     only correct at the width it was written for.
 *   - **It is finished before you arrive.** Nothing moves. A reader can study a
 *     figure; nobody has ever studied an animation.
 *
 * The drawing is wider than a phone, so the figure scrolls rather than shrinking
 * its labels into illegibility — which is what a folded plate in a paper manual
 * amounts to, and is more honest than a thumbnail.
 *
 * ---------------------------------------------------------------------------
 * The caption is read from the register
 * ---------------------------------------------------------------------------
 *
 * `figureTitle(n)` rather than a `title` prop, so a caption cannot disagree with
 * the contents page. `verify-manual.mjs` asserts the rendered `Figure N` captions
 * match the register exactly, which is a check that only means something if there
 * is one source for both.
 */
export function Figure({
  n,
  note,
  legend,
  children,
}: {
  /** The figure's number, from the register in `lib/document.ts`. */
  n: number;
  /** A sentence under the caption: what to look at, or what it is not to scale of. */
  note?: string;
  /** Numbered callouts, matching the discs drawn in the figure. */
  legend?: readonly { n: number; title: string; body: string }[];
  children: ReactNode;
}) {
  return (
    <figure className="figure wide">
      <div className="figure-scroll">{children}</div>

      <figcaption className="figure-caption">
        <b>Figure {n}</b> — {figureTitle(n)}.{note ? ` ${note}` : ""}
      </figcaption>

      {legend && (
        <ol className="figure-legend">
          {legend.map((entry) => (
            <li key={entry.n}>
              <span className="legend-n num">
                {String(entry.n).padStart(2, "0")}
              </span>
              <span>
                <b>{entry.title}</b> {entry.body}
              </span>
            </li>
          ))}
        </ol>
      )}
    </figure>
  );
}

/**
 * A numbered disc, and the leader line that reaches it from the thing it numbers.
 *
 * Exported because the figures are split across three files and this is the one
 * piece of drawing language they share — which is the point: a reader learns the
 * convention from the first figure and it holds in the third.
 */
export function Disc({ n, x, y }: { n: number; x: number; y: number }) {
  return (
    <g>
      {/* `fill` as a style rather than a class: `.f-line` sets `fill: none` and
          the winner would depend on declaration order in the stylesheet, which is
          exactly the kind of coupling that breaks a year later. */}
      <circle
        cx={x}
        cy={y}
        r={9.5}
        strokeWidth={0.8}
        className="f-line"
        style={{ fill: "var(--page)" }}
      />
      <text
        x={x}
        y={y + 3.6}
        textAnchor="middle"
        fontSize={10}
        fontWeight={600}
        className="f-accent-ink"
      >
        {n}
      </text>
    </g>
  );
}

/** A leader line from a point on the drawing out to its numbered disc. */
export function Leader({
  n,
  from,
  to,
}: {
  n: number;
  from: readonly [number, number];
  to: readonly [number, number];
}) {
  return (
    <g>
      <line
        x1={from[0]}
        y1={from[1]}
        x2={to[0]}
        y2={to[1]}
        strokeWidth={0.8}
        className="f-hair"
      />
      <Disc n={n} x={to[0]} y={to[1]} />
    </g>
  );
}
