import Link from "next/link";
import { DOWNLOAD, REPO, SITE } from "@/lib/site";
import { LoomMark } from "@/components/loom-mark";

/**
 * The footer, as the selvedge.
 *
 * A selvedge is the tightly-woven border down the edge of a cloth that stops it
 * fraying — the part that holds the rest together. This is the band at the bottom
 * of every page, drawn denser than the cloth above it, and it carries the things
 * that do that job: where the source lives, what the licence is, and what the app
 * does and does not send.
 *
 * The column headings are `h2`, not `h3`. Not a stylistic choice: they are
 * top-level sections of the footer, and on a page with no preceding `h2` — the 404
 * has only an `h1` — an `h3` makes the document outline jump a level.
 * `verify-a11y.mjs` reads the built HTML and fails on exactly that.
 */
export function Selvedge() {
  return (
    <footer className="mt-24 px-4 pb-10 sm:px-6">
      <div className="mx-auto max-w-5xl">
        <div className="selvedge pt-8">
          <div className="flex flex-col gap-8 sm:flex-row sm:justify-between">
            <div className="max-w-xs">
              <div className="flex items-center gap-2">
                <LoomMark size={18} className="text-[var(--accent)]" />
                <span className="text-[14.5px] font-medium tracking-tight">Loom</span>
              </div>
              <p className="text-faint mt-3 text-[13px] leading-5">
                A desktop app for AI chat and agents. Made by{" "}
                <a
                  href={SITE.publisherUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="text-[var(--accent)] hover:underline"
                >
                  {SITE.publisher}
                </a>
                .
              </p>
            </div>

            <div className="grid grid-cols-2 gap-x-12 gap-y-6 sm:grid-cols-3">
              <Column
                title="The cloth"
                links={[
                  { href: DOWNLOAD.publicPath, label: "Download" },
                  { href: "/#ends", label: "What it does" },
                  { href: "/#pick", label: "A turn" },
                  { href: "/#count", label: "Providers" },
                  { href: "/#heddles", label: "Questions" },
                ]}
              />
              <Column
                title="The source"
                links={[
                  { href: REPO.url, label: "Repository", external: true },
                  { href: REPO.releasesUrl, label: "Releases", external: true },
                  { href: SITE.licenseUrl, label: "Licence", external: true },
                  { href: REPO.issuesUrl, label: "Issues", external: true },
                ]}
              />
              <Column
                title="The edge"
                links={[
                  { href: "/privacy", label: "Privacy" },
                  { href: "/terms", label: "Terms" },
                ]}
              />
            </div>
          </div>

          <div className="text-faint mt-10 flex flex-col gap-2 border-t border-[var(--glass-border)] pt-6 text-[12.5px] sm:flex-row sm:items-center sm:justify-between">
            <p>
              © {new Date().getFullYear()} {SITE.publisher}. Free to use.
            </p>
            <p className="flex flex-wrap items-center gap-x-3 gap-y-1">
              <span>
                {SITE.license} licensed ·{" "}
                <span className="text-soft">{DOWNLOAD.requirements}</span>
              </span>
              <span>No telemetry. No account.</span>
            </p>
          </div>
        </div>
      </div>
    </footer>
  );
}

function Column({
  title,
  links,
}: {
  title: string;
  links: { href: string; label: string; external?: boolean }[];
}) {
  return (
    <div>
      <h2 className="text-faint text-[11px] font-semibold tracking-[0.09em] uppercase">
        {title}
      </h2>
      <ul className="mt-3 space-y-2">
        {links.map((link) => (
          <li key={link.href}>
            {link.external ? (
              <a
                href={link.href}
                target="_blank"
                rel="noreferrer"
                className="text-soft text-[13px] hover:text-[var(--ink)]"
              >
                {link.label}
              </a>
            ) : (
              <Link
                href={link.href}
                className="text-soft text-[13px] hover:text-[var(--ink)]"
              >
                {link.label}
              </Link>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
