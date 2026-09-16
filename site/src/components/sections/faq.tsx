import { Section } from "../section";

/**
 * The FAQ.
 *
 * Written from the questions the app's own README and spec answer, which are
 * the ones a reader would actually have — including the two that are honest
 * about limitations rather than flattering: the installer is unsigned, and
 * computer use is Windows-only and needs a vision model.
 *
 * Native `<details>`, so this needs no JavaScript at all and works with the
 * keyboard and with in-page search (browsers open a closed `<details>` when a
 * match is found inside it).
 */
const QUESTIONS = [
  {
    q: "Is it really free?",
    a: (
      <>
        Yes, and there is nothing to unlock. Loom has no paid tier, no account and
        no licence key. You pay your provider directly for the tokens you use, and
        that bill never passes through us. Running a local model through Ollama or
        LM Studio costs nothing at all.
      </>
    ),
  },
  {
    q: "Why does Windows say the publisher is unknown?",
    a: (
      <>
        Because the installer is not code-signed yet. A certificate is a recurring
        cost, and until one is in place Windows SmartScreen shows the generic
        warning for any installer without it. Release payloads are signed with
        minisign and the app verifies that signature before applying an update, so
        an update cannot be tampered with in transit — but that is a different
        thing from proving who built the installer. The download page publishes
        the SHA-256 of the file you are about to run so you can check it yourself.
      </>
    ),
  },
  {
    q: "Does it work on macOS or Linux?",
    a: (
      <>
        Not yet. Loom is built on Tauri, which is cross-platform, but the parts
        that are specific to Windows are not incidental: computer use is built on
        UI Automation and Win32 input, the credential store is Windows Credential
        Manager, and launch-at-login is a registry entry. The site will say so
        plainly when another platform is real rather than planned.
      </>
    ),
  },
  {
    q: "What does Loom do with my code?",
    a: (
      <>
        It reads the folder you point it at, and sends what it reads to the
        provider of the model you chose — the same as pasting a file into any
        other client. Nothing goes anywhere else, and indexing happens with your
        own provider&rsquo;s embedding endpoint. If you need code to stay on the
        machine, run a local model and nothing leaves at all.
      </>
    ),
  },
  {
    q: "What is the difference between Atelier and the other modes?",
    a: (
      <>
        Plan, Review and Build differ in what they will do to your workspace.
        Atelier is a different axis: it additionally exposes tools that edit
        Loom&rsquo;s own configuration — personas, prompts, skills, MCP servers,
        providers, settings. Every write takes a snapshot first, the five delete
        operations still ask, and it is deliberately per-chat with no way to make
        it the global default.
      </>
    ),
  },
  {
    q: "Can it run commands while I am not watching?",
    a: (
      <>
        Yes, deliberately, and it is bounded. A command that outlives the turn is
        adopted rather than killed, and at most eight can be tracked at once so
        processes cannot pile up invisibly. Everything is listed in the Runs panel
        with a Stop button, output is capped at 5 MB per command, and a command
        still running after a restart is reported as orphaned instead of assumed
        dead.
      </>
    ),
  },
  {
    q: "What happens to my chats if I uninstall?",
    a: (
      <>
        They stay. Uninstalling removes the application from{" "}
        <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
          %LOCALAPPDATA%\Loom
        </code>{" "}
        and the shortcuts it created;{" "}
        <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
          ~/.loom
        </code>{" "}
        is left alone, because deleting someone&rsquo;s conversations as a side
        effect of removing a binary would be hostile. Delete the folder yourself
        when you want it gone.
      </>
    ),
  },
] as const;

export function Faq() {
  return (
    <Section
      id="faq"
      eyebrow="Questions"
      title="The things worth asking before you install."
    >
      <div className="divide-y divide-[var(--glass-border)] overflow-hidden rounded-control border border-[var(--glass-border)]">
        {QUESTIONS.map((item) => (
          <details key={item.q} className="group">
            <summary className="hover-surface flex cursor-pointer items-center gap-3 px-4 py-3.5 [&::-webkit-details-marker]:hidden">
              <span className="flex-1 text-[14px] font-medium">{item.q}</span>
              <span className="text-faint shrink-0 transition group-open:rotate-180">
                <svg
                  width={16}
                  height={16}
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={1.7}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden="true"
                >
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </span>
            </summary>
            <div className="text-soft px-4 pb-4 text-[13.5px] leading-[1.65]">
              {item.a}
            </div>
          </details>
        ))}
      </div>
    </Section>
  );
}
