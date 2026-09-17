import { useState } from "react";
import { cn } from "../lib/cn";
import type { PendingQuestion } from "../types";
import { useChat } from "../stores/chat";

/**
 * The model's question, shown in the composer's place while the turn waits.
 * Options and a typed answer are both optional; Skip hands the turn back to
 * the model to continue on its own judgement.
 */
export function QuestionCard({ question }: { question: PendingQuestion }) {
  const answer = useChat((state) => state.answerQuestion);
  const [selected, setSelected] = useState<string[]>([]);
  const [text, setText] = useState("");
  const ask = question.question;

  const toggle = (label: string) => {
    setSelected((current) => {
      if (ask.allowMultiple) {
        return current.includes(label)
          ? current.filter((item) => item !== label)
          : [...current, label];
      }
      return current.includes(label) ? [] : [label];
    });
  };

  // Submit in the order the options were shown, per the answer's contract.
  const picked = ask.options
    .map((option) => option.label)
    .filter((label) => selected.includes(label));
  const typed = ask.allowFreeText ? text.trim() : "";
  const canSubmit = picked.length > 0 || typed.length > 0;

  const submit = () => {
    if (!canSubmit) return;
    void answer(question, {
      selected: picked,
      text: typed.length > 0 ? typed : null,
      cancelled: false,
    });
  };

  return (
    <LiquidSurface
      surface="cards"
      layout="block"
      className="animate-fade-up w-full rounded-sheet"
      tint="var(--panel-bg-strong)"
    >
      <div className="px-3 pt-2">
        {ask.header && (
          <p className="mb-1 text-[11px] font-medium tracking-wide text-faint uppercase">
            {ask.header}
          </p>
        )}
        <p className="text-[14.5px] leading-6 text-[var(--ink)]">{ask.question}</p>
      </div>

      {ask.options.length > 0 && (
        <div className="mt-2.5 flex flex-col gap-1.5 px-3">
          {ask.options.map((option) => {
            const active = selected.includes(option.label);
            return (
              <button
                key={option.label}
                type="button"
                onClick={() => toggle(option.label)}
                className={cn(
                  "rounded-control border px-3 py-2 text-left transition",
                  active
                    ? "border-[var(--accent)] bg-[var(--hover-bg)]"
                    : "border-[var(--glass-border)] hover:bg-[var(--hover-bg)]",
                )}
              >
                <span className="text-[13.5px] text-[var(--ink)]">{option.label}</span>
                {option.description && (
                  <span className="mt-0.5 block text-[12px] text-faint">
                    {option.description}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}

      {ask.allowFreeText && (
        <textarea
          rows={1}
          value={text}
          autoFocus={ask.options.length === 0}
          placeholder={ask.options.length > 0 ? "Or type an answer…" : "Type an answer…"}
          spellCheck={false}
          onChange={(event) => setText(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              submit();
            }
          }}
          className="mt-2.5 block max-h-[140px] w-full resize-none bg-transparent px-3 pt-2 text-[14.5px] leading-6 text-[var(--ink)] placeholder:text-[var(--ink-faint)]"
        />
      )}

      <div className="mt-1.5 flex items-center gap-1.5 pl-3">
        <button
          type="button"
          onClick={() => void answer(question, { selected: [], text: null, cancelled: true })}
          title="Let the model continue without an answer"
          className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft hover:text-[var(--ink)]"
        >
          Skip
        </button>

        <div className="flex-1" />

        <button
          type="button"
          onClick={submit}
          disabled={!canSubmit}
          className="rounded-full bg-[var(--control-bg)] px-4 py-1.5 text-[12.5px] font-medium text-[var(--control-ink)] disabled:opacity-40"
        >
          Answer
        </button>
      </div>
    </LiquidSurface>
  );
}
