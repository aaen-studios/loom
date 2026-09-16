import type { Metadata } from "next";
import { Background } from "@/components/background";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { LegalLayout } from "@/components/legal-layout";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Privacy",
  description:
    "What Loom stores, where it stores it, and the short list of what leaves your machine.",
  alternates: { canonical: "/privacy" },
};

/**
 * The privacy page.
 *
 * Written as a factual inventory rather than a policy: which paths exist, what
 * is transmitted, and who receives it. A reader can verify every claim here by
 * opening a folder or reading the source, which is the only kind of privacy
 * statement worth publishing.
 */
export default function PrivacyPage() {
  return (
    <>
      <Background />
      <SiteHeader />
      <LegalLayout
        title="Privacy"
        summary="Loom has no account system, no analytics and no server of its own. This page lists exactly what is stored, where, and what is sent to whom."
      >
        <Section title="This website">
          <p>
            loom.rip serves static files and sets no cookies. There is no
            analytics script, no tag manager, no session recording, no A/B
            testing and no advertising pixel. Nothing on this site asks for your
            email address, and there is no form that submits anywhere.
          </p>
          <p>
            The one outbound request the site makes on your behalf is to GitHub,
            to read the current release version for the download button. The
            cheap analytics that come with hosting — aggregate request counts —
            are collected by the hosting provider as a function of serving HTTP,
            not by anything in this codebase.
          </p>
        </Section>

        <Section title="The application">
          <p>
            Loom runs on your machine and talks to the model provider you
            configured. It does not contact any server operated by{" "}
            {SITE.publisher}, because none exists.
          </p>
          <h3>What is stored, and where</h3>
          <ul>
            <li>
              <b>API keys</b> — Windows Credential Manager, under the service
              name <Code>com.ellio.loom</Code>, one entry per provider. They are
              never written to a configuration file and never logged.
            </li>
            <li>
              <b>Chats, messages and the workspace index</b> —{" "}
              <Code>~/.loom/loom.db</Code>, a SQLite file.
            </li>
            <li>
              <b>Settings, personas, prompts, workspaces</b> —{" "}
              <Code>~/.loom/config.json</Code>.
            </li>
            <li>
              <b>Attachments and generated images</b> —{" "}
              <Code>~/.loom/attachments</Code> and{" "}
              <Code>~/.loom/generated</Code>.
            </li>
            <li>
              <b>Command output</b> — <Code>~/.loom/logs</Code>, one file per
              tracked command, each capped at 5 MB. Commands Loom starts are
              logged whether or not you watch them, so this folder is worth
              knowing about if a command could print a secret.
            </li>
            <li>
              <b>Configuration snapshots</b> — <Code>~/.loom/backups</Code>,
              taken before every change the model makes to Loom&rsquo;s own
              configuration. The ten newest are kept.
            </li>
          </ul>
          <p>
            Set <Code>LOOM_HOME</Code> to move all of it somewhere else. There is
            no sync, no cloud backup and no export that uploads anything.
          </p>
        </Section>

        <Section title="What leaves your machine">
          <p>Three things, and nothing else:</p>
          <ul>
            <li>
              <b>Your conversation, to your provider.</b> Prompts, the files you
              attach, and whatever the model asks a tool to read — sent to
              whichever provider and model you selected, under their terms and
              their privacy policy. Loom is a client; it does not sit in the
              middle and cannot see this traffic.
            </li>
            <li>
              <b>Embeddings, when you index a workspace.</b> File chunks are sent
              to your provider&rsquo;s embedding endpoint so they can be searched
              semantically. Indexing is opt-in per chat and can be skipped
              entirely.
            </li>
            <li>
              <b>Update checks, to GitHub.</b> Loom fetches a release manifest and
              verifies its signature. No identifiers, no usage data, and no
              machine fingerprint are sent with it.
            </li>
          </ul>
          <p>
            Web search and page fetching go to whichever service is configured —
            Jina AI when you have stored a key, or DuckDuckGo&rsquo;s HTML
            endpoint and a local parser when you have not. Computer use captures
            screenshots, which are stored locally and inlined only into the
            request being made.
          </p>
          <p>
            To send nothing at all, configure a local model through Ollama or LM
            Studio and leave the workspace index off. In that configuration Loom
            makes no outbound request except the update check.
          </p>
        </Section>

        <Section title="Telemetry">
          <p>
            There is none. No crash reporting, no usage statistics, no feature
            flags, no remote configuration, and no identifier generated to
            recognise an installation. The application has no code path that
            reports to {SITE.publisher}.
          </p>
        </Section>

        <Section title="Children and personal data">
          <p>
            Loom is a developer tool with no account system, so there is no
            personal data held by us to request, correct or delete. Your
            conversations are on your disk; deleting the folder is the whole
            deletion process, and uninstalling deliberately leaves it in place so
            removing the application does not destroy your history.
          </p>
        </Section>

        <Section title="Changes">
          <p>
            This page describes Loom as it is currently built, not as it might be
            later. Because the{" "}
            <a
              href={SITE.licenseUrl}
              target="_blank"
              rel="noreferrer"
              className="text-[var(--accent)] hover:underline"
            >
              source is public
            </a>
            , any change to the claims above is visible in the commit history
            rather than announced after the fact.
          </p>
        </Section>
      </LegalLayout>
      <SiteFooter />
    </>
  );
}

/** Local helpers so the prose above stays readable. */
function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="mt-8">
      <h2 className="text-[17px] font-medium">{title}</h2>
      <div className="text-soft mt-3 space-y-3 text-[13.5px] leading-[1.7] [&_h3]:mt-5 [&_h3]:text-[14px] [&_h3]:font-medium [&_h3]:text-[var(--ink)] [&_li]:mt-1.5 [&_ul]:list-disc [&_ul]:space-y-1 [&_ul]:pl-5">
        {children}
      </div>
    </section>
  );
}

function Code({ children }: { children: React.ReactNode }) {
  return (
    <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
      {children}
    </code>
  );
}
