import type { AgentMode, PermissionMode } from "../types";

/**
 * One selectable mode. `tone` drives the chip tint: accent for a deliberate
 * choice, danger for a warning, neutral otherwise.
 */
export interface ModeOption<T extends string> {
  id: T;
  label: string;
  help: string;
  tone: "neutral" | "accent" | "danger";
}

/**
 * The permission modes, in the order the chips render them: Ask, Auto read,
 * Auto all, then Atelier underneath. Atelier is deliberately last and tinted
 * accent, not danger — it is a choice, not a warning — and it is per chat only
 * (the Settings segmented control offers the first three as a global default).
 */
export const PERMISSION_MODES: ModeOption<PermissionMode>[] = [
  { id: "ask", label: "Ask", help: "Confirm every tool call", tone: "neutral" },
  {
    id: "auto-read-only",
    label: "Auto read",
    help: "Read-only tools run silently",
    tone: "neutral",
  },
  { id: "auto-all", label: "Auto all", help: "Run every tool without asking", tone: "danger" },
  {
    id: "atelier",
    label: "Atelier",
    help: "Run everything, let the model edit Loom's harness — personas, MCP servers, skills, prompts, providers, settings — and delete without asking",
    tone: "accent",
  },
];

/** The global default can never be Atelier; these are the Settings options. */
export const GLOBAL_PERMISSION_MODES = PERMISSION_MODES.filter(
  (mode) => mode.id !== "atelier",
);

export const AGENT_MODES: ModeOption<AgentMode>[] = [
  {
    id: "plan",
    label: "Plan",
    help: "Inspect and propose; write tools are refused",
    tone: "accent",
  },
  { id: "build", label: "Build", help: "Change the workspace and run commands", tone: "neutral" },
  {
    id: "review",
    label: "Review",
    help: "Read, then report issues ranked by severity; write tools are refused",
    tone: "accent",
  },
  {
    id: "chat",
    label: "Chat",
    help: "Answer from the model and the web only; fastest",
    tone: "accent",
  },
];

/**
 * Whether the permission card may offer "Always allow".
 *
 * Two cases say no.
 *
 * In Atelier the button would persist Auto all as the global default, which
 * both drops the harness tools and silently discards the mode. That matters
 * for the one card Atelier can still raise — a new schedule — where "always
 * allow" would quietly hand the same question to every later chat as well.
 *
 * `delete_path` is the other. Under Auto all it only produces a card when
 * losing the path would be real, so "Always allow" is the one answer that must
 * not be on offer: it would spend a single click on silencing exactly the
 * question worth asking. The other two buttons cover it.
 */
export function canAlwaysAllow(
  mode: PermissionMode | null | undefined,
  name?: string,
): boolean {
  if (mode === "atelier") return false;
  return name !== "delete_path";
}
