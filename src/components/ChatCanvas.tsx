import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { parseUserPrefix } from "../lib/commands";
import { compactTokens } from "../lib/format";
import {
  formatUsage,
  mergeToolCalls,
  parseAttachments,
  parseCondensed,
  parseNotice,
  parseReasoningBlocks,
  parseToolCalls,
  parseUsage,
  segmentMessage,
  type Notice,
  type ReasoningBlock,
} from "../lib/messageExtra";
import { ipc } from "../lib/ipc";
import type { Condensed, Message, ToolCallDisplay, ToolCallRecord } from "../types";
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
import { LiquidSurface } from "./LiquidSurface";

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
    <div className="glass-thin flex h-11 shrink-0 items-center gap-3 border-b border-[var(--glass-border)] px-5">
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

function MessageContentView({ content }: { content: string }) {
  const prefix = parseUserPrefix(content);
  if (!prefix) return <>{content}</>;
  return (
    <>
      <span className={cn("loom-msg-badge", `loom-msg-badge-${prefix.kind}`)}>
        {prefix.label}
      </span>
      {prefix.rest}
    </>
  );
}

/** Shared empty lists so selectors keep stable references. */
const NO_TOOLS: ToolCallRecord[] = [];
const NO_REASONING: ReasoningBlock[] = [];

type MessageRowProps = {
  message: Message;
  streaming: boolean;
  revealThinking: boolean;
  onRevealThinking: () => void;
  selected: boolean;
  toolDisplay: ToolCallDisplay;
};

/**
 * Whether a row would draw anything different.
 *
 * `onRevealThinking` is deliberately left out. The call site builds a fresh
 * closure on every render, so a shallow compare would see a changed prop every
 * time and the memo would never skip a row — which is the whole reason this
 * comparison exists. Leaving it out is safe because that closure only calls
 * `setRevealed`, a `useState` setter that is stable for the life of the canvas,
 * and closes over `message.id`, which cannot change for a row that keeps its
 * key. The state it flips comes back in as `revealThinking`, and that *is*
 * compared.
 *
 * Without this, a delta re-rendered every message in the transcript: the store
 * builds a new `messages` array, and each row re-parsed its own markdown. The
 * streaming row still re-renders, because its `message` is a new object — the
 * one row that should.
 */
function messageRowPropsMatch(
  previous: MessageRowProps,
  next: MessageRowProps,
): boolean {
  return (
    previous.message === next.message &&
    previous.streaming === next.streaming &&
    previous.revealThinking === next.revealThinking &&
    previous.selected === next.selected &&
    previous.toolDisplay === next.toolDisplay
  );
}

const MessageRow = memo(function MessageRow({
  message,
  streaming,
  revealThinking,
  onRevealThinking,
  selected,
  toolDisplay,
}: MessageRowProps) {
  const attachments = parseAttachments(message.extra);
  const usage = parseUsage(message.extra);
  const notice = parseNotice(message.extra);
  const condensed = parseCondensed(message.extra);
  const showCondensing = useSettings((state) => state.config.interface.showCondensing);
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
            <LiquidSurface
        surface="cards"
        layout="block"
        tint="var(--panel-bg-strong)"
              className="message-body max-w-full select-text rounded-sheet px-3.5 py-2.5 text-[14.5px] leading-6 whitespace-pre-wrap"
              title={new Date(message.createdAt).toLocaleString()}
            >
              <MessageContentView content={message.content} />
            </LiquidSurface>
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
        {notice && <TurnNote notice={notice} />}
        {streaming && !message.content && !thinkingShown && !runningTools && !notice && (
          <span className="cursor-blink inline-block h-4 w-[7px] translate-y-[3px] rounded-[2px] bg-[var(--ink-soft)]" />
        )}
        {!streaming && !notice && condensed && showCondensing && (
          <CondensedLine condensed={condensed} sessionId={message.sessionId} />
        )}
        {!streaming && !notice && usage && (
          <p className="mt-1.5 text-[11.5px] text-faint">{formatUsage(usage)}</p>
        )}
      </div>
      <MessageActions message={message} onRevealThinking={onRevealThinking} />
    </div>
  );
}, messageRowPropsMatch);

/**
 * A turn that ended early: a limit, a provider refusal, a loop.
 *
 * Deliberately not styled as a failure. The reason is stored on the message so
 * it survives a reload, `detail` keeps the provider's own words behind a
 * toggle, and the reply above is still the user's to read.
 */
