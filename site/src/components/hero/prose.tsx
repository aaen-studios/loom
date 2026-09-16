"use client";

import { cn } from "@/lib/cn";
import { parseBlocks, parseInline } from "@/lib/markdown";

/**
 * The reply text, rendered the way the app's markdown pipeline renders it.
 *
 * The interesting part is the fenced block. A streamed reply contains *half* a
 * code block for as long as it takes to send the closing fence, and the app
 * handles that explicitly: a fence with no end is marked `data-incomplete` and
 * its copy button drops to 35% opacity, because half a block is not worth
 * copying. Reproducing that state is the difference between a still of a reply
 * and a reply that is actually arriving.
 *
 * The parsing is in `@/lib/markdown`, where it is tested against every prefix of
 * the reply the hero renders — the property that matters for streamed text.
 */
export function Prose({ text, streaming }: { text: string; streaming: boolean }) {
  const blocks = parseBlocks(text);

  return (
    <div className="text-[14.5px] leading-[1.75]">
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
            {/* The app's caret trails the last block while a reply streams. */}
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

/**
 * A code block, dressed as Streamdown dresses one: a quiet language header that
 * owns the actions, line numbers painted outside the text so they never travel
 * with a selection, and one surface per block.
 *
 * The measurements are the app's, from the `.loom-markdown [data-streamdown]`
 * rules in `src/styles.css`: a 30px header with 12px left padding and 42px
 * reserved on the right for the action buttons, a body padded 11px by 14px at
 * 13px/1.65, and a gutter 2.5em wide with a 1em gap.
 */
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
    // `relative` because the actions are positioned over the header, exactly as
    // the app's `.loom-markdown [data-streamdown="code-block"]` rule does it.
    <div className="border-[var(--glass-border)] relative my-[0.85em] overflow-hidden rounded-[14px] border bg-[rgb(255_255_255_/_0.62)] dark:bg-[rgb(8_10_16_/_0.55)]">
      <div className="border-[var(--glass-border)] bg-[var(--hover-bg)] text-faint flex min-h-[30px] items-center border-b pr-[42px] pl-3 text-[11.5px] leading-[1.4] whitespace-nowrap">
        <span className="font-mono tracking-[0.04em]">
          {/* A fence with no language still gets a header, so the actions have
              a home — the app's rule fills in the word "text". */}
          {language || "text"}
        </span>
      </div>

      <div className="overflow-auto px-3.5 py-[11px] font-mono text-[13px] leading-[1.65]">
        {lines.map((line, index) => (
          <div key={index} className="flex">
            {/* The gutter: 2.5em wide, 1em gap, right-aligned, 60% opacity so it
                reads as a margin rather than as content. */}
            <span
              className="text-faint shrink-0 text-right opacity-60 select-none"
              style={{ width: "2.5em", marginRight: "1em" }}
              aria-hidden="true"
            >
              {index + 1}
            </span>
            <span className="whitespace-pre">{line || " "}</span>
          </div>
        ))}
      </div>

      {/* The actions sit on the header's right edge: `right: 4px`, a 2px gap,
          24px buttons. Read off `.loom-markdown [data-streamdown]` in the app's
          stylesheet, which is why this is `right-1` and `gap-[2px]` rather than
          the rounder values that look similar.

          The app dims them on an unfinished block, because half a block is not
          worth copying. */}
      <div className="absolute top-0 right-1 flex h-[30px] items-center gap-[2px]">
        <span
          className={cn(
            "text-faint grid h-6 w-6 place-items-center rounded-[7px]",
            incomplete && "opacity-35",
          )}
          aria-hidden="true"
        >
          <CopyIcon />
        </span>
      </div>
    </div>
  );
}

/** Renders `code spans` inline, tolerating an unterminated one mid-stream. */
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
