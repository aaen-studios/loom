import { Code, Pick } from "@/components/weave/pass";
import { passById } from "@/lib/weave/passes";
import { SITE } from "@/lib/site";

/**
 * Pass five: the selvedge.
 *
 * The selvedge is the tightly-woven border down the edge of a cloth that stops it
 * fraying — the part that holds the rest together. So this pass carries the things
 * that do that job for a piece of software: where your data actually lives, what
 * leaves the machine, and what the licence permits.
 *
 * Written as an inventory rather than as a promise. "We take your privacy
 * seriously" means nothing to a reader because it cannot be checked. Which paths
 * exist, what is transmitted and to whom — all of that can be checked, by opening a
 * folder or reading a repository that is public.
 */

interface Kept {
  term: string;
  /** A path, rendered as code. Absent when the answer is not a location. */
  path?: string;
  body: string;
}

const KEPT: readonly Kept[] = [
  {
    term: "API keys",
    body: "Windows Credential Manager, one entry per provider. Never written to a config file, never logged.",
  },
  {
    term: "Chats and the index",
    path: "~/.loom/loom.db",
    body: " — a SQLite file on your own disk.",
  },
  {
    term: "Command output",
    path: "~/.loom/logs",
    body: ", one file per tracked command, each capped at 5 MB.",
  },
  {
    term: "Attachments",
    path: "~/.loom/attachments",
    body: ", per chat. Point LOOM_HOME elsewhere to move all of it.",
  },
  {
    term: "Config snapshots",
    path: "~/.loom/backups",
    body: ", taken before every change the model makes to Loom itself. The ten newest are kept.",
  },
];

export function SelvedgeBand() {
  const pass = passById("selvedge");

  return (
    <Pick
      pass={pass}
      title="Nothing leaves the machine except to your provider."
      lead="No analytics, no crash reporting, no account. Here is specifically what that means, so you can check it rather than take it on faith."
    >
      <div className="grid gap-3 lg:grid-cols-2">
        {/* The dense weave: where things are kept. */}
        <div className="panel rounded-sheet overflow-hidden">
          <div className="border-b border-[var(--glass-border)] px-4 py-2.5">
            <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
              Where it lives
            </p>
          </div>
          <dl className="divide-y divide-[var(--glass-border)]">
            {KEPT.map((entry) => (
              <div key={entry.term} className="px-4 py-3">
                <dt className="text-[12.5px] font-medium">{entry.term}</dt>
                <dd className="text-soft mt-0.5 text-[13px] leading-[1.55]">
                  {entry.path && <Code>{entry.path}</Code>}
                  {entry.body}
                </dd>
              </div>
            ))}
          </dl>
        </div>

        <div className="flex flex-col gap-3">
          <div className="panel rounded-sheet p-4">
            <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
              What leaves
            </p>
            <p className="text-soft mt-3 text-[13px] leading-[1.65]">
              Your prompts, the files you attach, and whatever a tool reads — sent to
              whichever provider you configured, because that is what having a
              conversation with a model means. Indexing a workspace sends file chunks
              to your provider&rsquo;s embedding endpoint, and is opt-in per chat.
            </p>
            <p className="text-soft mt-3 text-[13px] leading-[1.65]">
              Nothing else. The updater fetches a manifest and a signed payload from
              GitHub and verifies the signature before applying anything; it sends no
              identifiers. This website sets no cookies and runs no analytics, so there
              is nothing here to consent to either.
            </p>
          </div>

          <div className="panel rounded-sheet p-4">
            <p className="text-faint text-[10.5px] font-medium tracking-[0.16em] uppercase">
              And the absence of a promise
            </p>
            <p className="text-soft mt-3 text-[13px] leading-[1.65]">
              There is no telemetry to opt out of, no crash reporter to disable, and no
              installation identifier to rotate — because none of them were built. For
              a second opinion the{" "}
              <a
                href={SITE.licenseUrl}
                target="_blank"
                rel="noreferrer"
                className="text-[var(--accent)] hover:underline"
              >
                source is public
              </a>{" "}
              and the list of outbound calls is short enough to read in one sitting.
            </p>
            <p className="text-faint mt-3 text-[12.5px] leading-[1.6]">
              Loom is {SITE.license} licensed, so you can read it, build it, fork it and
              ship your own version. Copyright is held by {SITE.publisher}.
            </p>
          </div>
        </div>
      </div>
    </Pick>
  );
}
