import Link from "next/link";
import { APPENDICES, CONTENTS, FIGURES, SECTIONS, TABLES } from "@/lib/document";

/**
 * The contents list, and the registers of plates.
 *
 * This is the document's front matter and its navigation at once, which is what a
 * table of contents has always been — and it is the device that replaced the
 * previous version's "draft". The draft was the same shape of data (an ordered
 * list of numbered parts) carrying a metaphor a reader had to learn; a contents
 * list is that data with nothing on top, and every reader already knows how to
 * use it.
 *
 * The rubric sits at the end of each line and faint, so scanning the list teaches
 * the vocabulary — `Ends`, `Selvedge`, `Heddles` — without ever putting a term in
 * front of the plain words it names. The glossary then defines each one, and
 * `verify-manual.mjs` asserts that every rubric on the page is defined there.
 *
 * The registers are not decoration either. A manual lists its plates because a
 * reader who wants the drawing of the window does not want to hunt for it, and
 * because writing the list down is what forces the figures to be few and worth
 * drawing.
 */
export function Contents() {
  return (
    <section id="contents" className="section">
      <div className="section-rule" />
      <header className="section-head">
        <p className="rubric">Contents</p>
        <h2 className="t-section mt-3">
          {SECTIONS.length} sections, {APPENDICES.length} appendices,{" "}
          {FIGURES.length} figures
        </h2>
      </header>

      <ol className="contents-list">
        {CONTENTS.map((entry) => (
          <li className="contents-item" key={entry.id}>
            <span className="contents-num num">{entry.number}</span>
            <Link href={`#${entry.id}`} className="contents-title">
              {entry.title}
            </Link>
            <span className="contents-rubric">{entry.rubric}</span>
          </li>
        ))}
      </ol>

      <div className="register">
        <p className="label">Figures</p>
        <dl className="mt-2">
          {FIGURES.map((plate) => (
            <div className="register-row" key={plate.n}>
              <dt className="register-n">Figure {plate.n}</dt>
              <dd>{plate.title}</dd>
            </div>
          ))}
        </dl>
      </div>

      <div className="register">
        <p className="label">Tables</p>
        <dl className="mt-2">
          {TABLES.map((chart) => (
            <div className="register-row" key={chart.n}>
              <dt className="register-n">Table {chart.n}</dt>
              <dd>{chart.title}</dd>
            </div>
          ))}
        </dl>
      </div>
    </section>
  );
}
