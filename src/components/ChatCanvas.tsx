import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { compactTokens } from "../lib/format";
import { formatUsage, parseAttachments, parseError, parseUsage } from "../lib/messageExtra";
import type { Message, ToolCallRecord } from "../types";
import { useChat } from "../stores/chat";
import { useProviders } from "../stores/providers";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import { AttachmentStrip } from "./AttachmentChips";
import { Composer } from "./Composer";
import { Markdown } from "./Markdown";
import { PermissionCard, ToolCallList } from "./ToolCalls";
import {
  BrainIcon,
  CopyIcon,
  EditIcon,
  LoomMark,
  RefreshIcon,
  SearchIcon,
  TrashIcon,
} from "./icons";

function Reasoning({
  text,
  streaming,
  hasContent,
  revealed,
}: {
  text: string;
  streaming: boolean;
  hasContent: boolean;
  revealed: boolean;
}) {
  const showThinking = useSettings((state) => state.config.interface.showThinking);
  const [open, setOpen] = useState(showThinking === "expanded");

  useEffect(() => {
    setOpen(showThinking === "expanded");
  }, [showThinking]);

  // Hidden by default, unless this reply was opened from its actions.
  if (showThinking === "hidden" && !revealed) return null;
  // While the model is still thinking, the transcript shows a small indicator
  // instead of the running commentary.
  if (streaming && !hasContent) return null;

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


/** Title, token totals for this chat, and a search box for the transcript. */
function ChatHeader({
  query,
  onQuery,
  matches,
}: {
  query: string;
  onQuery: (value: string) => void;
  matches: number;
}) {
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const messages = useChat((state) => state.messages);

  const totals = messages.reduce(
    (sum, message) => {
      const usage = parseUsage(message.extra);
      return {
        input: sum.input + (usage?.inputTokens ?? 0),
        output: sum.output + (usage?.outputTokens ?? 0),
      };
    },
    { input: 0, output: 0 },
  );

  return (
    <div className="flex h-11 shrink-0 items-center gap-3 border-b border-[var(--glass-border)] px-5">
      <span className="min-w-0 truncate text-[13px] text-soft">
        {session?.title || "New chat"}
      </span>
      {(totals.input > 0 || totals.output > 0) && (
        <span
          className="shrink-0 text-[11.5px] text-faint"
          title="Tokens used in this chat"
        >
          {compactTokens(totals.input) ?? 0} in · {compactTokens(totals.output) ?? 0} out
        </span>
      )}
      <div className="flex-1" />
      <div className="flex items-center gap-1.5">
        <SearchIcon size={14} className="shrink-0 text-faint" />
        <input
          value={query}
          placeholder="Search chat"
          onChange={(event) => onQuery(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") onQuery("");
          }}
          className="w-40 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2 py-1 text-[12px] placeholder:text-[var(--ink-faint)]"
        />
        {query.trim() && (
          <span className="shrink-0 text-[11.5px] text-faint">
            {matches} match{matches === 1 ? "" : "es"}
          </span>
        )}
      </div>
    </div>
  );
}

/** Actions revealed when hovering a message. */
function MessageActions({
  message,
  onRevealThinking,
}: {
  message: Message;
  onRevealThinking: () => void;
}) {
  const regenerate = useChat((state) => state.regenerate);
  const editFrom = useChat((state) => state.editFrom);
  const removeMessage = useChat((state) => state.removeMessage);
  const setDraft = useChat((state) => state.setDraft);
  const showThinking = useSettings((state) => state.config.interface.showThinking);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(message.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // clipboard denied: nothing to do but stay quiet
    }
  };

  return (
    <div className="ml-1 flex shrink-0 items-start gap-0.5 opacity-0 transition group-hover:opacity-100 focus-within:opacity-100">
      <button
        type="button"
        title={copied ? "Copied" : "Copy"}
        aria-label="Copy message"
        onClick={() => void copy()}
        className="hover-surface grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
      >
        <CopyIcon size={14} />
      </button>
      {message.role === "user" ? (
        <button
          type="button"
          title="Edit and resend from here"
          aria-label="Edit message"
          onClick={() => {
            void editFrom(message.id).then((text) => {
              if (text !== null) setDraft(text);
            });
          }}
          className="hover-surface grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
        >
          <EditIcon size={14} />
        </button>
      ) : (
        <button
          type="button"
          title="Regenerate this reply"
          aria-label="Regenerate"
          onClick={() => void regenerate(message.id)}
          className="hover-surface grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
        >
          <RefreshIcon size={14} />
        </button>
      )}
      {message.role === "assistant" && message.reasoning && showThinking === "hidden" && (
        <button
          type="button"
          title="Show thinking for this reply"
          aria-label="Show thinking"
          onClick={onRevealThinking}
          className="hover-surface grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--ink)]"
        >
          <BrainIcon size={14} />
        </button>
      )}
      <button
        type="button"
        title="Delete message"
        aria-label="Delete message"
        onClick={() => void removeMessage(message.id)}
        className="hover-surface grid h-7 w-7 place-items-center rounded-control text-faint hover:text-[var(--danger)]"
      >
        <TrashIcon size={14} />
      </button>
    </div>
  );
}

