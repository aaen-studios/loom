import { useState } from "react";
import { cn } from "../lib/cn";
import { useChat } from "../stores/chat";
import { LiquidSurface } from "./LiquidSurface";
import type { QueuedMessage } from "../types";
import { ArrowUpIcon, CloseIcon, GripIcon } from "./icons";

const NO_QUEUE: QueuedMessage[] = [];

/**
 * Messages typed while a reply is running. They go out one by one as turns
 * finish; dragging reorders them, and the arrow sends one immediately,
 * interrupting whatever is streaming.
 */
export function MessageQueue() {
  const items = useChat((state) =>
    state.activeId ? state.queues[state.activeId] ?? NO_QUEUE : NO_QUEUE,
  );
  const removeQueued = useChat((state) => state.removeQueued);
  const reorderQueue = useChat((state) => state.reorderQueue);
  const sendQueuedNow = useChat((state) => state.sendQueuedNow);
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);

  if (items.length === 0) return null;

  return (
    <LiquidSurface
      surface="cards"
      layout="block"
      className="mb-2 w-full rounded-sheet"
      tint="var(--panel-bg-strong)"
    >
      <div className="flex items-baseline gap-2 px-3 pt-2 pb-1">
        <span className="shrink-0 text-[10px] font-semibold tracking-[0.1em] text-faint uppercase">
          Queued · {items.length}
        </span>
        <span className="min-w-0 flex-1 truncate text-right text-[10.5px] text-faint">
          drag to reorder · sends when the reply finishes
        </span>
      </div>
      <ul className="px-1.5 pb-1.5">
        {items.map((item) => (
          <li
            key={item.id}
            draggable
            onDragStart={(event) => {
              setDragId(item.id);
              event.dataTransfer.effectAllowed = "move";
            }}
            onDragEnd={() => {
              setDragId(null);
              setOverId(null);
            }}
            onDragOver={(event) => {
              event.preventDefault();
              setOverId(item.id);
            }}
            onDragLeave={() =>
              setOverId((current) => (current === item.id ? null : current))
            }
            onDrop={(event) => {
              event.preventDefault();
              if (dragId) reorderQueue(dragId, item.id);
              setDragId(null);
              setOverId(null);
            }}
            className={cn(
              "group flex cursor-grab items-center gap-2 rounded-row py-1.5 pr-1.5 pl-1 transition",
              dragId === item.id
                ? "opacity-40"
                : "hover:bg-[var(--hover-bg)]",
              overId === item.id && dragId !== item.id && "ring-1 ring-[var(--accent)]",
            )}
          >
            <GripIcon className="text-faint/70" />
            <span
              className="min-w-0 flex-1 truncate text-[12.5px] text-soft"
              title={item.text}
            >
              {item.text}
            </span>
            {item.attachments.length > 0 && (
              <span className="shrink-0 rounded-capsule border border-[var(--glass-border)] px-1.5 py-0.5 text-[10.5px] text-faint">
                {item.attachments.length} file{item.attachments.length === 1 ? "" : "s"}
              </span>
            )}
            <button
              type="button"
              title="Send now — interrupts the running reply"
              aria-label="Send now"
              onMouseDown={(event) => event.stopPropagation()}
              onClick={() => void sendQueuedNow(item.id)}
              className="grid h-6 w-6 shrink-0 place-items-center rounded-control text-faint hover:text-[var(--accent)]"
            >
              <ArrowUpIcon size={13} />
            </button>
            <button
              type="button"
              title="Remove from the queue"
              aria-label="Remove from queue"
              onMouseDown={(event) => event.stopPropagation()}
              onClick={() => removeQueued(item.id)}
              className="grid h-6 w-6 shrink-0 place-items-center rounded-control text-faint hover:text-[var(--danger)]"
            >
              <CloseIcon size={12} />
            </button>
          </li>
        ))}
      </ul>
    </LiquidSurface>
  );
}
