"use client";

import { parseBlocks, parseInline } from "@/lib/markdown";

/**
 * Renders streamed assistant text.
 *
 * The interesting state is the fenced block. A reply that is still arriving holds
 * *half* a code block for as long as the closing fence takes to come in, and the
 * app treats that as a real state rather than an error: the block is marked
 * incomplete and its copy action dims, because half a block is not worth copying.
 * Drawing that state is the difference between a still of a reply and a reply that
 * is actually arriving.
 *
 * The code block's measurements are read off the app's own
 * `.loom-markdown [data-streamdown]` rules in `src/styles.css` — a 30px header
 * with 12px left padding and 42px held back for the actions, a body padded 11px by
 * 14px at 13px/1.65, and a line-number gutter 2.5em wide. Those rules are
 * deliberately outside the shared token regions, so the numbers are transcribed
 * rather than inherited. `verify-tokens.mjs` is what guarantees they had to be.
 *
 * The parsing lives in `@/lib/markdown`, where it is tested against every prefix of
 * the reply this page renders.
 */
export function Prose({ text, streaming }: { text: string; streaming: boolean }) {
  const blocks = parseBlocks(text);

  return (
    <div className="text-[14px] leading-[1.7]">
      {blocks.map((block, index) =>
        block.kind === "code" ? (
          <CodeBlock
            key={index}
            language={block.language}
            code={block.code}
            incomplete={block.incomplete}
          />
        ) : (
          <p key={index} className={index > 0 ? "mt-3" : undefined}>
            <Inline text={block.text} />
            {/* The caret trails the last block while the reply is still coming. */}
            {streaming && index === blocks.length - 1 && <Caret />}
          </p>
        ),
      )}
    </div>
  );
}

export function Caret() {
  return (
    <span className="cursor-blink bg-[var(--ink-soft)] ml-0.5 inline-block h-[15px] w-[7px] translate-y-[3px] rounded-[2px]" />
  );
}

function CodeBlock({
  language,
  code,
  incomplete,
}: {
  language: string;
  code: string;
  incomplete: boolean;
}) {
  const lines = code.split("\n");

  return (
    <div className="border-[var(--glass-border)] relative my-[0.85em] overflow-hidden rounded-[14px] border bg-[rgb(255_255_255_/_0.62)] dark:bg-[rgb(8_10_16_/_0.55)]">
      <div className="border-[var(--glass-border)] bg-[var(--hover-bg)] text-faint flex min-h-[30px] items-center border-b pr-[42px] pl-3 text-[11.5px] leading-[1.4] whitespace-nowrap">
        <span className="font-mono tracking-[0.04em]">
          {/* A fence with no language still gets a header, so the actions have a
              home. The app fills in the word "text" from CSS; doing it in the
              markup avoids depending on an `:empty::before` surviving
              minification. */}
          {language || "text"}
        </span>
      </div>

      <div className="overflow-auto px-3.5 py-[11px] font-mono text-[12.5px] leading-[1.65]">
        {lines.map((line, index) => (
          <div key={index} className="flex">
            <span
              className="text-faint shrink-0 text-right opacity-60 select-none"
              style={{ width: "2.5em", marginRight: "1em" }}
              aria-hidden="true"
            >
              {index + 1}
            </span>
            {/* A blank line still needs a box, or it collapses and every number
                below it shifts up by one. */}
            <span className="whitespace-pre">{line || " "}</span>
          </div>
        ))}
      </div>

      {/* On the header's right edge: `right: 4px`, a 2px gap, 24px targets.
          Dimmed on an unfinished block, which is the app's own signal that half a
          block is not worth copying. */}
      <div className="absolute top-0 right-1 flex h-[30px] items-center gap-[2px]">
        <span
          className={
            "text-faint grid h-6 w-6 place-items-center rounded-[7px]" +
            (incomplete ? " opacity-35" : "")
          }
          aria-hidden="true"
        >
          <CopyIcon />
        </span>
      </div>
    </div>
  );
}

/** Renders `code spans`, tolerating one that has not been closed yet. */
function Inline({ text }: { text: string }) {
  return (
    <>
      {parseInline(text).map((part, index) =>
        part.code ? (
          <code
            key={index}
            className="bg-[var(--ink-ghost)] rounded-[6px] px-[0.4em] py-[0.12em] font-mono text-[0.9em]"
          >
            {part.text}
          </code>
        ) : (
          <span key={index}>{part.text}</span>
        ),
      )}
    </>
  );
}

function CopyIcon() {
  return (
    <svg
      width={14}
      height={14}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="9" y="9" width="11" height="11" rx="2.5" />
      <path d="M5 15V6.5A2.5 2.5 0 0 1 7.5 4H15" />
    </svg>
  );
}
