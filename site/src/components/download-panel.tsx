import { DOWNLOAD, REPO } from "@/lib/site";
import { formatBytes, getRelease } from "@/lib/release";

/**
 * The download panel: version, size, checksums, and the honest note about
 * code signing.
 *
 * A server component, so the version and hashes are in the HTML rather than
 * appearing after hydration. That matters here more than usual — a download
 * page whose version number pops in a beat late looks broken, and the hashes
 * are the whole point of the page.
 */
export async function DownloadPanel() {
  const release = await getRelease();
  const hasInstaller = release.installer !== null;

  return (
    <div className="panel-strong rounded-sheet p-5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <h2 className="text-[17px] font-medium">Loom {release.version}</h2>
        {/* Three distinct states, because "no release yet" and "GitHub is
            unreachable" would otherwise read identically — and telling a
            visitor something is broken when the truth is simply that we have
            not shipped is worse than saying nothing. */}
        {release.source === "live" && (
          <span className="chip px-2 py-0.5 text-[11px]">latest release</span>
        )}
        {release.source === "none" && (
          <span className="chip px-2 py-0.5 text-[11px]">
            not released yet — {release.version} is the version in development
          </span>
        )}
        {release.source === "unreachable" && (
          <span className="chip px-2 py-0.5 text-[11px]">
            could not reach GitHub — showing the last known version
          </span>
        )}
      </div>

      <p className="text-faint mt-2 text-[12.5px]">{DOWNLOAD.requirements}</p>

      <div className="mt-5 flex flex-col gap-3 sm:flex-row sm:items-center">
        {hasInstaller ? (
          <a
            href={DOWNLOAD.publicPath}
            className="btn-primary h-11 px-5 text-[14.5px]"
            // Not `download`: the href is a same-origin redirect, and letting
            // the browser follow it keeps the file name GitHub serves.
          >
            Download Loom-Setup-{release.tag}.exe
          </a>
        ) : (
          <span className="btn-ghost pointer-events-none h-11 px-5 text-[14.5px] opacity-60">
            {release.source === "unreachable"
              ? "Download unavailable right now"
              : "No release published yet"}
          </span>
        )}
        <a
          href={release.notesUrl}
          target="_blank"
          rel="noreferrer"
          className="btn-ghost h-11 px-5 text-[14.5px]"
        >
          Release notes
        </a>
      </div>

      <dl className="mt-6 grid gap-x-6 gap-y-3 border-t border-[var(--glass-border)] pt-5 text-[13px] sm:grid-cols-2">
        <Field term="Version">{release.version}</Field>
        <Field term="File size">
          {release.installer ? formatBytes(release.installer.size) : "—"}
        </Field>
        <Field term="Signature">
          {release.signed
            ? "minisign signed — verified by the app before installing"
            : "not signed"}
        </Field>
        <Field term="Installer SHA-256">
          {release.installer
            ? "published in the release notes"
            : "—"}
        </Field>
        <Field term="Payload SHA-256" wide>
          {release.payloadSha256 ? (
            <code className="bg-[var(--ink-ghost)] block rounded-[6px] px-2 py-1.5 font-mono text-[11.5px] break-all">
              {release.payloadSha256}
            </code>
          ) : (
            "available once a release is published"
          )}
        </Field>
      </dl>

      <div id="unsigned" className="mt-6 scroll-mt-24 border-t border-[var(--glass-border)] pt-5">
        <h3 className="text-[14px] font-medium">
          Why Windows says the publisher is unknown
        </h3>
        <p className="text-soft mt-2 text-[13px] leading-[1.65]">
          The installer is not code-signed. A Windows code-signing certificate is
          a recurring cost, and without one SmartScreen shows the generic warning
          for any installer regardless of what it does. You can verify what you
          downloaded instead:
        </p>
        <pre className="text-soft mt-3 overflow-x-auto rounded-control border border-[var(--glass-border)] px-3 py-2.5 font-mono text-[12px] leading-[1.7]">
{`# In PowerShell, in the folder you saved the installer:
Get-FileHash .\\Loom-Setup-${release.tag}.exe -Algorithm SHA256

# Compare the result with the hash in the release notes:
${release.notesUrl}`}
        </pre>
        <p className="text-soft mt-3 text-[13px] leading-[1.65]">
          Updates are a separate matter: the payload is signed with minisign and
          the app verifies that signature before applying anything, so an update
          cannot be tampered with in transit.{" "}
          {release.signed
            ? "This release is signed."
            : "This release does not carry a signature — the app will refuse to auto-update from it, which is the intended behaviour."}
        </p>
      </div>

      <p className="text-faint mt-5 text-[12px]">
        Releases live at{" "}
        <a
          href={REPO.releasesUrl}
          target="_blank"
          rel="noreferrer"
          className="text-[var(--accent)] hover:underline"
        >
          {REPO.slug}
        </a>
        . This button points at{" "}
        <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[11.5px]">
          {DOWNLOAD.publicPath}
        </code>
        , which always resolves to the current release, so a link you copy keeps
        working after the next version ships.
      </p>
    </div>
  );
}

function Field({
  term,
  wide,
  children,
}: {
  term: string;
  wide?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className={wide ? "sm:col-span-2" : undefined}>
      <dt className="text-faint text-[11.5px]">{term}</dt>
      <dd className="text-soft mt-0.5 leading-[1.55]">{children}</dd>
    </div>
  );
}
