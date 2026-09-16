import { useEffect, useState, type ReactNode } from "react";
import { cn } from "../lib/cn";
import { canAlwaysAllow } from "../lib/modes";
import { assetUrl } from "../lib/tauri";
import { useChat } from "../stores/chat";
import { ipc } from "../lib/ipc";
import { groupToolRuns } from "../lib/messageExtra";
import type {
  PendingPermission,
  ToolCallDisplay,
  ToolCallRecord,
  ToolCallStatus,
  ToolScope,
} from "../types";
import {
  BrainIcon,
  CameraIcon,
  CheckIcon,
  ChevronDownIcon,
  CopyIcon,
  DownloadIcon,
  EditIcon,
  ExternalLinkIcon,
  FileIcon,
  FolderIcon,
  GitBranchIcon,
  PersonIcon,
  SearchIcon,
  SparkIcon,
  StopIcon,
  TrashIcon,
  WrenchIcon,
} from "./icons";

/** How a call presents in the transcript. */
interface Look {
  icon: ReactNode;
  /** Short verb phrase; the present participle while the call is running. */
  label: string;
  /** The interesting argument, shown quietly after the label. */
  detail: string | null;
  /** Inline calls render as a bare line; the rest get a small card. */
  inline: boolean;
  mono?: boolean;
}

const PENDING = "running";

