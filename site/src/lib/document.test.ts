import { describe, expect, test } from "bun:test";
import {
  APPENDICES,
  CONTENTS,
  FIGURES,
  LOOM_TERMS,
  SECTIONS,
  TABLES,
  figureTitle,
  pad,
  tableTitle,
} from "./document";

/**
 * The integrity of the document's skeleton.
 *
 * These are cheap assertions over plain data, and they are here rather than only
 * in `verify-manual.mjs` because that script needs a completed build to say
 * anything, and the failures below are authoring mistakes: a section renumbered
 * without its neighbours, a figure used twice, a rubric that names a term the
 * glossary does not define.
 *
 * The last of those is the one that matters most, and it is the reason this file
 * exists at all. The document's whole arrangement rests on a bargain — the weaving
 * vocabulary is allowed on the page only because the document defines it — and a
 * rubric with no glossary entry breaks that bargain silently. Nothing would look
 * wrong; one word would simply be unexplained.
 */

describe("the contents", () => {
  test("numbers the body from one, in order, with no gaps", () => {
    expect(SECTIONS.map((section) => section.number)).toEqual(
      SECTIONS.map((_, index) => String(index + 1)),
    );
  });

  test("letters the appendices, after the body", () => {
    expect(APPENDICES.map((entry) => entry.number)).toEqual(["A", "B"]);
    expect(CONTENTS.length).toBe(SECTIONS.length + APPENDICES.length);
  });

  test("gives every entry an id no other entry uses", () => {
    const ids = CONTENTS.map((entry) => entry.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  test("gives every entry a title a reader could navigate by", () => {
    // A rubric is not a title. The failure the previous site had was headings like
    // "Nine threads, held under tension", which name the metaphor and not the
    // subject — so the check is that a title is short and states a thing.
    for (const entry of CONTENTS) {
      expect(entry.title.length).toBeGreaterThan(4);
      expect(entry.title.length).toBeLessThan(48);
      // Sentence case, not Title Case: a manual's headings are sentences.
      const words = entry.title.split(" ");
      const overCapitalised = words.slice(1).filter((word) => /^[A-Z][a-z]/.test(word));
      expect(overCapitalised).toEqual([]);
    }
  });

  test("uses every weaving term as a rubric, and no rubric without one", () => {
    // The bargain, asserted in both directions. A term in `LOOM_TERMS` that has
    // been dropped from the document is dead vocabulary; a rubric that is not a
    // term is either plain English (fine, and what `Instrument` and `Reference`
    // are) or a loom word nobody defined (not fine).
    const rubrics = SECTIONS.map((section) => section.rubric);
    for (const term of LOOM_TERMS) {
      expect(rubrics).toContain(term);
    }
    // Every rubric is either one of the terms or one of the two plain words the
    // document uses for parts that have no honest loom name.
    const allowed = new Set<string>([...LOOM_TERMS, "Instrument", "Reference"]);
    for (const rubric of rubrics) {
      expect(allowed.has(rubric)).toBe(true);
    }
  });
});

describe("the registers", () => {
  test("numbers figures and tables from one, without repetition", () => {
    expect(FIGURES.map((figure) => figure.n)).toEqual(
      FIGURES.map((_, index) => index + 1),
    );
    expect(TABLES.map((table) => table.n)).toEqual(TABLES.map((_, index) => index + 1));
  });

  test("attaches every plate to a section that exists", () => {
    // A figure whose `section` is a stale id would be counted by the verifier
    // against a section that no longer renders it, and the check would pass while
    // the register was wrong.
    const ids = new Set(CONTENTS.map((entry) => entry.id));
    for (const plate of [...FIGURES, ...TABLES]) {
      expect(ids.has(plate.section)).toBe(true);
    }
  });

  test("gives each plate a caption that reads as a caption", () => {
    for (const plate of [...FIGURES, ...TABLES]) {
      expect(plate.title.length).toBeGreaterThan(6);
      // No trailing full stop: the caption is completed by the note beneath it.
      expect(plate.title.endsWith(".")).toBe(false);
      expect(plate.title).not.toBe("Untitled");
    }
  });

  test("resolves a plate by number, and refuses one that is not there", () => {
    expect(figureTitle(1)).toBe("The window, with every zone shut");
    expect(tableTitle(4)).toBe("Keyboard shortcuts");
    expect(() => figureTitle(99)).toThrow(/no figure numbered 99/);
    expect(() => tableTitle(99)).toThrow(/no table numbered 99/);
  });

  test("pads an index the way a printed manual does", () => {
    expect(pad(1)).toBe("01");
    expect(pad(10)).toBe("10");
  });
});
