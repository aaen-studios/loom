import { Section } from "../section";

/**
 * The providers Loom ships presets for.
 *
 * Listed as text rather than logos on purpose. Provider logo packs are
 * licensed, they go stale every time a company rebrands, and a wall of
 * trademarks on a download page reads as an endorsement claim. A plain list is
 * more useful and less to maintain.
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

export function Providers() {
  return (
    <Section
      eyebrow="Bring your own keys"
      // Twelve names follow this, and the copy two lines down says "Go and Zen"
      // — which is two of the twelve, not a thirteenth. The count has to match
      // what is actually listed or the section contradicts itself.
      title="Twelve providers, and your own endpoint."
      lead="Loom has no subscription and never proxies your traffic. Add a key and it goes straight to whoever you chose — or point it at a model running on the machine in front of you."
    >
      <div className="flex flex-wrap gap-2">
        {PROVIDERS.map((provider) => (
          <span
            key={provider}
            className="chip px-3 py-1.5 text-[13px]"
          >
            {provider}
          </span>
        ))}
        <span className="chip px-3 py-1.5 text-[13px]">
          Any OpenAI-compatible endpoint
        </span>
        <span className="chip px-3 py-1.5 text-[13px]">
          Any Anthropic-compatible endpoint
        </span>
      </div>

      <div className="mt-6 grid gap-3 sm:grid-cols-3">
        <Note title="Keys stay in Windows">
          API keys are written to Windows Credential Manager, not to a config
          file, a log, or anything Loom syncs. The rest of your data lives in{" "}
          <code className="bg-[var(--ink-ghost)] rounded-[6px] px-1 py-[0.1em] font-mono text-[12px]">
            ~/.loom
          </code>{" "}
          on your own disk.
        </Note>
        <Note title="Models are looked up, then editable">
          Loom asks the provider which models exist and merges that over a
          bundled catalogue. Anything it gets wrong — a context window, an output
          cap — you can correct by hand, and your correction is never overwritten.
        </Note>
        <Note title="Favourites float">
          Model pickers get long. Starred models and recently used ones rise to
          the top, and you can add a model id by hand for providers whose model
          list endpoint is unavailable.
        </Note>
      </div>
    </Section>
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