function argsObject(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function str(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function oneLine(value: string, max = 90): string {
  const flat = value.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

/** What each tool means, in one line a person can read. */
function describe(call: ToolCallRecord): Look {
  const args = argsObject(call.arguments);
  const running = call.status === PENDING;

  if (call.name.startsWith("mcp__")) {
    const [, server, tool] = call.name.split("__");
    return {
      icon: <WrenchIcon size={13} />,
      label: tool || call.name,
      detail: server ? `${server} · mcp` : "mcp",
      inline: false,
    };
  }

  switch (call.name) {
    case "web_search":
      return {
        icon: <SearchIcon size={13} />,
        label: running ? "Searching" : "Searched",
        detail: str(args.query),
        inline: true,
      };
    case "search_workspace":
      return {
        icon: <SearchIcon size={13} />,
        label: running ? "Searching the workspace" : "Searched the workspace",
        detail: str(args.query),
        inline: true,
      };
    case "grep":
      return {
        icon: <SearchIcon size={13} />,
        label: running ? "Searching files" : "Searched files",
        detail: str(args.pattern),
        inline: true,
      };
    case "fetch_url":
      return {
        icon: <DownloadIcon size={13} />,
        label: running ? "Fetching" : "Fetched",
        detail: str(args.url),
        inline: true,
      };
    case "read_file":
      return {
        icon: <FileIcon size={13} />,
        label: running ? "Reading" : "Read",
        detail: str(args.path),
        inline: true,
      };
    case "list_dir":
      return {
        icon: <FolderIcon size={13} />,
        label: running ? "Listing" : "Listed",
        detail: str(args.path),
        inline: true,
      };
    case "datetime":
      return {
        icon: <BrainIcon size={13} />,
        label: running ? "Checking the clock" : "Checked the clock",
        detail: null,
        inline: true,
      };
    case "write_file":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Writing" : "Wrote",
        detail: str(args.path),
        inline: false,
      };
    case "edit_file":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Editing" : "Edited",
        detail: str(args.path),
        inline: false,
      };
    case "run_command":
      return {
        icon: <WrenchIcon size={13} />,
        label: str(args.command) ?? call.name,
        // Background runs are worth calling out: they outlive the reply.
        detail: args.background === true ? "background" : null,
        inline: false,
        mono: true,
      };
    case "list_commands":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Listing commands" : "Listed commands",
        detail: null,
        inline: true,
      };
    case "command_output":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Reading a command's log" : "Read a command's log",
        detail: str(args.id),
        inline: true,
      };
    case "stop_command":
      return {
        icon: <StopIcon size={13} />,
        label: running ? "Stopping a command" : "Stopped a command",
        detail: str(args.id),
        inline: false,
      };
    case "generate_image":
      return {
        icon: <SparkIcon size={13} />,
        label: running ? "Generating an image" : "Generated an image",
        detail: str(args.prompt),
        inline: false,
      };
    case "spawn_agent":
      return {
        icon: <PersonIcon size={13} />,
        label: running ? "Running an agent" : "Ran an agent",
        detail: str(args.task),
        inline: false,
      };
    case "ask_user":
      return {
        icon: <PersonIcon size={13} />,
        label: "Asked a question",
        detail: str(args.question),
        inline: true,
      };
    // Computer tools (only offered while the chat's Computer chip is on).
    case "screenshot":
      return {
        icon: <CameraIcon size={13} />,
        label: running ? "Looking at the screen" : "Looked at the screen",
        detail: str(args.target) ?? "active",
        inline: true,
      };
    case "ui":
      return {
        icon: <PersonIcon size={13} />,
        label: running ? "Driving the UI" : "Drove the UI",
        detail: str(args.action) ? `${str(args.action)} ${str(args.id) ?? ""}`.trim() : null,
        inline: true,
      };
    case "mouse":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Using the mouse" : "Used the mouse",
        detail: str(args.action) ?? null,
        inline: true,
      };
    case "keyboard":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Typing" : "Typed",
        detail: str(args.text) ?? str(args.key) ?? str(args.action),
        inline: true,
      };
    case "clipboard":
      return {
        icon: <CopyIcon size={13} />,
        label: running ? "Using the clipboard" : "Used the clipboard",
        detail: str(args.action),
        inline: true,
      };
    case "list_windows":
      return {
        icon: <SearchIcon size={13} />,
        label: running ? "Listing windows" : "Listed windows",
        detail: str(args.filter),
        inline: true,
      };
    case "window":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Managing a window" : "Managed a window",
        detail: str(args.window),
        inline: false,
      };
    case "list_processes":
      return {
        icon: <SearchIcon size={13} />,
        label: running ? "Listing processes" : "Listed processes",
        detail: str(args.filter),
        inline: true,
      };
    case "launch_app":
      return {
        icon: <ExternalLinkIcon size={13} />,
        label: running ? "Opening" : "Opened",
        detail: str(args.target),
        inline: false,
      };
    case "kill_process":
      return {
        icon: <StopIcon size={11} />,
        label: running ? "Stopping a process" : "Stopped a process",
        detail: str(args.name) ?? (args.pid != null ? String(args.pid) : null),
        inline: false,
      };
    case "wait":
      return {
        icon: <BrainIcon size={13} />,
        label: running ? "Waiting" : "Waited",
        detail: str(args.for_window) ?? (args.seconds != null ? `${args.seconds}s` : null),
        inline: true,
      };
    case "user_takeover":
      return {
        icon: <PersonIcon size={13} />,
        label: "You took over",
        detail: "handed back after the pause",
        inline: true,
      };
    case "find_files":
      return {
        icon: <SearchIcon size={13} />,
        label: running ? "Finding files" : "Found files",
        detail: str(args.pattern),
        inline: true,
      };
    case "create_dir":
      return {
        icon: <FolderIcon size={13} />,
        label: running ? "Creating folder" : "Created folder",
        detail: str(args.path),
        inline: false,
      };
    case "move_path":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Moving" : "Moved",
        detail:
          str(args.from) && str(args.to)
            ? `${str(args.from)} → ${str(args.to)}`
            : str(args.from) ?? str(args.to),
        inline: false,
      };
    case "copy_path":
      return {
        icon: <CopyIcon size={13} />,
        label: running ? "Copying" : "Copied",
        detail:
          str(args.from) && str(args.to)
            ? `${str(args.from)} → ${str(args.to)}`
            : str(args.from) ?? str(args.to),
        inline: false,
      };
    case "delete_path":
      return {
        icon: <TrashIcon size={13} />,
        label: running ? "Deleting" : "Deleted",
        detail: str(args.path),
        inline: false,
      };
    case "git_status":
      return {
        icon: <GitBranchIcon size={13} />,
        label: running ? "Checking git status" : "Checked git status",
        detail: null,
        inline: true,
      };
    case "git_diff":
      return {
        icon: <GitBranchIcon size={13} />,
        label: running ? "Reading the diff" : "Read the diff",
        detail: str(args.path),
        inline: false,
      };
    case "git_log":
      return {
        icon: <GitBranchIcon size={13} />,
        label: running ? "Reading git log" : "Read git log",
        detail: null,
        inline: true,
      };
    case "todo_write": {
      const items = Array.isArray(args.todos) ? args.todos.length : null;
      return {
        icon: <CheckIcon size={13} />,
        label: running ? "Updating the task list" : "Updated the task list",
        detail: items !== null ? `${items} item${items === 1 ? "" : "s"}` : null,
        inline: false,
      };
    }
    case "todo_read":
      return {
        icon: <CheckIcon size={13} />,
        label: "Read the task list",
        detail: null,
        inline: true,
      };
    // Harness tools (Atelier only). The summary rows read like the engine's
    // one-line summaries so a run of edits is legible at a glance.
    case "list_harness":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Listing the harness" : "Listed the harness",
        detail: Array.isArray(args.sections) ? args.sections.join(", ") : null,
        inline: true,
      };
    case "upsert_persona":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Editing persona" : "Edited persona",
        detail: str(args.name) ?? str(args.id),
        inline: false,
      };
    case "delete_persona":
      return {
        icon: <TrashIcon size={13} />,
        label: running ? "Deleting persona" : "Deleted persona",
        detail: str(args.id),
        inline: false,
      };
    case "upsert_prompt":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Editing prompt" : "Edited prompt",
        detail: str(args.title) ?? str(args.id),
        inline: false,
      };
    case "delete_prompt":
      return {
        icon: <TrashIcon size={13} />,
        label: running ? "Deleting prompt" : "Deleted prompt",
        detail: str(args.id),
        inline: false,
      };
    case "write_skill":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Writing skill" : "Wrote skill",
        detail: str(args.id) ? `/${str(args.id)}` : null,
        inline: false,
      };
    case "delete_skill":
      return {
        icon: <TrashIcon size={13} />,
        label: running ? "Deleting skill" : "Deleted skill",
        detail: str(args.id) ? `/${str(args.id)}` : null,
        inline: false,
      };
    case "upsert_mcp_server":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Saving MCP server" : "Saved MCP server",
        detail: str(args.id),
        inline: false,
      };
    case "delete_mcp_server":
      return {
        icon: <TrashIcon size={13} />,
        label: running ? "Deleting MCP server" : "Deleted MCP server",
        detail: str(args.id),
        inline: false,
      };
    case "test_mcp_server": {
      const count = running ? null : /:\s*(\d+)\s+tools?\b/.exec(call.output)?.[1] ?? null;
      return {
        icon: <WrenchIcon size={13} />,
        label: running
          ? "Testing MCP server"
          : count
            ? `Tested MCP server — ${count} tool${count === "1" ? "" : "s"}`
            : "Tested MCP server",
        detail: str(args.id),
        inline: false,
      };
    }
    case "upsert_provider":
      return {
        icon: <WrenchIcon size={13} />,
        label: running ? "Saving provider" : "Saved provider",
        detail: str(args.id),
        inline: false,
      };
    case "delete_provider":
      return {
        icon: <TrashIcon size={13} />,
        label: running ? "Deleting provider" : "Deleted provider",
        detail: str(args.id),
        inline: false,
      };
    case "update_model":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Updating model" : "Updated model",
        detail: str(args.modelId),
        inline: false,
      };
    case "update_settings":
      return {
        icon: <EditIcon size={13} />,
        label: running ? "Updating settings" : "Updated settings",
        detail: null,
        inline: false,
      };
    default:
      return {
        icon: <WrenchIcon size={13} />,
        label: call.name,
        detail: null,
        inline: false,
      };
  }
}

