// Verifies that the manual is the document it claims to be.
//
// The other three verifiers check the tokens, the pages and the accessibility of the
// markup. None of them can check the thing this document actually rests on, which is a
// set of *agreements between files that cannot read each other*:
//
//   1. `SECTIONS` and `APPENDICES` in `lib/document.ts` — what the contents list says
//      is in the document.
//   2. `data-section` on each rendered `<section>` — what the document actually
//      contains, and what the running head and the margin index both measure to decide
//      where you are.
//   3. `FIGURES` and `TABLES` — the registers the plates are numbered from, and where
//      their captions are read rather than written.
//   4. `LOOM_TERMS` — the seven weaving words, which are on the page only because
//      Appendix B defines them.
//
// Every one of those can drift silently. A section renamed in the contents but not in
// the body gives a link that scrolls nowhere and a running head that names a section
// that is not there. A figure whose caption was written by hand keeps looking fine
// after the register is edited. A rubric that names a term the glossary dropped makes
// the reader decode a word nobody explained — which is precisely and specifically the
// failure the rebuild was asked to fix, so it is the one this file is loudest about.
//
// It reads TypeScript source as *text*. Importing `lib/document.ts` would be more
// honest in principle and would drag a TypeScript loader into a build step that has to
// stay dependency-free for CI; the patterns below are strict enough that a malformed
// entry does not match at all, and every count is cross-checked against the declared
// length rather than against a literal.
//
// It exits 1 on any failure, so it gates CI.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const read = (relative) => readFileSync(join(here, relative), "utf8");

