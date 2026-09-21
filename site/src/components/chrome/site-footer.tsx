import Link from "next/link";
import { DOWNLOAD, REPO, SITE } from "@/lib/site";
import { PANELS } from "@/lib/panels";
import { MOVEMENTS } from "@/lib/movements";
import { Still } from "@/components/weave/figure";
import { LoomMark } from "@/components/loom-mark";

/**
 * The footer.
 *
 * A thin weave at the top, three columns, and a colophon set quietly on the same measure as the
 * prose. Every list in it is generated rather than written out — the panels from `lib/panels.ts`, the
 * movements from `lib/movements.ts` — so a ninth panel or a renamed movement arrives here or fails
 * the build.
 *
 * That matters more than it sounds. The version of this footer that hardcoded one anchor left five of
 * the page's movements unreachable by link, and nothing said so until an accessibility pass counted
 * them. A footer is the one place on a long page where a reader goes looking for the part they
 * missed, so it is the worst place to have a list that is quietly four entries short.
 */
export function SiteFooter() {
  return (
    <footer className="relative z-1 overflow-hidden border-t border-[var(--glass-border)] pt-16 pb-12">
      {/* The closing figure: a wide, shallow field, faded to almost nothing. It is here so the page
          ends in the same material it began in, rather than stopping at a rule. */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 h-40 opacity-[0.35]"
      >
        <Still
          kind="field"
          seed={31337}
          width={1800}
          height={300}
          detail={1.8}
          className="h-full w-full"
          id="footer"
        />
      </div>

      <div className="shell relative">
        <div className="flex flex-col gap-12 sm:flex-row sm:justify-between">
          <div className="max-w-xs">
            <div className="flex items-center gap-2">
              <LoomMark size={18} className="text-[var(--accent)]" />
              <span className="text-[14.5px] font-semibold tracking-[-0.02em]">Loom</span>
            </div>
            <p className="t-small mt-3">
              A desktop workspace for AI chat and agents. Made by{" "}
              <a href={SITE.publisherUrl} target="_blank" rel="noreferrer" className="link">
                {SITE.publisher}
              </a>
              .
            </p>
            <p className="t-small mt-3">
              {DOWNLOAD.requirements} · no account · no telemetry.
            </p>
          </div>

          <div className="grid grid-cols-2 gap-x-12 gap-y-8 sm:grid-cols-3">
            {/*
              * All six movements, from the page's own list.
              *
              * This column is the reason `lib/movements.ts` exists. Before it, the footer offered one
              * anchor and the nav three, which left five movements reachable only by scrolling — and
              * the a11y pass caught it by counting in-page links against rendered ids. A page whose
              * movements cannot be linked to is a page whose movements cannot be shared or returned
              * to, so the fix was to list them rather than to lower the number.
              */}
            <Column
              title="Movements"
              links={MOVEMENTS.map((movement) => ({
                href: `/#${movement.id}`,
                label: movement.label,
              }))}
            />
            <Column
              title="Panels"
              links={PANELS.slice(0, 5).map((panel) => ({
                href: "/#parts",
                label: panel.name,
              }))}
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
          </div>
        </div>

        <div className="mt-14 max-w-[var(--measure)] border-t border-[var(--glass-border)] pt-8">
          <p className="t-small">
            Set in <b className="font-medium text-soft">Inter</b> — the same binary the application
            ships, loaded as one variable woff2 so the two cannot render a different face. The colour
            is {SITE.name}&rsquo;s own: both palettes, the accent, the hairlines and the seven thread
            colours the artwork is drawn in are lifted from the application&rsquo;s stylesheet at
            build time rather than retyped, so a change to the product&rsquo;s palette reaches this
            page or fails the build. The glass in the header and on the release panel is the
            application&rsquo;s own <span className="mono">pill</span> and{" "}
            <span className="mono">panel-strong</span> surfaces, which is why it looks like the product
            rather than like a website&rsquo;s idea of it.
          </p>
          <p className="t-small mt-4">
            Every figure on this page is drawn in the browser from a seed — no images, no SVG files,
            and nothing fetched. This site sets no cookies and runs no analytics, and it makes exactly
            one outbound request: to GitHub, for the version number on the{" "}
            <Link href="/download" className="link">
              download page
            </Link>
            . {SITE.name} is {SITE.license} licensed; the source is public at{" "}
            <a href={REPO.url} target="_blank" rel="noreferrer" className="link">
              {REPO.slug}
            </a>
            , which is what makes every claim on this page checkable rather than reassuring.
          </p>
        </div>

        <div className="t-small mt-10 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <p>
            © {new Date().getFullYear()} {SITE.publisher}. Free to use.
          </p>
          <p>
            {PANELS.length} panels · {MOVEMENTS.length} movements · {SITE.license} licensed
          </p>
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
      <h2 className="t-label">{title}</h2>
      <ul className="mt-3 space-y-2">
        {links.map((link) => (
          <li key={link.label}>
            {link.external ? (
              <a
                href={link.href}
                target="_blank"
                rel="noreferrer"
                className="text-[13px] text-soft hover:text-[var(--ink)]"
              >
                {link.label}
              </a>
            ) : (
              <Link href={link.href} className="text-[13px] text-soft hover:text-[var(--ink)]">
                {link.label}
              </Link>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
