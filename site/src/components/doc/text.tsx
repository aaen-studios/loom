import type { ReactNode } from "react";
import { pad } from "@/lib/document";

/**
 * A paragraph.
 *
 * One component rather than a class repeated at every call site, because the
 * rhythm of a long document is set by its paragraph spacing and a single drifting
 * `mt-5` is visible on a page of two hundred paragraphs even though it is
 * invisible on a page of six.
 */
export function P({ children }: { children: ReactNode }) {
  return <p className="t-body mt-4">{children}</p>;
}

/**
 * A note: a closing remark on what has just been said.
 *
 * Set apart by a hairline rather than by a box. A bordered callout on a page of
 * continuous prose draws more attention than the thing it is annotating, which is
 * backwards — a note is by definition the smaller remark.
 */
export function Note({ children }: { children: ReactNode }) {
  return <p className="note">{children}</p>;
}

/** Inline code, for paths, commands and identifiers — and for nothing else. */
export function Code({ children }: { children: ReactNode }) {
  return <code className="code">{children}</code>;
}

/**
 * A shell session the reader is meant to run.
 *
 * `rounded-control` is the application's own utility, so the corner is concentric
 * with the block's border rather than a number chosen to look about right.
 */
export function Shell({ children }: { children: ReactNode }) {
  return <pre className="shell rounded-control">{children}</pre>;
}

/**
 * An entry with its number hanging in the margin.
 *
 * Used by the questions and by the glossary, because both are lists a reader
 * scans for one item rather than reads in order — and a hanging number is what
 * makes an item findable without moving the text off the manuscript line.
 *
 * The body is a separate grid cell from the heading, so a selection made inside
 * the answer cannot drag the number along with it, and a long answer can run to
 * several paragraphs without the number repeating.
 */
export function Hanging({
  n,
  term,
  children,
}: {
  /** A number, or a letter for a glossary entry that is a term. */
  n: number | string;
  /** The heading of the entry, when it has one. */
  term: string;
  children: ReactNode;
}) {
  return (
    <div className="hanging">
      <span className="hanging-n num">{typeof n === "number" ? pad(n) : n}</span>
      <span className="hanging-h">{term}</span>
      <div className="hanging-b">{children}</div>
    </div>
  );
}
