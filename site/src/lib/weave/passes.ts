/**
 * The structure of the front page, as a weaving draft.
 *
 * A draft is the plan for a cloth: what the warp is, how many picks the pattern
 * takes, and what happens on each one. This is the same thing for the landing
 * page — and it is the *only* place the page's shape is declared. The nav strip,
 * the section headings, the weft's travel and `verify-weave.mjs` all read from
 * here, so the structure on screen and the structure in the code cannot disagree.
 *
 * The vocabulary is real: a loom is warped (the threads are strung and tensioned)
 * before the first pick, each pass of the shuttle is a pick, the thread count is
 * the ends, the selvedge is the edge that stops it fraying, and heddles are what
 * lift threads to open a shed.
 */
export interface Pass {
  /** The anchor, and what the draft strip links to. */
  id: string;
  /** Position in the weave. One-based, because a draft counts picks from one. */
  pick: number;
  /** The structural element this pass is, in the loom's own vocabulary. */
  element: string;
  /** What the pass is for, in plain English. */
  purpose: string;
  /** Which warp columns the pass's content occupies, as a shed. */
  shed: { from: number; span: number };
}

/**
 * Seven picks.
 *
 * The order is the argument, in the same way the order of a real draft is the
 * pattern: the frame is declared, then the material it is made of, then one turn
 * demonstrated end to end, then the thread count, then the edge that holds it,
 * then what you would ask, then the finished thing itself.
 */
export const PASSES: readonly Pass[] = [
  {
    id: "warp",
    pick: 1,
    element: "Warp",
    purpose: "The frame, and the machine running inside it",
    shed: { from: 2, span: 10 },
  },
  {
    id: "ends",
    pick: 2,
    element: "Ends",
    purpose: "The threads the cloth is made of",
    shed: { from: 2, span: 10 },
  },
  {
    id: "pick",
    pick: 3,
    element: "Pick",
    purpose: "One pass of the shuttle, start to finish",
    shed: { from: 2, span: 10 },
  },
  {
    id: "count",
    pick: 4,
    element: "Count",
    purpose: "How many ends, and who supplies them",
    shed: { from: 2, span: 10 },
  },
  {
    id: "selvedge",
    pick: 5,
    element: "Selvedge",
    purpose: "The edge that stops it fraying",
    shed: { from: 2, span: 10 },
  },
  {
    id: "heddles",
    pick: 6,
    element: "Heddles",
    purpose: "Questions, and what lifts to answer them",
    shed: { from: 2, span: 10 },
  },
  {
    id: "off",
    pick: 7,
    element: "Off the loom",
    purpose: "Take it",
    shed: { from: 2, span: 10 },
  },
] as const;

/**
 * A pass by id.
 *
 * Throws rather than returning `undefined`: every caller passes a literal from
 * the list above, so a miss is a typo in this file rather than a runtime state
 * worth handling.
 */
export function passById(id: string): Pass {
  const pass = PASSES.find((entry) => entry.id === id);
  if (!pass) throw new Error(`no pass named "${id}" in the draft`);
  return pass;
}

/** The destinations the header's draft strip offers, in order. */
export const DESTINATIONS = [
  { href: "/", label: "Home", match: /^\/$/ },
  { href: "/download", label: "Download", match: /^\/download$/ },
  { href: "/privacy", label: "Privacy", match: /^\/privacy$/ },
  { href: "/terms", label: "Terms", match: /^\/terms$/ },
] as const;
