import type { Metadata } from "next";
import { Standalone, Heading } from "@/components/chrome/standalone";
import { Code, Note, P } from "@/components/ui/primitives";
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
 * ---------------------------------------------------------------------------
 * Why this page exists when the front page already covers it
 * ---------------------------------------------------------------------------
 *
 * Because a privacy page is a document people *cite*. It is linked from issue threads, quoted in
 * permission requests and read by someone deciding whether to install software on a work machine — and
 * all three of those want a page whose whole subject is the answer, not a movement four thousand words
 * into a landing page.
 *
 * So this is the short, quotable form: what is stored, what leaves, and what is deliberately absent. It
 * is written as an inventory rather than as a policy, because the alternative is reassurance, and
 * reassurance is unfalsifiable by design — every claim below can be checked by opening a folder or
 * reading a repository that is public.
 *
 * The one thing it does not do is restate the reasoning. The front page argues the case; this page
 * states the facts for someone who needs to be able to link to them.
 */
export default function PrivacyPage() {
  return (
    <Standalone
      label="Privacy"
      title="What Loom stores, and what leaves."
      standfirst="No account system, no analytics, and no server of the publisher's own. This page lists what is stored, where, and what is sent to whom."
    >
      <Heading>This website</Heading>

      <P>
        This site serves static files and sets no cookies. There is no analytics script, no tag manager,
        no session recording, no A/B testing and no advertising pixel. There is no form, and nothing
        here asks for an email address.
      </P>

      <P>
        The one outbound request it makes is to GitHub, for the version number on the{" "}
        <a href="/download" className="link">
          download page
        </a>
        . The host&rsquo;s own aggregate request counts exist as a function of serving HTTP; nothing in
        this repository generates or forwards them.
      </P>

      <Heading>The application</Heading>

      <P>
        Loom runs on your machine and talks to the model provider you configured. It does not contact
        any server operated by {SITE.publisher}, because none exists.
      </P>

      <P>
        <b className="font-medium text-[var(--ink)]">
          Stored, in <Code>~/.loom</Code>:
        </b>{" "}
        chats, messages and the per-chat workspace index (<Code>loom.db</Code>); settings, personas,
        prompts, MCP server definitions, workspaces and the dock layout (<Code>config.json</Code>);
        files sent in chats (<Code>attachments/</Code>); one log per tracked shell command, capped at
        5&nbsp;MB each (<Code>logs/</Code>); configuration snapshots taken before every change the model
        makes to Loom&rsquo;s own configuration, ten kept (<Code>backups/</Code>); and images produced
        by the image tool (<Code>generated/</Code>).
      </P>

      <P>
        <b className="font-medium text-[var(--ink)]">Stored, outside it:</b> provider API keys, in
        Windows Credential Manager, one entry per provider. They are never written to a configuration
        file, never logged, and never uploaded.
      </P>

      <P>
        Set <Code>LOOM_HOME</Code> to move all of it. There is no sync, no cloud backup and no export
        that uploads anything.
      </P>

      <Note>
        Command output is logged whether or not you are watching the command, which is worth knowing
        before running something that might print a secret. The 5&nbsp;MB cap is per command rather than
        in total, so eight long-running commands can hold 40&nbsp;MB between them — in the clear, on
        your disk.
      </Note>

      <Heading>What leaves your machine</Heading>

      <P>
        <b className="font-medium text-[var(--ink)]">Your conversation, to your provider.</b> Prompts,
        the files you attach, and whatever a tool asks to read — sent to whichever provider and model
        you selected, under their terms and their privacy policy. Loom is a client: it does not sit in
        the middle and cannot see that traffic.
      </P>

      <P>
        <b className="font-medium text-[var(--ink)]">Embeddings, when you index a workspace.</b> File
        chunks go to your provider&rsquo;s embedding endpoint so they can be searched semantically.
        Opt-in per chat, and skipped entirely if you leave it off.
      </P>

      <P>
        <b className="font-medium text-[var(--ink)]">Update checks, to GitHub.</b> Loom fetches a
        release manifest and verifies its signature. No identifiers, no usage data and no machine
        fingerprint are sent with it.
      </P>

      <P>
        That is the whole list. Web search and page fetching go to whichever service is configured —
        your own Jina key if you have stored one, DuckDuckGo otherwise. Computer use captures
        screenshots, stored locally and inlined into the request only for the newest capture, so context
        does not grow with the length of a session.
      </P>

      <Note>
        To send nothing at all: run a local model through Ollama or LM Studio and leave the workspace
        index off. In that configuration Loom makes no outbound request except the update check, and
        that can be turned off too.
      </Note>

      <Heading>Telemetry</Heading>

      <P>
        There is none, and it is not a setting. No crash reporting, no usage statistics, no feature
        flags, no remote configuration, and no identifier generated to recognise an installation. There
        is no code path that reports to {SITE.publisher}. Because the{" "}
        <a href={SITE.licenseUrl} target="_blank" rel="noreferrer" className="link">
          source is public
        </a>
        , this is a claim you can check rather than one you have to take on faith.
      </P>

      <Heading>Your data, and other people&rsquo;s</Heading>

      <P>
        Loom is a developer tool with no account system, so there is no personal data held by the
        publisher to request, correct or delete. Your conversations are on your disk, and deleting{" "}
        <Code>~/.loom</Code> is the entire deletion process. Uninstalling deliberately leaves that
        folder in place, because destroying someone&rsquo;s history as a side effect of removing a
        binary would be hostile.
      </P>

      <P>
        One thing worth knowing: the model can read your other chats, through two read-only tools. Both
        are refused in Chat mode on purpose, because &ldquo;answer from the model and the web&rdquo;
        must not quietly become &ldquo;and anything else we have ever discussed&rdquo;.
      </P>

      <Heading>Changes</Heading>

      <P>
        This page describes Loom as it is currently built, not as it might be later. Any change to the
        claims above is visible in the commit history rather than announced after the fact.
      </P>
    </Standalone>
  );
}
