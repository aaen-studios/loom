import type { ReactNode } from "react";
import { tableTitle } from "@/lib/document";

/**
 * A numbered table, in the booktabs style.
 *
 * Three rules: above the table, under the header, below the table. Nothing
 * vertical anywhere. Removing vertical rules and giving the rows room is most of
 * what separates a printed table from a spreadsheet pasted into a page, and the
 * second rule is what lets the eye find the header again after a long row without
 * a shaded band to do it with.
 *
 * The caption is a real `<caption>`, not a paragraph above the table, so a screen
 * reader announces it with the table rather than before it. It reads from the
 * register, like a figure's, so the contents page and the table cannot disagree.
 *
 * A table is wider than a phone before it is two columns wide with anything in
 * them, so it scrolls. `min-width` on the table rather than on the wrapper is what
 * makes the scroll appear only when the columns actually need it.
 */
export function Table({
  n,
  head,
  children,
}: {
  n: number;
  /** Column headings. The count must match the cells in each row. */
  head: readonly string[];
  children: ReactNode;
}) {
  return (
    <div className="table-scroll wide mt-7">
      <table className="booktabs">
        <caption>
          <b>Table {n}</b> — {tableTitle(n)}
        </caption>
        <thead>
          <tr>
            {head.map((label) => (
              <th key={label} scope="col">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}
