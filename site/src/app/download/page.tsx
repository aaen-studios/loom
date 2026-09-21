import type { Metadata } from "next";
import { DownloadPanel } from "@/components/pages/download-panel";
import { Standalone } from "@/components/doc/standalone";
import { Sub } from "@/components/doc/section";
import { Code, Note, P, Shell } from "@/components/doc/text";
import { REPO } from "@/lib/site";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Download Loom for Windows 10 and 11 (64-bit). Free, MIT licensed, no account required.",
  alternates: { canonical: "/download" },
};

/**
 * The download page.
 *
 * ---------------------------------------------------------------------------
 * Why this is not part of the manual
 * ---------------------------------------------------------------------------
 *
 * The manual's eighth section covers installation in the way a manual covers it:
 * numbered steps and a note on verifying the hash. That is the right treatment for a
 * reader who is already convinced.
 *
 * This page exists for a different reader and a different moment — someone who has
 * been sent a link, who wants the file, and who needs the state of the release stated
 * truthfully before they take it. That is why it is a separate page rather than a
 * subsection: it needs a server, because the version is read from GitHub at request
 * time with a five-minute revalidation, and a build-time snapshot would go stale the
 * moment a release was published.
 *
 * What it deliberately does *not* do is repeat the manual. Installation, the
 * permission model, where your data lives and what leaves the machine are all written
 * out at length on the front page, and a second, slightly different telling of any of
 * them is how a site begins to contradict itself. So the prose here is the prose that
 * only belongs here: what the file is, why Windows will complain about it, and how to
 * check it.
 */
export default function DownloadPage() {
  return (
    <Standalone
      title="Download"
      standfirst="One portable installer with the application embedded. No runtime to install first, no account, and nothing to configure beyond adding a provider key."
    >
      <DownloadPanel />

      <Sub>Why Windows says the publisher is unknown</Sub>

      <P>
        The installer is not code-signed. A Windows code-signing certificate is a
        recurring cost, and until one is in place SmartScreen shows the same generic
        warning for any installer that lacks it, regardless of what the installer does.
        So the honest substitute is to verify the file yourself rather than to trust the
        absence of a warning.
      </P>

      <Shell>
        {`# in the folder you saved the installer
Get-FileHash .\\Loom-Setup-<tag>.exe -Algorithm SHA256

# compare against the hash in the release notes
${REPO.releasesUrl}`}
      </Shell>

      <Note>
        This is separate from the updater, and the updater is the stronger of the two.
        The payload an update installs is signed with minisign and verified before
        anything is applied, so an update cannot be tampered with in transit even though
        the installer that delivered it cannot prove who built it. The one thing a hash
        cannot tell you is who wrote the code &mdash; and that is what the{" "}
        <a
          href={REPO.url}
          target="_blank"
          rel="noreferrer"
          className="text-[var(--accent)] hover:underline"
        >
          public repository
        </a>{" "}
        is for.
      </Note>

      <Sub>After installing</Sub>

      <P>
        Add a provider key in Settings &rarr; Providers and pick a model in the composer.
        To skip the key entirely, install Ollama or LM Studio and point Loom at it &mdash;
        local models are detected rather than configured. The{" "}
        <a href="/#warp" className="text-[var(--accent)] hover:underline">
          second section of the manual
        </a>{" "}
        covers the window, and{" "}
        <a href="/#shortcuts" className="text-[var(--accent)] hover:underline">
          Appendix A
        </a>{" "}
        has the nine keys worth knowing.
      </P>

      <Sub>Uninstalling</Sub>

      <P>
        Either add or remove programs, or run{" "}
        <Code>%LOCALAPPDATA%\Loom\uninstall.cmd</Code>. Your chats, keys and settings
        are deliberately left in place; the{" "}
        <a href="/privacy" className="text-[var(--accent)] hover:underline">
          privacy page
        </a>{" "}
        lists exactly what remains and where.
      </P>

      <Note>
        Releases are built and published from that repository, which is public and MIT
        licensed. The download button resolves through this site rather than pointing at
        GitHub directly, so a link you have already copied keeps working if the
        repository is ever renamed or moved.
      </Note>
    </Standalone>
  );
}
