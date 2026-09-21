import Link from "next/link";
import { DOWNLOAD, REPO } from "@/lib/site";
import { formatBytes, getRelease } from "@/lib/release";
import { Code, Note, P } from "@/components/ui/primitives";

/**
 * The release panel.
 *
 * A server component, so the version, the size and the payload hash are in the HTML rather than
 * arriving after hydration. That matters more here than anywhere else on the site: a download page
 * whose version number appears a beat late reads as broken, and the hash is half the reason the page
 * exists rather than being a button at the end of the front page.
 *
 * ---------------------------------------------------------------------------
 * Three release states, drawn differently
 * ---------------------------------------------------------------------------
 *
 * `live`, `none` and `unreachable` are not interchangeable, and the failure this guards against is a
 * real one: telling a visitor that something is *broken* when the truth is that the project has not
 * shipped yet is a worse sentence than saying nothing at all, and it is the default behaviour of every
 * naive implementation — one `catch` around one `fetch` collapses both cases into "error".
 *
 * So `getRelease()` distinguishes them and this component gives each its own wording, its own colour
 * and its own mark. The only state that is red is the one that is a fault.
 *
 * The state is also written into the markup as `data-release-state`, as data rather than only as a
 * sentence, because `verify-pages.mjs` reads that attribute. The four lines it costs are worth it:
 * without it, the only way to tell which state rendered is to search the prose for one of three
 * phrases, and one of those phrases occurs elsewhere on the page in ordinary words — so a prose search
 * reports two states at once on a page that rendered one.
 *
 * ---------------------------------------------------------------------------
 * Why the button is sometimes a `span`
 * ---------------------------------------------------------------------------
 *
 * When there is no installer, the control becomes a `span` styled as the button it will become — not a
 * `button` with `disabled`, and not an `a` with no `href`. A disabled control still takes focus in some
 * browsers and still announces itself as actionable; a link with no destination is a lie about being
 * pressable. There is nothing to press, so there is nothing to focus.
 */
export async function DownloadPanel() {
  const release = await getRelease();
  const hasInstaller = release.installer !== null;

  const live = release.source === "live";
  const none = release.source === "none";

  return (
    // `panel-strong` is the application's own glass, and this is one of the two places on the site
    // where it is honestly chrome rather than decoration — the other is the header.
    <div
      data-release-state={release.source}
      className="panel-strong rounded-sheet mt-8 max-w-[var(--measure)] p-5 sm:p-6"
    >
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h2 className="t-h3">Loom {release.version}</h2>
        <span className="flex items-center gap-2 text-[0.8125rem]">
          <span
            aria-hidden="true"
            className="block h-1.5 w-1.5 rounded-full"
            style={{
              background: live
                ? "var(--accent)"
                : none
                  ? "var(--ink-faint)"
                  : "var(--danger)",
            }}
          />
          <span style={{ color: live ? "var(--accent)" : "var(--ink-faint)" }}>
            {live
              ? "the current release"
              : none
                ? "not released yet"
                : "GitHub could not be reached"}
          </span>
        </span>
      </div>

      {none && (
        <P>
          Nothing has been tagged yet, so there is no installer to hand you. {release.version} is the
          version in development, and the{" "}
          <Link href="/" className="link">
            front page
          </Link>{" "}
          describes what the build in the repository already does.
        </P>
      )}

      {!live && !none && (
        <P>
          GitHub could not be reached, so the version below is the last one this page knows about rather
          than the latest. It may be stale. The installer link still resolves to whatever is currently
          published, so it is safe to follow — but the size and the hash under it may not match what you
          get.
        </P>
      )}

      <div className="mt-6 flex flex-wrap items-center gap-3">
        {hasInstaller ? (
          <a
            href={DOWNLOAD.publicPath}
            className="btn-primary h-10 px-4 text-sm"
            // No `download` attribute: the href is a same-origin redirect, and letting the browser
            // follow it keeps the file name GitHub actually serves.
          >
            Download Loom-Setup-{release.tag}.exe
          </a>
        ) : (
          <span className="btn-ghost pointer-events-none h-10 px-4 text-sm opacity-60">
            {none ? "No release published yet" : "Download unavailable right now"}
          </span>
        )}
        <a
          href={release.notesUrl}
          target="_blank"
          rel="noreferrer"
          className="btn-ghost h-10 px-4 text-sm"
        >
          Release notes
        </a>
      </div>

      <dl className="spec mt-6">
        <Row term="Platform">{DOWNLOAD.requirements}</Row>
        <Row term="File size">{release.installer ? formatBytes(release.installer.size) : "—"}</Row>
        <Row term="Update signature">
          {release.signed
            ? "minisign signed, and verified by the application before anything is applied"
            : "unsigned — the application will refuse to auto-update from it, which is the intended behaviour"}
        </Row>
        <Row term="Payload SHA-256">
          {release.payloadSha256 ? (
            <code className="mono break-all">{release.payloadSha256}</code>
          ) : (
            "published in the release notes once a release exists"
          )}
        </Row>
      </dl>

      <Note>
        The button points at <Code>{DOWNLOAD.publicPath}</Code>, which is this site&rsquo;s own route
        and always resolves to whichever release is newest — so a link you copy into an issue keeps
        working after the next version ships. Releases live at{" "}
        <a href={REPO.releasesUrl} target="_blank" rel="noreferrer" className="link">
          {REPO.slug}
        </a>
        .
      </Note>
    </div>
  );
}

function Row({ term, children }: { term: string; children: React.ReactNode }) {
  return (
    <div className="spec-row">
      <dt className="spec-term">{term}</dt>
      <dd className="spec-def">{children}</dd>
    </div>
  );
}
