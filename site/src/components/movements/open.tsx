import Link from "next/link";
import { DOWNLOAD } from "@/lib/site";
import { movementById } from "@/lib/movements";
import { PANELS } from "@/lib/panels";
import { AnimatedWeave, Drifting } from "@/components/weave/animated";
import { Reveal } from "@/components/ui/reveal";
import {
  Card,
  Code,
  Figures,
  Movement,
  Note,
  P,
  Spec,
  Statement,
  Table,
} from "@/components/ui/primitives";

/**
 * Movements one to four: the weave, the parts, where they meet, and narrowing.
 *
 * ---------------------------------------------------------------------------
 * Why the figures are abstract
 * ---------------------------------------------------------------------------
 *
 * The page does not show the product. It shows the *shape* of the product — threads under tension,
 * parts holding each other in a lattice, two systems deflecting where they meet, many things
 * narrowing to one point — and the copy does the work that a screenshot would otherwise do.
 *
 * That is a real decision and not an aesthetic preference. A screenshot of a docking layout at the
 * size a web page can afford is illegible; a labelled diagram of one is a picture of software, which
 * is what every other page has; and an animated mock-up of one is the pattern this page has already
 * abandoned twice. A generative figure says something a screenshot cannot — that the thing is made
 * of parts that hold each other under tension — and it does it at any size and with no asset.
 *
 * Every figure is drawn in the application's own thread colours, from `lib/weave.ts`, with a seed
 * that is stated at the call site. Two of the figures would be identical if they shared a seed, so
 * the seeds are all different and the tests check that a seed change changes the drawing.
 */

/* ===========================================================================
   One — the weave
=========================================================================== */

/**
 * The hero.
 *
 * ---------------------------------------------------------------------------
 * Why the headline is four words
 * ---------------------------------------------------------------------------
 *
 * The page's job is to get someone to a 90 MB download, and the thing standing between them and it
 * is one question: *what is this, and is it any good?* A headline answers the first half, a figure
 * answers the second, and the figure can only answer it if it is given the room. So the headline is
 * measured in characters rather than in lines, the standfirst is two sentences, and everything below
 * them is the weave.
 *
 * "A window that holds everything." is the product in five words, and the rest of the page supports
 * it rather than repeating it — the terminal, the editor, git and the conversation are genuinely in
 * one window, and that is architecture rather than positioning.
 */