function MessageRow({
  message,
  streaming,
  runningTools,
  revealThinking,
  onRevealThinking,
}: {
  message: Message;
  streaming: boolean;
  runningTools: boolean;
  revealThinking: boolean;
  onRevealThinking: () => void;
}) {
  const attachments = parseAttachments(message.extra);
  const usage = parseUsage(message.extra);
  const failure = parseError(message.extra);

  if (message.role === "user") {
    return (
      <div className="group flex flex-col items-end gap-1.5">
        <MessageActions message={message} onRevealThinking={onRevealThinking} />
        <div className="flex max-w-[85%] flex-col items-end">
          {attachments.length > 0 && (
            <div className="mb-1.5 flex flex-wrap justify-end gap-2">
              <AttachmentStrip attachments={attachments} />
            </div>
          )}
          {message.content && (
            <div className="message-body panel-strong max-w-full select-text rounded-sheet px-3.5 py-2.5 text-[14.5px] leading-6 whitespace-pre-wrap">
              {message.content}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="group flex gap-3">
      <div className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full border border-[var(--glass-border)] text-soft">
        <LoomMark size={15} />
      </div>
      <div className="message-body min-w-0 flex-1 select-text pt-0.5">
        {message.reasoning && (
          <Reasoning
            text={message.reasoning}
            streaming={streaming}
            hasContent={message.content.length > 0}
            revealed={revealThinking}
          />
        )}
        <ToolCallList messageId={message.id} extra={message.extra} />
        {failure ? (
          <FailedTurn message={failure} />
        ) : message.content ? (
          <Markdown content={message.content} />
        ) : (
          streaming &&
          !runningTools && (
            <span className="cursor-blink inline-block h-4 w-[7px] translate-y-[3px] rounded-[2px] bg-[var(--ink-soft)]" />
          )
        )}
        {!streaming && !failure && usage && (
          <p className="mt-1.5 text-[11.5px] text-faint">{formatUsage(usage)}</p>
        )}
      </div>
      <MessageActions message={message} onRevealThinking={onRevealThinking} />
    </div>
  );
}

/** A turn that failed: the reason is stored on the message, so it survives a reload. */
function FailedTurn({ message }: { message: string }) {
  const retryLast = useChat((state) => state.retryLast);
  return (
    <div className="mt-1.5 rounded-control border border-[var(--danger)]/40 bg-[var(--danger)]/10 px-3 py-2">
      <p className="text-[12.5px] leading-5 break-words text-[var(--ink)]">
        This reply failed: {message}
      </p>
      <button
        type="button"
        onClick={() => void retryLast()}
        className="mt-1.5 text-[12.5px] font-medium text-[var(--accent)] hover:underline"
      >
        Try again
      </button>
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
  const alwaysFollow = useSettings((state) => state.config.interface.alwaysFollow);
  const compact = useSettings((state) => state.config.interface.compact);
  const [query, setQuery] = useState("");
  const [revealed, setRevealed] = useState<Record<string, boolean>>({});

  const needle = query.trim().toLowerCase();
  const visible = needle
    ? messages.filter((message) => message.content.toLowerCase().includes(needle))
    : messages;

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
    if (pinned || alwaysFollow) {
      endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
    }
  }, [messages, pinned, alwaysFollow]);

  const jumpToLatest = () => {
    setPinned(true);
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  };

  if (messages.length === 0) {
    return (
      <section className="relative flex h-full min-w-0 items-center justify-center px-6 pb-16">
        <div className="w-full max-w-2xl -translate-y-8">
          <div className="panel rounded-window p-4 pb-3">
            <p className="mb-3 px-1 text-center text-[13px] text-soft">
              Ask anything. Attach files with the paperclip, drop them on the
              window, or paste an image.
            </p>
            <Composer variant="hero" />
          </div>
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
    <section className="relative flex h-full min-w-0 flex-col px-4 pb-4">
      {/* The reading surface: messages and composer sit on this panel, so text
          stays legible over whatever the background art is doing. */}
      <div className="panel relative mx-auto flex h-full w-full max-w-4xl flex-col overflow-hidden rounded-window">
        <ChatHeader query={query} onQuery={setQuery} matches={visible.length} />
        <div
          ref={scrollRef}
          onScroll={onScroll}
          className={cn(
            "min-h-0 flex-1 overflow-y-auto px-8 pb-2 pt-7",
            compact && "density-compact",
          )}
        >
          <div
            className={cn(
              "mx-auto flex w-full max-w-3xl flex-col",
              compact ? "gap-4" : "gap-6",
            )}
          >
            {visible.map((message) => (
              <MessageRow
                key={message.id}
                message={message}
                streaming={busy && message.role === "assistant"}
                runningTools={hasRunningTools(message.id, liveTools)}
                revealThinking={revealed[message.id] ?? false}
                onRevealThinking={() =>
                  setRevealed((current) => ({ ...current, [message.id]: true }))
                }
              />
            ))}
            <div ref={endRef} />
          </div>
        </div>

        {!pinned && (
          <button
            type="button"
            onClick={jumpToLatest}
            className="panel-strong rounded-capsule absolute bottom-28 left-1/2 z-20 -translate-x-1/2 px-3 py-1.5 text-[12px] text-soft"
          >
            Jump to latest ↓
          </button>
        )}

        {error && (
          <div className="mx-auto w-full max-w-3xl px-8">
            <div className="flex items-start gap-2 rounded-control border border-[var(--danger)]/40 bg-[var(--danger)]/10 px-3 py-2 text-[13px]">
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
          <div className="px-8 pb-2">
            <PermissionCard permission={permission} />
          </div>
        )}

        <div className="px-8 pb-6 pt-3">
          <div className="mx-auto w-full max-w-3xl">
            <Composer />
          </div>
        </div>
      </div>
    </section>
  );
}
