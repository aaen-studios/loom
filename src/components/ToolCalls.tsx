import { useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { assetUrl } from "../lib/tauri";
import { useChat } from "../stores/chat";
import { parseToolCalls } from "../lib/messageExtra";
import { ipc } from "../lib/ipc";
import type { PendingPermission, ToolCallRecord } from "../types";
import { ChevronDownIcon, WrenchIcon } from "./icons";

const STATUS_LABEL: Record<ToolCallRecord["status"], string> = {
  running: "running",
  ok: "done",
  error: "failed",
  denied: "denied",
};

const STATUS_CLASS: Record<ToolCallRecord["status"], string> = {
  running: "border-[var(--accent)]/50 text-[var(--accent)]",
  ok: "border-[var(--glass-border)] text-faint",
  error: "border-[var(--danger)]/50 text-[var(--danger)]",
  denied: "border-[var(--danger)]/40 text-[var(--danger)]",
};

function argumentsSummary(raw: string): string {
  if (!raw.trim()) return "";
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object") {
      const entries = Object.entries(parsed as Record<string, unknown>);
      return entries
        .map(([key, value]) => `${key}=${String(value).slice(0, 40)}`)
        .join(" ");
    }
  } catch {
    // fall through to the raw string
  }
  return raw.slice(0, 60);
}

function ToolCallCard({ call }: { call: ToolCallRecord }) {
  const running = call.status === "running";
  return (
    <details
      className="group rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)]"
      open={running}
    >
      <summary className="flex cursor-pointer list-none items-center gap-2 px-2.5 py-1.5 text-[12.5px]">
        <WrenchIcon size={14} className={running ? "cursor-blink" : ""} />
        <span className="font-medium">{call.name}</span>
        <span
          className={cn(
            "rounded-full border px-1.5 py-0.5 text-[10.5px]",
            STATUS_CLASS[call.status],
          )}
        >
          {STATUS_LABEL[call.status]}
        </span>
        <span className="ml-1 min-w-0 flex-1 truncate text-[11.5px] text-faint">
          {argumentsSummary(call.arguments)}
        </span>
        <ChevronDownIcon
          size={13}
          className="text-faint transition group-open:rotate-180"
        />
      </summary>
      <div className="max-h-72 overflow-auto border-t border-[var(--glass-border)] px-2.5 py-2">
        {call.name === "generate_image" && call.status === "ok" && call.output && (
          <img
            src={assetUrl(call.output.trim())}
            alt="Generated"
            className="mb-2 max-h-80 rounded-row border border-[var(--glass-border)]"
          />
        )}
        {call.arguments && (
          <pre className="mb-2 whitespace-pre-wrap font-mono text-[11.5px] text-soft">
            {prettify(call.arguments)}
          </pre>
        )}
        {call.output && call.name !== "generate_image" && (
          <pre className="whitespace-pre-wrap border-t border-[var(--glass-border)] pt-2 font-mono text-[11.5px] text-soft">
            {call.output.length > 4000 ? `${call.output.slice(0, 4000)}\n…` : call.output}
          </pre>
        )}
      </div>
    </details>
  );
}

function prettify(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/** Shared empty list so selectors keep a stable reference. */
const NO_CALLS: ToolCallRecord[] = [];

/** Tool cards for one assistant message, live calls merged over stored ones. */
export function ToolCallList({ messageId, extra }: { messageId: string; extra: string | null }) {
  // The selector must not allocate: returning a fresh `[]` makes React's
  // getSnapshot check fail on every render and loops the component forever.
  const live = useChat((state) => state.liveTools[messageId]);
  const stored = parseToolCalls(extra);
  const merged = mergeCalls(stored, live ?? NO_CALLS);

  if (merged.length === 0) return null;
  return (
    <div className="mb-2 flex flex-col gap-1.5">
      {merged.map((call) => (
        <ToolCallCard key={`${call.id}-${call.status}`} call={call} />
      ))}
    </div>
  );
}

function mergeCalls(stored: ToolCallRecord[], live: ToolCallRecord[]): ToolCallRecord[] {
  if (stored.length === 0) return live;
  const merged = [...stored];
  for (const call of live) {
    const index = merged.findIndex((item) => item.id === call.id);
    if (index >= 0) merged[index] = call;
    else merged.push(call);
  }
  return merged;
}

/** Prompt for a tool call that needs the user's approval. */
export function PermissionCard({ permission }: { permission: PendingPermission }) {
  const answer = useChat((state) => state.answerPermission);
  const [tool, setTool] = useState<{ readOnly: boolean } | null>(null);

  useEffect(() => {
    void ipc.listTools().then((tools) => {
      const found = tools?.find((item) => item.name === permission.name);
      setTool(found ? { readOnly: found.readOnly } : null);
    });
  }, [permission.name]);

  return (
    <div className="panel-strong animate-fade-up mx-auto w-full max-w-3xl rounded-sheet p-3">
      <p className="text-[13px]">
        Loom wants to run{" "}
        <span className="font-semibold">{permission.name}</span>
        {tool?.readOnly === false && (
          <span className="ml-1.5 rounded-full border border-[var(--danger)]/40 px-1.5 py-0.5 text-[10.5px] text-[var(--danger)]">
            can modify files
          </span>
        )}
      </p>
      {permission.arguments && (
        <pre className="mt-2 max-h-32 overflow-auto rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] p-2 font-mono text-[11.5px] text-soft">
          {prettify(permission.arguments)}
        </pre>
      )}
      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          onClick={() => void answer(false)}
          className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft hover:text-[var(--danger)]"
        >
          Deny
        </button>
        <button
          type="button"
          onClick={() => void answer(true)}
          className="rounded-full bg-[var(--control-bg)] px-3 py-1 text-[12.5px] font-medium text-[var(--control-ink)]"
        >
          Allow once
        </button>
        <button
          type="button"
          onClick={() =>
            void answer(true, permission.readOnly ? "read-only" : "all")
          }
          className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft"
        >
          {permission.readOnly ? "Always allow reads" : "Always allow"}
        </button>
      </div>
    </div>
  );
}

export { prettify };

