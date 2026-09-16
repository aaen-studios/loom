import type { Metadata } from "next";
import { DownloadPanel } from "@/components/pages/download-panel";
import { Code } from "@/components/pages/legal";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Download Loom for Windows 10 and 11 (64-bit). Free, MIT licensed, no account required.",
  alternates: { canonical: "/download" },
};

/**
 * The download page.
 *
 * Thin on purpose. The interesting part is `DownloadPanel`, a server component: the
 * version, the size and the checksum are in the HTML rather than appearing after
 * hydration, because a version number that pops in a beat late reads as broken and
 * the checksums are the point of the page.
 *
 * The prose around it carries the two things a download page should not be coy
 * about: that Windows will warn about the publisher, and how to verify the file
 * yourself instead of trusting it.
 */
export default function DownloadPage() {
  return (
    <section className="px-4 pt-12 pb-4 sm:px-6 sm:pt-16">
      <div className="warp-grid">
        <div className="max-w-3xl" style={{ gridColumn: "2 / span 10" }}>
          <p className="text-faint flex items-center gap-2.5 text-[11.5px] font-medium tracking-[0.16em] uppercase">
            <span className="knot-lit knot" />
            <span>Off the loom</span>
          </p>

          <h1 className="mt-4 text-[30px] leading-tight font-medium tracking-tight sm:text-[38px]">
            Download Loom
          </h1>
          <p className="text-soft mt-4 text-[15px] leading-6">
            A single portable installer with the application embedded. No account, no
            runtime to install first, and nothing to configure beyond adding a
            provider key.
          </p>

          <div className="mt-8">
            <DownloadPanel />
          </div>

          <section className="mt-10">
            <h2 className="text-[16px] font-medium">Installing</h2>
            <ol className="text-soft mt-3 space-y-2 text-[13.5px] leading-[1.65]">
              <li>
                Run the installer. Windows will warn that the publisher is unknown —{" "}
                <a href="#unsigned" className="text-[var(--accent)] hover:underline">
                  here is why
                </a>
                .
              </li>
              <li>
                Choose a folder, or accept the default. If Loom is already installed
                somewhere else, the installer finds it through its uninstall entry and
                updates in place.
              </li>
              <li>
                Open Loom, add a provider key in Settings → Providers, and pick a
                model in the composer.
              </li>
            </ol>

            <h2 className="mt-8 text-[16px] font-medium">Uninstalling</h2>
            <p className="text-soft mt-3 text-[13.5px] leading-[1.65]">
              Use Add or remove programs, or run{" "}
              <Code>%LOCALAPPDATA%\Loom\uninstall.cmd</Code>. Your chats, keys and
              settings are deliberately left in place — see{" "}
              <a href="/privacy" className="text-[var(--accent)] hover:underline">
                privacy
              </a>{" "}
              for exactly what lives where.
            </p>
          </section>
        </div>
      </div>

      <p className="sr-only">
        Loom is published by {SITE.publisher}. Licensed {SITE.license}.
      </p>
    </section>
  );
}
