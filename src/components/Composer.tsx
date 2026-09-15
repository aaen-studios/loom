import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { cn } from "../lib/cn";
import { ipc, type Skill } from "../lib/ipc";

interface SlashEntry {
  id: string;
  name: string;
  description: string;
  body: string;
}
import { isTauri } from "../lib/tauri";
import type { Attachment } from "../types";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { ArrowUpIcon, PaperclipIcon, StopIcon } from "./icons";
import { AttachmentChips } from "./AttachmentChips";
import { ModelPicker } from "./ModelPicker";
import { PersonaMenu } from "./PersonaMenu";
import { PermissionChip, WorkspaceChip } from "./WorkspaceChip";

interface ComposerProps {
  variant?: "hero" | "docked";
}

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
  const [skills, setSkills] = useState<Skill[]>([]);
  const prompts = useSettings((state) => state.config.prompts);
  const [slashOpen, setSlashOpen] = useState(true);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    void ipc.listSkills().then((result) => setSkills(result ?? []));
  }, []);

  const send = useChat((state) => state.send);
  const stop = useChat((state) => state.stop);
  const ensureSession = useChat((state) => state.ensureSession);
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

  const submit = () => {
    if ((!value.trim() && attachments.length === 0) || busy) return;
    void send(value, { attachments });
    setValue("");
    setAttachments([]);
    setSlashOpen(false);
  };

  const slashQuery =
    value.startsWith("/") && !value.includes(" ") && !value.includes("\n")
      ? value.slice(1).toLowerCase()
      : null;
  // Skills come from markdown files, prompts from the settings, and both are
  // offered from the same slash menu.
  const slashEntries: SlashEntry[] = [
    ...skills.map((skill) => ({
      id: skill.id,
      name: skill.name,
      description: skill.description,
      body: skill.prompt,
    })),
    ...prompts.map((prompt) => ({
      id: prompt.title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""),
      name: prompt.title,
      description: prompt.body.slice(0, 70),
      body: prompt.body,
    })),
  ];

  const slashMatches =
    slashQuery === null
      ? []
      : slashEntries.filter(
          (entry) =>
            entry.id.toLowerCase().includes(slashQuery) ||
            entry.name.toLowerCase().includes(slashQuery),
        );

  const applySkill = (entry: SlashEntry) => {
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
        <div className="panel-strong absolute bottom-full left-0 z-40 mb-2 w-[320px] overflow-hidden rounded-sheet p-1.5">
          {slashMatches.slice(0, 6).map((skill) => (
            <button
              key={skill.id}
              type="button"
              onClick={() => applySkill(skill)}
              className="hover-surface flex w-full flex-col items-start rounded-row px-2 py-1.5 text-left"
            >
              <span className="text-[13px] text-soft">
                <span className="font-mono text-[12px] text-faint">
                  /{skill.id}
                </span>{" "}
                {skill.name}
              </span>
              {skill.description && (
                <span className="text-[11.5px] text-faint">
                  {skill.description}
                </span>
              )}
            </button>
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
        placeholder="Do anything…"
        spellCheck={false}
        onChange={(event) => setValue(event.currentTarget.value)}
        onPaste={(event) => void onPaste(event)}
        onKeyDown={(event) => {
          const withModifier = event.ctrlKey || event.metaKey;
          const wantsSend =
            sendKey === "ctrl-enter" ? withModifier : !event.shiftKey && !withModifier;

          if (slashQuery !== null && slashMatches.length > 0) {
            if (event.key === "Tab" || event.key === "Enter") {
              event.preventDefault();
              applySkill(slashMatches[0]);
              return;
            }
            if (event.key === "Escape") {
              setSlashOpen(false);
              return;
            }
          }
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
        <PersonaMenu />
        <WorkspaceChip />
        <PermissionChip />

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
          <button
            type="button"
            onClick={() => void stop()}
            aria-label="Stop"
            title="Stop generating"
            className="grid h-9 w-9 place-items-center rounded-full border border-[var(--glass-border)] text-[var(--ink)]"
          >
            <StopIcon size={15} />
          </button>
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
