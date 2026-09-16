import { useState } from "react";
import { cn } from "../lib/cn";
import { assetUrl } from "../lib/tauri";
import type { Attachment } from "../types";
import { CameraIcon, ChevronDownIcon, CloseIcon, FileIcon } from "./icons";

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function AttachmentIcon({ attachment }: { attachment: Attachment }) {
  if (attachment.kind === "image") {
    return (
      <img
        src={assetUrl(attachment.path)}
        alt=""
        className="h-7 w-7 rounded-md object-cover"
      />
    );
  }
  return (
    <span className="grid h-7 w-7 place-items-center rounded-md bg-[var(--hover-bg)] text-faint">
      <FileIcon size={14} />
    </span>
  );
}

/** Chips shown in the composer before sending. */
export function AttachmentChips({
  attachments,
  onRemove,
}: {
  attachments: Attachment[];
  onRemove: (id: string) => void;
}) {
  if (attachments.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-1.5 px-1 pb-1.5 pt-2">
      {attachments.map((attachment) => (
        <span
          key={attachment.id}
          className="flex items-center gap-2 rounded-row border border-[var(--glass-border)] py-1 pl-1 pr-1.5"
        >
          <AttachmentIcon attachment={attachment} />
          <span className="max-w-[160px]">
            <span className="block truncate text-[12px]">{attachment.name}</span>
            <span className="block text-[10.5px] text-faint">
              {formatBytes(attachment.size)}
            </span>
          </span>
          <button
            type="button"
            aria-label={`Remove ${attachment.name}`}
            onClick={() => onRemove(attachment.id)}
            className="text-faint hover:text-[var(--ink)]"
          >
            <CloseIcon size={13} />
          </button>
        </span>
      ))}
    </div>
  );
}

/**
 * A screenshot the model was sent but the transcript only tags. Clicking the
 * tag reveals the image the model actually saw.
 */
function HiddenAttachment({
  attachment,
  compact,
}: {
  attachment: Attachment;
  compact: boolean;
}) {
  const [open, setOpen] = useState(false);

  return (
    <span className="flex flex-col items-start gap-1.5">
      <button
        type="button"
        title={open ? "Hide screenshot" : attachment.name}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={cn(
          "flex items-center gap-1.5 rounded-capsule border border-[var(--glass-border)] px-2.5 py-1 text-[11.5px] transition-colors",
          open ? "text-[var(--ink)]" : "text-faint hover:text-[var(--ink)]",
        )}
      >
        <CameraIcon size={12} />
        Screen
        <ChevronDownIcon
          size={12}
          className={cn("transition-transform duration-150", open && "rotate-180")}
        />
      </button>
      {open && (
        <img
          src={assetUrl(attachment.path)}
          alt={attachment.name}
          className={cn(
            "animate-fade-up rounded-row border border-[var(--glass-border)] object-contain",
            compact ? "max-h-40" : "max-h-64",
          )}
        />
      )}
    </span>
  );
}

/**
 * Attachments shown inside a sent message. `compact` keeps thumbnails small
 * for the quick-ask overlay, where a full-height screenshot would swamp the
 * blob it belongs to.
 */
export function AttachmentStrip({
  attachments,
  compact = false,
}: {
  attachments: Attachment[];
  compact?: boolean;
}) {
  if (attachments.length === 0) return null;

  return (
    <div className={cn("mb-2 flex flex-wrap gap-2", "select-none")}>
      {attachments.map((attachment) =>
        attachment.hidden ? (
          <HiddenAttachment
            key={attachment.id}
            attachment={attachment}
            compact={compact}
          />
        ) : attachment.kind === "image" ? (
          <img
            key={attachment.id}
            src={assetUrl(attachment.path)}
            alt={attachment.name}
            title={attachment.name}
            className={cn(
              "rounded-row border border-[var(--glass-border)] object-cover",
              compact ? "max-h-28" : "max-h-52",
            )}
          />
        ) : (
          <span
            key={attachment.id}
            title={attachment.name}
            className="flex items-center gap-2 rounded-row border border-[var(--glass-border)] px-2.5 py-1.5 text-[12px] text-soft"
          >
            <FileIcon size={14} />
            {attachment.name}
            <span className="text-faint">{formatBytes(attachment.size)}</span>
          </span>
        ),
      )}
    </div>
  );
}



