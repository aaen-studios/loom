import { useEffect, useMemo, useState } from "react";
import { cn } from "../lib/cn";
import { ipc } from "../lib/ipc";
import { isTauri } from "../lib/tauri";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { activeCommandCount, activeTaskCount, useTasks } from "../stores/tasks";
import { useUi } from "../stores/ui";
import type { CommandRun, CommandStatus, Job, Message, Task, TaskStatus } from "../types";
import { CloseIcon, PlayIcon, PlusIcon, StopIcon, TrashIcon } from "./icons";
import { EmptyState, Row, Section, Toggle, inputClass } from "./ui";

/** A dot whose colour carries the run's state. */
function StatusDot({ status }: { status: TaskStatus }) {
  const tone =
    status === "running"
      ? "bg-[var(--accent)]"
      : status === "queued"
        ? "bg-[var(--ink-faint)]"
        : status === "done"
          ? "bg-emerald-400"
          : status === "failed"
            ? "bg-[var(--danger)]"
            : "bg-[var(--ink-faint)] opacity-60";
  return <span className={cn("mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full", tone)} />;
}

/** A dot whose colour carries a command's state. */
function commandTone(status: CommandStatus): string {
  switch (status) {
    case "running":
      return "bg-[var(--accent)]";
    case "done":
      return "bg-emerald-400";
    case "failed":
      return "bg-[var(--danger)]";
    default:
      // stopped / orphaned: over, but not a result.
      return "bg-[var(--ink-faint)] opacity-60";
  }
}

function commandStatusLabel(command: CommandRun): string {
  switch (command.status) {
    case "running":
      return "Running";
    case "done":
      return "Finished";
    case "failed":
      return `Failed${command.exitCode != null ? ` (exit ${command.exitCode})` : ""}`;
    case "stopped":
      return "Stopped";
    case "orphaned":
      return "Orphaned — Loom restarted";
  }
}

