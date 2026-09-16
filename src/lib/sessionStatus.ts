import type { ToolCallRecord } from "../types";

/** "mcp__server__tool" reads as "tool" in a one-line status. */
function toolName(name: string): string {
  if (name.startsWith("mcp__")) {
    const parts = name.split("__");
    return parts[2] || name;
  }
  return name;
}

/**
 * What a running chat is doing, in one line for the sidebar. Tool calls beat
 * buffers (the most concrete thing happening), then reply text, then
 * reasoning; a reply with reasoning already banked is still "Replying".
 */
export function activityLabel(
  live: { messageId: string; content: string; reasoning: string } | undefined,
  liveTools: readonly ToolCallRecord[] | undefined,
): string {
  const running = liveTools?.find((call) => call.status === "running");
  if (running) return `Running ${toolName(running.name)}…`;
  if (live?.content) return "Replying…";
  if (live?.reasoning) return "Thinking…";
  return "Working…";
}