/** How a run of the same tool reads on one line. */
const RUN_LABELS: Record<string, { active: string; past: string; noun: string }> = {
  read_file: { active: "Reading", past: "Read", noun: "files" },
  write_file: { active: "Writing", past: "Wrote", noun: "files" },
  edit_file: { active: "Editing", past: "Edited", noun: "files" },
  list_dir: { active: "Listing", past: "Listed", noun: "folders" },
  grep: { active: "Searching files", past: "Searched files", noun: "times" },
  search_workspace: {
    active: "Searching the workspace",
    past: "Searched the workspace",
    noun: "times",
  },
  web_search: { active: "Searching", past: "Searched", noun: "times" },
  fetch_url: { active: "Fetching", past: "Fetched", noun: "pages" },
  run_command: { active: "Running", past: "Ran", noun: "commands" },
  list_commands: { active: "Listing", past: "Listed", noun: "times" },
  command_output: { active: "Reading", past: "Read", noun: "logs" },
  stop_command: { active: "Stopping", past: "Stopped", noun: "commands" },
  generate_image: { active: "Generating", past: "Generated", noun: "images" },
  spawn_agent: { active: "Running", past: "Ran", noun: "agents" },
  screenshot: { active: "Looking at the screen", past: "Looked at the screen", noun: "times" },
  ui: { active: "Driving the UI", past: "Drove the UI", noun: "times" },
  mouse: { active: "Using the mouse", past: "Used the mouse", noun: "times" },
  keyboard: { active: "Typing", past: "Typed", noun: "times" },
  clipboard: { active: "Using the clipboard", past: "Used the clipboard", noun: "times" },
  list_windows: { active: "Listing windows", past: "Listed windows", noun: "times" },
  window: { active: "Managing windows", past: "Managed windows", noun: "windows" },
  list_processes: { active: "Listing processes", past: "Listed processes", noun: "times" },
  launch_app: { active: "Opening", past: "Opened", noun: "apps" },
  kill_process: { active: "Stopping", past: "Stopped", noun: "processes" },
  wait: { active: "Waiting", past: "Waited", noun: "times" },
  find_files: { active: "Finding", past: "Found", noun: "searches" },
  create_dir: { active: "Creating", past: "Created", noun: "folders" },
  move_path: { active: "Moving", past: "Moved", noun: "paths" },
  copy_path: { active: "Copying", past: "Copied", noun: "paths" },
  delete_path: { active: "Deleting", past: "Deleted", noun: "paths" },
  git_status: { active: "Checking git", past: "Checked git", noun: "times" },
  git_diff: { active: "Reading diffs", past: "Read diffs", noun: "times" },
  git_log: { active: "Reading git logs", past: "Read git logs", noun: "times" },
  todo_write: { active: "Updating", past: "Updated", noun: "task lists" },
  todo_read: { active: "Reading", past: "Read", noun: "task lists" },
  datetime: { active: "Checking the clock", past: "Checked the clock", noun: "times" },
  list_harness: { active: "Listing", past: "Listed", noun: "sections" },
  upsert_persona: { active: "Editing", past: "Edited", noun: "personas" },
  delete_persona: { active: "Deleting", past: "Deleted", noun: "personas" },
  upsert_prompt: { active: "Editing", past: "Edited", noun: "prompts" },
  delete_prompt: { active: "Deleting", past: "Deleted", noun: "prompts" },
  write_skill: { active: "Writing", past: "Wrote", noun: "skills" },
  delete_skill: { active: "Deleting", past: "Deleted", noun: "skills" },
  upsert_mcp_server: { active: "Saving", past: "Saved", noun: "MCP servers" },
  delete_mcp_server: { active: "Deleting", past: "Deleted", noun: "MCP servers" },
  test_mcp_server: { active: "Testing", past: "Tested", noun: "MCP servers" },
  upsert_provider: { active: "Saving", past: "Saved", noun: "providers" },
  delete_provider: { active: "Deleting", past: "Deleted", noun: "providers" },
  update_model: { active: "Updating", past: "Updated", noun: "models" },
  update_settings: { active: "Updating", past: "Updated", noun: "settings" },
};

