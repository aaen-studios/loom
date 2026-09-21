import { Figure, Leader } from "@/components/doc/figure";

/**
 * Figure 1 — the window, with every zone shut.
 *
 * ---------------------------------------------------------------------------
 * Why this is drawn rather than screenshotted
 * ---------------------------------------------------------------------------
 *
 * A screenshot of a window is a picture of a window: it invites you to look *at*
 * it, it ages with every redesign, and at the size a web page can afford it is
 * illegible. A *drawing* of the same structure can be labelled, can show the
 * closed state that a screenshot would not (the app ships with every zone shut,
 * so an honest screenshot would be a chat box and four empty margins), and can
 * carry leader lines to a legend. That is what a technical plate is for and it is
 * why manuals have used them for a century and a half.
 *
 * It is also more honest about what it is. The caption says "a drawing, not a
 * screenshot" in as many words, so a reader never has to wonder whether the
 * proportions are real. They are not, and saying so costs one clause.
 *
 * ---------------------------------------------------------------------------
 * The drawing itself
 * ---------------------------------------------------------------------------
 *
 * `viewBox="0 0 720 450"` on a 720-unit grid, so every coordinate is a readable
 * number rather than a fraction: the window runs from 160 to 560, the title bar's
 * rule sits at y=80, and the four zones tile the interior with no gaps — which is
 * what a docking layout *is*, and the reason the drawing reads as a real
 * arrangement rather than as a diagram of one.
 *
 * Zones that are shut are dashed and unfilled; the centre is solid and filled with
 * the app's own thread colour at 28%. That distinction is the figure's whole
 * content: it is drawn to the state the application actually opens in.
 *
 * The measure line below the window is the one dimensional annotation, and it
 * marks the only boundary in the layout that a reader can move.
 */
