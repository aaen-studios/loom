import { Code, Pick } from "@/components/weave/pass";
import { passById } from "@/lib/weave/passes";

/**
 * Pass three: a pick.
 *
 * In weaving a *pick* is one pass of the shuttle across the warp — one complete
 * row, the smallest unit of cloth that means anything. So this pass is one
 * complete turn of the agent, drawn as the sequence that produced it: a line for
 * each thing that happened, in the order it happened.
 *
 * It sits here, after the threads and before the thread count, because it is the
 * only section that shows a *sequence*. Everything above is an inventory; this is
 * what the inventory is for.
 *
 * The transcript is real rather than illustrative — it names a file that exists in
 * this repository and a helper that exists inside it, and the duplication it
 * describes was genuinely in the code.
 *
 * Deliberately NOT animated. The hero already shows a turn moving; a second moving
 * thing would make the page fidget, and a reader who has scrolled this far wants to
 * study a row rather than watch one.
 */
const TURN = [
  {
    who: "You",
    body: "The rename left four call sites behind. Fix them, and prove the fallback still resolves.",
    tool: false,
  },
  {
    who: "Thinking",
    body: "Two of the four are copy-paste rather than renames, so repointing the import alone would leave the duplication in place. The fallback in presetById looks like defensive coding, but something depends on it — check the tests before changing its behaviour.",
    tool: false,
  },
  { who: "Read", body: "src/lib/background.ts", tool: true, meta: "0.4s" },
  { who: "Grep", body: '"presetById"  →  6 matches', tool: true, meta: "0.1s" },
  { who: "Edit", body: "src/lib/background.ts  +4 −2", tool: true, meta: "—" },
  { who: "Run", body: "bun run test  ·  exit 1", tool: true, meta: "4.1s" },
  {
    who: "Loom",
    body: "Six failures, all from the same rename. Three more call sites used the string literal, and one test asserted the old id. Fixing those and re-running.",
    tool: false,
  },
] as const;

export function PickTurn() {
  const pass = passById("pick");

  return (
    <Pick
      pass={pass}
      title="One pick, end to end."
      lead="Not a tour. This is the shape of a single pass of the shuttle: something asked, something thought, something run, something written back."
    >
      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,19rem)]">
        {/* The pick itself: every line of it, numbered. */}
        <div className="panel-strong rounded-sheet overflow-hidden">
          <div className="border-b border-[var(--glass-border)] px-4 py-2.5">
            <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
              Pick 01 · the whole row
            </p>
          </div>

          <ol className="divide-y divide-[var(--glass-border)]">
            {TURN.map((line, index) => (
              <li key={index} className="flex gap-4 px-4 py-3">
                {/* The pick's own line number in the draft, outside the text so it
                    never travels with a selection. */}
                <span
                  aria-hidden="true"
                  className="text-faint w-6 shrink-0 pt-[2px] text-right font-mono text-[11px] tabular-nums opacity-70 select-none"
                >
                  {String(index + 1).padStart(2, "0")}
                </span>

                <div className="min-w-0 flex-1">
                  <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
                    {line.who}
                    {"meta" in line && line.meta && line.meta !== "—" && (
                      <span className="ml-2 normal-case tracking-normal opacity-70">
                        {line.meta}
                      </span>
                    )}
                  </p>
                  <p
                    className={
                      "mt-1 text-[13.5px] leading-[1.6] " +
                      (line.tool
                        ? "bg-[var(--ink-ghost)] inline-block rounded-[6px] px-1.5 py-[0.15em] font-mono text-[12px]"
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

        {/* What the pick demonstrates, as three short notes rather than a
            paragraph. Someone skimming the demo should be able to read the
            takeaway without parsing the transcript. */}
        <div className="flex flex-col gap-3">
          <Note title="The order is the point">
            Reasoning arrives before the answer, not after it. The tool calls land
            between the thinking and the reply, where they happened. A transcript
            that reorders these is showing you a summary, not a turn.
          </Note>
          <Note title="A failed command is information">
            The test run exits 1 and the turn continues. Loom reads the failure,
            groups the six errors under one cause, and fixes the cause rather than
            each symptom.
          </Note>
          <Note title="Long commands do not block it">
            A command that outlives the turn is adopted instead of killed. It gets
            an id, keeps streaming to <Code>~/.loom/logs</Code>, and appears in the
            Runs panel with a Stop button.
          </Note>
        </div>
      </div>
    </Pick>
  );
}

function Note({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="panel rounded-control p-4">
      <h3 className="text-[13.5px] font-medium">{title}</h3>
      <p className="text-soft mt-2 text-[13px] leading-[1.6]">{children}</p>
    </div>
  );
}