/** A run of calls to the same tool, summarized as one line. */
function describeRun(run: ToolCallRecord[]): Look {
  const look = describe(run[0]);
  const count = run.length;
  const labels = RUN_LABELS[run[0].name];
  if (!labels) {
    return {
      icon: look.icon,
      label: `${look.label} × ${count}`,
      detail: null,
      inline: look.inline,
      mono: look.mono,
    };
  }
  const running = run.some((call) => call.status === PENDING);
  return {
    icon: look.icon,
    label: `${running ? labels.active : labels.past} ${count} ${labels.noun}`,
    detail: null,
    inline: look.inline,
  };
}

/** Running beats failed beats all-clear, for a whole run. */
function runStatus(run: ToolCallRecord[]): ToolCallStatus {
  if (run.some((call) => call.status === PENDING)) return "running";
  if (run.some((call) => call.status === "error" || call.status === "denied")) return "error";
  return "ok";
}

function StatusGlyph({ status }: { status: ToolCallStatus }) {
  if (status === "running") {
    return (
      <span
        aria-label="running"
        className="inline-block h-3 w-3 shrink-0 animate-spin rounded-full border-[1.5px] border-current border-t-transparent text-faint"
      />
    );
  }
  if (status === "ok") {
    return <CheckIcon size={12} className="shrink-0 text-faint" />;
  }
  return <StopIcon size={11} className="shrink-0 text-[var(--danger)]" />;
}

