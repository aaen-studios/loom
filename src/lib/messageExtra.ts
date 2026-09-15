import type { Attachment, ToolCallRecord, Usage } from "../types";

/**
 * `messages.extra` has two shapes: user messages store an attachment array,
 * assistant messages store `{ toolCalls: [...], usage: {...} }`.
 */
export function parseAttachments(extra: string | null): Attachment[] {
  if (!extra) return [];
  try {
    const parsed = JSON.parse(extra);
    return Array.isArray(parsed) ? (parsed as Attachment[]) : [];
  } catch {
    return [];
  }
}

export function parseToolCalls(extra: string | null): ToolCallRecord[] {
  if (!extra) return [];
  try {
    const parsed = JSON.parse(extra);
    if (parsed && Array.isArray(parsed.toolCalls)) {
      return parsed.toolCalls as ToolCallRecord[];
    }
  } catch {
    // not a tool-call payload
  }
  return [];
}

/** Why a turn failed, recorded on the assistant message by the engine. */
export function parseError(extra: string | null): string | null {
  if (!extra) return null;
  try {
    const parsed = JSON.parse(extra);
    const error = parsed?.error;
    return typeof error === "string" && error.trim() ? error : null;
  } catch {
    return null;
  }
}

export function parseUsage(extra: string | null): Usage | null {
  if (!extra) return null;
  try {
    const parsed = JSON.parse(extra);
    if (parsed && parsed.usage && typeof parsed.usage === "object") {
      const usage = parsed.usage as Usage;
      if (usage.inputTokens != null || usage.outputTokens != null) return usage;
    }
  } catch {
    // not a usage payload
  }
  return null;
}

/** "2.1k in · 340 out" for the message footer. */
export function formatUsage(usage: Usage): string {
  const compact = (value: number | null) => {
    if (value == null) return null;
    return value >= 1000 ? `${Math.round(value / 100) / 10}k` : String(value);
  };

  const input = compact(usage.inputTokens);
  const output = compact(usage.outputTokens);
  if (input && output) return `${input} in · ${output} out`;
  if (input) return `${input} in`;
  if (output) return `${output} out`;
  return "";
}
