import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { useChat } from "../stores/chat";
import { ArrowUpIcon, ChevronDownIcon, PaperclipIcon, SparkIcon } from "./icons";

interface ComposerProps {
  variant?: "hero" | "docked";
}

/**
 * The message composer. Auto-growing input, model chip, attach + send.
 * In M0 the model chip is honest about there being no providers yet; M1 wires
 * it to the provider/model picker.
 */
export function Composer({ variant = "docked" }: ComposerProps) {
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const send = useChat((state) => state.send);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 216)}px`;
  }, [value]);

  const submit = () => {
    if (!value.trim()) return;
    send(value);
    setValue("");
  };

  return (
    <div
      className={cn(
        "glass w-full rounded-[24px] p-2.5",
        variant === "hero" && "animate-fade-up",
      )}
    >
      <textarea
        ref={textareaRef}
        rows={1}
        value={value}
        placeholder="Do anything…"
        spellCheck={false}
        onChange={(event) => setValue(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault();
            submit();
          }
        }}
        className="block max-h-[216px] w-full resize-none bg-transparent px-3 pt-2 text-[15px] leading-6 text-[var(--ink)] placeholder:text-[var(--ink-faint)]"
      />

      <div className="mt-1.5 flex items-center gap-1.5 pl-0.5">
        <button
          type="button"
          disabled
          title="Providers arrive in M1"
          className="flex items-center gap-1.5 rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12.5px] text-faint"
        >
          <SparkIcon size={14} />
          No model
          <ChevronDownIcon size={13} />
        </button>

        <div className="flex-1" />

        <button
          type="button"
          disabled
          title="Attachments arrive in M2"
          aria-label="Attach"
          className="glass-hover grid h-8 w-8 place-items-center rounded-full text-faint opacity-70"
        >
          <PaperclipIcon size={16} />
        </button>

        <button
          type="button"
          onClick={submit}
          disabled={!value.trim()}
          aria-label="Send"
          title="Send"
          className={cn(
            "grid h-9 w-9 place-items-center rounded-full bg-[var(--control-bg)] text-[var(--control-ink)]",
            "transition enabled:hover:scale-[1.04] enabled:active:scale-95 disabled:opacity-40",
          )}
        >
          <ArrowUpIcon size={17} />
        </button>
      </div>
    </div>
  );
}
