import type { ReactNode } from "react";

/**
 * A block of specification lines: a term, its value, and a hairline between each.
 *
 * The `div` wrapper is what HTML permits inside a `dl` to keep a `dt` and its `dd`
 * together when they are not adjacent — which matters here because the rows are
 * grid items, and `dt`/`dd` as direct children of a grid would be laid out as
 * separate cells rather than as pairs.
 *
 * Two columns above `sm`, one below. A term like `Update signature` does not fit
 * beside its value on a phone, and squeezing it does not make the table denser, it
 * makes it unreadable.
 */
export function Spec({
  rows,
}: {
  rows: readonly { term: string; def: ReactNode }[];
}) {
  return (
    <dl className="spec">
      {rows.map((row) => (
        <div className="spec-row" key={row.term}>
          <dt className="spec-term">{row.term}</dt>
          <dd className="spec-def">{row.def}</dd>
        </div>
      ))}
    </dl>
  );
}
