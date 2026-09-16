import { Section } from "../section";
import { SITE } from "@/lib/site";

/**
 * Privacy and licensing, stated as facts rather than reassurances.
 *
 * "We take your privacy seriously" is meaningless. What a reader can actually
 * verify is: which files exist, what leaves the machine, and what the licence
 * permits — so this section names paths and links the licence text.
 */
export function Trust() {
  return (
    <Section
      id="trust"
      eyebrow="Privacy and licensing"
      title="Nothing is sent anywhere except to your provider."
      lead="Loom has no analytics, no crash reporting and no account system. Here is specifically what that means, so you can check it rather than take it on faith."
    >
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="panel rounded-control p-4">
          <h3 className="text-[14px] font-medium">Where your data lives</h3>
          <dl className="mt-3 space-y-2 text-[13px]">
            <Row term="API keys">
              Windows Credential Manager. Never written to a config file.
            </Row>
            <Row term="Chats and index">
              <code className="mono bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
                ~/.loom/loom.db
              </code>{" "}
              — a SQLite file on your disk.
            </Row>
            <Row term="Command output">
              <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
                ~/.loom/logs
              </code>
              , one log per tracked command, capped at 5 MB each.
            </Row>
            <Row term="Attachments">
              <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
                ~/.loom/attachments
              </code>
              , per chat. Point{" "}
              <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
                LOOM_HOME
              </code>{" "}
              elsewhere to move all of it.
            </Row>
          </dl>
        </div>

        <div className="panel rounded-control p-4">
          <h3 className="text-[14px] font-medium">What leaves the machine</h3>
          <p className="text-soft mt-3 text-[13px] leading-[1.6]">
            Your prompts, the files you attach, and whatever a tool reads, sent
            to whichever provider you configured — because that is what a
            conversation with a model is.
          </p>
          <p className="text-soft mt-3 text-[13px] leading-[1.6]">
            Nothing else. The updater fetches a manifest and a signed payload
            from GitHub and verifies both; it sends no identifiers. This website
            sets no cookies and runs no analytics, so there is nothing here to
            consent to either.
          </p>
          <p className="text-soft mt-3 text-[13px] leading-[1.6]">
            If you want a second opinion, the{" "}
            <a
              href={SITE.licenseUrl}
              target="_blank"
              rel="noreferrer"
              className="text-[var(--accent)] hover:underline"
            >
              source is public
            </a>{" "}
            and the network calls are a short list.
          </p>
        </div>
      </div>

      <p className="text-faint mt-4 text-[13px]">
        Loom is {SITE.license} licensed, so you can read it, build it, fork it and
        ship your own version. Copyright is held by {SITE.publisher}.
      </p>
    </Section>
  );
}

function Row({ term, children }: { term: string; children: React.ReactNode }) {
  return (
    <div>
      <dt className="text-[var(--ink)] text-[12.5px] font-medium">{term}</dt>
      <dd className="text-soft mt-0.5 leading-[1.55]">{children}</dd>
    </div>
  );
}
