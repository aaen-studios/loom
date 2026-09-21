import { DOWNLOAD, REPO } from "@/lib/site";
import { formatBytes, getRelease } from "@/lib/release";
import { Code, Note, P } from "@/components/doc/text";
import { Action } from "@/components/doc/section";

/**
 * The release block.
 *
 * A server component, so the version, the size and the payload hash are in the HTML
 * rather than arriving after hydration. That matters more here than it would anywhere
 * else on the site: a download page whose version number appears a beat late reads as
 * broken, and the hash is half the reason the page exists rather than being a button
 * in the installation section.
 *
 * ---------------------------------------------------------------------------
 * Three release states, and why they are drawn differently
 * ---------------------------------------------------------------------------
 *
 * `live`, `none` and `unreachable` are not interchangeable, and the failure this
 * guards against is a real one: telling a visitor that something is
 * <em>broken</em> when the truth is simply that the project has not shipped yet is a
 * worse sentence than saying nothing at all, and it is the default behaviour of every
 * naive implementation — one `catch` around one `fetch` collapses both cases into
 * "error".
 *
 * So `getRelease()` distinguishes them, and this component gives each its own wording
 * and its own tone. The only state that is red is the one that is a fault.
 *
 * ---------------------------------------------------------------------------
 * Why the button is sometimes a `span`
 * ---------------------------------------------------------------------------
 *
 * When there is no installer, the control becomes a `span` styled as the button it
 * will become — not a `button` with `disabled`, and not an `a` with no `href`. A
 * disabled control still takes focus in some browsers and still announces itself as
 * actionable; a link with no destination is a lie about being pressable. There is
 * nothing to press, so there is nothing to focus.
 */
export async function DownloadPanel() {
  const release = await getRelease();
  const hasInstaller = release.installer !== null;

  const live = release.source === "live";
  const none = release.source === "none";

  return (
    // `rounded-sheet` rather than a radius: it is one of the application's own
    // `@utility` surfaces, so the release block has the same corner as one of the
    // product's own sheets rather than one that looks similar.
    <div className="rounded-sheet mt-8 border border-[var(--glass-border-strong)] p-5 sm:p-6">
      {/* The state, stated before the download. A reader should know whether they are
          about to get an installer before they look for the button. */}
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h3 className="t-sub">Loom {release.version}</h3>
        <span
          className="text-[0.8125rem]"
          style={{
            color: live
              ? "var(--accent)"
              : none
                ? "var(--ink-faint)"
                : "var(--danger)",
          }}
        >
          {live
            ? "the current release"
            : none
              ? "not released yet"
              : "GitHub could not be reached"}
        </span>
      </div>

      {none && (
        <P>
          Nothing has been tagged yet, so there is no installer to hand you.{" "}
          {release.version} is the version currently in development, and the keys
          documented in{" "}
          <a href="/#shortcuts" className="text-[var(--accent)] hover:underline">
            Appendix A
          </a>{" "}
          are what the build in the repository already does.
        </P>
      )}

      {!live && !none && (
        <P>
          GitHub could not be reached, so the version below is the last one this page
          knows about rather than the latest. It may be stale. The installer link still
          resolves to whatever is currently published, so it is safe to follow — but the
          size and the hash under it may not match what you get.
        </P>
      )}

      <Action>
        {hasInstaller ? (
          <a
            href={DOWNLOAD.publicPath}
            className="btn-primary h-10 px-4 text-sm"
            // No `download` attribute: the href is a same-origin redirect, and letting
            // the browser follow it keeps the file name GitHub actually serves.
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
      </Action>

      <dl className="spec mt-6">
        <Row term="Platform">{DOWNLOAD.requirements}</Row>
        <Row term="File size">
          {release.installer ? formatBytes(release.installer.size) : "—"}
        </Row>
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
        The button points at <Code>{DOWNLOAD.publicPath}</Code>, which is this site&rsquo;s
        own route and always resolves to the current release &mdash; so a link you copy
        into an issue keeps working after the next version ships. Releases live at{" "}
        <a
          href={REPO.releasesUrl}
          target="_blank"
          rel="noreferrer"
          className="text-[var(--accent)] hover:underline"
        >
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
