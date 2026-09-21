import type { Metadata } from "next";
import { DownloadPanel } from "@/components/pages/download-panel";
import { Standalone, Heading } from "@/components/chrome/standalone";
import { Code, Note, P, Shell } from "@/components/ui/primitives";
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
 * Why this is not a movement of the front page
 * ---------------------------------------------------------------------------
 *
 * The front page's last movement covers installation the way a product page covers it: the formats,
 * the flags, and a note on verifying the hash. That is the right treatment for a reader who has read
 * their way there.
 *
 * This page exists for a different reader and a different moment — someone who has been sent a link,
 * who wants the file, and who needs the state of the release stated truthfully before they take it.
 * That is why it is a separate route rather than an anchor: it needs a server, because the version is
 * read from GitHub at request time with a five-minute revalidation, and a build-time snapshot would go
 * stale the moment a release was published.
 *
 * It deliberately does *not* repeat the front page. Installation, the permission model, where your data
 * lives and what leaves the machine are all written out there, and a second, slightly different telling
 * of any of them is how a site begins to contradict itself. So the prose here is the prose that only
 * belongs here: what the file is, why Windows will complain about it, and how to check it.
 */
export default function DownloadPage() {
  return (
    <Standalone
      label="Download"
      title="One portable installer."
      standfirst="The application is embedded in the installer at build time, so there is no runtime to install first and no payload to fetch afterwards. No account, and nothing to configure beyond adding a provider key."
    >
      <DownloadPanel />

      <Heading>Why Windows says the publisher is unknown</Heading>

      <P>
        The installer is not code-signed. A Windows code-signing certificate is a recurring cost, and
        until one is in place SmartScreen shows the same generic warning for any installer that lacks
        it, regardless of what the installer does. So the honest substitute is to verify the file
        yourself rather than to trust the absence of a warning.
      </P>

      <Shell>
        {`# in the folder you saved the installer
Get-FileHash .\\Loom-Setup-<tag>.exe -Algorithm SHA256

# compare against the hash in the release notes
${REPO.releasesUrl}`}
      </Shell>

      <Note>
        This is separate from the updater, and the updater is the stronger of the two. The payload an
        update installs is signed with minisign and verified before anything is applied, so an update
        cannot be tampered with in transit even though the installer that delivered it cannot prove who
        built it. The one thing a hash cannot tell you is who wrote the code — and that is what the{" "}
        <a href={REPO.url} target="_blank" rel="noreferrer" className="link">
          public repository
        </a>{" "}
        is for.
      </Note>

      <Heading>After installing</Heading>

      <P>
        Add a provider key in Settings → Providers and pick a model in the composer. To skip the key
        entirely, install Ollama or LM Studio and point Loom at it — local models are detected rather
        than configured. The{" "}
        <a href="/#parts" className="link">
          second movement of the front page
        </a>{" "}
        covers the window and{" "}
        <a href="/#install" className="link">
          the last one
        </a>{" "}
        answers the questions worth asking first.
      </P>

      <Heading>Uninstalling</Heading>

      <P>
        Either add or remove programs, or run <Code>%LOCALAPPDATA%\Loom\uninstall.cmd</Code>. Your
        chats, keys and settings are deliberately left in place; the{" "}
        <a href="/privacy" className="link">
          privacy page
        </a>{" "}
        lists exactly what remains and where.
      </P>
    </Standalone>
  );
}