function TurnNote({ notice }: { notice: Notice }) {
  const retryLast = useChat((state) => state.retryLast);
  return (
    <div className="mt-1.5 rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] px-3 py-2">
      <p className="text-[12.5px] leading-5 break-words text-soft">{notice.text}</p>
      {notice.detail && (
        <details className="group mt-1.5">
          <summary className="cursor-pointer text-[12px] text-faint hover:text-[var(--ink)] [&::-webkit-details-marker]:hidden">
            Details
          </summary>
          <pre className="mt-1.5 max-h-60 overflow-auto rounded-row bg-[var(--ink-ghost)] p-2 font-mono text-[11.5px] whitespace-pre-wrap text-soft">
            {notice.detail}
          </pre>
        </details>
      )}
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
 * A reply that was answered from a condensed view of the chat's older turns.
 *
 * Deliberately the quietest thing in the transcript: one faint line in the slot
 * the token-usage line already occupies, no border, no button and no red. The
 * turn succeeded — this only says what the model could actually see, which is
 * the one thing a reader of a long chat cannot otherwise tell. Expanding it
 * shows the block verbatim, so "the summary lost something" is checkable rather
 * than a matter of trust.
 *
 * The text is fetched on first expand rather than carried on every message: it
 * is identical for every reply between two folds, and a copy on each of them
 * would roughly double the transcript's size.
 */
function CondensedLine({
  condensed,
  sessionId,
}: {
  condensed: Condensed;
  sessionId: string;
}) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState<string | null>(null);

  useEffect(() => {
    if (!open || text !== null) return;
    let live = true;
    void ipc
      .sessionSummary(sessionId)
      .then((summary) => {
        if (live) setText(summary?.text ?? "");
      })
      .catch(() => {
        if (live) setText("");
      });
    return () => {
      live = false;
    };
  }, [open, text, sessionId]);

  const turns = `${condensed.covered} earlier ${
    condensed.covered === 1 ? "message" : "messages"
  }`;

  return (
    <div className="mt-1.5">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        title={
          condensed.source === "summary"
            ? "Condensed by the lite model so the request would fit the window"
            : "Condensed in-process so the request would fit the window"
        }
        className="flex items-center gap-1.5 text-[11.5px] text-faint hover:text-soft"
      >
        <span>Condensed · {turns}</span>
        <span className="opacity-70">
          {condensed.source === "summary" ? "summary" : "digest"}
        </span>
        <span className={cn("transition", open && "rotate-180")}>
          <ChevronDownIcon />
        </span>
      </button>
      {open && (
        <div className="mt-1.5 rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2.5 py-2">
          {text === null ? (
            <p className="text-[11.5px] text-faint">Reading the summary…</p>
          ) : text.trim() ? (
            <pre className="loom-scroll max-h-72 overflow-auto font-mono text-[11.5px] leading-[1.55] whitespace-pre-wrap text-soft">
              {text}
            </pre>
          ) : (
            <p className="text-[11.5px] text-faint">
              This fold was built in-process, so there is no stored summary to
              show. The model was given these turns listed as headings,
              changes and commands.
            </p>
          )}
        </div>
      )}
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
  // Gated on `loaded`, not on the count alone. An empty list before the first
  // load is not evidence of anything, and treating it as evidence is what made
  // the opening screen tell people to add a provider and then retract it.
  const modelsLoaded = useProviders((state) => state.loaded);
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
            {/* The header stands on the background art, with no panel under it,
                so it carries its own contrast — see `.blob-text`. Without it a
                greeting in translucent ink disappears into a bright wallpaper. */}
            <h1
              className="intro-step blob-text text-[24px] font-medium tracking-tight text-[var(--ink)]"
              style={{ animationDelay: "180ms" }}
            >
              {greeting()}
            </h1>
            <p
              className="intro-step blob-text mt-1.5 max-w-lg text-[13px] leading-5 text-soft"
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
          {modelsLoaded && modelCount === 0 && (
            <p
              className="intro-step blob-text mt-3 text-center text-[12.5px] text-soft"
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
          stays legible over whatever the background art is doing.

          Centred, and it can be now. The reason this felt wrong before was that
          a splitter was resizing it: shrinking a centred column moves *both* of
          its edges, so it looked like it was drifting rather than being pushed.
          The dock overlays the region instead of taking width from it, so
          nothing resizes this element and its centre is fixed for the life of
          the window. Centring also puts equal slack on both sides, which is
          what lets the chats panel open over the background art rather than
          over the transcript. */}
      <div className="panel relative mx-auto flex h-full w-full max-w-4xl min-w-0 flex-col overflow-hidden rounded-window">
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
          <LiquidSurface
            surface="cards"
            className="rounded-capsule absolute bottom-28 left-1/2 z-20 -translate-x-1/2"
            tint="var(--panel-bg-strong)"
          >
            <button
              type="button"
              onClick={jumpToLatest}
              className="px-3 py-1.5 text-[12px] text-soft"
            >
              Jump to latest ↓
            </button>
          </LiquidSurface>
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