let failed = 0;
const check = (label, ok, detail) => {
  if (!ok) failed += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${label}${detail ? `\n        ${detail}` : ""}`);
};

// --- 1. The source of truth ------------------------------------------------

const documentSource = read("src/lib/document.ts");
const css = read("src/app/globals.css");
const pageSource = read("src/app/page.tsx");
const figureComponent = read("src/components/doc/figure.tsx");
const tableComponent = read("src/components/doc/table.tsx");
const contentsComponent = read("src/components/doc/contents.tsx");
const indexComponent = read("src/components/doc/margin-index.tsx");
const runningHead = read("src/components/doc/running-head.tsx");

/**
 * One entry of the contents, as declared.
 *
 * Matched on `id`, `number`, `rubric` and `title` appearing in that order on one
 * line, which is how they are written. A hand-edited entry that drops a field does
 * not match at all, and the entry count is then compared against the number of
 * `id:` keys below — so a missed entry cannot pass silently as a shorter list.
 */
const ENTRY =
  /\{\s*id:\s*"([a-z-]+)",\s*number:\s*"(\w+)",\s*rubric:\s*"([^"]+)",\s*title:\s*"([^"]+)"\s*\}/g;

/** The entries between two exported const declarations. */
function slice(fromMarker, toMarker) {
  const from = documentSource.indexOf(fromMarker);
  const to = documentSource.indexOf(toMarker);
  if (from === -1 || to === -1 || to <= from) return "";
  return documentSource.slice(from, to);
}

const sectionsBlock = slice("export const SECTIONS", "export const APPENDICES");
const appendicesBlock = slice("export const APPENDICES", "export const CONTENTS");

const parse = (block) => [...block.matchAll(ENTRY)].map((match) => ({
  id: match[1],
  number: match[2],
  rubric: match[3],
  title: match[4],
}));

const sections = parse(sectionsBlock);
const appendices = parse(appendicesBlock);
const declaredInSections = (sectionsBlock.match(/\n\s{2}\{/g) ?? []).length;
const declaredInAppendices = (appendicesBlock.match(/\n\s{2}\{/g) ?? []).length;

check(
  "the body's contents were parsed completely",
  sections.length > 0 && sections.length === declaredInSections,
  `parsed ${sections.length} entr(ies) but found ${declaredInSections} object(s)`,
);
check(
  "the appendices were parsed completely",
  appendices.length > 0 && appendices.length === declaredInAppendices,
  `parsed ${appendices.length} entr(ies) but found ${declaredInAppendices} object(s)`,
);
check(
  "the body is numbered from one, in order, with no gaps",
  sections.every((entry, index) => entry.number === String(index + 1)),
  sections.map((entry) => `${entry.number}:${entry.id}`).join(" "),
);
check(
  "the appendices are lettered, in order",
  appendices.every((entry, index) => entry.number === String.fromCharCode(65 + index)),
  appendices.map((entry) => `${entry.number}:${entry.id}`).join(" "),
);

/**
 * The plates and charts, as declared.
 *
 * One pattern for both, with `\s*` between the fields rather than a newline: figures are
 * written across four lines because their captions are long enough to wrap, and tables
 * on one line because they are not. A pattern that insisted on either shape would parse
 * half the register and report the other half as a mismatch.
 */
const plate = /\{\s*n:\s*(\d+),\s*title:\s*"([^"]+)",\s*section:\s*"([a-z-]+)",?\s*\}/g;

const figureBlock = slice("export const FIGURES", "export const TABLES");
const tableBlock = slice("export const TABLES", "export function figureTitle");

const figures = [...figureBlock.matchAll(plate)].map((match) => ({
  n: Number(match[1]),
  title: match[2],
  section: match[3],
}));
const tables = [...tableBlock.matchAll(plate)].map((match) => ({
  n: Number(match[1]),
  title: match[2],
  section: match[3],
}));

check(
  "the figures were parsed completely",
  figures.length === (figureBlock.match(/\n\s{2}\{/g) ?? []).length && figures.length > 0,
  `parsed ${figures.length} figure(s)`,
);
check(
  "the tables were parsed completely",
  tables.length === (tableBlock.match(/\n\s{2}\{/g) ?? []).length && tables.length > 0,
  `parsed ${tables.length} table(s)`,
);
check(
  "figures and tables are each numbered from one",
  figures.every((figure, index) => figure.n === index + 1) &&
    tables.every((table, index) => table.n === index + 1),
);

/*
 * The glossary's terms, and the bargain over them.
 *
 * The seven weaving words are allowed on this page on exactly one condition: that
 * Appendix B defines them. So the check is two-sided — every term is *used* as a
 * section rubric, and every rubric is either a term or one of the two plain words the
 * document uses where no honest loom word exists.
 *
 * The previous version of this site used the same seven words as its navigation with
 * none of them defined, which meant a reader had to learn a vocabulary before they
 * could find out what the software did. That is the specific defect this assertion
 * exists to prevent, and it is why it is two-sided rather than one.
 */
const termBlock = slice("export const LOOM_TERMS", "export const FIGURES");
const terms = [...termBlock.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
check("the glossary's terms were parsed", terms.length >= 5, `found ${terms.length}`);

const rubrics = sections.map((entry) => entry.rubric);
const missingFromSections = terms.filter((term) => !rubrics.includes(term));
check(
  "every term in the glossary names a section of the body",
  missingFromSections.length === 0,
  missingFromSections.join(", "),
);

const PLAIN = new Set([...terms, "Instrument", "Reference"]);
const undefinedRubrics = rubrics.filter((rubric) => !PLAIN.has(rubric));
check(
  "no section is named for a term the glossary does not define",
  undefinedRubrics.length === 0,
  undefinedRubrics
    .map((rubric) => `"${rubric}" is not in LOOM_TERMS and is not a plain word`)
    .join(", "),
);

// --- 2. The document the front matter claims -------------------------------

/*
 * What the page says it contains, read out of `page.tsx`.
 *
 * Compared against the declarations above rather than against a hardcoded number, so
 * that a section added to `lib/document.ts` and not to the claim — or the reverse —
 * is a failure rather than two numbers that happen to agree because both were edited.
 */
const claimsBlock = pageSource.match(/DOCUMENT_CLAIMS\s*=\s*\{([^}]*)\}/)?.[1] ?? "";
const claim = (key) => {
  const value = claimsBlock.match(new RegExp(`${key}:\\s*(\\d+)`))?.[1];
  return value === undefined ? null : Number(value);
};

check(
  "the page's claim about the body matches the contents",
  claim("sections") === sections.length,
  `page says ${claim("sections")}, the contents declare ${sections.length}`,
);
check(
  "the page's claim about the appendices matches",
  claim("appendices") === appendices.length,
  `page says ${claim("appendices")}, the contents declare ${appendices.length}`,
);
check(
  "the page's claim about the figures matches the register",
  claim("figures") === figures.length,
  `page says ${claim("figures")}, the register holds ${figures.length}`,
);
check(
  "the page's claim about the tables matches the register",
  claim("tables") === tables.length,
  `page says ${claim("tables")}, the register holds ${tables.length}`,
);
check(
  "the page's claim about the contents matches both lists",
  claim("contents") === sections.length + appendices.length,
  `page says ${claim("contents")}, the lists hold ${sections.length + appendices.length}`,
);

// --- 3. The built document --------------------------------------------------

const htmlPath = join(here, ".next", "server", "app", "index.html");
if (!existsSync(htmlPath)) {
  console.error(`\nNo build output at ${htmlPath}. Run \`bun run build\` first.\n`);
  process.exit(1);
}

