import { useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import type { ChatMessage } from "../types";
import { useChat } from "../stores/chat";
import { Composer } from "./Composer";
import { LoomMark } from "./icons";

function MessageRow({ message }: { message: ChatMessage }) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="glass-strong max-w-[85%] select-text rounded-2xl px-3.5 py-2.5 text-[14.5px] leading-6 whitespace-pre-wrap">
          {message.content}
        </div>
      </div>
    );
  }

  return (
    <div className="flex gap-3">
      <div className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full border border-[var(--glass-border)] text-soft">
        <LoomMark size={15} />
      </div>
      <div className="min-w-0 flex-1 select-text pt-0.5 text-[14.5px] leading-7 whitespace-pre-wrap">
        {message.content}
        {message.pending && (
          <span className="cursor-blink ml-0.5 inline-block h-4 w-[7px] translate-y-[2px] rounded-[2px] bg-[var(--ink-soft)]" />
        )}
      </div>
    </div>
  );
}

/**
 * The chat canvas: hero composer when empty, scrolling transcript + docked
 * composer once messages exist.
 */
export function ChatCanvas() {
  const messages = useChat((state) => state.messages);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages]);

  if (messages.length === 0) {
    return (
      <section className="relative z-10 flex min-w-0 flex-1 flex-col items-center justify-center px-6 pb-10">
        <div className="w-full max-w-2xl">
          <div className="mb-5 text-center">
            <h1 className="text-[22px] font-semibold tracking-[-0.015em]">
              Loom
            </h1>
            <p className="mt-1 text-[13px] text-soft">
              M0 shell · engine, providers, and tools arrive in M1–M3
            </p>
          </div>
          <Composer variant="hero" />
        </div>
      </section>
    );
  }

  return (
    <section className="relative z-10 flex min-w-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6 pt-20">
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-5">
          {messages.map((message) => (
            <MessageRow key={message.id} message={message} />
          ))}
          <div ref={endRef} />
        </div>
      </div>

      <div className={cn("px-6 pb-5")}>
        <div className="mx-auto w-full max-w-3xl">
          <Composer />
        </div>
      </div>
    </section>
  );
}
