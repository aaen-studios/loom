import Link from "next/link";
import { DOWNLOAD, REPO, SITE } from "@/lib/site";
import { CONTENTS } from "@/lib/document";

/**
 * The colophon.
 *
 * The last page of a book, and the most human thing on this one. It says how the
 * document is set, what it is licensed under, and — the part that matters — what it
 * does not do. A page that states these things plainly cannot have been assembled
 * by a generator that had nothing to say, which is the only reliable defence
 * against a page reading as though one did.
 *
 * ---------------------------------------------------------------------------
 * Why it is worth the space
 * ---------------------------------------------------------------------------
 *
 * The three things below are unusually checkable for a website. The typography is
 * the application's own two palettes and its own stylesheet; the licence is MIT and
 * the source is public; the absence of analytics is verifiable by opening the
 * network tab. A reader who has been given four paragraphs of that in the privacy
 * section will find it convincing, and a reader who suspects a landing page of
 * being generated will find it the only part worth reading.
 *
 * It is also where the "set in" tradition earns its keep: naming the typeface and
 * the sizes is a small, unmistakable signal that a person made decisions.
 */
export function Colophon() {
  return (
    <section id="colophon" className="colophon" aria-label="Colophon">
      <p className="label">Colophon</p>

      <div className="mt-4">
        <p>
          Set in <b>Inter</b> — the same binary the application ships, loaded as a
          single variable woff2 with the metric overrides inlined, so the two cannot
          render a subtly different face. Headings are tracked tight at 600; body copy
          is 16px on a 26.9px leading in a 34rem measure, which is about 68
          characters. Figures, tables and code are set in the system monospace, at
          a smaller size and a tighter leading, because the only job that face has
          here is to be visibly not prose.
        </p>

        <p>
          The colours are <b>{SITE.name}&rsquo;s own</b> — both palettes come from the
          application&rsquo;s stylesheet, extracted at build time rather than retyped,
          so a change to the product&rsquo;s accent or to either ground reaches this
          page or fails the build. The dark ground is the colour the application
          paints into its window before its bundle loads; the light ground is the
          same, for the light theme. This page sets no cookies and runs no analytics,
          and it makes exactly one outbound request — to GitHub, for the version
          number on the{" "}
          <Link href="/download" className="text-[var(--accent)] hover:underline">
            download page
          </Link>
          .
        </p>

        <p>
          <b>{SITE.name}</b> is {SITE.license} licensed and published by{" "}
          <a
            href={SITE.publisherUrl}
            target="_blank"
            rel="noreferrer"
            className="text-[var(--accent)] hover:underline"
          >
            {SITE.publisher}
          </a>
          . The source is public at{" "}
          <a
            href={REPO.url}
            target="_blank"
            rel="noreferrer"
            className="text-[var(--accent)] hover:underline"
          >
            {REPO.slug}
          </a>
          , which is what makes every claim on this page checkable rather than
          reassuring. The seven weaving terms in the glossary are part of the
          application&rsquo;s own vocabulary, not this document&rsquo;s decoration:
          several panels in the product are named the same way, and{" "}
          <Link href="#glossary" className="text-[var(--accent)] hover:underline">
            Appendix B
          </Link>{" "}
          is where that stops being a puzzle.
        </p>

        <p className="text-faint">
          {CONTENTS.length} sections and appendices, one action, no cookie banner.
          This page is at{" "}
          <a
            href={SITE.url}
            className="text-[var(--accent)] hover:underline"
          >
            {SITE.url.replace("https://", "")}
          </a>
          ; the installer is at{" "}
          <Link
            href={DOWNLOAD.publicPath}
            className="text-[var(--accent)] hover:underline"
          >
            {DOWNLOAD.publicPath}
          </Link>
          , which always resolves to the current release.
        </p>
      </div>
    </section>
  );
}
