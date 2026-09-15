import { cn } from "../lib/cn";
import { assetUrl } from "../lib/tauri";
import type { Attachment } from "../types";
import { CloseIcon, FileIcon } from "./icons";

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

/** Attachments shown inside a sent message. */
export function AttachmentStrip({ attachments }: { attachments: Attachment[] }) {
  if (attachments.length === 0) return null;

  return (
    <div className={cn("mb-2 flex flex-wrap gap-2", "select-none")}>
      {attachments.map((attachment) =>
        attachment.kind === "image" ? (
          <img
            key={attachment.id}
            src={assetUrl(attachment.path)}
            alt={attachment.name}
            title={attachment.name}
            className="max-h-52 rounded-row border border-[var(--glass-border)] object-cover"
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