/** The expanded arguments and output for one call. */
function ToolBody({ call }: { call: ToolCallRecord }) {
  if (call.name === "ask_user") {
    const question = str(argsObject(call.arguments).question);
    return (
      <div className="border-t border-[var(--glass-border)] px-2.5 py-2 text-[12px]">
        {question && <p className="text-soft">{question}</p>}
        {call.output && <p className="mt-1 text-faint">{call.output}</p>}
      </div>
    );
  }

  const output = call.output.length > 6000 ? `${call.output.slice(0, 6000)}\n…` : call.output;
  const search = call.name === "web_search" || call.name === "search_workspace";

  return (
    <div className="border-t border-[var(--glass-border)] px-2.5 py-2">
      {call.arguments && call.arguments !== "{}" && (
        <pre className="mb-2 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] text-faint">
          {prettify(call.arguments)}
        </pre>
      )}
      {output && call.name !== "generate_image" && (
        search ? (
          <SearchResults text={output} />
        ) : (
          <pre className="max-h-72 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] text-soft">
            {output}
          </pre>
        )
      )}
    </div>
  );
}

/** Search output is `n. title\nurl\nsnippet` blocks; show them as a small list. */
function SearchResults({ text }: { text: string }) {
  const rows = text
    .split(/\n\s*\n/)
    .map((block) => block.split("\n").map((line) => line.trim()).filter(Boolean))
    .filter((lines) => lines.length >= 2 && /^\d+\.\s/.test(lines[0]));

  if (rows.length === 0) {
    return (
      <pre className="max-h-72 overflow-auto whitespace-pre-wrap font-mono text-[11.5px] text-soft">
        {text}
      </pre>
    );
  }

  return (
    <ol className="flex flex-col gap-1.5">
      {rows.map((lines, index) => {
        const [title, url, ...rest] = lines;
        return (
          <li key={index} className="min-w-0">
            <p className="text-[12px] text-soft">{title.replace(/^\d+\.\s*/, "")}</p>
            {url && <p className="truncate text-[11px] text-faint">{url}</p>}
            {rest.length > 0 && (
              <p className="text-[11.5px] leading-5 text-faint">{rest.join(" ")}</p>
            )}
          </li>
        );
      })}
    </ol>
  );
}

