import type { ReactNode } from "react";

/**
 * The shell the two legal pages share.
 *
 * The woven field, the threads, the knots — all of it is in `layout.tsx` and stays
 * put. What changes here is the *measure*: these are dense prose rather than
 * marketing, so they get a narrower column and a larger line height, and a heading
 * rhythm that reads at length.
 *
 * Kept in one component because privacy and terms drifting apart in width or
 * heading size is exactly the kind of small inconsistency that makes a project look
 * careless without anyone being able to point at what is wrong.
 */
export function Legal({
  title,
  summary,
  children,
}: {
  title: string;
  summary: string;
  children: ReactNode;
}) {
  return (
    // A `div`, not a `main`: `layout.tsx` already renders one, and a `<main>` inside
    // a `<main>` is invalid and makes the landmark ambiguous to a screen reader.
    <div className="px-4 pt-12 pb-4 sm:px-6 sm:pt-16">
      <div className="mx-auto max-w-2xl">
        <h1 className="text-[30px] leading-tight font-medium tracking-tight sm:text-[36px]">
          {title}
        </h1>
        <p className="text-soft mt-4 text-[15px] leading-6">{summary}</p>
        <div className="mt-2">{children}</div>
      </div>
    </div>
  );
}

/**
 * One titled section of a legal page.
 *
 * `h2`, and the pages' `h1` is in `Legal` above — so the outline is
 * `h1 → h2 → h3` throughout. `verify-a11y.mjs` fails the build on a skipped level,
 * and on a page made entirely of prose that is an easy mistake to make.
 */
export function Clause({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mt-8">
      <h2 className="text-[17px] font-medium">{title}</h2>
      <div className="text-soft mt-3 space-y-3 text-[13.5px] leading-[1.7]">{children}</div>
    </section>
  );
}

export function Code({ children }: { children: ReactNode }) {
  return (
    <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
      {children}
    </code>
  );
}