export function Hero() {
  /*
   * The hero is the first movement, so it carries the first movement's anchor.
   *
   * It does not use the `Movement` primitive, and that is deliberate rather than an oversight: this is
   * the one section on the page with an `h1`, with a full-bleed animated figure behind it, and with a
   * heading set at display size — so routing it through the same component as the other five would mean
   * five props that only ever toggle one section's layout. What it does share is the anchor, taken from
   * the same list, which is what keeps the footer's first link pointing somewhere real.
   *
   * That gap is worth recording, because the verifier caught it: the footer was listing `#weave` while
   * the hero had no id at all, so the first link in the footer's list of movements scrolled nowhere.
   * Nothing else noticed — a missing anchor is invisible in the markup, in the outline, and to the eye.
   */
  const movement = movementById("weave");

  return (
    <section id={movement.id} className="relative isolate overflow-hidden pt-16 pb-10 sm:pt-24">
      {/* The weave, behind everything, full-bleed. Fixed height rather than an aspect ratio, so the
          threads are the same length whatever the viewport does. */}
      <div className="pointer-events-auto absolute inset-0 -z-10 h-full">
        <AnimatedWeave seed={20250} className="h-full w-full" />
      </div>

      {/*
        * The scrim.
        *
        * ---------------------------------------------------------------------------
        * Why this is two gradients and why the first version of it was wrong
        * ---------------------------------------------------------------------------
        *
        * The copy has to sit on a ground it can be read against, and the figure has to survive being
        * underneath it. The first version tried to do both with one vertical wash — 82% of the page
        * colour at the top, 48% in the middle, fully opaque at the bottom — and the effect on screen was
        * that the *scrim* was doing the fading, not the artwork. The threads dissolved into a grey
        * gradient about a third of the way down and were simply erased below that, and the figure read as
        * a watermark behind a headline rather than as cloth the page is printed on.
        *
        * The fix is to separate the two jobs, because they are in different directions:
        *
        *   - **Horizontally**, the type is in a column on the left. So the page colour fades *out*
        *     left to right: solid enough behind the headline and the lede, completely gone by the
        *     right-hand 40% of the viewport, where the cloth is at full strength and there is nothing to
        *     read. That is also what stops the right half of the hero being empty — it is not empty, it
        *     is the clearest view of the work on the page.
        *   - **Vertically**, only the bottom needs anything, and only a short fade, so that the threads
        *     do not run into the hairline rule that opens the next movement. A hard edge there reads as a
        *     crop; fading over the last fifth reads as the cloth continuing past the frame.
        *
        * The top gets no wash at all. A thread field is at its most convincing where it is uninterrupted,
        * and the headline is 5rem of near-black — it does not need help.
        */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 -z-10"
        style={{
          background: [
            // Vertical, for the join with the next movement. Listed first so it paints on top.
            "linear-gradient(to bottom, transparent 74%, var(--page) 100%)",
            // Horizontal, for the copy column. Transparent well before the right-hand figure.
            "linear-gradient(to right, color-mix(in srgb, var(--page) 78%, transparent) 0%, color-mix(in srgb, var(--page) 42%, transparent) 34%, transparent 58%)",
          ].join(", "),
        }}
      />

      <div className="shell relative">
        {/*
          * The hero's eyebrow, in the same form as every other movement's: the index, then the word.
          *
          * The first version put the platform line here instead — "Windows 10 or 11, 64-bit · free ·
          * MIT licensed" — which produced a page whose numbering started at **02**. Every movement below
          * announces itself with an index and a word, so a hero without one is not neutral: it reads as
          * a missing first term. A sequence that opens at two is a sequence the reader stops trusting.
          *
          * The spec line is not gone, it has moved to where it is actually useful — beside the download
          * button, which is the only thing on the page that a platform requirement constrains.
          */}
        <p className="movement-index">
          <span className="t-index" aria-hidden="true">
            {String(movement.n).padStart(2, "0")}
          </span>
          <span className="t-label">{movement.label}</span>
        </p>

        <h1 className="t-display mt-8 max-w-[15ch]">A window that holds everything.</h1>

        <p className="t-lede mt-8 max-w-[50ch]">
          Loom is a desktop workspace for AI chat and agents. The conversation, a real terminal, an
          editor with git, and the files they are working on — with the model&rsquo;s reasoning kept
          in the transcript, where it happened.
        </p>

        <div className="mt-10 flex flex-wrap items-center gap-x-5 gap-y-3">
          <Link href={DOWNLOAD.publicPath} className="btn-primary h-11 px-5 text-[14.5px]">
            Download for Windows
          </Link>
          {/* Down, not up: the hero is the top of the page, so the natural second action is to keep
              going rather than to return to where you already are. */}
          <Link href="#parts" className="btn-ghost h-11 px-5 text-[14.5px]">
            See what it does
          </Link>
          {/* The platform, beside the button it constrains. On the first line of the page it was a
              claim about the product; here it is a fact about the file. */}
          <span className="t-small">{DOWNLOAD.requirements} · free · MIT licensed</span>
        </div>
      </div>
    </section>
  );
}

/* ===========================================================================
   Two — the parts
=========================================================================== */

/**
 * The eight panels, as a lattice.
 *
 * The figure is a jittered grid of joints joined to their two nearest neighbours, which is the
 * smallest amount of structure that reads as a *system* rather than as a scatter plot. It is the
 * right figure for this movement because the argument is exactly that: eight parts, each holding the
 * two beside it, and the whole arrangement stronger than any of them.
 *
 * The cards below are the same eight the figure's joints stand for, and both come from
 * `lib/panels.ts`.
 */
export function Parts() {
  return (
    <Movement
      id="parts"
      heading="Eight parts, holding each other."
      lead="Every panel is a real tool rather than a view of one. The terminal is a pty, the editor is Monaco, and git goes through your own git — so your credential manager, your hooks and your signing keys all keep working."
    >
      {/* The figure, bleed to the right edge and clipped, so it reads as a texture the prose sits
          beside rather than as a picture on a page. */}
      <div className="figure-plate" aria-hidden="true">
        <Drifting kind="lattice" seed={4181} className="figure-art" opacity={0.9} id="parts" />
      </div>

      <div className="mt-12 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {PANELS.map((panel, index) => (
          <Reveal key={panel.id} delay={index * 40}>
            <Card>
              <span className="t-index" aria-hidden="true">
                {String(index + 1).padStart(2, "0")}
              </span>
              <h3 className="t-h3 mt-2">{panel.name}</h3>
              <p className="mt-2 text-[13.5px] leading-[1.6] text-soft">{panel.role}</p>
            </Card>
          </Reveal>
        ))}
      </div>

      <P>
        And the parts that are not panels: a quick-ask overlay on{" "}
        <kbd className="kbd">Ctrl+Shift+Space</kbd> that summons a chat from anywhere in Windows,
        voice mode on <kbd className="kbd">Ctrl+Shift+V</kbd> — which has no button, so the shortcut
        sheet is the only place its keys exist — IDE mode, which swaps the dock&rsquo;s centre for a
        file tree, an editor and a slim chat column, and an updater that verifies a signature before
        it applies anything.
      </P>

      <Spec
        rows={[
          {
            term: "Terminal",
            def: (
              <>
                A real pty — ConPTY on Windows — so colours, interactive prompts, resizing and
                full-screen programs all work. All sixteen ANSI colours are Loom&rsquo;s own rather
                than xterm&rsquo;s stock palette. And the terminal is <em>yours</em>:{" "}
                <Code>pty_write</Code> is not an agent tool and is not advertised to the model,
                because a live shell has no permission card in front of it and the only safe
                arrangement is that the agent cannot type into one.
              </>
            ),
          },
          {
            term: "Editor",
            def: "Monaco, in a theme derived from the application's own stylesheet. A changed file opens as an exact diff in a real diff editor rather than as patch text — a description of a change is not the change.",
          },
          {
            term: "Autosave",
            def: "On by default, and only safe because every write carries the SHA-256 of what it loaded. If the file moved on underneath it, the write is refused and you get a conflict rather than a silent overwrite.",
          },
          {
            term: "Runs",
            def: "A command that outlives its turn is adopted rather than killed. It keeps streaming to a log on disk, gets a Stop button, and at most eight are tracked at once so processes cannot pile up invisibly.",
          },
          {
            term: "Why one window",
            def: "The dock, the agent and the editor share one shell deliberately. A second application window would have reimplemented the docking, the shortcuts and the theme, and then drifted from them.",
          },
        ]}
      />
    </Movement>
  );
}

