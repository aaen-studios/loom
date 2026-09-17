import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { ipc } from "../lib/ipc";
import {
  BUILTIN_COMMANDS,
  GOAL_PREFIX,
  TASK_PREFIX,
  parseCommand,
  type ParsedCommand,
} from "../lib/commands";
import {
  findMention,
  insertSuggestion,
  matchChats,
  matchFiles,
  resolveChatMentions,
  type MentionSpan,
} from "../lib/mentions";
import { isTauri } from "../lib/tauri";
import type { Attachment, Todo } from "../types";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useSkills } from "../stores/skills";
import { useWorkspaceFiles } from "../stores/workspaceFiles";
import { ArrowUpIcon, MicIcon, MicOffIcon, PaperclipIcon, StopIcon } from "./icons";
import { canCapture } from "../lib/microphone";
import { barHeight } from "../lib/voiceActivity";
import { useVoice } from "../stores/voice";
import { AttachmentChips } from "./AttachmentChips";
import { MentionMenu, chatDetail, fileDetail, type MentionOption } from "./MentionMenu";
import { ModelPicker } from "./ModelPicker";
import { ModeChip } from "./WorkspaceChip";

interface ComposerProps {
  variant?: "hero" | "docked";
}

type SlashEntry =
  | { kind: "command"; id: string; name: string; description: string; argsHint: string | null }
  | { kind: "skill"; id: string; name: string; description: string; body: string }
  | { kind: "prompt"; id: string; name: string; description: string; body: string };

/** Stable identity: a selector returning a fresh `[]` loops in zustand v5. */
const NO_TODOS: Todo[] = [];

const FILE_FILTERS = [
  {
    name: "Attachments",
    extensions: [
      "png", "jpg", "jpeg", "gif", "webp", "bmp",
      "pdf", "txt", "md", "json", "csv", "log",
      "rs", "ts", "tsx", "js", "jsx", "py", "toml", "yaml", "yml",
      "html", "css", "sql", "sh", "ps1", "bat",
    ],
  },
];

/**
 * Message composer: auto-growing input, attachments (dialog, paste, drop),
 * model + persona chips, send, and a stop button while streaming.
 *
 * Two menus open above it — `/` for commands, skills and prompts, and `#` / `@`
 * for chats and workspace files — and both are driven by the same keyboard
 * contract, because a menu you can only click is one you have to look at.
 */
