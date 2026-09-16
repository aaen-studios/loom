import { DOWNLOAD, REPO } from "@/lib/site";
import { formatBytes, getRelease } from "@/lib/release";

/**
 * The download panel.
 *
 * A server component, so the version and the hashes are in the HTML rather than
 * arriving after hydration. That matters more here than it would anywhere else: a
 * download page whose version number pops in a beat late reads as broken, and the
 * hashes are the entire reason this page exists rather than being a button on the
 * landing page.
 *
 * The three release states are drawn as three different *thread tensions* rather
 * than three differently-worded chips, because "nothing has been released yet" and
 * "GitHub could not be reached" must never look alike: telling a visitor something
 * is broken when the truth is simply that the project has not shipped is a worse
 * sentence than saying nothing.
 */
export async function DownloadPanel() {
  const release = await getRelease();
  const hasInstaller = release.installer !== null;

  const state =
    release.source === "live"
      ? { label: "latest release", tone: "thread-bright" }
      : release.source === "none"
        ? { label: "not released yet", tone: "var(--ink-faint)" }
        : { label: "GitHub unreachable", tone: "var(--danger)" };

  return (
    <div className="panel-strong rounded-sheet overflow-hidden">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--glass-border)] px-5 py-4">
        <h2 className="text-[17px] font-medium">Loom {release.version}</h2>
        <span className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className="knot-lit knot"
            style={{ background: state.tone }}
          />
          <span className="text-faint text-[11.5px]">{state.label}</span>
        </span>
      </div>

      <div className="px-5 py-5">
        <p className="text-faint text-[12.5px]">{DOWNLOAD.requirements}</p>

        {/* The state, spelled out. Only ever one of these, and the wording for each
            is deliberately not interchangeable. */}
        {release.source === "none" && (
          <p className="text-soft mt-3 text-[13px] leading-[1.6]">
            Nothing has been tagged yet, so there is no installer to hand you.{" "}
            {release.version} is the version currently in development.
          </p>
        )}
        {release.source === "unreachable" && (
          <p className="text-soft mt-3 text-[13px] leading-[1.6]">
            GitHub could not be reached, so the version below is the last one this
            page knows about rather than the latest. That may be stale.
          </p>
        )}

        <div className="mt-5 flex flex-col gap-3 sm:flex-row sm:items-center">
          {hasInstaller ? (
            <a
              href={DOWNLOAD.publicPath}
              className="btn-primary h-11 px-5 text-[14.5px]"
              // No `download` attribute: the href is a same-origin redirect, and
              // letting the browser follow it keeps the file name GitHub serves.
            >
              Download Loom-Setup-{release.tag}.exe
            </a>
          ) : (
            // Not a link and not a disabled button — a `span`, because there is
            // nothing to press. Dressed as the button it will become.
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
          <Field term="Update signature">
            {release.signed
              ? "minisign signed — verified by the app before installing"
              : "not signed"}
          </Field>
          <Field term="Installer SHA-256">published in the release notes</Field>
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

        {/* The anchor the install instructions link to. */}
        <div
          id="unsigned"
          className="mt-6 scroll-mt-24 border-t border-[var(--glass-border)] pt-5"
        >
          <h3 className="text-[14px] font-medium">
            Why Windows says the publisher is unknown
          </h3>
          <p className="text-soft mt-2 text-[13px] leading-[1.65]">
            The installer is not code-signed. A Windows code-signing certificate is a
            recurring cost, and without one SmartScreen shows the same generic warning
            for any installer regardless of what it does. So the honest alternative is
            to verify what you downloaded:
          </p>
          <pre className="text-soft rounded-control mt-3 overflow-x-auto border border-[var(--glass-border)] px-3 py-2.5 font-mono text-[12px] leading-[1.7]">
            {`# In PowerShell, in the folder you saved the installer:
Get-FileHash .\\Loom-Setup-${release.tag}.exe -Algorithm SHA256

# Then compare the result with the hash in the release notes:
${release.notesUrl}`}
          </pre>
          <p className="text-soft mt-3 text-[13px] leading-[1.65]">
            Updates are a separate matter. The payload is signed with minisign and the
            app verifies that signature before applying anything, so an update cannot
            be tampered with in transit.{" "}
            {release.signed
              ? "This release is signed."
              : "This release carries no signature — the app will refuse to auto-update from it, which is the intended behaviour rather than a fault."}
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
          . The button above points at{" "}
          <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[11.5px]">
            {DOWNLOAD.publicPath}
          </code>
          , which always resolves to the current release — so a link you copy keeps
          working after the next version ships.
        </p>
      </div>
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
