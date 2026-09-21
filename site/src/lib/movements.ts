/**
 * The page's structure, in one place.
 *
 * Six movements, and the order is the argument rather than a convention. The page is not a features
 * list with headings: each movement states one thing with a generative figure behind it, and the
 * sequence runs from the largest abstraction to the smallest — a field of threads, a lattice of
 * parts, the interference where two meet, a thing narrowed to one point, and finally the two places
 * where a reader has to be given facts rather than a feeling.
 *
 * ---------------------------------------------------------------------------
 * Why this module exists
 * ---------------------------------------------------------------------------
 *
 * It was written because a check failed, and the failure was worth fixing rather than relaxing.
 *
 * The nav offered three destinations and the footer one anchor, which between them left five of the
 * six movements reachable only by scrolling. The accessibility pass counts in-page links against
 * rendered ids, and it reported three where it expected eight — which is exactly the shape of thing
 * that gets "fixed" by lowering the number. The number was right: a page whose movements cannot be
 * linked to is a page whose movements cannot be shared, cited or returned to.
 *
 * So the footer lists all six, and both readers take the list from here. The nav keeps its three,
 * because a header with six links is a header nobody reads — but the nav *chooses* its three from
 * this list rather than writing them out, so the two can never name different movements.
 */
export interface MovementEntry {
  /** The anchor. Must match the id the rendered `<section>` ends up with. */
  id: string;
  /** One-based, for the mono index beside the heading. */
  n: number;
  /**
   * The label for a list of links. Short, and the same word the movement's own eyebrow uses, so a
   * reader who arrives from the footer recognises where they have landed.
   */
  label: string;
}

export const MOVEMENTS: readonly MovementEntry[] = [
  { id: "weave", n: 1, label: "The weave" },
  { id: "parts", n: 2, label: "Parts" },
  { id: "meeting", n: 3, label: "Where they meet" },
  { id: "narrow", n: 4, label: "Narrowing" },
  { id: "ground", n: 5, label: "Ground truth" },
  { id: "install", n: 6, label: "Install" },
] as const;

/**
 * A movement by id.
 *
 * Throws rather than returning `undefined`, because every caller passes a literal from the list
 * above — so a miss is a typo in a component, which is worth failing the build over rather than
 * handling at runtime with a fallback heading.
 */
export function movementById(id: string): MovementEntry {
  const movement = MOVEMENTS.find((entry) => entry.id === id);
  if (!movement) throw new Error(`no movement named "${id}" in lib/movements.ts`);
  return movement;
}

/**
 * The three the header offers.
 *
 * Chosen by id from the list above rather than written out, so the header cannot point at a movement
 * that does not exist. Three is the number that fits beside a wordmark, a theme toggle and a download
 * button on a laptop without wrapping, and the three that are not here are one scroll away — which on
 * a single-page site is not a hardship.
 */
export const NAV_MOVEMENTS = MOVEMENTS.filter((movement) =>
  ["weave", "parts", "install"].includes(movement.id),
);
