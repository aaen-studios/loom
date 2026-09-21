import { CONTENTS } from "@/lib/document";
import { Spec } from "@/components/doc/spec";

/**
 * Appendix B — the glossary.
 *
 * ---------------------------------------------------------------------------
 * Why the weaving vocabulary needs a page of its own
 * ---------------------------------------------------------------------------
 *
 * The previous version of this site used this vocabulary as its *navigation*:
 * seven sections called `Warp`, `Ends`, `Pick`, `Count`, `Selvedge`, `Heddles` and
 * `Off the loom`, with headings like "Nine threads, held under tension" and an
 * `01`–`07` counter beside each. A reader had to learn what a pick was before they
 * could find out what the software costs. That is the failure the rebuild was
 * asked to fix, and it was *structural* rather than cosmetic — removing the
 * decoration would not have removed the costume, because the costume was carrying
 * the page's contents.
 *
 * A manual fixes it differently. A manual is allowed to use trade vocabulary, and
 * the thing that makes that legitimate is that it defines its terms. So each of the
 * seven words still appears — as the *part name* on a section, and here — and every
 * one of them arrives with its plain meaning attached, in a list a reader can
 * reach in one click from the contents.
 *
 * The rule this enforces is checked rather than trusted: `document.test.ts` asserts
 * that every rubric in the contents is either one of these seven terms or one of
 * the two plain words (`Instrument`, `Reference`) used where no honest loom term
 * exists. A rubric that names a term the glossary does not define fails the build.
 *
 * ---------------------------------------------------------------------------
 * Why the definitions are real
 * ---------------------------------------------------------------------------
 *
 * Each entry says what the word means on an actual loom *and* what it means here,
 * because the two are not the same claim and conflating them would be the
 * half-measure — a reader who already knows the craft would notice immediately.
 * `Ends` really are the individual warp threads and their count is how a cloth is
 * described; the section it names really is a list of the parts the application is
 * made of. Where the parallel is loose, the entry says so: `Heddles` is a stretch,
 * and the definition admits as much rather than dressing it up.
 */
export function Glossary() {
  return (
    <Spec
      rows={[
        {
          term: "Warp",
          def: (
            <>
              <em>On a loom:</em> the threads strung lengthwise and held under
              tension before any weaving starts — the fixed structure the pattern is
              woven into. <em>Here:</em> the window itself. It is what everything
              else is arranged inside, and like a warp it is set up first and does
              not move.
            </>
          ),
        },
        {
          term: "Pick",
          def: (
            <>
              <em>On a loom:</em> one pass of the shuttle across the warp — one row
              of cloth, the smallest unit that means anything. <em>Here:</em> one
              turn of the agent: something asked, something thought, something run,
              something written back.
            </>
          ),
        },
        {
          term: "Ends",
          def: (
            <>
              <em>On a loom:</em> the individual warp threads, counted per inch — a
              cloth is described by how many ends it has. <em>Here:</em> the
              capabilities the application is assembled from.
            </>
          ),
        },
        {
          term: "Count",
          def: (
            <>
              <em>On a loom:</em> the thread count, which is what fixes a cloth&rsquo;s
              weight and character. <em>Here:</em> how many model providers it can be
              strung with, and what that number means in practice.
            </>
          ),
        },
        {
          term: "Selvedge",
          def: (
            <>
              <em>On a loom:</em> the tightly-woven border down each edge that stops
              a cloth fraying — the part that holds the rest together.{" "}
              <em>Here:</em> the section on where your data lives, what leaves the
              machine and what the licence permits.
            </>
          ),
        },
        {
          term: "Heddles",
          def: (
            <>
              <em>On a loom:</em> the wires that lift selected warp threads to open a
              gap for the shuttle, and so decide what the pattern will be.{" "}
              <em>Here:</em> questions. This is the loosest of the seven parallels
              and it is stated as such: a question is what determines which of the
              facts you were given actually matter to you, which is close enough to
              be worth the name and no closer.
            </>
          ),
        },
        {
          term: "Off the loom",
          def: (
            <>
              <em>On a loom:</em> the last thing that happens to a cloth. It is cut
              free of the tension that made it, and after that it is a finished
              object rather than a process. <em>Here:</em> installation — the point
              at which this stops being something you are reading about.
            </>
          ),
        },
      ]}
    />
  );
}

/**
 * The contents list, as a plain list of links, for the colophon's cross-reference.
 *
 * Not used on the page — the running head's `Contents` link and the margin index
 * both reach the real one — but exported so `verify-manual.mjs` can assert that the
 * number of entries it expects is the number the data declares, without reaching
 * into the DOM for something the DOM renders twice.
 */
export const CONTENTS_FOR_VERIFICATION = CONTENTS;
