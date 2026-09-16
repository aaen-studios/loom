import { useState } from "react";
import { CodeBlock } from "streamdown";
import type { CustomRendererProps } from "streamdown";
import { cn } from "../lib/cn";
import { Markdown } from "./Markdown";
import { CheckIcon, CopyIcon, FileIcon } from "./icons";

const VIEWS = ["preview", "source"] as const;

/**
 * A ```markdown fence: the model's own markdown, rendered as a document.
 *
 * Preview is the default, because a document is meant to be read; the source
 * view is one click away for when the exact text matters. The preview runs
 * with the rich renderers off, so a fence inside a document stays a fence.
 */
export function MarkdownDoc({ code, isIncomplete }: CustomRendererProps) {
  const [view, setView] = useState<(typeof VIEWS)[number]>("preview");
  const [copied, setCopied] = useState(false);

  const copy = () => {
    void navigator.clipboard.writeText(code).catch(() => {});
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };

  return (
    <div className="my-2 overflow-hidden rounded-[14px] border border-[var(--glass-border)] bg-[var(--panel-bg-strong)] shadow-[var(--glass-highlight)]">
      <div className="flex items-center gap-2 border-b border-[var(--glass-border)] bg-[var(--hover-bg)] py-[5px] pr-1.5 pl-3">
        <FileIcon size={12} className="shrink-0 text-faint" />
        <span className="font-mono text-[11.5px] tracking-wide text-faint">markdown</span>
        <span className="min-w-0 flex-1" />
        <div className="flex shrink-0 items-center rounded-capsule border border-[var(--glass-border)] p-[2px]">
          {VIEWS.map((item) => (
            <button
              key={item}
              type="button"
              aria-pressed={view === item}
              onClick={() => setView(item)}
              className={cn(
                "rounded-capsule px-2 py-[1.5px] text-[11.5px] capitalize transition",
                view === item
                  ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                  : "text-faint hover:text-[var(--ink)]",
              )}
            >
              {item}
            </button>
          ))}
        </div>
        <button
          type="button"
          title={copied ? "Copied" : "Copy markdown"}
          aria-label="Copy markdown"
          onClick={copy}
          className="hover-surface grid h-6 w-6 shrink-0 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
        >
          {copied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
        </button>
      </div>

      {view === "source" ? (
        <div className="loom-doc-source">
          <CodeBlock
            code={code}
            language="markdown"
            isIncomplete={isIncomplete}
            lineNumbers={false}
          />
        </div>
      ) : (
        <div className="px-4 py-3">
          <Markdown content={code} allowGeneratedUi={false} />
        </div>
      )}
    </div>
  );
}