/** `1m 20s`, `2.1s`, `0.4s`: how long a command has been (or was) running. */
function elapsed(command: CommandRun): string {
  const end = command.finishedAt ?? Date.now();
  const seconds = Math.max(0, (end - command.createdAt) / 1000);
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${Math.round(seconds % 60)}s`;
}

function statusLabel(task: Task): string {
  switch (task.status) {
    case "queued":
      return "Queued";
    case "running":
      return task.detail?.includes("approval") ? "Waiting for you" : "Running";
    case "done":
      return "Finished";
    case "failed":
      return "Failed";
    case "interrupted":
      return "Interrupted";
    case "cancelled":
      return "Cancelled";
    case "skipped":
      return "Skipped";
  }
}

function when(ms: number | null): string {
  if (!ms) return "—";
  const date = new Date(ms);
  const today = new Date();
  const sameDay = date.toDateString() === today.toDateString();
  return sameDay
    ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : date.toLocaleString([], {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
}

export function TasksPanel() {
  const open = useUi((state) => state.tasksOpen);
  const setOpen = useUi((state) => state.setTasksOpen);
  const tasks = useTasks((state) => state.tasks);
  const jobs = useTasks((state) => state.jobs);
  const commands = useTasks((state) => state.commands);
  const load = useTasks((state) => state.load);
  const [tab, setTab] = useState<"runs" | "shell" | "jobs">("runs");
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<Job | null>(null);
  const active = activeTaskCount(tasks);
  const busyCommands = activeCommandCount(commands);
  const selectedTask = tasks.find((task) => task.id === selected) ?? null;
  const selectedCommand = commands.find((command) => command.id === selected) ?? null;

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (editing) setEditing(null);
        else if (selected) setSelected(null);
        else setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, editing, selected, setOpen]);

  if (!open) return null;

  return (
    <div className="absolute inset-0 z-40 flex justify-end p-3 pt-16">
      <button
        type="button"
        aria-label="Close runs"
        onClick={() => setOpen(false)}
        className="absolute inset-0 cursor-default bg-black/10"
      />
      <div className="animate-fade-up panel-strong relative flex h-full w-[560px] flex-col overflow-hidden rounded-sheet">
        <div className="flex items-center gap-2 px-4 pt-3 pb-2">
          <h2 className="text-[14.5px] font-semibold">Runs</h2>
          {active + busyCommands > 0 && (
            <span className="chip px-2 py-0.5 text-[11px]">
              {active + busyCommands} active
            </span>
          )}
          <div className="ml-auto flex items-center gap-1">
            {(["runs", "shell", "jobs"] as const).map((id) => (
              <button
                key={id}
                type="button"
                onClick={() => {
                  setTab(id);
                  setSelected(null);
                  setEditing(null);
                }}
                className={cn(
                  "rounded-capsule px-2.5 py-1 text-[12px] capitalize",
                  tab === id
                    ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                    : "text-faint hover:text-[var(--ink)]",
                )}
              >
                {id}
              </button>
            ))}
            <button
              type="button"
              aria-label="Close runs"
              onClick={() => setOpen(false)}
              className="hover-surface ml-1 grid h-8 w-8 place-items-center rounded-control text-soft"
            >
              <CloseIcon size={16} />
            </button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
          {tab === "runs" &&
            (selectedTask ? (
              <TaskDetail task={selectedTask} onBack={() => setSelected(null)} />
            ) : tasks.length === 0 ? (
              <EmptyState
                title="No background runs yet"
                hint="Ask a chat to do something in the background, or use a scheduled job."
              />
            ) : (
              <div className="rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)]">
                {tasks.map((task) => (
                  <button
                    key={task.id}
                    type="button"
                    onClick={() => setSelected(task.id)}
                    className="flex w-full items-start gap-2.5 border-b border-[var(--glass-border)] px-3 py-2.5 text-left last:border-b-0 hover:bg-[var(--hover-bg)]"
                  >
                    <StatusDot status={task.status} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13px] text-soft">
                        {task.title || task.prompt.slice(0, 60)}
                      </span>
                      <span className="block truncate text-[11.5px] text-faint">
                        {statusLabel(task)}
                        {task.detail ? ` · ${task.detail}` : ""} · {when(task.createdAt)}
                      </span>
                    </span>
                    {task.jobId && (
                      <span className="chip shrink-0 px-1.5 py-0.5 text-[10.5px]">
                        job
                      </span>
                    )}
                  </button>
                ))}
              </div>
            ))}

          {tab === "shell" &&
            (selectedCommand ? (
              <CommandDetail
                command={selectedCommand}
                onBack={() => setSelected(null)}
              />
            ) : commands.length === 0 ? (
              <EmptyState
                title="No commands yet"
                hint="Every command the agent runs shows up here — including one it left running in the background."
              />
            ) : (
              <div className="rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)]">
                {commands.map((command) => (
                  <button
                    key={command.id}
                    type="button"
                    onClick={() => setSelected(command.id)}
                    className="flex w-full items-start gap-2.5 border-b border-[var(--glass-border)] px-3 py-2.5 text-left last:border-b-0 hover:bg-[var(--hover-bg)]"
                  >
                    <span
                      className={cn(
                        "mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full",
                        commandTone(command.status),
                      )}
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-mono text-[12.5px] text-soft">
                        {command.label || command.command}
                      </span>
                      <span className="block truncate text-[11.5px] text-faint">
                        {commandStatusLabel(command)} · {elapsed(command)}
                        {command.pid ? ` · pid ${command.pid}` : ""}
                      </span>
                    </span>
                    {command.background && (
                      <span className="chip shrink-0 px-1.5 py-0.5 text-[10.5px]">
                        background
                      </span>
                    )}
                  </button>
                ))}
              </div>
            ))}

          {tab === "jobs" &&
            (editing ? (
              <JobEditor
                job={editing}
                onDone={() => {
                  setEditing(null);
                  void load();
                }}
              />
            ) : (
              <>
                <button
                  type="button"
                  onClick={() =>
                    setEditing({
                      id: "",
                      name: "",
                      cron: "0 8 * * 1-5",
                      enabled: true,
                      prompt: "",
                      providerId: null,
                      modelId: null,
                      personaId: null,
                      workdir: null,
                      permissionMode: "auto-read-only",
                      notifyOnSuccess: false,
                      catchUpMinutes: 720,
                      lastRunAt: null,
                      lastStatus: null,
                      nextRunAt: null,
                      createdAt: 0,
                      updatedAt: 0,
                    })
                  }
                  className="btn-ghost mb-2 flex items-center gap-1.5 px-2.5 py-1.5 text-[12px]"
                >
                  <PlusIcon size={13} /> New job
                </button>
                {jobs.length === 0 ? (
                  <EmptyState
                    title="Nothing scheduled"
                    hint="Create a job, or ask a chat: “every weekday at 8, summarise my workspace”."
                  />
                ) : (
                  <div className="rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)]">
                    {jobs.map((job) => (
                      <div
                        key={job.id}
                        className="flex items-start gap-2.5 border-b border-[var(--glass-border)] px-3 py-2.5 last:border-b-0"
                      >
                        <span
                          className={cn(
                            "mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full",
                            job.enabled
                              ? "bg-emerald-400"
                              : "bg-[var(--ink-faint)] opacity-60",
                          )}
                        />
                        <button
                          type="button"
                          onClick={() => setEditing(job)}
                          className="min-w-0 flex-1 text-left"
                        >
                          <span className="block truncate text-[13px] text-soft">
                            {job.name}
                          </span>
                          <span className="block truncate font-mono text-[11.5px] text-faint">
                            {job.cron} · next {when(job.nextRunAt)}
                            {job.lastStatus ? ` · last ${job.lastStatus}` : ""}
                          </span>
                        </button>
                        <button
                          type="button"
                          title="Run now"
                          onClick={() => void ipc.runJobNow(job.id)}
                          className="hover-surface grid h-7 w-7 shrink-0 place-items-center rounded-control text-soft"
                        >
                          <PlayIcon size={13} />
                        </button>
                        <button
                          type="button"
                          title="Delete"
                          onClick={() => {
                            void ipc.deleteJob(job.id).then(() => load());
                          }}
                          className="hover-surface grid h-7 w-7 shrink-0 place-items-center rounded-control text-soft hover:!text-[var(--danger)]"
                        >
                          <TrashIcon size={13} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </>
            ))}
        </div>
      </div>
    </div>
  );
}

function TaskDetail({ task, onBack }: { task: Task; onBack: () => void }) {
  const [messages, setMessages] = useState<Message[]>([]);
  const permission = useChat((state) => state.permissions[task.sessionId]);
  const answerPermission = useChat((state) => state.answerPermission);
  const retryTask = useTasks((state) => state.applyTask);
  const load = useTasks((state) => state.load);

  useEffect(() => {
    let live = true;
    void ipc
      .sessionMessages(task.sessionId)
      .then((list) => {
        if (live) setMessages(list ?? []);
      })
      .catch(() => setMessages([]));
    return () => {
      live = false;
    };
  }, [task.sessionId, task.status]);

  return (
    <div className="space-y-2">
      <button
        type="button"
        onClick={onBack}
        className="text-[12px] text-faint hover:text-[var(--ink)]"
      >
        ← All runs
      </button>

      <Section title={task.title || "Run"}>
        <Row label="Status">
          <span className="text-[12.5px] text-soft">{statusLabel(task)}</span>
        </Row>
        <Row label="Started">
          <span className="text-[12.5px] text-soft">{when(task.startedAt ?? task.createdAt)}</span>
        </Row>
        <Row label="Finished">
          <span className="text-[12.5px] text-soft">{when(task.finishedAt)}</span>
        </Row>
      </Section>

      {permission && (
        <div className="rounded-control border border-[var(--accent)] bg-[var(--card-bg)] px-3 py-3">
          <p className="text-[13px] text-soft">
            This run wants to use <code className="font-mono">{permission.name}</code>.
          </p>
          <pre className="mt-1 max-h-32 overflow-auto rounded-row bg-[var(--hover-bg)] p-2 font-mono text-[11.5px] text-faint">
            {permission.arguments}
          </pre>
          <div className="mt-2 flex gap-2">
            <button
              type="button"
              className="btn-ghost px-2.5 py-1.5 text-[12px]"
              onClick={() => void answerPermission(permission, true)}
            >
              Allow
            </button>
            <button
              type="button"
              className="btn-ghost px-2.5 py-1.5 text-[12px]"
              onClick={() => void answerPermission(permission, false)}
            >
              Deny
            </button>
          </div>
        </div>
      )}

      <Section title="Prompt">
        <p className="px-1 py-2 text-[12.5px] leading-5 whitespace-pre-wrap text-soft">
          {task.prompt}
        </p>
      </Section>

      {(task.result || task.detail) && (
        <Section title={task.status === "failed" ? "Failure" : "Result"}>
          <p className="px-1 py-2 text-[12.5px] leading-5 whitespace-pre-wrap text-soft">
            {task.result || task.detail}
          </p>
        </Section>
      )}

      {messages.length > 0 && (
        <Section title="Transcript">
          <div className="max-h-72 space-y-2 overflow-y-auto px-1 py-2">
            {messages.map((message) => (
              <div key={message.id}>
                <span className="text-[10.5px] tracking-wide text-faint uppercase">
                  {message.role}
                </span>
                <p className="text-[12.5px] leading-5 whitespace-pre-wrap text-soft">
                  {message.content}
                </p>
              </div>
            ))}
          </div>
        </Section>
      )}

      <div className="flex gap-2 px-1">
        {task.status === "running" && (
          <button
            type="button"
            className="btn-ghost px-2.5 py-1.5 text-[12px]"
            onClick={() => void ipc.cancelTask(task.id).then(() => load())}
          >
            Stop
          </button>
        )}
        {["done", "failed", "interrupted", "cancelled"].includes(task.status) && (
          <button
            type="button"
            className="btn-ghost px-2.5 py-1.5 text-[12px]"
            onClick={() => void ipc.retryTask(task.id).then((id) => {
              void load();
              void ipc.listTasks().then((tasks) => {
                const fresh = (tasks ?? []).find((entry) => entry.id === id);
                if (fresh) retryTask(fresh);
              });
            })}
          >
            Run again
          </button>
        )}
        <button
          type="button"
          className="btn-ghost px-2.5 py-1.5 text-[12px]"
          onClick={() => void ipc.deleteTask(task.id).then(() => {
            onBack();
            void load();
          })}
        >
          Delete
        </button>
      </div>
    </div>
  );
}

function CommandDetail({
  command,
  onBack,
}: {
  command: CommandRun;
  onBack: () => void;
}) {
  const [output, setOutput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const load = useTasks((state) => state.load);

  useEffect(() => {
    let live = true;
    void ipc
      .commandOutput(command.id, 400)
      .then((text) => {
        if (live) setOutput(text ?? "");
      })
      .catch((failure) => {
        if (live) {
          setError(failure instanceof Error ? failure.message : String(failure));
        }
      });
    return () => {
      live = false;
    };
  }, [command.id, command.status, command.finishedAt]);

  // A running command's log keeps growing, so tail it while it is going.
  useEffect(() => {
    if (command.status !== "running") return;
    const timer = window.setInterval(() => {
      void ipc
        .commandOutput(command.id, 400)
        .then((text) => setOutput(text ?? ""))
        .catch(() => {
          // A log that cannot be read is already reported by the first load.
        });
    }, 1500);
    return () => window.clearInterval(timer);
  }, [command.id, command.status]);

  return (
    <div className="space-y-2">
      <button
        type="button"
        onClick={onBack}
        className="text-[12px] text-faint hover:text-[var(--ink)]"
      >
        ← All commands
      </button>

      <Section title={command.label || command.command}>
        <Row label="Status">
          <span className="text-[12.5px] text-soft">{commandStatusLabel(command)}</span>
        </Row>
        <Row label="Ran for">
          <span className="text-[12.5px] text-soft">{elapsed(command)}</span>
        </Row>
        {command.pid > 0 && (
          <Row label="Process">
            <span className="font-mono text-[12px] text-soft">pid {command.pid}</span>
          </Row>
        )}
        <Row label="Started">
          <span className="text-[12.5px] text-soft">{when(command.createdAt)}</span>
        </Row>
        <Row label="Folder">
          <span className="font-mono text-[11.5px] break-all text-soft">{command.cwd}</span>
        </Row>
      </Section>

      <Section title="Command">
        <pre className="max-h-32 overflow-auto rounded-row bg-[var(--hover-bg)] p-2 font-mono text-[11.5px] whitespace-pre-wrap text-soft">
          {command.command}
        </pre>
      </Section>

      <Section title="Output">
        {error ? (
          <p className="px-1 py-2 text-[12.5px] text-soft">{error}</p>
        ) : (
          <pre className="max-h-80 overflow-auto rounded-row bg-[var(--hover-bg)] p-2 font-mono text-[11.5px] whitespace-pre-wrap text-soft">
            {output || "(no output yet)"}
          </pre>
        )}
      </Section>

      <div className="flex gap-2 px-1">
        {command.status === "running" && (
          <button
            type="button"
            className="btn-ghost flex items-center gap-1.5 px-2.5 py-1.5 text-[12px]"
            onClick={() => void ipc.stopCommand(command.id).then(() => load())}
          >
            <StopIcon size={13} /> Stop
          </button>
        )}
        <button
          type="button"
          className="btn-ghost px-2.5 py-1.5 text-[12px]"
          onClick={() =>
            void ipc.deleteCommand(command.id).then(() => {
              onBack();
              void load();
            })
          }
        >
          Delete
        </button>
      </div>
    </div>
  );
}

function JobEditor({ job, onDone }: { job: Job; onDone: () => void }) {
  const workspaces = useSettings((state) => state.config.workspaces);
  const [draft, setDraft] = useState<Job>(job);
  const [preview, setPreview] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    const timer = window.setTimeout(() => {
      void ipc
        .previewSchedule(draft.cron, 4)
        .then((runs) => {
          setPreview(runs ?? []);
          setError(null);
        })
        .catch((failure) =>
          setError(failure instanceof Error ? failure.message : String(failure)),
        );
    }, 300);
    return () => window.clearTimeout(timer);
  }, [draft.cron]);

  const patch = (change: Partial<Job>) => setDraft({ ...draft, ...change });

  const save = async () => {
    setBusy(true);
    try {
      await ipc.upsertJob({ ...draft, id: draft.id || crypto.randomUUID() });
      onDone();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };

  const presets = useMemo(
    () => [
      { label: "Every hour", cron: "0 * * * *" },
      { label: "Every weekday at 08:00", cron: "0 8 * * 1-5" },
      { label: "Every day at 09:00", cron: "0 9 * * *" },
      { label: "Mondays at 08:00", cron: "0 8 * * 1" },
      { label: "Every 15 minutes", cron: "*/15 * * * *" },
    ],
    [],
  );

  return (
    <div className="space-y-3">
      <button
        type="button"
        onClick={onDone}
        className="text-[12px] text-faint hover:text-[var(--ink)]"
      >
        ← Jobs
      </button>
      <Section title={draft.id ? "Edit job" : "New job"}>
        <div className="space-y-2 px-1 py-2.5">
          <input
            value={draft.name}
            onChange={(event) => patch({ name: event.currentTarget.value })}
            placeholder="Job name"
            className={inputClass}
          />
          <textarea
            value={draft.prompt}
            onChange={(event) => patch({ prompt: event.currentTarget.value })}
            placeholder="What should this job do each time it runs?"
            rows={4}
            className={cn(inputClass, "resize-y")}
          />
          <div>
            <input
              value={draft.cron}
              onChange={(event) => patch({ cron: event.currentTarget.value })}
              placeholder="minute hour day month weekday"
              className={cn(inputClass, "font-mono")}
            />
            <div className="mt-1.5 flex flex-wrap gap-1">
              {presets.map((preset) => (
                <button
                  key={preset.cron}
                  type="button"
                  onClick={() => patch({ cron: preset.cron })}
                  className="chip px-2 py-0.5 text-[11px]"
                >
                  {preset.label}
                </button>
              ))}
            </div>
          </div>
          {error ? (
            <p className="text-[12px] text-[var(--danger)]">{error}</p>
          ) : (
            <p className="text-[11.5px] text-faint">
              Next: {preview.map((run) => when(run)).join(" · ") || "—"}
            </p>
          )}
          <select
            value={draft.workdir ?? ""}
            onChange={(event) =>
              patch({ workdir: event.currentTarget.value || null })
            }
            className={inputClass}
          >
            <option value="">No workspace</option>
            {workspaces.map((workspace) => (
              <option key={workspace.path} value={workspace.path}>
                {workspace.name || workspace.path}
              </option>
            ))}
          </select>
          <select
            value={draft.permissionMode ?? "auto-read-only"}
            onChange={(event) =>
              patch({ permissionMode: event.currentTarget.value })
            }
            className={inputClass}
          >
            <option value="auto-read-only">Read-only while unattended</option>
            <option value="ask">Ask before every tool</option>
            <option value="auto-all">Run everything</option>
          </select>
        </div>
        <Toggle
          label="Notify on success"
          hint="Failures and approval requests always notify."
          checked={draft.notifyOnSuccess}
          onChange={(value) => patch({ notifyOnSuccess: value })}
        />
        <Toggle
          label="Enabled"
          checked={draft.enabled}
          onChange={(value) => patch({ enabled: value })}
        />
      </Section>
      <div className="flex justify-end gap-2 px-1">
        <button type="button" onClick={onDone} className="btn-ghost px-2.5 py-1.5 text-[12px]">
          Cancel
        </button>
        <button
          type="button"
          disabled={busy || !draft.name.trim() || !draft.prompt.trim()}
          onClick={() => void save()}
          className="btn-ghost px-2.5 py-1.5 text-[12px] disabled:opacity-50"
        >
          Save
        </button>
      </div>
    </div>
  );
}
