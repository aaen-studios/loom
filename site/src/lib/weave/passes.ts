/**
 * The structure of the front page, as a weaving draft.
 *
 * A draft is the plan for a cloth: what the warp is, how many picks the pattern
 * takes, and what happens on each one. This is the same thing for the landing
 * page — and it is the *only* place the page's shape is declared. The spine's
 * knots, the section headings and `verify-weave.mjs` all read from here, so the
 * structure on screen and the structure in the code cannot disagree.
 *
 * The vocabulary is real: a loom is warped (the threads are strung and tensioned)
 * before the first pick, each pass of the shuttle is a pick, the thread count is
 * the ends, the selvedge is the edge that stops it fraying, and heddles are what
 * lift threads to open a shed.
 *
 * ---------------------------------------------------------------------------
 * What is deliberately NOT here any more
 * ---------------------------------------------------------------------------
 *
 * A `shed` field used to live on each pass — which warp columns that pass's
 * content occupied — and it was a mistake, for two reasons.
 *
 * It was one number trying to place two different things. A pass has a narrow
 * *heading* and a body that is sometimes a paragraph and sometimes a
 * twelve-column grid of thread stubs, and no single span is right for both. So
 * the heading and the body now choose separately, in `weave/pass.tsx`.
 *
 * And it was declared in the wrong file. `shed` is a layout decision, and this
 * module's value is that it holds only what the *draft* knows: how many picks
 * there are, in what order, and what each one is for. A pass's width is not part
 * of the draft any more than the colour of the thread is.
 *
 * There was a matching bug, which is what finally settled it: every pass declared
 * `{ from: 2, span: 10 }`, so the field was a constant that looked like data.
 */
export interface Pass {
  /** The anchor, and what the spine's knot links to. */
  id: string;
  /** Position in the weave. One-based, because a draft counts picks from one. */
  pick: number;
  /** The structural element this pass is, in the loom's own vocabulary. */
  element: string;
  /** What the pass is for, in plain English. */
  purpose: string;
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
  },
  {
    id: "ends",
    pick: 2,
    element: "Ends",
    purpose: "The threads the cloth is made of",
  },
  {
    id: "pick",
    pick: 3,
    element: "Pick",
    purpose: "One pass of the shuttle, start to finish",
  },
  {
    id: "count",
    pick: 4,
    element: "Count",
    purpose: "How many ends, and who supplies them",
  },
  {
    id: "selvedge",
    pick: 5,
    element: "Selvedge",
    purpose: "The edge that stops it fraying",
  },
  {
    id: "heddles",
    pick: 6,
    element: "Heddles",
    purpose: "Questions, and what lifts to answer them",
  },
  {
    id: "off",
    pick: 7,
    element: "Off the loom",
    purpose: "Take it",
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

/** `03` — the pick number as a draft writes it. */
export function pickLabel(pick: number): string {
  return String(pick).padStart(2, "0");
}

/** The destinations the header's draft strip offers, in order. */
export const DESTINATIONS = [
  { href: "/", label: "Home", match: /^\/$/ },
  { href: "/download", label: "Download", match: /^\/download$/ },
  { href: "/privacy", label: "Privacy", match: /^\/privacy$/ },
  { href: "/terms", label: "Terms", match: /^\/terms$/ },
] as const;