/** One call, inline line or small card, expandable for the details. */
function ToolRow({
  call,
  display,
  nested = false,
}: {
  call: ToolCallRecord;
  display: ToolCallDisplay;
  /** Inside a run, the parent card already frames the row. */
  nested?: boolean;
}) {
  const look = describe(call);
  const [open, setOpen] = useState(display === "expanded");
  useEffect(() => setOpen(display === "expanded"), [display]);

  const failed = call.status === "error" || call.status === "denied";
  const header = (
    <RowHeader
      look={look}
      status={call.status}
      hasBody={Boolean(call.arguments || call.output)}
      open={open}
      onToggle={() => setOpen((value) => !value)}
      bare={nested || look.inline}
      title={failed && call.output ? oneLine(call.output, 400) : undefined}
    />
  );
  const body = (
    <>
      {call.name === "generate_image" && call.status === "ok" && call.output && (
        <div className="border-t border-[var(--glass-border)] px-2.5 py-2">
          <img
            src={assetUrl(call.output.trim())}
            alt={look.detail ?? "Generated image"}
            className="max-h-80 rounded-row border border-[var(--glass-border)]"
          />
        </div>
      )}
      {call.images && call.images.length > 0 && (
        <div className="flex flex-col gap-1.5 border-t border-[var(--glass-border)] px-2.5 py-2">
          {call.images.map((image) => (
            <img
              key={image.path}
              src={assetUrl(image.path)}
              alt={image.name}
              className="max-h-80 self-start rounded-row border border-[var(--glass-border)]"
            />
          ))}
        </div>
      )}
      {open && <ToolBody call={call} />}
    </>
  );

  if (nested || look.inline) {
    return (
      <div>
        {header}
        {body}
      </div>
    );
  }

  return (
    <div className="rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)]/40">
      {header}
      {body}
    </div>
  );
}

/** A run of calls to the same tool, folded into one line that opens to them. */
function ToolRun({ run, display }: { run: ToolCallRecord[]; display: ToolCallDisplay }) {
  const look = describeRun(run);
  const [open, setOpen] = useState(display === "expanded");
  useEffect(() => setOpen(display === "expanded"), [display]);

  const toggle = () => setOpen((value) => !value);
  const calls = (
    <div className="flex flex-col gap-1">
      {run.map((call) => (
        <ToolRow key={call.id} call={call} display={display} nested />
      ))}
    </div>
  );

  if (look.inline) {
    return (
      <div>
        <RowHeader look={look} status={runStatus(run)} hasBody open={open} onToggle={toggle} bare />
        {open && (
          <div className="mt-1 ml-2 border-l border-[var(--glass-border)] pl-2">{calls}</div>
        )}
      </div>
    );
  }

  return (
    <div className="rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)]/40">
      <RowHeader look={look} status={runStatus(run)} hasBody open={open} onToggle={toggle} />
      {open && <div className="border-t border-[var(--glass-border)] p-1.5">{calls}</div>}
    </div>
  );
}

function RowHeader({
  look,
  status,
  hasBody,
  open,
  onToggle,
  bare = false,
  title,
}: {
  look: Look;
  status: ToolCallStatus;
  hasBody: boolean;
  open: boolean;
  onToggle: () => void;
  bare?: boolean;
  title?: string;
}) {
  const failed = status === "error" || status === "denied";
  return (
    <button
      type="button"
      onClick={hasBody ? onToggle : undefined}
      title={title}
      className={cn(
        "flex w-full items-center gap-2 text-left text-[12.5px] transition",
        bare
          ? "rounded-control px-1.5 py-1 hover:bg-[var(--hover-bg)]"
          : "rounded-row px-2.5 py-1.5 hover:bg-[var(--hover-bg)]",
        hasBody ? "cursor-pointer" : "cursor-default",
      )}
    >
      <span className={cn("shrink-0", failed ? "text-[var(--danger)]" : "text-faint")}>
        {look.icon}
      </span>
      <span
        className={cn(
          "min-w-0 flex-1 truncate",
          look.mono ? "font-mono text-[12px] text-soft" : "text-soft",
        )}
      >
        {look.mono && <span className="text-faint">$ </span>}
        {look.label}
      </span>
      {look.detail && (
        <span className="min-w-0 max-w-[45%] truncate text-[11.5px] text-faint">
          {look.detail}
        </span>
      )}
      <StatusGlyph status={status} />
      {hasBody && (
        <ChevronDownIcon
          size={12}
          className={cn("shrink-0 text-faint transition-transform", open && "rotate-180")}
        />
      )}
    </button>
  );
}

