import type { ReactNode } from "react";

/**
 * The shell the three pages outside the product share.
 *
 * `/download`, `/privacy` and `/terms` are separate documents: one has to state the current release
 * truthfully and therefore needs a server, and the other two exist because they get *cited* — linked
 * from issue threads, quoted in permission requests — which means they want a page whose whole
 * subject is the answer rather than a movement four thousand words into the front page.
 *
 * They are painted the same ground and set in the same type as the product page, because a site whose
 * legal pages are styled differently from its prose is a site where nobody read the legal pages. But
 * their headings are `h2`, not movements: they are not part of the page's numbered argument, and
 * giving them a number would be claiming a structure they do not have.
 *
 * (This file lives beside the header and footer rather than in `ui/` because it is page *chrome* —
 * the thing a route wraps itself in — and not one of the primitives a movement is built from. The
 * distinction is the one that decides where a new component goes: if a movement would use it, it is a
 * primitive; if only a page would, it is chrome.)
 */
export function Standalone({
  label,
  title,
  standfirst,
  children,
}: {
  label: string;
  title: string;
  standfirst: string;
  children: ReactNode;
}) {
  return (
    <div className="shell relative z-1 pt-16 pb-20">
      <p className="t-label">{label}</p>
      <h1 className="t-h2 mt-4 max-w-[20ch]">{title}</h1>
      <p className="t-lede measure mt-4">{standfirst}</p>
      <div className="mt-8 border-t border-[var(--glass-border)]" />
      <div className="mt-2">{children}</div>
    </div>
  );
}

/** A top-level heading on a page outside the product. Always `h2`, under `Standalone`'s `h1`. */
export function Heading({ children }: { children: ReactNode }) {
  return <h2 className="t-h3 mt-10">{children}</h2>;
}
