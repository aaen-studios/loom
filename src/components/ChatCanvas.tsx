import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { compactTokens } from "../lib/format";
import {
  formatUsage,
  mergeToolCalls,
  parseAttachments,
  parseError,
  parseReasoningBlocks,
  parseToolCalls,
  parseUsage,
  segmentMessage,
  type ReasoningBlock,
} from "../lib/messageExtra";
import type { Message, ToolCallDisplay, ToolCallRecord } from "../types";
import { folderName } from "../lib/workspaces";
import { useChat } from "../stores/chat";
import { useProviders } from "../stores/providers";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import { AttachmentStrip } from "./AttachmentChips";
import { Composer } from "./Composer";
import { GoalPanel } from "./GoalPanel";
import { Markdown } from "./Markdown";
import { MessageQueue } from "./MessageQueue";
import { QuestionCard } from "./QuestionCard";
import { PermissionCard, ToolCallGroup } from "./ToolCalls";
import { UsageBadge } from "./UsageBadge";
import {
  BrainIcon,
  ChevronDownIcon,
  CopyIcon,
  EditIcon,
  LoomMark,
  RefreshIcon,
  TrashIcon,
} from "./icons";

/** Greeting for the blank-chat screen, fitting whatever hour it is. */
function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 5) return "Still up?";
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

