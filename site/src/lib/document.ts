/**
 * The document's skeleton: its contents, its figures and its tables.
 *
 * A printed manual opens with a contents list and a register of its plates, and
 * every one of them is a claim about what follows. This module is that claim, in
 * the only form the code can hold it: one list, read by the contents page, by the
 * margin index, by the section headings and by `verify-manual.mjs` — so the
 * structure on screen and the structure in the code cannot disagree.
 *
 * ---------------------------------------------------------------------------
 * Why this replaced a "draft"
 * ---------------------------------------------------------------------------
 *
 * The previous version had `PASSES`: seven numbered *picks of a weaving draft*,
 * each with an id, a number, a structural element and a shed. It was the same
 * shape of data as this file and it was the thing that made the site unreadable,
 * because the metaphor was the table of contents. A reader had to learn what a
 * pick was before they could find the pricing.
 *
 * A contents list is the same discipline without the costume: an ordered list of
 * what the document contains, which is exactly what a reader needs and exactly
 * what a table of contents has always been. The weaving words survive — but as
 * the *rubrics* on those entries, glossed in the glossary, where they are
 * terminology rather than navigation.
 *
 * Two consequences worth stating, since they are what the verifier enforces:
 *
 *   1. `FIGURES` and `TABLES` are registers, not annotations. A figure's caption
 *      text is read from here, so a caption cannot drift from the contents.
 *   2. A section's `rubric` must be a term the glossary defines. That is the
 *      whole bargain with the vocabulary: it is only allowed on the page if the
 *      document says what it means.
 */

/** A chapter of the manual. */
export interface Entry {
  /** The anchor. Also what the running head and the margin index read. */
  id: string;
  /** `1`…`8`, or `A`/`B` for the appendices. */
  number: string;
  /**
   * The part name, in the loom's vocabulary.
   *
   * `Warp`, `Pick`, `Ends`, `Count`, `Selvedge`, `Heddles`, `Off the loom` — and
   * for the first section and the appendices, a plain word, because there is no
   * honest loom term for "what this is" or for a list of shortcuts, and inventing
   * one would be the costume returning.
   */
  rubric: string;
  /** The heading, in plain English. This is what a reader navigates by. */
  title: string;
}

/** The body, in order. Numbers are contiguous from one. */
export const SECTIONS: readonly Entry[] = [
  { id: "instrument", number: "1", rubric: "Instrument", title: "What this is" },
  { id: "warp", number: "2", rubric: "Warp", title: "The window it runs in" },
  { id: "pick", number: "3", rubric: "Pick", title: "One turn of the agent" },
  { id: "ends", number: "4", rubric: "Ends", title: "The threads it is made of" },
  { id: "count", number: "5", rubric: "Count", title: "Models, and who supplies them" },
  { id: "selvedge", number: "6", rubric: "Selvedge", title: "The edge that holds" },
  { id: "heddles", number: "7", rubric: "Heddles", title: "Questions that are worth asking" },
  { id: "off", number: "8", rubric: "Off the loom", title: "Installation" },
] as const;

/** The reference material, lettered. */
export const APPENDICES: readonly Entry[] = [
  { id: "shortcuts", number: "A", rubric: "Reference", title: "Keyboard shortcuts" },
  { id: "glossary", number: "B", rubric: "Reference", title: "Glossary" },
] as const;

/** Everything the contents page lists, in reading order. */
export const CONTENTS: readonly Entry[] = [...SECTIONS, ...APPENDICES];

/**
 * The weaving terms this document uses as part names.
 *
 * Held here, and not only in the glossary component, because
 * `verify-manual.mjs` asserts that each one is *both* used as a section rubric
 * and defined in the glossary. A term on the page with no definition is the
 * failure mode this whole arrangement exists to prevent: the reader is made to
 * decode something, which is precisely what the previous version did to them.
 */
export const LOOM_TERMS = [
  "Warp",
  "Pick",
  "Ends",
  "Count",
  "Selvedge",
  "Heddles",
  "Off the loom",
] as const;

/** A drawn plate. */
export interface Plate {
  n: number;
  /** The caption, set after `Figure N —`. */
  title: string;
  /** The section that holds it, by id. */
  section: string;
}

/**
 * Figures.
 *
 * Three, and each is a drawing rather than a picture of a window: the zones and
 * the moves a panel can make, the order the parts of a turn arrive in. There is no
 * screenshot anywhere on this page, and `verify-manual.mjs` asserts that the
 * number of rendered captions equals the length of this list.
 */
export const FIGURES: readonly Plate[] = [
  {
    n: 1,
    title: "The window, with every zone shut",
    section: "warp",
  },
  {
    n: 2,
    title: "Everywhere a tab can be dropped",
    section: "warp",
  },
  {
    n: 3,
    title: "One turn, in the order the parts arrive",
    section: "pick",
  },
] as const;

/** A numbered table. */
export interface Chart {
  n: number;
  /** The caption, set after `Table N —`. */
  title: string;
  section: string;
}

export const TABLES: readonly Chart[] = [
  { n: 1, title: "The four agent modes", section: "pick" },
  { n: 2, title: "Permission modes, and what they allow", section: "selvedge" },
  { n: 3, title: "Where everything is kept", section: "selvedge" },
  { n: 4, title: "Keyboard shortcuts", section: "shortcuts" },
] as const;

/**
 * The caption title for a figure.
 *
 * Throws rather than returning `undefined`: every caller passes a literal from the
 * registers above, so a miss is a typo in this file rather than a runtime state
 * worth handling.
 */
export function figureTitle(n: number): string {
  const plate = FIGURES.find((entry) => entry.n === n);
  if (!plate) throw new Error(`no figure numbered ${n} in the register`);
  return plate.title;
}

/** The caption title for a table. */
export function tableTitle(n: number): string {
  const chart = TABLES.find((entry) => entry.n === n);
  if (!chart) throw new Error(`no table numbered ${n} in the register`);
  return chart.title;
}

/** `01` — a two-digit index, as a printed manual numbers its entries. */
export function pad(n: number): string {
  return String(n).padStart(2, "0");
}