/* ===========================================================================
   Three — where they meet
=========================================================================== */

/**
 * Contention: the four agent modes, and the permission system behind them.
 *
 * The figure is two ring systems deflecting each other, which is what this movement is about — a
 * model's judgement meeting yours, and neither being unchanged by it. It is the least literal figure
 * on the page and the most apt.
 */
const MODES = [
  {
    name: "Plan",
    line: "Researches, asks questions, proposes.",
    body: "Reads the workspace and the web, then hands you a plan. Refuses the write and command tools outright rather than asking and being refused — so there is no path by which a planning turn touches a file.",
  },
  {
    name: "Review",
    line: "Findings, ranked, with a file and a line.",
    body: "Reads a diff or a directory and reports severity-ranked issues with the location of each, plus a proposed fix. The findings are the deliverable; it changes nothing.",
  },
  {
    name: "Build",
    line: "Does the work.",
    body: "Edits files, runs commands, reads the failures and iterates. What it may do without asking is a separate control — the permission mode — because it answers a different question.",
  },
  {
    name: "Atelier",
    line: "Everything Build does, plus Loom itself.",
    body: "Additionally exposes the tools that edit Loom's own configuration — personas, prompts, skills, MCP servers, providers, settings. Every write snapshots first. It is the one mode that runs its own removals without a card, and it is deliberately per-chat with no way to make it the default.",
  },
] as const;

