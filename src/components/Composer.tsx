import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { ipc } from "../lib/ipc";
import { BUILTIN_COMMANDS, parseCommand, type ParsedCommand } from "../lib/commands";
import { isTauri } from "../lib/tauri";
import type { Attachment, Todo } from "../types";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useSkills } from "../stores/skills";
import { ArrowUpIcon, PaperclipIcon, StopIcon } from "./icons";
import { AttachmentChips } from "./AttachmentChips";
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
 */
export function Composer({ variant = "docked" }: ComposerProps) {
  const [value, setValue] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [attachError, setAttachError] = useState<string | null>(null);
  const skills = useSkills((state) => state.skills);
  const loadSkills = useSkills((state) => state.load);
  const prompts = useSettings((state) => state.config.prompts);
  const [slashOpen, setSlashOpen] = useState(true);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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

  // "Edit and resend" hands the message back to the composer.
  useEffect(() => {
    if (draft === null) return;
    setValue(draft);
    setDraft(null);
    textareaRef.current?.focus();
  }, [draft, setDraft]);

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
        void ensureSession().then(() => setGoal(clear ? null : args));
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
        void ensureSession().then(() => setTodos(next));
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
          .then(() => {
            if (args && !busy) void send(args);
          });
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
      return;
    }
    if (!value.trim() && attachments.length === 0) return;
    if (busy) {
      void enqueue(value, attachments);
    } else {
      void send(value, { attachments });
    }
    setValue("");
    setAttachments([]);
    setSlashOpen(false);
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

  return (
    <div
      className={cn(
        "panel-strong relative w-full rounded-sheet p-2.5",
        variant === "hero" && "animate-fade-up",
      )}
    >
      {slashOpen && slashQuery !== null && slashMatches.length > 0 && (
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[360px] overflow-hidden rounded-sheet p-1.5">
          {slashMatches.slice(0, 8).map((entry, index) => (
            <div key={`${entry.kind}-${entry.id}`}>
              {(index === 0 || sectionLabel(entry) !== sectionLabel(slashMatches[index - 1])) && (
                <p className="px-2 pt-1.5 pb-0.5 text-[10.5px] font-semibold tracking-[0.08em] text-faint uppercase">
                  {sectionLabel(entry)}
                </p>
              )}
              <button
                type="button"
                onClick={() => applyEntry(entry)}
                className="hover-surface flex w-full flex-col items-start rounded-row px-2 py-1.5 text-left"
              >
                <span className="text-[13px] text-soft">
                  <span className="font-mono text-[12px] text-faint">
                    /{entry.id}
                    {entry.kind === "command" && entry.argsHint ? ` ${entry.argsHint}` : ""}
                  </span>{" "}
                  {entry.name}
                </span>
                {entry.description && (
                  <span className="line-clamp-2 text-[11.5px] text-faint">
                    {entry.description}
                  </span>
                )}
              </button>
            </div>
          ))}
          <p className="px-2 py-1 text-[11px] text-faint">
            Tab selects · Esc dismisses
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
        placeholder={busy ? "Queue a message…" : "Do anything…"}
        spellCheck={false}
        onChange={(event) => {
          const next = event.currentTarget.value;
          if (next.startsWith("/") && !value.startsWith("/")) setSlashOpen(true);
          setValue(next);
        }}
        onPaste={(event) => void onPaste(event)}
        onKeyDown={(event) => {
          const withModifier = event.ctrlKey || event.metaKey;

          if (slashQuery !== null && slashMatches.length > 0) {
            if (event.key === "Tab" || event.key === "Enter") {
              event.preventDefault();
              applyEntry(slashMatches[0]);
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

      <div className="mt-1.5 flex items-center gap-1.5 pl-0.5">
        <ModelPicker />
        <ModeChip />

        <div className="flex-1" />

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
