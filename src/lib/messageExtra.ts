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

/** Live records win over stored ones with the same id. */
export function mergeToolCalls(
  stored: ToolCallRecord[],
  live: ToolCallRecord[],
): ToolCallRecord[] {
  if (live.length === 0) return stored;
  if (stored.length === 0) return live;
  const merged = [...stored];
  for (const call of live) {
    const index = merged.findIndex((item) => item.id === call.id);
    if (index >= 0) merged[index] = call;
    else merged.push(call);
  }
  return merged;
}

/**
 * Splits calls into runs of consecutive calls to the same tool, so the
 * transcript can fold "read, read, read" into one line.
 */
export function groupToolRuns(calls: ToolCallRecord[]): ToolCallRecord[][] {
  const runs: ToolCallRecord[][] = [];
  for (const call of calls) {
    const last = runs[runs.length - 1];
    if (last && last[0].name === call.name) last.push(call);
    else runs.push([call]);
  }
  return runs;
}

/** One spell of thinking, with the reply-text offset it preceded. */
export interface ReasoningBlock {
  text: string;
  after?: number;
  /** Stream order within the turn, breaking ties at the same offset. */
  seq?: number;
}

export function parseReasoningBlocks(extra: string | null): ReasoningBlock[] {
  if (!extra) return [];
  try {
    const parsed = JSON.parse(extra);
    if (parsed && Array.isArray(parsed.reasoningBlocks)) {
      return parsed.reasoningBlocks as ReasoningBlock[];
    }
  } catch {
    // not a reasoning payload
  }
  return [];
}

/** One piece of an assistant reply: prose, thinking, or a run of tool calls. */
export type MessageSegment =
  | { kind: "text"; text: string }
  | { kind: "reasoning"; text: string; seq: number }
  | { kind: "tools"; calls: ToolCallRecord[] };

/** True when the reply is already at a clean sentence end at this offset. */
function atBoundary(chars: string[], offset: number): boolean {
  if (offset <= 0 || offset >= chars.length) return true;
  const previous = chars[offset - 1];
  return previous === "\n" || previous === "." || previous === "!" || previous === "?";
}

const isWordChar = (ch: string | undefined) =>
  !!ch && /[\p{L}\p{N}_]/u.test(ch);

/**
 * Moves a segment's offset to a natural reading boundary: the end of the
 * sentence it falls inside (within a short limit), or at least the end of the
 * word. Models resume mid-word across tool rounds, and splitting the reply
 * there reads as a bug even though it is the real stream order.
 */
function snapToBoundary(chars: string[], offset: number): number {
  if (atBoundary(chars, offset)) return offset;
  let end = offset;
  while (end < chars.length && isWordChar(chars[end])) end += 1;
  const limit = Math.min(chars.length, end + 240);
  for (let index = end; index < limit; index += 1) {
    const ch = chars[index];
    if (ch === "\n") return index + 1;
    if (ch === "." || ch === "!" || ch === "?") {
      const previous = chars[index - 1];
      const next = chars[index + 1];
      // Skip ellipses and decimals; a missing space after the stop is fine —
      // models join sentences without one.
      if (next === ".") continue;
      if (/[0-9]/.test(previous ?? "") && /[0-9]/.test(next ?? "")) continue;
      return index + 1;
    }
  }
  return end;
}

/**
 * Splits a reply into text, thinking and tool-call segments in stream order.
 *
 * Tool records and reasoning blocks both carry `after` (reply-text offset) and
 * `seq` (turn order). Sorting by both keeps "think → call" ahead of
 * "call → think" even when no text separates them, which happens on every
 * agentic loop. Calls that share an offset stay in one stack. Offsets inside a
 * sentence are snapped forward so no segment ever cuts prose mid-word.
 */
export function segmentMessage(
  content: string,
  calls: ToolCallRecord[],
  reasoning: ReasoningBlock[] = [],
  snapBoundaries = true,
): MessageSegment[] {
  const text = [...content];
  const clamp = (after: number | undefined) =>
    Math.max(0, Math.min(after ?? 0, text.length));
  const snap = (after: number | undefined) =>
    snapBoundaries ? snapToBoundary(text, clamp(after)) : clamp(after);

  const entries: {
    after: number;
    seq: number;
    call?: ToolCallRecord;
    reasoning?: string;
  }[] = [];
  for (const call of calls) {
    entries.push({ after: snap(call.after), seq: call.seq ?? 0, call });
  }
  for (const block of reasoning) {
    if (!block.text.trim()) continue;
    entries.push({
      after: snap(block.after),
      seq: block.seq ?? 0,
      reasoning: block.text,
    });
  }
  entries.sort((a, b) => a.after - b.after || a.seq - b.seq);

  const segments: MessageSegment[] = [];
  let cursor = 0;
  let pending: ToolCallRecord[] = [];
  const flush = () => {
    if (pending.length > 0) {
      segments.push({ kind: "tools", calls: pending });
      pending = [];
    }
  };

  for (const entry of entries) {
    if (entry.after > cursor) {
      flush();
      segments.push({ kind: "text", text: text.slice(cursor, entry.after).join("") });
      cursor = entry.after;
    }
    if (entry.call) {
      pending.push(entry.call);
    } else {
      flush();
      segments.push({ kind: "reasoning", text: entry.reasoning ?? "", seq: entry.seq });
    }
  }
  flush();
  if (cursor < text.length) {
    segments.push({ kind: "text", text: text.slice(cursor).join("") });
  }
  return segments;
}

/** Why a turn stopped early, as recorded on the assistant message. */
export interface Notice {
  /** The one line the transcript shows. */
  text: string;
  /** The provider's own words, behind a Details toggle. */
  detail: string | null;
}

/**
 * Why a turn stopped early, if it did. Reads the current field, and upgrades a
 * reply written by a build that only had `error`.
 */
export function parseNotice(extra: string | null): Notice | null {
  if (!extra) return null;
  try {
    const parsed = JSON.parse(extra);
    const notice = parsed?.notice;
    if (notice && typeof notice.text === "string" && notice.text.trim()) {
      return {
        text: notice.text,
        detail: typeof notice.detail === "string" ? notice.detail : null,
      };
    }
    // Older replies stored the reason as a bare string under `error`.
    const legacy = parsed?.error;
    return typeof legacy === "string" && legacy.trim()
      ? { text: legacy, detail: null }
      : null;
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