export function Meeting() {
  return (
    <Movement
      id="meeting"
      heading="Where your judgement meets its own."
      lead="The mode decides what the model is willing to do. It is set per chat and per turn, and the two read-only modes refuse the write and command tools rather than asking for permission — which is the difference between a mode and a preference."
    >
      <div className="figure-plate" aria-hidden="true">
        <Drifting kind="rings" seed={1618} className="figure-art" opacity={0.85} id="meeting" />
      </div>

      <Statement>
        A permission system is only worth having if refusing is cheaper than allowing.
      </Statement>

      <div className="mt-12 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {MODES.map((mode, index) => (
          <Reveal key={mode.name} delay={index * 40}>
            <Card>
              <div className="flex items-baseline gap-3">
                <span className="t-index" aria-hidden="true">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <h3 className="t-h3">{mode.name}</h3>
              </div>
              <p className="mt-2 text-[13.5px] font-medium text-[var(--ink)]">{mode.line}</p>
              <p className="mt-2 text-[13.5px] leading-[1.6] text-soft">{mode.body}</p>
            </Card>
          </Reveal>
        ))}
      </div>

      <Table
        caption="Table 1 — what each permission level runs without asking"
        head={["Level", "Runs without asking", "Still asks"]}
      >
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Ask</b>
          </td>
          <td>Reads within the workspace.</td>
          <td>Every write, every command, every path change.</td>
        </tr>
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Auto read-only</b>
          </td>
          <td>Reads, grep, the git tools, the semantic index.</td>
          <td>Writes and commands, which still card.</td>
        </tr>
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Auto run all</b>
          </td>
          <td>Reads, writes and commands.</td>
          <td>
            Deletes the risk check flags — Atelier&rsquo;s exemption is deliberately not inherited at
            this level.
          </td>
        </tr>
      </Table>

      <Note>
        There is no level that runs a delete without a card, and no way to turn the card off
        globally. The one exemption is a property of a single mode, chosen per chat, and it is
        stated plainly below rather than buried here.
      </Note>
    </Movement>
  );
}

/* ===========================================================================
   Four — narrowing
=========================================================================== */

/**
 * One turn, and the one mode that can change Loom itself.
 *
 * The figure is many threads converging to a point, because that is what a turn is: a wide field of
 * possible actions narrowing to the ones actually taken, in order. The turn below really happened —
 * the file exists in this repository, the helper exists inside it, the duplication was genuinely in
 * the code, and the test run really did exit 1.
 */
const TURN = [
  {
    who: "You",
    body: "The rename left four call sites behind. Fix them, and prove the fallback still resolves.",
    kind: "say" as const,
    meta: null,
  },
  {
    who: "Thinking",
    body: "Two of the four are copy-paste rather than renames, so repointing the import alone would leave the duplication in place. The fallback in presetById looks like defensive coding, but something depends on it — check the tests before changing its behaviour.",
    kind: "think" as const,
    meta: null,
  },
  { who: "Read", body: "src/lib/background.ts", kind: "tool" as const, meta: "0.4s" },
  { who: "Grep", body: '"presetById"  →  6 matches', kind: "tool" as const, meta: "0.1s" },
  { who: "Edit", body: "src/lib/background.ts  +4 −2", kind: "tool" as const, meta: null },
  { who: "Run", body: "bun run test  ·  exit 1", kind: "tool" as const, meta: "4.1s" },
  {
    who: "Loom",
    body: "Six failures, all from the same rename. Three more call sites used the string literal, and one test asserted the old id. Fixing those and re-running.",
    kind: "say" as const,
    meta: null,
  },
] as const;

