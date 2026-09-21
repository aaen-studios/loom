import { Disc, Figure } from "@/components/doc/figure";

/**
 * Figure 3 — one turn, in the order the parts arrive.
 *
 * ---------------------------------------------------------------------------
 * What the shape of the drawing is saying
 * ---------------------------------------------------------------------------
 *
 * Four lanes, one per participant: you, the model's reasoning, its tools, and its
 * reply. Time runs left to right along a single axis. Everything the figure has to
 * say is in the vertical alignment — the reasoning sits *above* the tool calls it
 * produced, the tools sit above the reply, and the reply starts only after the last
 * tool has finished.
 *
 * Which is the claim section 3 makes in prose: the parts arrive in the order they
 * happened, not reordered into something tidier. A transcript that puts its
 * reasoning above the answer as a summary has reordered them, and this drawing is
 * what that reordering would look like if it were honest.
 *
 * The tool lane is deliberately four *separate* short bars rather than one long
 * one, because that is what a run of tool calls is: discrete, individually
 * observable, each one a thing you can read the result of. Collapsing them into a
 * single block would be the summary the figure exists to argue against.
 *
 * ---------------------------------------------------------------------------
 * The one dishonesty, stated in the caption
 * ---------------------------------------------------------------------------
 *
 * Positions along a lane are ordinal, not measured: the bars are drawn at lengths
 * that read well, not at the durations the real turn took. The *order* is exact.
 * That distinction is worth a clause in the caption, because a drawing that looks
 * like a timeline and is not one is the kind of figure that quietly misleads —
 * and saying so costs eight words.
 */
export function FigureTurn() {
  return (
    <Figure
      n={3}
      note="Drawn to scale in order and not in duration: the sequence is exact, the bar lengths are illustrative."
      legend={[
        {
          n: 1,
          title: "You.",
          body: "One message. Everything to the right of it is the turn that message started.",
        },
        {
          n: 2,
          title: "Thinking.",
          body: "The model's reasoning, as its own collapsed panel, in the position it actually occurred. A turn with three spells of thinking gets three panels, each independently openable.",
        },
        {
          n: 3,
          title: "Tools.",
          body: "Read, grep, edit, run — each one a separate call, landing between the reasoning that produced it and the reply that follows.",
        },
        {
          n: 4,
          title: "Loom.",
          body: "The answer, streamed. The usage line appears under it once the turn has stopped, because a running turn has no final count to report.",
        },
      ]}
    >
      <svg
        className="figure-art"
        viewBox="0 0 720 320"
        role="img"
        aria-label="A timeline of one turn. You speak, then the model reasons, then it makes four separate tool calls in sequence, and only after the last one does the reply begin and stream to the end of the turn."
      >
        <defs>
          <marker
            id="f3-axis"
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

        {/* The origin: the instant the message was sent. Everything to the right
            of it is derived from it. */}
        <line x1={96} y1={64} x2={96} y2={266} className="f-hair f-dash" />

        {/* Lane names. */}
        <text x={66} y={88} textAnchor="end" fontSize={11} className="f-ink">
          You
        </text>
        <text x={66} y={136} textAnchor="end" fontSize={11} className="f-ink">
          Thinking
        </text>
        <text x={66} y={184} textAnchor="end" fontSize={11} className="f-ink">
          Tools
        </text>
        <text x={66} y={232} textAnchor="end" fontSize={11} className="f-ink">
          Loom
        </text>

        {/* You: one message. */}
        <rect x={96} y={75} width={16} height={18} className="f-thread-solid" />

        {/* Thinking: one spell, one panel. */}
        <rect x={126} y={123} width={124} height={18} className="f-thread" />

        {/* Tools: four calls, discrete. */}
        <rect x={262} y={171} width={12} height={18} className="f-thread-solid" />
        <rect x={276} y={171} width={10} height={18} className="f-thread-solid" />
        <rect x={288} y={171} width={18} height={18} className="f-thread-solid" />
        <rect x={308} y={171} width={46} height={18} className="f-thread-solid" />

        {/* The reply: the longest bar, because it is the longest thing in a turn
            that a reader waits for. */}
        <rect x={366} y={219} width={274} height={18} className="f-thread" />

        {/* The axis. */}
        <line
          x1={96}
          y1={272}
          x2={646}
          y2={272}
          className="f-hair"
          markerEnd="url(#f3-axis)"
        />
        <line x1={96} y1={265} x2={96} y2={279} className="f-hair" />
        <text x={654} y={276} fontSize={9} letterSpacing="0.08em" className="f-faint">
          TIME
        </text>

        {/* The callouts, hanging in the margin the lane names leave spare. */}
        <Disc n={1} x={78} y={84} />
        <Disc n={2} x={78} y={132} />
        <Disc n={3} x={78} y={180} />
        <Disc n={4} x={78} y={228} />
      </svg>
    </Figure>
  );
}