/** Stacks the calls of one round, folding runs of the same tool together. */
export function ToolCallGroup({
  calls,
  display,
}: {
  calls: ToolCallRecord[];
  display: ToolCallDisplay;
}) {
  if (display === "hidden" || calls.length === 0) return null;
  const runs = groupToolRuns(calls);
  return (
    <div className="mb-2 flex flex-col gap-1">
      {runs.map((run) =>
        run.length > 1 ? (
          <ToolRun key={run[0].id} run={run} display={display} />
        ) : (
          <ToolRow key={run[0].id} call={run[0]} display={display} />
        ),
      )}
    </div>
  );
}

/** Prompt for a tool call that needs the user's approval. */
export function PermissionCard({ permission }: { permission: PendingPermission }) {
  const answer = useChat((state) => state.answerPermission);
  const session = useChat((state) =>
    state.sessions.find((item) => item.id === permission.sessionId),
  );
  const [tool, setTool] = useState<{ readOnly: boolean; scope?: ToolScope } | null>(null);

  useEffect(() => {
    void ipc.listTools().then((tools) => {
      const found = tools?.find((item) => item.name === permission.name);
      setTool(found ? { readOnly: found.readOnly, scope: found.scope } : null);
    });
  }, [permission.name]);

  // In Atelier "Always allow" would persist Auto all globally, dropping both
  // the harness tools and the mode itself; the mode already is the standing
  // consent, so the card offers no way to lose it.
  const allowAlways = canAlwaysAllow(session?.permissionMode);

  return (
    <div className="panel-strong animate-fade-up mx-auto w-full max-w-3xl rounded-sheet p-3">
      <p className="text-[13px]">
        Loom wants to run{" "}
        <span className="font-semibold">{permission.name}</span>
        {tool?.scope === "harness" ? (
          <span className="ml-1.5 rounded-full border border-[var(--accent)]/50 px-1.5 py-0.5 text-[10.5px] text-[var(--accent)]">
            edits Loom&apos;s harness
          </span>
        ) : tool?.scope === "computer" ? (
          <span className="ml-1.5 rounded-full border border-[var(--danger)]/40 px-1.5 py-0.5 text-[10.5px] text-[var(--danger)]">
            controls this computer
          </span>
        ) : tool?.readOnly === false ? (
          <span className="ml-1.5 rounded-full border border-[var(--danger)]/40 px-1.5 py-0.5 text-[10.5px] text-[var(--danger)]">
            can modify files
          </span>
        ) : null}
      </p>
      {permission.arguments && (
        <pre className="mt-2 max-h-32 overflow-auto rounded-control border border-[var(--glass-border)] bg-[var(--hover-bg)] p-2 font-mono text-[11.5px] text-soft">
          {prettify(permission.arguments)}
        </pre>
      )}
      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          onClick={() => void answer(permission, false)}
          className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft hover:text-[var(--danger)]"
        >
          Deny
        </button>
        <button
          type="button"
          onClick={() => void answer(permission, true)}
          className="rounded-full bg-[var(--control-bg)] px-3 py-1 text-[12.5px] font-medium text-[var(--control-ink)]"
        >
          Allow once
        </button>
        {allowAlways ? (
          <button
            type="button"
            onClick={() =>
              void answer(permission, true, permission.readOnly ? "read-only" : "all")
            }
            className="rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12.5px] text-soft"
          >
            {permission.readOnly ? "Always allow reads" : "Always allow"}
          </button>
        ) : (
          <span className="text-[11.5px] leading-5 text-faint">
            Atelier already allows every tool; a deletion is the only step that
            asks.
          </span>
        )}
      </div>
    </div>
  );
}

export function prettify(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
