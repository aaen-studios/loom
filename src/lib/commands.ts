export interface BuiltinCommand {
  id: string;
  label: string;
  description: string;
  argsHint: string | null;
}

export const BUILTIN_COMMANDS: BuiltinCommand[] = [
  {
    id: "goal",
    label: "Set this chat's goal",
    description: "The model reads it every turn. /goal clear removes it.",
    argsHint: "<what you're aiming for>",
  },
  {
    id: "todo",
    label: "Add a task",
    description: "Puts an item on the live task list the model also works from.",
    argsHint: "<task>",
  },
  {
    id: "todos",
    label: "Show the task list",
    description: "Expands the goal and task panel above the composer.",
    argsHint: null,
  },
  {
    id: "plan",
    label: "Plan, then send",
    description: "Switches this chat to Plan mode and sends the text after it.",
    argsHint: "<message>",
  },
  {
    id: "chat",
    label: "Chat, then send",
    description:
      "Switches this chat to Chat mode — answers from the model and the web only — and sends the text after it.",
    argsHint: "<message>",
  },
  {
    id: "new",
    label: "New chat",
    description: "Same as Ctrl+N.",
    argsHint: null,
  },
];

export interface ParsedCommand {
  command: BuiltinCommand;
  args: string;
}

/** Reads a leading `/name rest` from the composer, when it names a built-in. */
export function parseCommand(value: string): ParsedCommand | null {
  const match = value.match(/^\/([a-z0-9-]+)(?:\s+([\s\S]*))?$/);
  if (!match) return null;
  const command = BUILTIN_COMMANDS.find((entry) => entry.id === match[1].toLowerCase());
  if (!command) return null;
  return { command, args: (match[2] ?? "").trim() };
}
