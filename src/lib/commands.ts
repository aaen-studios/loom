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

/**
 * The prefix a command adds to the message it sends.
 *
 * A command that starts a turn sets something *and* says something, and the
 * message is what the model reads. `Goal:` and `Task:` are therefore load
 * bearing rather than decoration: they are the instruction standing on its own
 * in the transcript, which is what lets the reply make sense to anyone reading
 * it later — including a condensed view of it, where the goal-setting command
 * itself is long gone.
 */
export const GOAL_PREFIX = "Goal: ";
export const TASK_PREFIX = "Task: ";

/** What a user message's leading marker says about where it came from. */
export interface UserPrefix {
  kind: "goal" | "task" | "command";
  /** The badge text: `Goal`, `Task`, or the command as typed. */
  label: string;
  /** The message with the marker removed. */
  rest: string;
}

/**
 * Reads the marker off a user message, for the badge the transcript draws in
 * front of it.
 *
 * Recognises two things: the prefixes a command adds when it sends (`Goal: …`,
 * `Task: …`), and a `/command` token left at the head of a message — which is
 * what older chats and a message sent past a dismissed menu contain.
 */
export function parseUserPrefix(content: string): UserPrefix | null {
  const goal = content.match(/^Goal:\s+([\s\S]*)$/);
  if (goal) return { kind: "goal", label: "Goal", rest: goal[1].trim() };

  const task = content.match(/^Task:\s+([\s\S]*)$/);
  if (task) return { kind: "task", label: "Task", rest: task[1].trim() };

  const slash = content.match(/^\/([a-z0-9-]+)\s+([\s\S]*)$/);
  if (slash) {
    const command = BUILTIN_COMMANDS.find((entry) => entry.id === slash[1].toLowerCase());
    if (command) {
      return { kind: "command", label: `/${command.id}`, rest: slash[2].trim() };
    }
  }
  return null;
}
