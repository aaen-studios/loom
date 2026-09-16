import { Section } from "../section";

/**
 * The three steps, written as a numbered walkthrough rather than a feature
 * list, because "does it need an account" and "where do my keys go" are the
 * first two questions a developer asks and neither is answered by a grid of
 * icons.
 */
const STEPS = [
  {
    title: "Install it",
    body: (
      <>
        One portable installer with the app embedded — no framework, no
        redistributable to fetch first. It writes shortcuts, registers an
        uninstall entry, and drops the uninstaller outside the install folder so
        it can actually remove it. Add{" "}
        <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
          --silent --dir &lt;path&gt;
        </code>{" "}
        and it installs unattended.
      </>
    ),
  },
  {
    title: "Add a provider",
    body: (
      <>
        Paste a key, or point Loom at a local server with no key at all. Loom
        asks what models are available so you are not typing ids from memory.
        Everything else — personas, skills, MCP servers — is optional and each
        piece is introduced in the interface where you would want it.
      </>
    ),
  },
  {
    title: "Point it at something",
    body: (
      <>
        Pick a folder for the chat. Ask for what you want. If it is a change, use
        Plan first to hear the approach, then Build to do it with the diff and
        the commands visible as they happen. Nothing is written without either
        your approval or a mode you set deliberately.
      </>
    ),
  },
] as const;

export function HowItWorks() {
  return (
    <Section
      id="how"
      eyebrow="Getting started"
      title="Three steps, and no account."
      lead="Loom is a desktop application with no backend of its own. There is nothing to sign up for, and no service in the middle of your conversations."
    >
      <ol className="grid gap-3 sm:grid-cols-3">
        {STEPS.map((step, index) => (
          <li key={step.title} className="panel rounded-control p-4">
            <span className="text-faint font-mono text-[12px] tabular-nums">
              {String(index + 1).padStart(2, "0")}
            </span>
            <h3 className="mt-2 text-[14.5px] font-medium">{step.title}</h3>
            <p className="text-soft mt-2 text-[13.5px] leading-[1.6]">{step.body}</p>
          </li>
        ))}
      </ol>

      {/* A real code block, because the app's own shell tooling is part of what
          is being sold, and this is the shortest honest demonstration of it. */}
      <div className="mt-6 overflow-hidden rounded-control border border-[var(--glass-border)]">
        <div className="text-faint flex items-center gap-2 border-b border-[var(--glass-border)] px-3 py-2 text-[11.5px]">
          <span className="font-mono">A turn, as it appears in the transcript</span>
        </div>
        <pre className="text-soft overflow-x-auto px-3 py-3 font-mono text-[12.5px] leading-[1.7]">
{`▸ You      Run the test suite and fix whatever fails.

▸ Thinking Four test files reference the old preset id. The first failure
           is the rename; the rest are likely the same cause, so fix one
           and re-run rather than patching each.

  Read     src/lib/background.ts                     · 0.4s
  Grep     "presetById"                    → 6 matches
  Edit     src/lib/background.ts            +4 −2
  Run      bun run test                     · 4.1s   exit 1

▸ Loom     6 failures, all from the same rename. Three more call sites
           used the string literal, and one test asserted the old id.
           Fixing those and re-running.`}
        </pre>
      </div>
    </Section>
  );
}