export function Narrowing() {
  return (
    <Movement
      id="narrow"
      heading="A field of possibilities, narrowed in order."
      lead="Not a tour. This is one turn of the agent, in the order its parts arrived: something asked, something thought, something run, something written back. A transcript that reorders these is showing you a summary, not a turn."
    >
      {/* `relative` and `isolate` because the figure is now an absolutely positioned layer inside this
          row rather than a grid track: without them the `-z-10` plate escapes to the page's root stacking
          context and can slide under the movement's own background. */}
      <div className="relative isolate grid gap-10 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)] lg:items-start">
        {/* The figure beside the turn rather than above it, so the two read as one movement: the
            convergence on the left, the sequence it converged to on the right. */}
        {/*
          * The figure is *behind* the turn, not beside it — and it took a screenshot to see that.
          *
          * This was a two-column grid: the figure in the first track, the transcript card in the second.
          * The intent was that the threads converge in the left track and stop at the card's edge, which
          * is what the narrow viewport does. On a wide one it did not, and the reason is a stacking rule
          * rather than a geometry one: `.figure-plate` is `position: relative` (and `lg:sticky`), so it
          * establishes a positioned box, while the card below it is a static `panel-strong`. A positioned
          * box paints above a static sibling *whatever the DOM order*, so every thread in the plate's
          * subtree was lifted above the card — and you could watch hairlines crossing the card's border
          * near "READ 0.4s" and "RUN 4.1s" while the middle stayed opaque. Two planes fighting, and the
          * seam was exactly where the card's background let one through.
          *
          * The fix is not `z-index` on the card, tempting as that is. Stacking the two is a *tie* — the
          * threads would then stop at the card's left edge and accumulate behind it, which is the figure
          * being cropped by an invisible wall. So the plate becomes a full-width layer behind the row,
          * the way the hero's figure already is, and the card sits in the right-hand column on top with
          * nothing to intersect. The threads now pass behind the transcript and out the other side, which
          * is what converging threads in a cloth should do.
          *
          * The plate keeps its bleed and its mask; only its role in the layout changed.
          */}
        <div className="absolute inset-0 -z-10">
          <div className="figure-plate lg:h-full" aria-hidden="true">
            <Drifting kind="bundle" seed={5772} width={900} height={900} className="figure-art" id="narrow" />
          </div>
        </div>

        <Reveal className="relative z-10 lg:col-start-2">
          <div className="panel-strong rounded-sheet overflow-hidden">
            <div className="flex items-center gap-2 border-b border-[var(--glass-border)] px-4 py-2.5">
              <span className="t-label">Loom · one turn</span>
              <span className="flex-1" />
              <span className="t-label">412 tests · 1 failing</span>
            </div>

            <ol>
              {TURN.map((line, index) => (
                <li
                  key={index}
                  className="flex gap-4 border-b border-[var(--glass-border)] px-4 py-3.5 last:border-b-0"
                >
                  {/* The line number, outside the text so it never travels with a selection made
                      inside the row. */}
                  <span
                    aria-hidden="true"
                    className="num w-6 shrink-0 pt-[3px] font-mono text-[11px] text-faint opacity-70 tabular-nums select-none"
                  >
                    {String(index + 1).padStart(2, "0")}
                  </span>

                  <div className="min-w-0 flex-1">
                    <p className="t-label">
                      {line.who}
                      {line.meta && (
                        <span className="ml-2 tracking-normal normal-case opacity-70">
                          {line.meta}
                        </span>
                      )}
                    </p>
                    <p
                      className={
                        "mt-1.5 text-[13.5px] leading-[1.6] " +
                        (line.kind === "tool"
                          ? "inline-block rounded-[6px] bg-[var(--ink-ghost)] px-1.5 py-[0.15em] font-mono text-[12px] text-soft"
                          : line.kind === "think"
                            ? "text-faint"
                            : "text-soft")
                      }
                    >
                      {line.body}
                    </p>
                  </div>
                </li>
              ))}
            </ol>
          </div>
        </Reveal>
      </div>

      <Figures
        items={[
          { value: "4", label: "agent modes" },
          { value: "8", label: "tracked commands, at most" },
          { value: "5 MB", label: "per command log" },
          { value: "0", label: "calls to any server of ours" },
        ]}
      />

      <P>
        The one mode worth reading twice is Atelier, because it is the only place in Loom where a
        model can remove something without being asked first. What that costs is precise: the harness
        removals are recoverable from <Code>~/.loom/backups</Code>, but a deleted workspace path and a
        deleted scheduled job are not backed up by anything. It is never accepted as the global
        default, for exactly this reason — and scheduling a job still cards even in Atelier, because
        a standing commitment to act while nobody is watching is not a removal and should not be
        treated as one.
      </P>
    </Movement>
  );
}
