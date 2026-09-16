import type { ReactNode } from "react";

/**
 * The shell the two legal pages share.
 *
 * They are dense prose rather than marketing, so the measure is narrower than
 * the landing page and the type is set for reading at length. Kept in one place
 * because privacy and terms drifting apart in width or heading size is exactly
 * the kind of small inconsistency that makes a project look careless.
 */
export function LegalLayout({
  title,
  summary,
  children,
}: {
  title: string;
  summary: string;
  children: ReactNode;
}) {
  return (
    <main className="px-4 pt-12 pb-4 sm:px-6 sm:pt-16">
      <div className="mx-auto max-w-2xl">
        <h1 className="text-[30px] leading-tight font-medium tracking-tight sm:text-[36px]">
          {title}
        </h1>
        <p className="text-soft mt-4 text-[15px] leading-6">{summary}</p>
        <div className="mt-2">{children}</div>
      </div>
    </main>
  );
}