function Reasoning({
  text,
  streaming,
  revealed,
}: {
  text: string;
  streaming: boolean;
  revealed: boolean;
}) {
  const showThinking = useSettings((state) => state.config.interface.showThinking);
  const [open, setOpen] = useState(showThinking === "expanded");
  const [live, setLive] = useState(false);

  useEffect(() => {
    setOpen(showThinking === "expanded");
  }, [showThinking]);

  useEffect(() => {
    if (!streaming) {
      setLive(false);
      return;
    }
    setLive(true);
    const timer = window.setTimeout(() => setLive(false), 1200);
    return () => window.clearTimeout(timer);
  }, [text, streaming]);

  if (showThinking === "hidden" && !revealed) return null;

  const lines = text
    .split("\n")
    .map((line) => line.replace(/^[#>*\-\s]+/, "").replace(/\s+/g, " ").trim())
    .filter(Boolean);
  const preview = lines.length > 0 ? lines[streaming ? lines.length - 1 : 0] : "";

  return (
    <div className="mb-2" data-thinking={open ? "open" : "closed"}>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="hover-surface -ml-1 flex items-center gap-1.5 rounded-control px-1 py-1 text-[12px] text-faint hover:text-[var(--ink)]"
      >
        <BrainIcon size={13} className={live ? "text-[var(--accent)]" : undefined} />
        <span className={live ? "thinking-shimmer" : undefined}>Thinking</span>
        <ChevronDownIcon
          size={11}
          className={cn("shrink-0 transition-transform", open && "rotate-180")}
        />
      </button>
      {!open && preview && (
        <p className="thinking-preview mt-0.5 overflow-hidden pl-[19px] text-[12px] whitespace-nowrap text-faint">
          {preview}
        </p>
      )}
      {open && (
        <div className="thinking-unfold grid">
          <div className="min-h-0 overflow-hidden">
            <div className="loom-thinking mt-1.5">
              <Markdown content={text} allowGeneratedUi={false} streaming={streaming} />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}


/** Title, token totals for this chat, and a search box for the transcript. */
function ChatHeader() {
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
      <UsageBadge align="down" />
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

/** Shared empty lists so selectors keep stable references. */
const NO_TOOLS: ToolCallRecord[] = [];
const NO_REASONING: ReasoningBlock[] = [];

function MessageRow({
  message,
  streaming,
  revealThinking,
  onRevealThinking,
  selected,
  toolDisplay,
}: {
  message: Message;
  streaming: boolean;
  revealThinking: boolean;
  onRevealThinking: () => void;
  selected: boolean;
  toolDisplay: ToolCallDisplay;
}) {
  const attachments = parseAttachments(message.extra);
  const usage = parseUsage(message.extra);
  const failure = parseError(message.extra);
  const personas = useSettings((state) => state.config.personas);
  const persona = personas.find((item) => item.id === message.personaId);
  const castIds = useChat((state) => state.castIds);
  const showSpeaker = Boolean(
    persona && castIds.length > 1 && castIds.includes(persona.id),
  );
  const showThinking = useSettings((state) => state.config.interface.showThinking);
  const generatedUi = useSettings((state) => state.config.interface.generatedUi);
  const thinkingShown =
    Boolean(message.reasoning) && (showThinking !== "hidden" || revealThinking);
  const live = useChat((state) => state.liveTools[message.id] ?? NO_TOOLS);
  const liveReasoning = useChat(
    (state) => state.live[message.sessionId]?.reasoningBlocks ?? NO_REASONING,
  );
  const calls = useMemo(
    () => mergeToolCalls(parseToolCalls(message.extra), live),
    [message.extra, live],
  );
  const reasoningBlocks = useMemo(() => {
    if (liveReasoning.length > 0) return liveReasoning;
    const stored = parseReasoningBlocks(message.extra);
    if (stored.length > 0) return stored;
    // Replies recorded before thinking had positions keep their block on top.
    return message.reasoning
      ? [{ text: message.reasoning, after: 0, seq: 0 }]
      : NO_REASONING;
  }, [liveReasoning, message.extra, message.reasoning]);
  const runningTools = calls.some((call) => call.status === "running");
  const segments = useMemo(
    () => segmentMessage(message.content, calls, reasoningBlocks, !streaming),
    [message.content, calls, reasoningBlocks, streaming],
  );

  if (message.role === "user") {
    return (
      <div
        data-message-id={message.id}
        className={cn(
          "group flex flex-col items-end gap-1.5 rounded-control transition",
          selected && "ring-1 ring-[var(--accent)]",
        )}
      >
        <MessageActions message={message} onRevealThinking={onRevealThinking} />
        <div className="flex max-w-[85%] flex-col items-end">
          {attachments.length > 0 && (
            <div className="mb-1.5 flex flex-wrap justify-end gap-2">
              <AttachmentStrip attachments={attachments} />
            </div>
          )}
          {message.content && (
            <div
              className="message-body panel-strong max-w-full select-text rounded-sheet px-3.5 py-2.5 text-[14.5px] leading-6 whitespace-pre-wrap"
              title={new Date(message.createdAt).toLocaleString()}
            >
              {message.content}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      data-message-id={message.id}
      className={cn(
        "group flex gap-3 rounded-control transition",
        selected && "ring-1 ring-[var(--accent)]",
      )}
    >
      <div
        className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full border border-[var(--glass-border)] text-soft"
        title={persona?.name}
      >
        {persona?.emoji ? (
          <span className="text-[14px] leading-none">{persona.emoji}</span>
        ) : (
          <LoomMark size={15} />
        )}
      </div>
      <div className="message-body min-w-0 flex-1 select-text pt-0.5">
        {showSpeaker && persona && (
          <p className="mb-1 text-[12px] font-medium text-soft">
            {persona.name}
          </p>
        )}
        {segments.map((segment, index) =>
          segment.kind === "tools" ? (
            <ToolCallGroup
              key={`tools-${index}`}
              calls={segment.calls}
              display={toolDisplay}
            />
          ) : segment.kind === "reasoning" ? (
            <Reasoning
              key={`reasoning-${segment.seq}`}
              text={segment.text}
              streaming={streaming}
              revealed={revealThinking}
            />
          ) : segment.text.trim() ? (
            <Markdown
              key={`text-${index}`}
              content={segment.text}
              allowGeneratedUi={generatedUi}
              streaming={streaming}
            />
          ) : null,
        )}
        {failure && <FailedTurn message={failure} />}
        {streaming && !message.content && !thinkingShown && !runningTools && !failure && (
          <span className="cursor-blink inline-block h-4 w-[7px] translate-y-[3px] rounded-[2px] bg-[var(--ink-soft)]" />
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

/**
 * The chat canvas: a hero composer when empty, the transcript plus a docked
 * composer once there are messages. Scrolling only follows new output while
 * you are already at the bottom.
 */
export function ChatCanvas() {
  const messages = useChat((state) => state.messages);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === state.activeId),
  );
  const busy = useChat(
    (state) => (state.activeId ? state.busy[state.activeId] : false) ?? false,
  );
  // Prompts belong to a chat: only the ones for the open chat are shown, so a
  // question asked in the background waits until you return to that chat.
  const permission = useChat((state) =>
    state.activeId ? state.permissions[state.activeId] : undefined,
  );
  const question = useChat((state) =>
    state.activeId ? state.questions[state.activeId] : undefined,
  );
  const error = useChat((state) =>
    state.activeId ? state.errors[state.activeId] : undefined,
  );
  const clearError = useChat((state) => state.clearError);
  const retryLast = useChat((state) => state.retryLast);
  const modelCount = useProviders((state) => state.models.length);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const alwaysFollow = useSettings((state) => state.config.interface.alwaysFollow);
  const compact = useSettings((state) => state.config.interface.compact);
  const toolDisplay = useSettings((state) => state.config.interface.showToolCalls);
  const [revealed, setRevealed] = useState<Record<string, boolean>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const stop = useChat((state) => state.stop);
  const regenerate = useChat((state) => state.regenerate);
  const editFrom = useChat((state) => state.editFrom);
  const removeMessage = useChat((state) => state.removeMessage);
  const setDraft = useChat((state) => state.setDraft);

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

  // Follow new output instantly. A smooth scroll fires scroll events while
  // it travels, which would set `pinned` false mid-flight and strand the view.
  useEffect(() => {
    if (pinned || alwaysFollow) {
      endRef.current?.scrollIntoView({ behavior: "auto", block: "end" });
    }
  }, [messages, pinned, alwaysFollow, question, permission, error]);

  const jumpToLatest = () => {
    setPinned(true);
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  };

  /** Moves the message selection and brings it into view. */
  const moveSelection = (delta: number) => {
    if (messages.length === 0) return;
    const currentIndex = selected
      ? messages.findIndex((message) => message.id === selected)
      : delta > 0
        ? -1
        : messages.length;
    const nextIndex = Math.min(messages.length - 1, Math.max(0, currentIndex + delta));
    const next = messages[nextIndex];
    setSelected(next.id);
    // Wait for the ring to render before scrolling it into view.
    window.setTimeout(() => {
      document
        .querySelector('[data-message-id="' + next.id + '"]')
        ?.scrollIntoView({ behavior: "smooth", block: "center" });
    }, 0);
  };

  /** Keyboard control of the transcript: a chat is usable without a mouse. */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        !!target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      if (typing) return;

      if (event.altKey && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        event.preventDefault();
        moveSelection(event.key === "ArrowDown" ? 1 : -1);
        return;
      }

      if (event.key === "Escape") {
        if (busy) {
          event.preventDefault();
          void stop();
        } else if (selected) {
          setSelected(null);
        }
        return;
      }

      if (!selected) return;
      const message = messages.find((entry) => entry.id === selected);
      if (!message || event.ctrlKey || event.metaKey) return;

      const key = event.key.toLowerCase();
      if (key === "c") {
        void navigator.clipboard.writeText(message.content).catch(() => {});
      } else if (key === "e" && message.role === "user") {
        void editFrom(message.id).then((text) => {
          if (text !== null) setDraft(text);
        });
      } else if (key === "r" && message.role === "assistant") {
        void regenerate(message.id);
      } else if (event.key === "Delete" && event.shiftKey) {
        void removeMessage(message.id);
        setSelected(null);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [messages, selected, busy, stop, editFrom, regenerate, removeMessage, setDraft]);

  if (messages.length === 0) {
    const workspaceName = session?.workdir ? folderName(session.workdir) : null;
    return (
      <section className="relative flex h-full min-w-0 flex-1 items-center justify-center px-6 pb-16">
        <div className="w-full max-w-2xl -translate-y-6">
          <div className="mb-6 flex flex-col items-center text-center">
            <LoomMark size={32} weaving className="mb-3 text-[var(--accent)]" />
            <h1
              className="intro-step text-[24px] font-medium tracking-tight text-[var(--ink)]"
              style={{ animationDelay: "180ms" }}
            >
              {greeting()}
            </h1>
            <p
              className="intro-step mt-1.5 max-w-lg text-[13px] leading-5 text-faint"
              style={{ animationDelay: "260ms" }}
            >
              {workspaceName ? (
                <>
                  Working in{" "}
                  <span className="font-medium text-soft">{workspaceName}</span>
                </>
              ) : (
                "Ask anything — attach a file, drop one on the window, or paste an image."
              )}
            </p>
          </div>
          <div
            className="intro-step panel rounded-window p-4 pb-3"
            style={{ animationDelay: "340ms" }}
          >
            <MessageQueue />
            <GoalPanel />
            <Composer variant="hero" />
          </div>
          {modelCount === 0 && (
            <p
              className="intro-step mt-3 text-center text-[12.5px] text-faint"
              style={{ animationDelay: "440ms" }}
            >
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
          )}
        </div>
      </section>
    );
  }

  return (
    <section className="relative flex h-full min-w-0 flex-1 flex-col px-4 pb-4">
      {/* The reading surface: messages and composer sit on this panel, so text
          stays legible over whatever the background art is doing. */}
      <div className="panel relative mx-auto flex h-full w-full max-w-4xl flex-col overflow-hidden rounded-window">
        <ChatHeader />
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
            {messages.map((message) => (
              <MessageRow
                key={message.id}
                message={message}
                streaming={busy && message.role === "assistant"}
                revealThinking={revealed[message.id] ?? false}
                onRevealThinking={() =>
                  setRevealed((current) => ({ ...current, [message.id]: true }))
                }
                selected={selected === message.id}
                toolDisplay={toolDisplay}
              />
            ))}
            <div ref={endRef} data-latest />
          </div>
        </div>

        {!pinned && !question && (
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
            <MessageQueue />
            <GoalPanel />
            {question ? <QuestionCard question={question} /> : <Composer />}
          </div>
        </div>
      </div>
    </section>
  );
}
