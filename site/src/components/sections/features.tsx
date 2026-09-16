import { Feature, FeatureGrid, Icons, Section } from "../section";

/**
 * What Loom does, in the order a new user cares about it.
 *
 * Every claim here is drawn from `docs/spec.md` and `README.md` rather than
 * invented for the marketing copy — the app is unusual enough that the honest
 * list is more persuasive than a generic one, and an inaccurate feature list is
 * the fastest way to lose a developer's trust on a download page.
 *
 * Ordered by what is hardest to find elsewhere: visible reasoning, the
 * four-mode agent model, and per-chat workspace indexing first; the conveniences
 * (overlay, updater) last.
 */
export function Features() {
  return (
    <Section
      id="features"
      eyebrow="What it does"
      title="An agent you can actually watch work."
      lead="Most chat clients hide the interesting part. Loom keeps it on screen: the reasoning behind a reply, every tool it calls, the commands it runs, and the task list it maintains — each in the order it happened."
    >
      <FeatureGrid>
        <Feature icon={<Icons.Brain />} title="Reasoning, in place">
          A thinking model's notes arrive in the transcript where they happened,
          between the text and the tool calls they produced — one panel per
          thinking spell, each independently collapsible. You can watch the model
          change its mind instead of seeing only the answer.
        </Feature>

        <Feature icon={<Icons.Shield />} title="Four agent modes">
          <b className="text-[var(--ink)]">Plan</b> researches and proposes without
          touching anything. <b className="text-[var(--ink)]">Review</b> reports
          severity-ranked findings with file and line.{" "}
          <b className="text-[var(--ink)]">Build</b> does the work.{" "}
          <b className="text-[var(--ink)]">Atelier</b> goes further and lets the
          model edit Loom itself — personas, prompts, skills, MCP servers and
          providers.
        </Feature>

        <Feature icon={<Icons.Terminal />} title="Commands that outlive the turn">
          Five minutes into a build, Loom stops waiting — but it does not kill
          the process. The command keeps running, gets an id, and streams to a log
          you can read later. Anything long-lived starts with{" "}
          <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
            background: true
          </code>{" "}
          and is stoppable, process tree and all.
        </Feature>

        <Feature icon={<Icons.Folder />} title="A workspace it can search">
          Point a chat at a folder and index it. Files are chunked and embedded
          with your own provider, so the model can look things up semantically
          instead of grepping blindly. Chats are grouped by workspace, and folders
          survive a restart.
        </Feature>

        <Feature icon={<Icons.Layers />} title="Providers, your choice">
          OpenAI, Anthropic, OpenRouter, DeepSeek, Z.ai, Groq, xAI, Google,
          OpenCode Go and Zen, Ollama and LM Studio — plus any OpenAI-compatible or
          Anthropic endpoint you point it at. One chat can use a different model
          from the next, and local models are first-class rather than an
          afterthought.
        </Feature>

        <Feature icon={<Icons.Plug />} title="MCP servers and skills">
          Connect MCP servers for extra tools, and drop markdown skills into{" "}
          <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
            ~/.loom/skills
          </code>{" "}
          to have them appear in the composer&rsquo;s{" "}
          <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
            /
          </code>{" "}
          menu alongside saved prompts and built-in commands.
        </Feature>

        <Feature icon={<Icons.Widget />} title="Replies that render live UI">
          A{" "}
          <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
            loom-ui
          </code>{" "}
          code fence becomes a real, themed widget inside the reply — a
          comparison, a form, a chart — sanitized against an allowlist with no
          scripts or event handlers. The model can hand you something you can use
          instead of describing it.
        </Feature>

        <Feature icon={<Icons.Mouse />} title="Computer use, with a brake">
          With the chip armed, the model can see the screen and drive the mouse
          and keyboard, run UI Automation, and launch apps. Real input from you
          pauses the whole turn,{" "}
          <kbd className="kbd">Ctrl</kbd> <kbd className="kbd">Alt</kbd>{" "}
          <kbd className="kbd">Esc</kbd> stops it from anywhere, and a pill on
          your monitor always shows what it is doing.
        </Feature>

        <Feature icon={<Icons.Chart />} title="Usage you can see">
          Settings → Usage shows live vendor limits where the provider exposes
          them — subscription windows, remaining credit, plan quota — while a
          local table totals tokens and list-price cost per provider. The composer
          carries the active model&rsquo;s tightest window as a small badge.
          Nothing is uploaded anywhere.
        </Feature>
      </FeatureGrid>

      <p className="text-faint mt-6 text-[13px]">
        Also: a quick-ask overlay on <kbd className="kbd">Ctrl</kbd>{" "}
        <kbd className="kbd">Shift</kbd> <kbd className="kbd">Space</kbd> that
        summons a new chat from anywhere, a tray icon, launch at login, image
        generation, subagents, attachments including text-layer PDFs, and a signed
        updater that verifies every release before it touches your install.
      </p>
    </Section>
  );
}
