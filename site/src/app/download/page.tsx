import type { Metadata } from "next";
import { DownloadPanel } from "@/components/download-panel";
import { Background } from "@/components/background";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Download",
  description:
    "Download Loom for Windows 10 and 11 (64-bit). Free, MIT licensed, no account required.",
  alternates: { canonical: "/download" },
};

export default function DownloadPage() {
  return (
    <>
      <Background />
      <SiteHeader />
      <main className="px-4 pt-12 pb-4 sm:px-6 sm:pt-16">
        <div className="mx-auto max-w-3xl">
          <h1 className="text-[30px] leading-tight font-medium tracking-tight sm:text-[38px]">
            Download Loom
          </h1>
          <p className="text-soft mt-4 text-[15px] leading-6">
            A single portable installer with the application embedded. No account,
            no bundled runtime to install first, and nothing to configure beyond
            adding a provider key.
          </p>

          <div className="mt-8">
            <DownloadPanel />
          </div>

          <section className="mt-10">
            <h2 className="text-[16px] font-medium">Installing</h2>
            <ol className="text-soft mt-3 space-y-2 text-[13.5px] leading-[1.65]">
              <li>
                Run the installer. Windows will warn that the publisher is
                unknown —{" "}
                <a href="#unsigned" className="text-[var(--accent)] hover:underline">
                  here is why
                </a>
                .
              </li>
              <li>
                Choose a folder, or accept the default. If Loom is already
                installed elsewhere the installer finds it through its uninstall
                entry and updates in place.
              </li>
              <li>
                Open Loom, add a provider key in Settings → Providers, and pick a
                model in the composer.
              </li>
            </ol>

            <h2 className="mt-8 text-[16px] font-medium">Uninstalling</h2>
            <p className="text-soft mt-3 text-[13.5px] leading-[1.65]">
              Use Add or remove programs, or run{" "}
              <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
                %LOCALAPPDATA%\Loom\uninstall.cmd
              </code>
              . Your chats, keys and settings are deliberately left in place — see{" "}
              <a
                href="/privacy"
                className="text-[var(--accent)] hover:underline"
              >
                privacy
              </a>{" "}
              for what lives where.
            </p>
          </section>
        </div>
      </main>
      <SiteFooter />
      <p className="sr-only">
        Loom is published by {SITE.publisher}. Licensed {SITE.license}.
      </p>
    </>
  );
}
