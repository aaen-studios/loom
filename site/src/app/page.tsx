import { MarginIndex } from "@/components/doc/margin-index";
import { Contents } from "@/components/doc/contents";
import { Colophon } from "@/components/doc/colophon";
import { TitlePage } from "@/components/pages/title-page";
import { SectionsA } from "@/components/body/sections-a";
import { SectionsB } from "@/components/body/sections-b";
import { SectionsC } from "@/components/body/sections-c";
import { Interlude } from "@/components/body/interlude";
import { APPENDICES, CONTENTS, FIGURES, SECTIONS, TABLES } from "@/lib/document";

/**
 * The document.
 *
 * One page, in reading order: title page, contents, eight sections, two appendices,
 * colophon. There is no routing between them and no navigation to speak of, because
 * a manual is not a set of pages — it is one artefact with a running order, and
 * splitting it would cost the reader the thing that makes a manual good, which is
 * the ability to keep going.
 *
 * The components are not arranged here so much as *read* from `lib/document.ts`:
 * each section looks its own entry up, so its anchor, its number, its part name and
 * its place in the contents cannot drift out of step with the front matter. That is
 * also what the running head measures, what the margin index lists and what
 * `verify-manual.mjs` counts — one source, four readers.
 *
 * ---------------------------------------------------------------------------
 * What this replaced
 * ---------------------------------------------------------------------------
 *
 * The previous front page was a landing page in seven numbered sections, each named
 * for a step of weaving and each opening on a heading like "Seven threads, held
 * under tension." Four modules existed to drive a scripted chat animation in the
 * hero, and six components existed to draw the metaphor: a fixed field of twelve
 * hairlines, a weft line that followed the scroll, a set of drafting cells in the
 * navigation, and a knot per section lit by an active-section observer.
 *
 * All of it is gone. The elements that carried *information* have been kept and
 * relabelled — the position indicator is a contents list, the twelve-column grid is
 * a text measure, the section counter is a section counter — and the elements that
 * carried only the metaphor have been deleted rather than restyled, because a
 * restyled costume is still a costume and the previous rebuild proved that.
 *
 * The three components below are new and are the whole of this version's design:
 *
 *   - `MarginIndex`, the contents list fixed in the left margin, marking your place.
 *   - `Interlude`, a full-bleed typographic turn between the argument and the
 *     reference half, which is what keeps eight sections from reading as one loop.
 *   - `Colophon`, the last page of a book, which is the most human thing here.
 *
 * There is no `app/layout.tsx` change to make and no route to add. A manual is one
 * document, and the two pages that are not the manual — `/download`, which has to
 * tell you the truth about the current release, and `/privacy`, which has to be
 * quotable — say so in their own first lines.
 */
export default function Home() {
  return (
    <>
      {/* Fixed, in the left margin, and only where the arithmetic leaves room for
          it. It lists the same entries the contents page does, so a reader never has
          two different maps. */}
      <MarginIndex />

      <div className="paper">
        <TitlePage />
        <Contents />

        <SectionsA />

        {/* The turn. Placed here rather than anywhere else because this is where the
            document stops describing and starts enumerating: the three sections above
            are an argument, and the five below are reference material. A reader who
            has got this far has earned a pause, and a manual that never changes
            register is the thing this page is trying not to be. */}
        <Interlude />

        <SectionsB />
        <SectionsC />
        <Colophon />
      </div>
    </>
  );
}

/**
 * What this document claims to contain, re-exported for `verify-manual.mjs`.
 *
 * A build script cannot import a React component to count the sections it renders,
 * and counting `data-section` in the built HTML would only measure the running order
 * against itself — it would happily pass a document with two sections and a
 * front matter claiming eight. So the expectation comes from the same module the
 * front matter reads, and the verifier compares the rendering against the claim.
 *
 * Written out as a plain object literal rather than as four separate exports so the
 * verifier can find it with one regex, and so that adding a fifth thing to count
 * means editing one block rather than remembering to add a line here.
 */
export const DOCUMENT_CLAIMS = {
  sections: SECTIONS.length,
  appendices: APPENDICES.length,
  figures: FIGURES.length,
  tables: TABLES.length,
  contents: CONTENTS.length,
} as const;
