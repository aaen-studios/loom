import type { Metadata } from "next";
import { Clause, Code, Legal } from "@/components/pages/legal";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Privacy",
  description:
    "What Loom stores, where it stores it, and the short list of what leaves your machine.",
  alternates: { canonical: "/privacy" },
};

/**
 * Privacy.
 *
 * Written as a factual inventory rather than as a policy: which paths exist, what is
 * transmitted, and who receives it. Every claim below can be checked by opening a
 * folder or reading a public repository, which is the only kind of privacy statement
 * worth publishing — the alternatives are all reassurance, and reassurance is
 * unfalsifiable by design.
 */
export default function PrivacyPage() {
  return (
    <Legal
      title="Privacy"
      summary="Loom has no account system, no analytics and no server of its own. This page lists exactly what is stored, where, and what is sent to whom."
    >
      <Clause title="This website">
        <p>
          loom.rip serves static files and sets no cookies. There is no analytics
          script, no tag manager, no session recording, no A/B testing and no
          advertising pixel. Nothing here asks for your email address, and there is
          no form that submits anywhere.
        </p>
        <p>
          The one outbound request the site makes is to GitHub, to read the current
          release version for the download button. The host&rsquo;s own aggregate
          request counts exist as a function of serving HTTP; nothing in this
          codebase generates or forwards them.
        </p>
      </Clause>

      <Clause title="The application">
        <p>
          Loom runs on your machine and talks to the model provider you configured.
          It does not contact any server operated by {SITE.publisher}, because none
          exists.
        </p>

        <h3 className="mt-5 text-[14px] font-medium text-[var(--ink)]">
          What is stored, and where
        </h3>
        <ul className="mt-2 list-disc space-y-1.5 pl-5 [&_li]:mt-1">
          <li>
            <b>API keys</b> — Windows Credential Manager, one entry per provider.
            They are never written to a configuration file and never logged.
          </li>
          <li>
            <b>Chats, messages and the workspace index</b> — <Code>~/.loom/loom.db</Code>
            , a SQLite file.
          </li>
          <li>
            <b>Settings, personas, prompts, workspaces</b> —{" "}
            <Code>~/.loom/config.json</Code>.
          </li>
          <li>
            <b>Attachments and generated images</b> —{" "}
            <Code>~/.loom/attachments</Code> and <Code>~/.loom/generated</Code>.
          </li>
          <li>
            <b>Command output</b> — <Code>~/.loom/logs</Code>, one file per tracked
            command, each capped at 5 MB. Commands Loom starts are logged whether or
            not you watch them, which is worth knowing if a command could print a
            secret.
          </li>
          <li>
            <b>Configuration snapshots</b> — <Code>~/.loom/backups</Code>, taken
            before every change the model makes to Loom&rsquo;s own configuration.
            The ten newest are kept.
          </li>
        </ul>
        <p>
          Set <Code>LOOM_HOME</Code> to move all of it somewhere else. There is no
          sync, no cloud backup, and no export that uploads anything.
        </p>
      </Clause>

      <Clause title="What leaves your machine">
        <p>Three things, and nothing else:</p>
        <ul className="mt-2 list-disc space-y-1.5 pl-5 [&_li]:mt-1">
          <li>
            <b>Your conversation, to your provider.</b> Prompts, the files you
            attach, and whatever the model asks a tool to read — sent to whichever
            provider and model you selected, under their terms and their privacy
            policy. Loom is a client; it does not sit in the middle and cannot see
            that traffic.
          </li>
          <li>
            <b>Embeddings, when you index a workspace.</b> File chunks go to your
            provider&rsquo;s embedding endpoint so they can be searched semantically.
            Indexing is opt-in per chat and can be skipped entirely.
          </li>
          <li>
            <b>Update checks, to GitHub.</b> Loom fetches a release manifest and
            verifies its signature. No identifiers, no usage data and no machine
            fingerprint are sent with it.
          </li>
        </ul>
        <p>
          Web search and page fetching go to whichever service is configured. Computer
          use captures screenshots, which are stored locally and inlined only into the
          request being made.
        </p>
        <p>
          To send nothing at all, configure a local model through Ollama or LM Studio
          and leave the workspace index off. In that configuration Loom makes no
          outbound request except the update check.
        </p>
      </Clause>

      <Clause title="Telemetry">
        <p>
          There is none. No crash reporting, no usage statistics, no feature flags, no
          remote configuration, and no identifier generated to recognise an
          installation. The application has no code path that reports to{" "}
          {SITE.publisher}.
        </p>
      </Clause>

      <Clause title="Children and personal data">
        <p>
          Loom is a developer tool with no account system, so there is no personal
          data held by us to request, correct or delete. Your conversations are on
          your disk; deleting the folder is the whole deletion process, and
          uninstalling deliberately leaves it in place so that removing the
          application does not destroy your history.
        </p>
      </Clause>

      <Clause title="Changes">
        <p>
          This page describes Loom as it is currently built, not as it might be later.
          Because the{" "}
          <a
            href={SITE.licenseUrl}
            target="_blank"
            rel="noreferrer"
            className="text-[var(--accent)] hover:underline"
          >
            source is public
          </a>
          , any change to the claims above is visible in the commit history rather
          than announced after the fact.
        </p>
      </Clause>
    </Legal>
  );
}