export function Composer({ variant = "docked" }: ComposerProps) {
  const [value, setValue] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [attachError, setAttachError] = useState<string | null>(null);
  const skills = useSkills((state) => state.skills);
  const loadSkills = useSkills((state) => state.load);
  const prompts = useSettings((state) => state.config.prompts);
  const [slashOpen, setSlashOpen] = useState(true);
  const [slashIndex, setSlashIndex] = useState(0);
  const [mentionSpan, setMentionSpan] = useState<MentionSpan | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  /** Caret to restore after React hands the textarea a new value. */
  const pendingCaret = useRef<number | null>(null);

  // Skills live in the shared store so the Settings editor and a
  // harnessChanged event (the model wrote one) both refresh this list.
  useEffect(() => {
    void loadSkills();
  }, [loadSkills]);

  const send = useChat((state) => state.send);
  const enqueue = useChat((state) => state.enqueue);
  const stop = useChat((state) => state.stop);
  const ensureSession = useChat((state) => state.ensureSession);
  const newSession = useChat((state) => state.newSession);
  const setGoal = useChat((state) => state.setGoal);
  const setTodos = useChat((state) => state.setTodos);
  const setAgentMode = useChat((state) => state.setAgentMode);
  const setTaskPanelOpen = useChat((state) => state.setTaskPanelOpen);
  const sessions = useChat((state) => state.sessions);
  const activeWorkdir = useChat(
    (state) => state.sessions.find((item) => item.id === state.activeId)?.workdir ?? null,
  );
  const todos = useChat((state) =>
    state.activeId ? state.todos[state.activeId] ?? NO_TODOS : NO_TODOS,
  );
  const goal = useChat((state) =>
    state.activeId ? state.goals[state.activeId] ?? null : null,
  );
  const busy = useChat(
    (state) => (state.activeId ? state.busy[state.activeId] : false) ?? false,
  );
  const sendKey = useSettings((state) => state.config.interface.sendKey);
  const draft = useChat((state) => state.draft);
  const setDraft = useChat((state) => state.setDraft);

  // The workspace's file list, for `@`. Cached per folder in the store, so
  // typing "@src" does not walk the tree six times.
  const files = useWorkspaceFiles((state) => state.files);
  const loadFiles = useWorkspaceFiles((state) => state.load);

  // Dictation. The transcript arrives as a value plus a counter rather than as
  // an event, so this effect appends exactly once per utterance — a re-render
  // cannot duplicate a sentence, and two quick utterances cannot lose one.
  const transcript = useVoice((state) => state.transcript);
  const transcriptSeq = useVoice((state) => state.transcriptSeq);
  const dictation = useVoice((state) => state.dictation);
  const dictationError = useVoice((state) => state.dictationError);
  const inputLevel = useVoice((state) => state.inputLevel);
  const hearing = useVoice((state) => state.hearing);
  const startListening = useVoice((state) => state.startListening);
  const stopListening = useVoice((state) => state.stopListening);
  // Whether a finished transcript sends itself. Read from the stored settings
  // rather than local state, so the toggle in Settings → Voice is the only
  // place this is decided.
  const autoSend = useVoice((state) => state.status?.autoSend ?? false);
  const seenTranscript = useRef(0);

  // "Edit and resend" hands the message back to the composer.
  useEffect(() => {
    if (draft === null) return;
    setValue(draft);
    setDraft(null);
    textareaRef.current?.focus();
  }, [draft, setDraft]);

  // React owns the textarea's value, so an insertion would leave the caret
  // wherever the browser last put it — usually the end. Restoring it has to
  // happen after the new value has been committed, which is what this effect
  // is: `setSelectionRange` on a stale value would be undone by the render.
  useEffect(() => {
    const element = textareaRef.current;
    if (!element || pendingCaret.current === null) return;
    element.setSelectionRange(pendingCaret.current, pendingCaret.current);
    pendingCaret.current = null;
    element.focus();
  }, [value]);

  useEffect(() => {
    if (transcriptSeq === seenTranscript.current) return;
    seenTranscript.current = transcriptSeq;

    const said = transcript?.trim();
    if (!said) return;

    if (autoSend && !busy) {
      void send(said);
      return;
    }

    // Appended to what is already typed, never replacing it: replacing would
    // silently discard a half-written message because someone spoke.
    setValue((current) => (current.trim() ? `${current.trimEnd()} ${said}` : said));
    textareaRef.current?.focus();
  }, [transcriptSeq, transcript, autoSend, busy, send]);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 216)}px`;
  }, [value]);

  const attachPaths = async (paths: string[]) => {
    if (paths.length === 0) return;
    setAttachError(null);
    try {
      const sessionId = await ensureSession();
      if (!sessionId) return;
      const stored = await ipc.attachFiles(sessionId, paths);
      if (stored) setAttachments((current) => [...current, ...stored]);
    } catch (error) {
      setAttachError(error instanceof Error ? error.message : String(error));
    }
  };

  const attachBase64 = async (name: string, data: string) => {
    setAttachError(null);
    try {
      const sessionId = await ensureSession();
      if (!sessionId) return;
      const stored = await ipc.attachBytes(sessionId, name, data);
      if (stored) setAttachments((current) => [...current, stored]);
    } catch (error) {
      setAttachError(error instanceof Error ? error.message : String(error));
    }
  };

  // Drag & drop files onto the window.
  useEffect(() => {
    if (!isTauri) return;
    let dispose: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          void attachPaths(event.payload.paths);
        }
      })
      .then((unlisten) => {
        dispose = unlisten;
      });
    return () => dispose?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const pickFiles = async () => {
    if (!isTauri) return;
    const picked = await openDialog({ multiple: true, filters: FILE_FILTERS });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    await attachPaths(paths.filter((path): path is string => typeof path === "string"));
  };

  const onPaste = async (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const file = [...event.clipboardData.files].find((item) =>
      item.type.startsWith("image/"),
    );
    if (!file) return;
    event.preventDefault();
    const buffer = await file.arrayBuffer();
    const bytes = new Uint8Array(buffer);
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    await attachBase64(file.name || "pasted.png", btoa(binary));
  };

  /**
   * Starts a turn with `text`, queueing it when a reply is already running.
   *
   * This is what the commands that both *set something* and *say something*
   * use. They used to do only the setting: `/goal fix the bug` recorded the
   * goal and then returned "consumed", so the text never reached the model and
   * a goal with no turn behind it just sat there. Queueing rather than dropping
   * matters for the same reason — a command is not a reason to lose a message.
   */
  const startTurn = (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    void ensureSession().then(() => {
      if (busy) void enqueue(trimmed, []);
      else void send(trimmed);
    });
  };

  /** Runs a `/command`; returns true when the message was consumed by it. */
  const runCommand = (parsed: ParsedCommand): boolean => {
    const { command, args } = parsed;
    switch (command.id) {
      case "goal": {
        if (!args) {
          setValue(goal ? `/goal ${goal}` : "/goal ");
          return true;
        }
        const clear = args.toLowerCase() === "clear" || args.toLowerCase() === "none";
        // The goal is written first, and the turn starts in the `then`, so the
        // message cannot outrun the state it is talking about.
        void ensureSession()
          .then(() => setGoal(clear ? null : args))
          .then(() => {
            if (!clear) startTurn(`${GOAL_PREFIX}${args}`);
          });
        setValue("");
        setTaskPanelOpen(true);
        return true;
      }
      case "todo": {
        if (!args) return true;
        const next: Todo[] = [
          ...todos,
          { id: crypto.randomUUID(), content: args, status: "pending", position: todos.length },
        ];
        void ensureSession()
          .then(() => setTodos(next))
          .then(() => startTurn(`${TASK_PREFIX}${args}`));
        setValue("");
        setTaskPanelOpen(true);
        return true;
      }
      case "todos": {
        setTaskPanelOpen(true);
        setValue("");
        return true;
      }
      case "plan": {
        void ensureSession()
          .then(() => setAgentMode("plan"))
          .then(() => startTurn(args));
        setValue("");
        return true;
      }
      case "chat": {
        void ensureSession()
          .then(() => setAgentMode("chat"))
          .then(() => startTurn(args));
        setValue("");
        return true;
      }
      case "new": {
        void newSession();
        setValue("");
        return true;
      }
      default:
        return false;
    }
  };

  const submit = () => {
    const parsed = parseCommand(value);
    if (parsed && runCommand(parsed)) {
      setSlashOpen(false);
      setMentionSpan(null);
      return;
    }
    // A `#Chat title` mention is resolved on send rather than on pick, so the
    // composer stays readable while you write and the model still gets the id
    // it needs to call `read_chat`.
    const text = resolveChatMentions(value, sessions);
    if (!text.trim() && attachments.length === 0) return;
    if (busy) {
      void enqueue(text, attachments);
    } else {
      void send(text, { attachments });
    }
    setValue("");
    setAttachments([]);
    setSlashOpen(false);
    setMentionSpan(null);
  };

  const slashQuery =
    slashOpen && value.startsWith("/") && !value.includes(" ") && !value.includes("\n")
      ? value.slice(1).toLowerCase()
      : null;

  // Built-in commands, skills, and prompts share one `/` menu.
  const slashEntries: SlashEntry[] = useMemo(
    () => [
      ...BUILTIN_COMMANDS.map((command) => ({
        kind: "command" as const,
        id: command.id,
        name: command.label,
        description: command.description,
        argsHint: command.argsHint,
      })),
      ...skills.map((skill) => ({
        kind: "skill" as const,
        id: skill.id,
        name: skill.name,
        description: skill.description,
        body: skill.prompt,
      })),
      ...prompts.map((prompt) => ({
        kind: "prompt" as const,
        id: prompt.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""),
        name: prompt.title,
        description: prompt.body.slice(0, 70),
        body: prompt.body,
      })),
    ],
    [skills, prompts],
  );

  const slashMatches =
    slashQuery === null
      ? []
      : slashEntries.filter(
          (entry) =>
            entry.id.toLowerCase().includes(slashQuery) ||
            entry.name.toLowerCase().includes(slashQuery),
        );

  const sectionLabel = (entry: SlashEntry) =>
    entry.kind === "command" ? "Commands" : entry.kind === "skill" ? "Skills" : "Prompts";

  const applyEntry = (entry: SlashEntry) => {
    if (entry.kind === "command") {
      setValue(`/${entry.id} `);
      setSlashOpen(false);
      textareaRef.current?.focus();
      return;
    }
    setValue(entry.body);
    setSlashOpen(false);
    textareaRef.current?.focus();
  };

  /**
   * The `#` or `@` popup's rows, built from what has been typed after the
   * trigger.
   */
  const mentionOptions: MentionOption[] = useMemo(() => {
    if (!mentionSpan) return [];
    if (mentionSpan.trigger === "#") {
      return matchChats(mentionSpan.query, sessions).map((session) => ({
        id: session.title || "New chat",
        label: session.title || "New chat",
        detail: chatDetail(session),
      }));
    }
    return matchFiles(mentionSpan.query, files).map((path) => ({
      id: path,
      label: path,
      detail: fileDetail(path),
    }));
  }, [mentionSpan, sessions, files]);

  /**
   * Decides whether a mention popup should be open at the caret.
   *
   * The `#` case is deliberately strict: it opens only when a chat actually
   * matches, which is what keeps "issue #42" from growing a menu it did not
   * ask for. `@` opens whenever there is a workspace to list, because the list
   * arrives from the engine and an empty first frame is not the same as
   * nothing to offer.
   */
  const openMention = (text: string, caret: number): MentionSpan | null => {
    const span = findMention(text, caret);
    if (!span) return null;
    if (span.trigger === "#") {
      return matchChats(span.query, sessions).length > 0 ? span : null;
    }
    if (!activeWorkdir) return null;
    void loadFiles(activeWorkdir);
    return span;
  };

  const applyMention = (option: MentionOption) => {
    if (!mentionSpan) return;
    const { value: next, caret } = insertSuggestion(value, mentionSpan, option.id);
    pendingCaret.current = caret;
    setValue(next);
    setMentionSpan(null);
  };

  const onTextChange = (next: string, caret: number) => {
    if (next.startsWith("/") && !value.startsWith("/")) setSlashOpen(true);
    setValue(next);
    const span = openMention(next, caret);
    setMentionSpan(span);
    if (span) setMentionIndex(0);
  };

  const mentionOpen = mentionSpan !== null && mentionOptions.length > 0;
  const mentionEmptyHint =
    mentionSpan?.trigger === "#"
      ? "No chat matches that yet."
      : activeWorkdir
        ? "Reading the workspace…"
        : "This chat has no workspace folder, so there are no files to offer. Pick one from the workspace chip.";

  return (
    <div
      className={cn(
        "panel-strong relative w-full rounded-sheet p-2.5",
        variant === "hero" && "animate-fade-up",
      )}
    >
      {mentionOpen && mentionSpan && (
        <MentionMenu
          trigger={mentionSpan.trigger}
          options={mentionOptions}
          index={mentionIndex}
          onPick={applyMention}
          onHover={setMentionIndex}
          emptyHint={mentionEmptyHint}
        />
      )}

      {slashOpen && slashQuery !== null && slashMatches.length > 0 && (
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[360px] overflow-hidden rounded-sheet p-1.5">
          <div className="max-h-[300px] overflow-y-auto">
            {slashMatches.slice(0, 8).map((entry, index) => (
              <div key={`${entry.kind}-${entry.id}`}>
                {(index === 0 ||
                  sectionLabel(entry) !== sectionLabel(slashMatches[index - 1])) && (
                  <p className="px-2 pt-1.5 pb-0.5 text-[10.5px] font-semibold tracking-[0.08em] text-faint uppercase">
                    {sectionLabel(entry)}
                  </p>
                )}
                <button
                  type="button"
                  aria-selected={index === slashIndex}
                  onMouseEnter={() => setSlashIndex(index)}
                  // mousedown, not click: the textarea must keep focus, so the
                  // composer is still usable the instant a row is picked.
                  onMouseDown={(event) => {
                    event.preventDefault();
                    applyEntry(entry);
                  }}
                  className={cn(
                    "flex w-full items-start gap-2 rounded-row px-2 py-1.5 text-left",
                    index === slashIndex ? "bg-[var(--hover-bg)]" : "hover:bg-[var(--hover-bg)]",
                  )}
                >
                  <span className="mt-[1px] shrink-0">
                    <span className="kbd">{`/${entry.id}`}</span>
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-baseline gap-1.5">
                      <span className="min-w-0 truncate text-[13px] text-soft">
                        {entry.name}
                      </span>
                      <span className="shrink-0 rounded-capsule border border-[var(--glass-border)] px-1.5 py-px text-[9.5px] tracking-[0.06em] text-faint uppercase">
                        {entry.kind}
                      </span>
                    </span>
                    {(entry.description ||
                      (entry.kind === "command" && entry.argsHint)) && (
                      <span className="mt-0.5 block text-[11.5px] leading-4 text-faint">
                        {entry.description}
                        {entry.kind === "command" && entry.argsHint && (
                          <span className="ml-1 font-mono opacity-80">{entry.argsHint}</span>
                        )}
                      </span>
                    )}
                  </span>
                </button>
              </div>
            ))}
          </div>
          <p className="px-2 py-1 text-[11px] text-faint">
            ↑↓ to choose · Enter to accept · Esc to dismiss
          </p>
        </div>
      )}

      <AttachmentChips
        attachments={attachments}
        onRemove={(id) =>
          setAttachments((current) => current.filter((item) => item.id !== id))
        }
      />

      <textarea
        ref={textareaRef}
        rows={1}
        value={value}
        placeholder={busy ? "Queue a message…" : "Do anything…  # a chat, @ a file"}
        spellCheck={false}
        onChange={(event) =>
          onTextChange(event.currentTarget.value, event.currentTarget.selectionStart ?? 0)
        }
        onClick={(event) =>
          setMentionSpan(
            openMention(
              event.currentTarget.value,
              event.currentTarget.selectionStart ?? 0,
            ),
          )
        }
        onBlur={() => setMentionSpan(null)}
        onPaste={(event) => void onPaste(event)}
        onKeyDown={(event) => {
          const withModifier = event.ctrlKey || event.metaKey;

          // The mention menu takes the keys first: when it is open, Enter means
          // "take this row", not "send half a mention".
          if (mentionOpen && mentionSpan) {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              const delta = event.key === "ArrowDown" ? 1 : -1;
              const count = mentionOptions.length;
              setMentionIndex((current) => (current + delta + count) % count);
              return;
            }
            if (event.key === "Tab" || event.key === "Enter") {
              event.preventDefault();
              applyMention(mentionOptions[mentionIndex] ?? mentionOptions[0]);
              return;
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setMentionSpan(null);
              return;
            }
          }

          if (slashQuery !== null && slashMatches.length > 0) {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              const delta = event.key === "ArrowDown" ? 1 : -1;
              const count = Math.min(8, slashMatches.length);
              setSlashIndex((current) => (current + delta + count) % count);
              return;
            }
            if (event.key === "Tab" || event.key === "Enter") {
              event.preventDefault();
              // The highlighted row, not the first one: arrowing down to a row
              // and then pressing Enter has to accept that row.
              applyEntry(slashMatches[slashIndex] ?? slashMatches[0]);
              return;
            }
            if (event.key === "Escape") {
              setSlashOpen(false);
              return;
            }
          }
          const wantsSend =
            sendKey === "ctrl-enter" ? withModifier : !event.shiftKey && !withModifier;
          if (event.key === "Enter" && !event.nativeEvent.isComposing && wantsSend) {
            event.preventDefault();
            submit();
          }
        }}
        className="block max-h-[216px] w-full resize-none bg-transparent px-3 pt-2 text-[15px] leading-6 text-[var(--ink)] placeholder:text-[var(--ink-faint)]"
      />

      {attachError && (
        <p className="px-3 pt-1 text-[12px] text-[var(--danger)]">{attachError}</p>
      )}

      {dictationError && (
        <p className="px-3 pt-1 text-[12px] text-[var(--danger)]">{dictationError}</p>
      )}

      {dictation === "listening" && (
        // The meter answers the one question a user has while dictating: is it
        // hearing me? A level that does not move means the wrong input device,
        // and that is worth showing before they finish a sentence.
        <div className="mt-1 flex items-center gap-2 px-3 pt-1">
          <div className="h-1 w-24 overflow-hidden rounded-capsule bg-[var(--ink-ghost)]">
            <div
              className={cn(
                "h-full rounded-capsule transition-[width] duration-75",
                hearing ? "bg-[var(--accent)]" : "bg-[var(--ink-faint)]",
              )}
              // `barHeight` rather than a local `× 400`: the voice surface
              // draws the same microphone, and two meters with different
              // sensitivities would look like a bug in one of them.
              style={{ width: `${Math.round(barHeight(inputLevel) * 100)}%` }}
            />
          </div>
          <span className="text-[11px] text-faint">
            {hearing ? "hearing you" : "listening"}
          </span>
        </div>
      )}

      <div className="mt-1.5 flex items-center gap-1.5 pl-0.5">
        <ModelPicker />
        <ModeChip />

        <div className="flex-1" />

        {canCapture() && (
          <button
            type="button"
            onClick={() => {
              if (dictation === "off" || dictation === "error") {
                void startListening();
              } else {
                void stopListening();
              }
            }}
            disabled={dictation === "starting"}
            aria-label={dictation === "off" ? "Dictate" : "Stop dictating"}
            title={
              dictation === "starting"
                ? "Loading the speech models…"
                : dictation === "off"
                  ? "Dictate — speak, and the words appear here"
                  : "Stop dictating"
            }
            className={cn(
              "hover-surface grid h-8 w-8 place-items-center rounded-full",
              dictation === "off" || dictation === "error"
                ? "text-faint"
                : "text-[var(--accent)]",
              dictation === "starting" && "opacity-50",
            )}
          >
            {dictation === "listening" ? (
              <MicOffIcon size={16} />
            ) : (
              <MicIcon size={16} />
            )}
          </button>
        )}

        <button
          type="button"
          onClick={() => void pickFiles()}
          aria-label="Attach files"
          title="Attach files (or paste / drop them)"
          className="hover-surface grid h-8 w-8 place-items-center rounded-full text-faint"
        >
          <PaperclipIcon size={16} />
        </button>

        {busy ? (
          <>
            {(value.trim() || attachments.length > 0) && (
              <button
                type="button"
                onClick={submit}
                aria-label="Queue message"
                title="Queue — sends when the reply finishes"
                className="grid h-9 w-9 place-items-center rounded-full bg-[var(--control-bg)] text-[var(--control-ink)] transition hover:scale-[1.04] active:scale-95"
              >
                <ArrowUpIcon size={17} />
              </button>
            )}
            <button
              type="button"
              onClick={() => void stop()}
              aria-label="Stop"
              title="Stop generating"
              className="grid h-9 w-9 place-items-center rounded-full border border-[var(--glass-border)] text-[var(--ink)]"
            >
              <StopIcon size={15} />
            </button>
          </>
        ) : (
          <button
            type="button"
            onClick={submit}
            disabled={!value.trim() && attachments.length === 0}
            aria-label="Send"
            title="Send"
            className={cn(
              "grid h-9 w-9 place-items-center rounded-full bg-[var(--control-bg)] text-[var(--control-ink)]",
              "transition enabled:hover:scale-[1.04] enabled:active:scale-95 disabled:opacity-40",
            )}
          >
            <ArrowUpIcon size={17} />
          </button>
        )}
      </div>
    </div>
  );
}

/* The microphone glyphs moved to `icons.tsx`.
 *
 * They were inline here with a 16 px viewBox and a 1.4 stroke, while every
 * other icon in the app is drawn on a 24 px grid at 1.7 — so the composer's
 * microphone was visibly lighter than the paperclip beside it. One icon set
 * means the corpus stays consistent and the next microphone cannot invent a
 * third weight. */
