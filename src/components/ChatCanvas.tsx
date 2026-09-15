import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { formatUsage, parseAttachments, parseUsage } from "../lib/messageExtra";
import type { Message, ToolCallRecord } from "../types";
import { useChat } from "../stores/chat";
import { useProviders } from "../stores/providers";
import { useUi } from "../stores/ui";
import { AttachmentStrip } from "./AttachmentChips";
import { Composer } from "./Composer";
import { Markdown } from "./Markdown";
import { PermissionCard, ToolCallList } from "./ToolCalls";
import { BrainIcon, LoomMark } from "./icons";

function Reasoning({ text, streaming }: { text: string; streaming: boolean }) {
  const [open, setOpen] = useState(true);

  useEffect(() => {
    if (streaming) setOpen(true);
  }, [streaming]);

  return (
    <div className="mb-2">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex items-center gap-1.5 text-[12px] text-faint hover:text-[var(--ink)]"
      >
        <BrainIcon size={13} />
        {open ? "Hide" : "Show"} thinking
        {streaming && <span className="cursor-blink">•</span>}
      </button>
      {open && (
        <div className="mt-2 border-l-2 border-[var(--glass-border)] pl-3 text-[13px] leading-6 whitespace-pre-wrap text-soft italic">
          {text}
        </div>
      )}
    </div>
  );
}

function MessageRow({
  message,
  streaming,
  runningTools,
}: {
  message: Message;
  streaming: boolean;
  runningTools: boolean;
}) {
  const attachments = parseAttachments(message.extra);
  const usage = parseUsage(message.extra);

  if (message.role === "user") {
    return (
      <div className="flex flex-col items-end gap-1.5">
        <div className="flex max-w-[85%] flex-col items-end">
          {attachments.length > 0 && (
            <div className="mb-1.5 flex flex-wrap justify-end gap-2">
              <AttachmentStrip attachments={attachments} />
            </div>
          )}
          {message.content && (
            <div className="panel-strong max-w-full select-text rounded-2xl px-3.5 py-2.5 text-[14.5px] leading-6 whitespace-pre-wrap">
              {message.content}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex gap-3">
      <div className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full border border-[var(--glass-border)] text-soft">
        <LoomMark size={15} />
      </div>
      <div className="min-w-0 flex-1 select-text pt-0.5">
        {message.reasoning && (
          <Reasoning text={message.reasoning} streaming={streaming} />
        )}
        <ToolCallList messageId={message.id} extra={message.extra} />
        {message.content ? (
          <Markdown content={message.content} />
        ) : (
          streaming &&
          !runningTools && (
            <span className="cursor-blink inline-block h-4 w-[7px] translate-y-[3px] rounded-[2px] bg-[var(--ink-soft)]" />
          )
        )}
        {!streaming && usage && (
          <p className="mt-1.5 text-[11.5px] text-faint">{formatUsage(usage)}</p>
        )}
      </div>
    </div>
  );
}

function hasRunningTools(
  messageId: string,
  liveTools: Record<string, ToolCallRecord[]>,
): boolean {
  return (liveTools[messageId] ?? []).some((call) => call.status === "running");
}

/**
 * The chat canvas: a hero composer when empty, the transcript plus a docked
 * composer once there are messages. Scrolling only follows new output while
 * you are already at the bottom.
 */
export function ChatCanvas() {
  const messages = useChat((state) => state.messages);
  const busy = useChat(
    (state) => (state.activeId ? state.busy[state.activeId] : false) ?? false,
  );
  const permission = useChat((state) => state.permission);
  const liveTools = useChat((state) => state.liveTools);
  const error = useChat((state) => state.error);
  const clearError = useChat((state) => state.clearError);
  const retryLast = useChat((state) => state.retryLast);
  const modelCount = useProviders((state) => state.models.length);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);

  const scrollRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const [pinned, setPinned] = useState(true);

  const onScroll = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    const distance =
      element.scrollHeight - element.scrollTop - element.clientHeight;
    setPinned(distance < 90);
  }, []);

  useEffect(() => {
    if (pinned) {
      endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
    }
  }, [messages, pinned]);

  const jumpToLatest = () => {
    setPinned(true);
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  };

  if (messages.length === 0) {
    return (
      <section className="relative flex h-full min-w-0 items-center justify-center px-6 pb-16">
        <div className="w-full max-w-2xl -translate-y-8">
          <Composer variant="hero" />
          {modelCount === 0 ? (
            <p className="mt-3 text-center text-[12.5px] text-faint">
              No models yet.{" "}
              <button
                type="button"
                onClick={() => setSettingsOpen(true)}
                className="text-[var(--accent)] hover:underline"
              >
                Add a provider
              </button>{" "}
              to start chatting.
            </p>
          ) : (
            <p className="mt-3 text-center text-[12px] text-faint">
              Enter to send · Shift+Enter for a new line · Ctrl+K for chats
            </p>
          )}
        </div>
      </section>
    );
  }

  return (
    <section className="relative flex h-full min-w-0 flex-col">
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-y-auto px-6 pb-6 pt-2"
      >
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-5">
          {messages.map((message) => (
            <MessageRow
              key={message.id}
              message={message}
              streaming={busy && message.role === "assistant"}
              runningTools={hasRunningTools(message.id, liveTools)}
            />
          ))}
          <div ref={endRef} />
        </div>
      </div>

      {!pinned && (
        <button
          type="button"
          onClick={jumpToLatest}
          className="panel-strong absolute bottom-32 left-1/2 z-20 -translate-x-1/2 rounded-full px-3 py-1.5 text-[12px] text-soft"
        >
          Jump to latest ↓
        </button>
      )}

      {error && (
        <div className="mx-auto w-full max-w-3xl px-6">
          <div className="flex items-start gap-2 rounded-xl border border-[var(--danger)]/40 bg-[var(--danger)]/10 px-3 py-2 text-[13px]">
            <span className="min-w-0 flex-1 break-words">{error}</span>
            <button
              type="button"
              onClick={() => void retryLast()}
              className="shrink-0 font-medium text-[var(--accent)] hover:underline"
            >
              Retry
            </button>
            <button
              type="button"
              onClick={clearError}
              className="shrink-0 text-faint hover:text-[var(--ink)]"
            >
              Dismiss
            </button>
          </div>
        </div>
      )}

      {permission && (
        <div className="px-6 pb-2">
          <PermissionCard permission={permission} />
        </div>
      )}

      <div className={cn("px-6 pb-5 pt-3")}>
        <div className="mx-auto w-full max-w-3xl">
          <Composer />
        </div>
      </div>
    </section>
  );
}