const html = readFileSync(htmlPath, "utf8");

// React emits `<!-- -->` between a static string and a dynamic value, so a check for a
// rendered sentence has to look at the normalised form. `<b>Figure {n}</b>` serialises
// as `Figure <!-- -->1` and a naive match finds nothing.
const visible = html.replace(/<!--\s*-->/g, "");

const renderedIds = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
const danglingSections = [...sections, ...appendices].filter(
  (entry) => !renderedIds.has(entry.id),
);
check(
  "every entry in the contents has an anchor in the body",
  danglingSections.length === 0,
  danglingSections.map((entry) => `#${entry.id}`).join(", "),
);

const renderedCount = (html.match(/data-section="/g) ?? []).length;
check(
  "the body renders one section per entry, and no more",
  renderedCount === sections.length + appendices.length,
  `rendered ${renderedCount}, the contents declare ${sections.length + appendices.length}`,
);

// The number has to survive to the DOM, because the running head and the margin index
// both decide where you are by reading it — a wrong or missing number would mark the
// wrong entry in the contents, with no error anywhere.
const renderedNumbers = [...html.matchAll(/data-section="([^"]+)"/g)].map((m) => m[1]);
const declaredNumbers = [...sections, ...appendices].map((entry) => entry.number);
check(
  "the rendered sections are the contents' entries, in order",
  renderedNumbers.join(",") === declaredNumbers.join(","),
  `rendered ${renderedNumbers.join(",")} — declared ${declaredNumbers.join(",")}`,
);

// The title travels in an attribute for the same reason the number does, and it is
// what the running head prints on a narrow viewport.
const renderedTitles = [...html.matchAll(/data-section-title="([^"]*)"/g)].map(
  (m) => m[1],
);
const declaredTitles = [...sections, ...appendices].map((entry) => entry.title);
check(
  "every section carries its heading for the running head to read",
  renderedTitles.join("|") === declaredTitles.join("|"),
  renderedTitles.length === 0 ? "no data-section-title attributes at all" : "",
);

// --- 4. The plates ---------------------------------------------------------

/*
 * Captions are counted rather than searched for.
 *
 * Each caption contains its own number, and each number appears twice on the page —
 * once in the caption and once in the register on the contents page — so counting
 * occurrences of `Figure 1` would be wrong by exactly the length of the register while
 * looking perfectly reasonable. Counting the elements is exact.
 */
const captionCount = (html.match(/<figcaption/g) ?? []).length;
check(
  "there is one figure caption per plate in the register",
  captionCount === figures.length,
  `rendered ${captionCount} <figcaption>, the register holds ${figures.length}`,
);

const tableCaptionCount = (html.match(/<table[^>]*>\s*<caption/g) ?? []).length;
check(
  "there is one table caption per chart in the register",
  tableCaptionCount === tables.length,
  `rendered ${tableCaptionCount} table <caption>, the register holds ${tables.length}`,
);

// Every caption must actually name its plate, from the register — which is the point
// of reading the title from `lib/document.ts` rather than passing it at the call site.
const missingFigureTitles = figures.filter(
  (figure) => !visible.includes(`Figure ${figure.n}</b> — ${figure.title}`),
);
check(
  "every figure's caption is the register's title, verbatim",
  missingFigureTitles.length === 0,
  missingFigureTitles.map((figure) => `Figure ${figure.n}`).join(", "),
);

const missingTableTitles = tables.filter(
  (table) => !visible.includes(`Table ${table.n}</b> — ${table.title}`),
);
check(
  "every table's caption is the register's title, verbatim",
  missingTableTitles.length === 0,
  missingTableTitles.map((table) => `Table ${table.n}`).join(", "),
);

// A figure pointing at a section that does not exist would be silently fine: nothing
// renders from that field, so a stale id survives every other check on this page.
const sectionIds = new Set([...sections, ...appendices].map((entry) => entry.id));
const strays = [...figures, ...tables].filter((item) => !sectionIds.has(item.section));
check(
  "every plate is attached to a section the body renders",
  strays.length === 0,
  strays.map((item) => `${item.title} → #${item.section}`).join(", "),
);

// --- 5. The drawings -------------------------------------------------------

/*
 * A drawing is worth drawing only if it is drawn.
 *
 * Every figure on this page is a real `<svg>` with a `viewBox` and a label, and each
 * carries at least one numbered callout disc — because a plate with numbered callouts
 * and no legend, or a legend and no callouts, is a drawing that has stopped explaining
 * itself. The counts are matched against each other rather than against a number: the
 * register says how many *figures* there are, not how many callouts each one needs.
 */
const svgCount = (html.match(/class="figure-art"/g) ?? []).length;
check(
  "every plate in the register is drawn as an SVG",
  svgCount === figures.length,
  `found ${svgCount} drawing(s) for ${figures.length} figure(s)`,
);

const discs = (html.match(/class="f-accent-ink"/g) ?? []).length;
const legendEntries = (html.match(/class="legend-n num"/g) ?? []).length;
check(
  "every numbered callout has a legend entry, and every entry a callout",
  discs > 0 && discs === legendEntries,
  `${discs} callout(s), ${legendEntries} legend entry(ies)`,
);

/*
 * Every drawing needs a text alternative, and the count is the check rather than a
 * search for a phrase.
 *
 * The first attempt looked for the literal string `aria-label="A drawing` — which the
 * turn figure's label does not begin with, because it is a timeline rather than a plan
 * of the window. That check would have failed on a perfectly labelled figure and passed
 * on one whose label was empty, which is two wrong answers from one bad assumption:
 * labels describe their own subject and there is no shared prefix to match on.
 */
const altLabels = (html.match(/<svg[^>]*figure-art[^>]*aria-label="/g) ?? []).length;
check(
  "every drawing carries a text alternative",
  altLabels === figures.length,
  `${altLabels} labelled drawing(s) for ${figures.length} figure(s)`,
);

// --- 6. The stylesheet that sets the document ------------------------------

/*
 * Classes and properties the document is composed from, checked in the *source*
 * stylesheet rather than in the built CSS. Tailwind's minifier is free to reorder and
 * rewrite these, and what is worth asserting is that they were authored at all.
 */
const WOVEN = [
  ["--measure", "the reading measure"],
  ["--page", "the ground, named once"],
  [".paper", "the text column"],
  [".wide", "the breakout width for plates and tables"],
  [".bar", "the running head"],
  [".bar-current", "the section name on a narrow viewport"],
  [".index", "the contents, fixed in the margin"],
  ["aria-current=", "the one mark for where you are"],
  [".rubric", "the part name above a heading"],
  [".figure-art", "a drawn plate"],
  [".figure-caption", "its caption"],
  [".figure-legend", "its numbered legend"],
  [".booktabs", "a table with three rules and no vertical ones"],
  [".hanging", "a numbered question or glossary entry"],
  [".contents-item", "an entry in the contents"],
  [".colophon", "the last page"],
];

for (const [needle, what] of WOVEN) {
  check(`the stylesheet defines ${what}`, css.includes(needle));
}

/*
 * The measure has to be a readable number of characters.
 *
 * 34rem at a 16px root is 544px, which is about 62–70 characters of Inter. The
 * previous version of this site set its prose in a ten-column shed that reached roughly
 * 1,200px at the cap — about 150 characters a line, or twice what anyone can read
 * without losing their place — and no check in the project noticed, because an
 * over-long line typechecks, builds and renders.
 */
const measureMatch = css.match(/--measure:\s*([\d.]+)rem/);
const measureRem = measureMatch ? Number(measureMatch[1]) : 0;
const measurePx = measureRem * 16;
check(
  "the measure is between 30 and 40rem",
  measureRem >= 30 && measureRem <= 40,
  `--measure is ${measureRem}rem (${measurePx}px), which is ${
    measurePx < 480 ? "too narrow to be worth the white space" : "wider than a readable line"
  }`,
);

// The column has to be the measure *plus* its gutters, because `max-width` includes
// padding under `border-box` — setting it to the measure alone would silently give a
// 56-character column, which reads as cramped with no visible cause.
check(
  "the text column adds its gutters to the measure rather than subtracting them",
  /\.paper\s*\{[^}]*max-width:\s*calc\(\s*var\(--measure\)\s*\+\s*2\s*\*\s*var\(--gutter\)/s.test(
    css,
  ),
  "the column no longer adds its own padding, so it is two gutters narrower than the measure",
);

// The index appears where there is a margin for it, and not before. `72rem` is
// 1152px, which is where 34rem of text, a 14rem index and two gutters first fit side
// by side.
check(
  "the margin index is gated at 72rem",
  /@media\s*\(min-width:\s*72rem\)[\s\S]{0,400}?\.index/.test(css),
  "the index is no longer gated behind a breakpoint, so it would overlap the text",
);
check(
  "the running head takes over the section name below that breakpoint",
  /\.bar-current\s*\{[\s\S]{0,300}?display:\s*none/.test(css) &&
    /min-width:\s*64rem\)[\s\S]{0,400}?\.bar-descriptor/.test(css),
  "the two position indicators are no longer gated in opposite directions",
);

// --- 7. What the document is deliberately not ------------------------------

/*
 * The deletions, asserted by name.
 *
 * The previous version of this page was built out of the application's glass: `panel`,
 * `panel-strong`, `pill` and `blob` on every block, twelve fixed warp hairlines behind
 * the text, a weft line that followed the scroll, and a grain overlay. All of it is
 * gone, and each one is a two-line change to add back — which is exactly the kind of
 * regression that comes back as "a bit of warmth".
 *
 * Deleting the checks instead of the elements would have been worse than useless: a
 * `querySelectorAll` that matches nothing returns an empty list and every assertion
 * over it passes, so the coverage would still have read as present.
 */
for (const gone of [
  ["repeating-linear-gradient", "the woven texture behind the page"],
  ["radial-gradient", "the drifting washes"],
  ["feTurbulence", "the film grain"],
  ["backdrop-filter", "the application's glass, on a document"],
  ["animation-name", "something that moves on its own"],
]) {
  check(`the stylesheet has no ${gone[1]}`, !css.includes(gone[0]));
}

for (const gone of [
  "src/components/chrome/warp.tsx",
  "src/components/chrome/shuttle.tsx",
  "src/components/chrome/spine.tsx",
  "src/components/chrome/draft-strip.tsx",
  "src/components/weave/pass.tsx",
  "src/components/weave/passes.ts",
  "src/components/pages/hero.tsx",
  "src/components/pages/dock-map.tsx",
  "src/components/pages/legal.tsx",
]) {
  check(`removed: ${gone}`, !existsSync(join(here, gone)));
}

for (const fingerprint of [
  "warp-field",
  "warp-line",
  "data-scene-phase",
  "draft-cell",
  "Do anything",
  "Pass 01",
  "panel-strong",
]) {
  check(
    `the built page does not contain "${fingerprint}"`,
    !visible.includes(fingerprint),
  );
}

// --- 8. The apparatus that is genuinely there -------------------------------

check(
  "the running head is rendered on every page",
  /<RunningHead\s*\/>/.test(read("src/app/layout.tsx")),
  "layout.tsx no longer renders the running head",
);
check(
  "the margin index is rendered with the document",
  /<MarginIndex\s*\/>/.test(pageSource),
  "page.tsx no longer renders the index, so the margin would be empty",
);
check(
  "the running head and the index read the same attribute",
  runningHead.includes("useCurrentSection") && indexComponent.includes("useCurrentSection"),
  "one of them has its own idea of where you are",
);
check(
  "the position mark is aria-current, and only that",
  indexComponent.includes('aria-current=') && css.includes('[aria-current="true"]'),
  "the mark is no longer carried by one attribute that both announces and draws it",
);

check(
  "a figure's caption is read from the register, not passed in",
  figureComponent.includes("figureTitle(n)") && !figureComponent.includes("title }: {"),
  "figure.tsx no longer reads its title from lib/document.ts, so a caption can drift",
);
check(
  "a table's caption is read from the register too",
  tableComponent.includes("tableTitle(n)"),
  "table.tsx no longer reads its caption from the register",
);
check(
  "the contents page lists every entry, from the one list",
  contentsComponent.includes("CONTENTS"),
  "contents.tsx no longer reads the contents list",
);

console.log("");
if (failed === 0) {
  console.log("VERDICT: the body renders what the contents claims, every plate is");
  console.log("drawn and captioned from the register, every weaving term is defined,");
  console.log("and the costume the rebuild removed has stayed removed.");
} else {
  console.log(`VERDICT: ${failed} check(s) failed.`);
  process.exit(1);
}
