import { useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import { shortId } from "../lib/mentions";
import type { Session } from "../types";
import { FileIcon, FolderIcon, MessageIcon } from "./icons";
import { LiquidSurface } from "./LiquidSurface";

/** One row of the popup, already rendered into what the picker shows. */
export interface MentionOption {
  /** Stable key and the value inserted after the trigger. */
  id: string;
  /** The text drawn in the row. */
  label: string;
  /** A quieter second line: a chat's age, or a file's folder. */
  detail: string | null;
}

export interface MentionMenuProps {
  trigger: "#" | "@";
  options: MentionOption[];
  /** Highlighted row. Owned by the composer so its key handling can move it. */
  index: number;
  onPick: (option: MentionOption) => void;
  onHover: (index: number) => void;
  /** Why there is nothing to show, when there is nothing to show. */
  emptyHint: string | null;
}

/**
 * The `#` and `@` autocomplete, positioned above the composer like the `/` menu.
 *
 * A pure view: the query, the highlighted row and the keyboard handling all
 * live in the composer, because those are what make the popup and the textarea
 * agree about what Enter means. This only draws.
 */
export function MentionMenu({
  trigger,
  options,
  index,
  onPick,
  onHover,
  emptyHint,
}: MentionMenuProps) {
  const listRef = useRef<HTMLDivElement>(null);

  // Keep the highlighted row visible while the arrow keys walk a long list.
  useEffect(() => {
    const row = listRef.current?.children[index] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const isChat = trigger === "#";

  return (
    <LiquidSurface
        surface="popovers"
        layout="block"
        tint="var(--panel-bg-strong)" className="absolute bottom-full left-0 z-40 mb-2 w-[360px] overflow-hidden rounded-sheet p-1.5">
      <p className="px-2 pt-1.5 pb-0.5 text-[10.5px] font-semibold tracking-[0.08em] text-faint uppercase">
        {isChat ? "Chats" : "Files"}
      </p>

      {options.length === 0 ? (
        <p className="px-2 py-2 text-[12px] leading-5 text-faint">
          {emptyHint ?? "Nothing matches yet."}
        </p>
      ) : (
        <div ref={listRef} role="listbox" className="max-h-[240px] overflow-y-auto">
          {options.map((option, position) => (
            <button
              key={option.id}
              type="button"
              role="option"
              aria-selected={position === index}
              onMouseEnter={() => onHover(position)}
              // mousedown, not click: the textarea must not lose focus, and a
              // click would let the blur land before the pick does.
              onMouseDown={(event) => {
                event.preventDefault();
                onPick(option);
              }}
              className={cn(
                "flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left",
                position === index ? "bg-[var(--hover-bg)]" : "hover:bg-[var(--hover-bg)]",
              )}
            >
              <span className="shrink-0 text-faint">
                {isChat ? (
                  <MessageIcon size={13} />
                ) : (
                  <FileIcon size={13} />
                )}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12.5px] text-soft">
                  {option.label}
                </span>
                {option.detail && (
                  <span className="block truncate text-[11px] text-faint">
                    {option.detail}
                  </span>
                )}
              </span>
            </button>
          ))}
        </div>
      )}

      <p className="px-2 py-1 text-[11px] text-faint">
        {isChat
          ? "The chat's id is added when you send, so the model can read it."
          : "↑↓ to choose · Enter to insert · Esc to dismiss"}
      </p>
    </LiquidSurface>
  );
}

/** What a chat row shows beneath its title: when it was last used. */
export function chatDetail(session: Session): string {
  const workspace = session.workdir
    ? session.workdir.split(/[\\/]/).filter(Boolean).pop()
    : null;
  const id = shortId(session.id);
  return workspace ? `${workspace} · ${id}` : id;
}

/** What a file row shows beneath its path: the folder it lives in. */
export function fileDetail(path: string): string | null {
  const parts = path.split("/");
  if (parts.length < 2) return null;
  return `${parts.slice(0, -1).join("/")}/`;
}

/** Kept out of the render path: the folder glyph is unused here but the import
 *  documents that a file row can carry one if the list ever groups by folder. */
export const FOLDER_GLYPH = FolderIcon;
