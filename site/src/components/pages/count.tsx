import { Code, Pick } from "@/components/weave/pass";
import { passById } from "@/lib/weave/passes";

/**
 * Pass four: the count.
 *
 * A cloth is described by its thread count — how many ends to the inch, and what
 * they are made of. So this pass is how many providers Loom can be strung with, and
 * what that means in practice.
 *
 * ---------------------------------------------------------------------------
 * The one pass that breaks the measure
 * ---------------------------------------------------------------------------
 *
 * Everything else on the page sits in columns 2–11. This one's grid runs the full
 * twelve, because it is the only pass whose *subject* is quantity: a thread count
 * shown at the same width as a paragraph is not making its point. One wide moment in
 * a page of narrow ones is rhythm; several would be a shambles.
 *
 * The ends are drawn as *stubs* — a short length of warp with a knot at the top and
 * the name hanging off it — rather than as a row of chips. A chip is a pill with a
 * word in it; a stub says these are interchangeable lengths of the same material,
 * which is the actual claim. The last two are longer and lifted, because "any
 * endpoint" is not one more supplier, it is the category.
 *
 * Named as text and not as logos, which is a decision rather than a shortcut.
 * Provider logo packs are licensed, they go stale every time a company rebrands, and
 * a wall of trademarks on a download page reads as a claim that those companies
 * endorse this one.
 */
const PROVIDERS = [
  "OpenAI",
  "Anthropic",
  "OpenRouter",
  "DeepSeek",
  "Z.ai",
  "Groq",
  "xAI",
  "Google",
  "OpenCode Go",
  "OpenCode Zen",
  "Ollama",
  "LM Studio",
] as const;

/** The open ends: categories rather than suppliers, drawn longer. */
const OPEN_ENDS = [
  "Any OpenAI-compatible endpoint",
  "Any Anthropic-compatible endpoint",
] as const;

export function Count() {
  const pass = passById("count");

  return (
    <Pick
      pass={pass}
      // The full twelve columns, uniquely on this pass.
      bodyFrom={1}
      bodySpan={12}
      title={`${PROVIDERS.length} ends, and your own supply.`}
      lead="Loom has no subscription and never proxies your traffic. Add a key and it goes straight to whoever you chose — or point it at a model running on the machine in front of you."
    >
      {/* The count itself, before the names. A number is the claim; the list is the
          evidence. */}
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1 border-b border-[var(--glass-border)] pb-6">
        <span className="text-[56px] leading-none font-medium tracking-tight tabular-nums">
          {PROVIDERS.length + OPEN_ENDS.length}
        </span>
        <span className="text-soft text-[14px] leading-6">
          ways to be strung
          <span className="text-faint"> — {PROVIDERS.length} named, {OPEN_ENDS.length} open</span>
        </span>
      </div>

      <ul className="mt-8 grid grid-cols-3 gap-x-4 gap-y-7 sm:grid-cols-4 lg:grid-cols-6">
        {PROVIDERS.map((provider) => (
          <End key={provider} label={provider} />
        ))}
      </ul>

      {/* The open ends, on their own row and drawn longer, because they are a
          different kind of thing from the twelve above. */}
      <ul className="mt-8 grid gap-x-4 gap-y-7 sm:grid-cols-2">
        {OPEN_ENDS.map((label) => (
          <End key={label} label={label} open />
        ))}
      </ul>

      <div className="mt-14 grid gap-3 sm:grid-cols-3">
        <Note title="Keys stay in Windows">
          API keys are written to Windows Credential Manager — not to a config file,
          not to a log, and not to anything Loom syncs. The rest of your data lives in{" "}
          <Code>~/.loom</Code> on your own disk.
        </Note>
        <Note title="Models are looked up, then editable">
          Loom asks the provider which models exist and merges the answer over a
          bundled catalogue. Anything it gets wrong — a context window, an output cap
          — you can correct by hand, and your correction is never overwritten.
        </Note>
        <Note title="Favourites float">
          Model pickers get long. Starred and recently-used models rise to the top, and
          you can add a model id by hand for providers whose model-list endpoint is
          unavailable or unhelpful.
        </Note>
      </div>
    </Pick>
  );
}

/**
 * One end: a length of warp with a knot at the top and a name beneath.
 *
 * `open` stretches the thread, which is the visual difference between a named
 * supplier and a category you can point the app at.
 */
function End({ label, open = false }: { label: string; open?: boolean }) {
  return (
    <li className="flex flex-col">
      <span
        aria-hidden="true"
        className="rail"
        style={{ height: open ? "2.5rem" : "1.4rem" }}
      />
      <span aria-hidden="true" className="knot-lit knot mt-[-3px] self-start" />
      <span className="text-soft mt-3 text-[13px] leading-[1.45]">{label}</span>
    </li>
  );
}

function Note({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="panel rounded-control p-4">
      <h3 className="text-[13.5px] font-medium">{title}</h3>
      <p className="text-soft mt-2 text-[13px] leading-[1.6]">{children}</p>
    </div>
  );
}
