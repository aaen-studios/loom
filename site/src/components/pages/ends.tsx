import { Code, End, Pick } from "@/components/weave/pass";
import { Icons } from "@/components/weave/icons";
import { passById } from "@/lib/weave/passes";

/**
 * Pass two: the ends.
 *
 * In weaving, the *ends* are the individual warp threads — the count of them is
 * what a cloth is described by. So this is the set of things the product is made
 * of, each one its own thread, standing rather than stacked.
 *
 * Not cards. A card is a bordered box, which says "discrete object"; a thread is a
 * line under tension with something tied to it, which says "part of a set that
 * holds together". After three or four, the eye reads the group as a warp rather
 * than as a grid — which is the whole reason the page is laid out this way.
 *
 * Ordered by what is hardest to find elsewhere rather than by what is easiest to
 * describe. Every claim is drawn from the repository's own README and spec, since
 * an inaccurate feature list is the fastest way to lose a developer on a page whose
 * only job is to hand them a download.
 */
export function Ends() {
  const pass = passById("ends");

  return (
    <Pick
      pass={pass}
      title="Nine threads, held under tension."
      lead="Take any one of them away and the cloth stops being worth weaving. These are the ones Loom is built from."
    >
      <div className="grid gap-x-8 gap-y-9 sm:grid-cols-2 lg:grid-cols-3">
        <End index={1} title="Reasoning, in place">
          A thinking model&rsquo;s notes arrive in the transcript where they
          happened — between the text and the tool calls they produced — one panel
          per thinking spell, each collapsible on its own. You can watch the model
          change its mind instead of only seeing what it settled on.
        </End>

        <End index={2} title="Four agent modes">
          <b className="text-[var(--ink)]">Plan</b> researches and proposes without
          touching anything. <b className="text-[var(--ink)]">Review</b> reports
          severity-ranked findings with a file and a line.{" "}
          <b className="text-[var(--ink)]">Build</b> does the work. Four agent modes
          in all — and the fourth, <b className="text-[var(--ink)]">Atelier</b>,
          goes further: it lets the model edit Loom itself. Personas, prompts,
          skills, MCP servers, providers.
        </End>

        <End index={3} title="Commands that outlive the turn">
          Five minutes into a build, Loom stops waiting — and does not kill the
          process. The command keeps running, gets an id, and streams to a log you
          can read afterwards. Anything long-lived starts with{" "}
          <Code>background: true</Code> and is stoppable, process tree and all.
        </End>

        <End index={4} title="A workspace it can search">
          Point a chat at a folder and index it. Files are chunked and embedded with
          your own provider, so the model can look things up semantically instead of
          grepping blindly. Chats group by workspace, and the folders survive a
          restart.
        </End>

        <End index={5} title="Providers, your choice">
          OpenAI, Anthropic, OpenRouter, DeepSeek, Z.ai, Groq, xAI, Google, OpenCode
          Go and Zen, Ollama and LM Studio — plus any OpenAI-compatible or
          Anthropic-compatible endpoint you point it at. One chat can use a
          different model from the next, and local models are first-class rather
          than an afterthought.
        </End>

        <End index={6} title="MCP servers and skills">
          Connect MCP servers for extra tools, and drop markdown skills into{" "}
          <Code>~/.loom/skills</Code> to have them appear in the composer&rsquo;s{" "}
          <Code>/</Code> menu beside saved prompts and the built-in commands.
        </End>

        <End index={7} title="Replies that render live UI">
          A <Code>loom-ui</Code> fence becomes a real, themed widget inside the reply
          — a comparison, a small table, a set of options to pick from — sanitized
          against an allowlist with no scripts and no event handlers. The model gets
          to hand you something you can use instead of describing it.
        </End>

        <End index={8} title="Computer use, with a brake">
          With the chip armed, the model can see the screen and drive the mouse and
          keyboard, run UI Automation, and launch applications. Real input from you
          pauses the whole turn, <kbd className="kbd">Ctrl</kbd>{" "}
          <kbd className="kbd">Alt</kbd> <kbd className="kbd">Esc</kbd> stops it from
          anywhere, and a pill on your monitor always shows what it is doing.
        </End>

        <End index={9} title="Usage you can see">
          Settings → Usage shows live vendor limits wherever the provider exposes
          them, while a local table totals tokens and list-price cost per provider.
          The composer carries the active model&rsquo;s tightest window as a small
          badge. None of it is uploaded anywhere.
        </End>
      </div>

      {/* The tail of the warp: everything real that did not earn a thread of its
          own, stated once, plainly, instead of padded into a tenth card. */}
      <p className="text-faint mt-10 flex flex-wrap items-start gap-2 text-[13px] leading-[1.7]">
        <span className="text-[var(--accent)] mt-[3px] shrink-0">
          <Icons.Widget />
        </span>
        <span>
          Also on the loom: a quick-ask overlay on{" "}
          <kbd className="kbd">Ctrl</kbd> <kbd className="kbd">Shift</kbd>{" "}
          <kbd className="kbd">Space</kbd> that summons a new chat from anywhere, a
          tray icon, launch at login, image generation, subagents, attachments
          including text-layer PDFs, and a signed updater that verifies every
          release before it touches your install.
        </span>
      </p>
    </Pick>
  );
}
