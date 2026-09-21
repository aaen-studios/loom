import { Figure, Leader } from "@/components/doc/figure";

/**
 * Figure 2 — everywhere a tab can be dropped.
 *
 * ---------------------------------------------------------------------------
 * Why this is a figure at all
 * ---------------------------------------------------------------------------
 *
 * Dragging a tab to rearrange a window is the kind of interaction that prose
 * cannot describe: "drag a tab onto another tab strip to move it there, onto a
 * window edge to dock against that edge, or out of the window to give the panel
 * its own" is four lines of English that a drawing settles in one look. It is also
 * the part of the dock that nobody guesses at from a screenshot, which makes it the
 * single most valuable thing on this page to draw.
 *
 * The convention is the manual's: the thing being moved is drawn at the left,
 * arrows radiate to the three places it can go, and each destination carries a
 * numbered callout whose entry in the legend says what happens. The destination
 * outline is dashed because dashed means *where it would go* — the same rule the
 * first figure uses for a shut zone, so a reader who has read one plate can read
 * the next.
 *
 * The source tab is drawn filled with the accent, which is the app's own
 * treatment for an active tab, and that is the only fill in either figure. One
 * filled shape on a page of hairlines is a strong enough signal to carry the whole
 * interaction.
 */
export function FigureDropTargets() {
  return (
    <Figure
      n={2}
      note="The arrow is the drag and the dashed outline is where it would land."
      legend={[
        {
          n: 1,
          title: "Another tab strip.",
          body: "The panel moves into that zone, taking the position of whatever tab it is dropped on.",
        },
        {
          n: 2,
          title: "A window edge.",
          body: "The panel is docked against that edge — and a zone is created if the edge did not have one, which is how a first panel is placed.",
        },
        {
          n: 3,
          title: "Out of the window.",
          body: "The panel gets its own window, holding that panel and nothing else, with a Dock it back control to send it home.",
        },
      ]}
    >
      <svg
        className="figure-art"
        viewBox="0 0 720 340"
        role="img"
        aria-label="A drawing of a tab being dragged from one window. Three arrows lead to the places it can be dropped: another tab strip, the edge of a window, and outside the window into a window of its own."
      >
        <defs>
          <marker
            id="f2-arrow"
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="5"
            markerHeight="5"
            orient="auto"
          >
            <path d="M0 1.5 L9 5 L0 8.5" className="f-hair" strokeWidth={1.4} />
          </marker>
        </defs>

        {/* Where the tab is now. */}
        <rect x={24} y={44} width={220} height={190} rx={6} className="f-line" />
        <rect x={38} y={56} width={64} height={18} rx={4} className="f-open-fill" />
        <text x={70} y={68.5} textAnchor="middle" fontSize={9} className="f-ink">
          Terminal
        </text>
        <rect x={106} y={56} width={46} height={18} rx={4} className="f-hair" />
        <text x={129} y={68.5} textAnchor="middle" fontSize={9} className="f-faint">
          Runs
        </text>

        <line x1={38} y1={94} x2={210} y2={94} className="f-hair" />
        <line x1={38} y1={108} x2={170} y2={108} className="f-hair" />
        <line x1={38} y1={122} x2={190} y2={122} className="f-hair" />
        <text x={24} y={254} fontSize={10} className="f-faint">
          the tab being dragged
        </text>

        {/* The drags. */}
        <g className="f-open" strokeWidth={1.25} fill="none">
          <path
            d="M102 66 C 200 66 300 52 392 55"
            markerEnd="url(#f2-arrow)"
          />
          <path
            d="M102 74 C 200 74 300 156 392 158"
            markerEnd="url(#f2-arrow)"
          />
          <path
            d="M102 82 C 210 82 320 274 426 276"
            markerEnd="url(#f2-arrow)"
          />
        </g>

        {/* One: another tab strip. */}
        <rect
          x={396}
          y={44}
          width={224}
          height={22}
          rx={4}
          className="f-line f-dash"
        />
        <rect
          x={450}
          y={49}
          width={80}
          height={12}
          rx={3}
          className="f-open-fill f-dash"
        />
        <text x={396} y={86} fontSize={10} className="f-faint">
          onto another tab strip
        </text>

        {/* Two: a window edge. */}
        <rect x={396} y={116} width={224} height={84} rx={6} className="f-hair" />
        <line x1={396} y1={120} x2={396} y2={196} className="f-open" strokeWidth={3.5} />
        <text x={396} y={220} fontSize={10} className="f-faint">
          against a window edge
        </text>

        {/* Three: out of the window. */}
        <rect x={430} y={250} width={190} height={52} rx={6} className="f-line" />
        <text x={396} y={322} fontSize={10} className="f-faint">
          out of the window, into its own
        </text>

        <Leader n={1} from={[622, 55]} to={[668, 55]} />
        <Leader n={2} from={[622, 158]} to={[668, 158]} />
        <Leader n={3} from={[622, 276]} to={[668, 276]} />
      </svg>
    </Figure>
  );
}