export function FigureWindow() {
  return (
    <Figure
      n={1}
      note="A drawing, not a screenshot — the proportions are right and the pixels are not."
      legend={[
        {
          n: 1,
          title: "The title bar.",
          body: "Settings, then the chats list and a new chat on the left; the Panels menu and the window controls on the right.",
        },
        {
          n: 2,
          title: "The left zone, shut.",
          body: "Git above Chats: the left edge is where a project's state belongs.",
        },
        {
          n: 3,
          title: "The right zone, shut.",
          body: "The editor above the files tree.",
        },
        {
          n: 4,
          title: "The bottom zone, shut.",
          body: "The terminal, with Runs beside it.",
        },
      ]}
    >
      <svg
        className="figure-art"
        viewBox="0 0 720 450"
        role="img"
        aria-label="A drawing of the Loom window. The title bar runs across the top. The window below it is divided into four zones — left, centre, right and bottom — and all four are shut except the centre, which holds the conversation and its composer."
      >
        <defs>
          <marker
            id="f1-arrow"
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="5"
            markerHeight="5"
            orient="auto-start-reverse"
          >
            <path d="M0 1.5 L9 5 L0 8.5" className="f-hair" strokeWidth={1.5} />
          </marker>
        </defs>

        {/* The window. */}
        <rect x={160} y={44} width={400} height={344} rx={8} className="f-line" />
        <line x1={160} y1={80} x2={560} y2={80} className="f-hair" />

        {/* The title bar's pills. */}
        <rect x={170} y={56} width={58} height={16} rx={4} className="f-hair" />
        <text x={199} y={67.5} textAnchor="middle" fontSize={10} className="f-faint">
          Settings
        </text>

        <rect x={234} y={56} width={44} height={16} rx={4} className="f-hair" />
        <text x={256} y={67.5} textAnchor="middle" fontSize={10} className="f-faint">
          Chats
        </text>

        <rect x={284} y={56} width={36} height={16} rx={4} className="f-hair" />
        <text x={302} y={67.5} textAnchor="middle" fontSize={10} className="f-faint">
          New
        </text>

        <rect x={456} y={56} width={50} height={16} rx={4} className="f-line" />
        <text x={481} y={67.5} textAnchor="middle" fontSize={10} className="f-ink">
          Panels
        </text>

        {/* The window controls, drawn as glyphs rather than as letters — the
            figure is of a frameless window, so there are no letters to draw. */}
        <line x1={518} y1={68} x2={526} y2={68} className="f-hair" />
        <rect x={532} y={64} width={9} height={9} rx={1.5} className="f-hair" />
        <line x1={546} y1={64} x2={555} y2={73} className="f-hair" />
        <line x1={555} y1={64} x2={546} y2={73} className="f-hair" />

        {/* The four zones. Left, right and bottom are dashed and unfilled: shut,
            which is how the application opens. */}
        <rect
          x={160}
          y={80}
          width={100}
          height={244}
          rx={5}
          className="f-hair f-dash"
        />
        <rect
          x={460}
          y={80}
          width={100}
          height={244}
          rx={5}
          className="f-hair f-dash"
        />
        <rect
          x={160}
          y={324}
          width={400}
          height={64}
          rx={5}
          className="f-hair f-dash"
        />

        {/* The centre: the conversation, and the only zone with anything in it. */}
        <rect x={260} y={80} width={200} height={244} rx={5} className="f-thread" />
        <rect x={260} y={80} width={200} height={244} rx={5} className="f-line" />

        <rect x={272} y={90} width={36} height={15} rx={3} className="f-hair" />
        <text x={290} y={101} textAnchor="middle" fontSize={9} className="f-faint">
          Chat
        </text>

        {/* Three hairlines standing in for the transcript. Not text: filling the
            drawing with lorem would turn a plate back into a mock-up. */}
        <line x1={272} y1={124} x2={444} y2={124} className="f-hair" />
        <line x1={272} y1={140} x2={404} y2={140} className="f-hair" />
        <line x1={272} y1={156} x2={424} y2={156} className="f-hair" />

        <rect x={272} y={288} width={176} height={26} rx={5} className="f-line" />
        <text x={281} y={305} fontSize={9} className="f-faint">
          Composer
        </text>

        {/* The tab chips in the shut zones: real panel names, in the order the
            app's default layout stacks them. */}
        <rect x={170} y={90} width={26} height={15} rx={3} className="f-hair" />
        <text x={183} y={101} textAnchor="middle" fontSize={8} className="f-faint">
          Git
        </text>
        <rect x={200} y={90} width={38} height={15} rx={3} className="f-hair" />
        <text x={219} y={101} textAnchor="middle" fontSize={8} className="f-faint">
          Chats
        </text>

        <rect x={470} y={90} width={42} height={15} rx={3} className="f-hair" />
        <text x={491} y={101} textAnchor="middle" fontSize={8} className="f-faint">
          Editor
        </text>
        <rect x={516} y={90} width={34} height={15} rx={3} className="f-hair" />
        <text x={533} y={101} textAnchor="middle" fontSize={8} className="f-faint">
          Files
        </text>

        <rect x={170} y={334} width={52} height={15} rx={3} className="f-hair" />
        <text x={196} y={345} textAnchor="middle" fontSize={8} className="f-faint">
          Terminal
        </text>
        <rect x={226} y={334} width={32} height={15} rx={3} className="f-hair" />
        <text x={242} y={345} textAnchor="middle" fontSize={8} className="f-faint">
          Runs
        </text>

        <text x={170} y={366} className="f-mono f-faint">
          bun run check
        </text>
        <text x={170} y={378} className="f-mono f-faint">
          412 passed · exit 0
        </text>

        {/* The one measurable boundary: a zone's edge, which is a splitter. */}
        <line
          x1={160}
          y1={406}
          x2={260}
          y2={406}
          className="f-hair"
          markerStart="url(#f1-arrow)"
          markerEnd="url(#f1-arrow)"
        />
        <line x1={160} y1={401} x2={160} y2={411} className="f-hair" />
        <line x1={260} y1={401} x2={260} y2={411} className="f-hair" />
        <text x={210} y={426} textAnchor="middle" fontSize={10} className="f-faint">
          a zone is resizable
        </text>

        {/* Callouts. */}
        <Leader n={1} from={[481, 56]} to={[614, 30]} />
        <Leader n={2} from={[160, 212]} to={[110, 212]} />
        <Leader n={3} from={[560, 212]} to={[610, 212]} />
        <Leader n={4} from={[300, 388]} to={[300, 426]} />
      </svg>
    </Figure>
  );
}
