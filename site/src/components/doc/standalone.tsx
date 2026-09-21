import type { ReactNode } from "react";

/**
 * The shell the pages that are *not* the manual share.
 *
 * `/download`, `/privacy` and `/terms` are three documents in the manual's clothes
 * and outside its numbering, and each says so in its own first line. They exist
 * because they are things people link to, quote and arrive at directly — a privacy
 * page has to be citable, and the download page is the one page that must state the
 * current release truthfully, which needs a server.
 *
 * Everything about them except the front matter is the manual's: the same paper, the
 * same measure, the same type scale, the same rules. A site whose legal pages are set
 * differently from its prose is a site where nobody read the legal pages, and this is
 * the arrangement that stops that from happening by accident.
 *
 * There is deliberately no `Section` here. `Section` takes an `Entry` from
 * `lib/document.ts`, and an `Entry` is a promise that the thing it names is in the
 * contents list — which these are not. A page outside the manual uses `Sub` for its
 * headings and nothing else.
 */
export function Standalone({
  title,
  standfirst,
  children,
}: {
  title: string;
  standfirst: string;
  children: ReactNode;
}) {
  return (
    <div className="paper pt-12 sm:pt-16">
      <p className="rubric">Also on this site</p>
      <h1 className="t-section mt-4">{title}</h1>
      <p className="t-standfirst mt-4">{standfirst}</p>
      <div className="section-rule mt-8" />
      <div className="mt-6">{children}</div>
    </div>
  );
}
