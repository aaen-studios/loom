import { useMemo } from "react";
import { cn } from "../lib/cn";
import { useChat } from "../stores/chat";
import type { Todo, TodoStatus } from "../types";
import { CheckIcon, ChevronDownIcon, CircleIcon, HalfCircleIcon, TargetIcon } from "./icons";
import { LiquidSurface } from "./LiquidSurface";

const NO_TODOS: Todo[] = [];

const NEXT_STATUS: Record<TodoStatus, TodoStatus> = {
  pending: "in_progress",
  in_progress: "completed",
  completed: "pending",
};

const STATUS_LABEL: Record<TodoStatus, string> = {
  pending: "To do",
  in_progress: "In progress",
  completed: "Done",
};

/** The chat's goal and live task list, shown above the composer. */
export function GoalPanel() {
  const activeId = useChat((state) => state.activeId);
  const goal = useChat((state) => (state.activeId ? state.goals[state.activeId] ?? null : null));
  const todos = useChat((state) => (state.activeId ? state.todos[state.activeId] ?? NO_TODOS : NO_TODOS));
  const open = useChat((state) => state.taskPanelOpen);
  const setOpen = useChat((state) => state.setTaskPanelOpen);
  const setTodos = useChat((state) => state.setTodos);

  const done = useMemo(
    () => todos.filter((todo) => todo.status === "completed").length,
    [todos],
  );

  if (!goal && todos.length === 0) return null;

  const cycle = (todo: Todo) => {
    void setTodos(
      todos.map((entry) =>
        entry.id === todo.id ? { ...entry, status: NEXT_STATUS[entry.status] } : entry,
      ),
    );
  };

  return (
    <LiquidSurface
        surface="cards"
        layout="block"
        tint="var(--panel-bg-strong)" className="mb-2 w-full overflow-hidden rounded-sheet">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="hover-surface flex w-full items-center gap-2 px-3 py-2 text-left"
        aria-expanded={open}
        aria-label={open ? "Collapse goal and tasks" : "Expand goal and tasks"}
      >
        <TargetIcon size={14} className="shrink-0 text-[var(--accent)]" />
        {goal ? (
          <span className="min-w-0 flex-1 truncate text-[12.5px] text-soft" title={goal}>
            <span className="text-faint">Goal · </span>
            {goal}
          </span>
        ) : (
          <span className="min-w-0 flex-1 truncate text-[12.5px] text-faint">
            Task list
          </span>
        )}
        {todos.length > 0 && (
          <span className="shrink-0 rounded-capsule border border-[var(--glass-border)] px-1.5 py-0.5 text-[10.5px] text-faint">
            {done}/{todos.length}
          </span>
        )}
        <ChevronDownIcon
          size={14}
          className={cn("shrink-0 text-faint transition", !open && "-rotate-90")}
        />
      </button>

      {open && todos.length > 0 && (
        <ul className="max-h-44 overflow-y-auto px-1.5 pb-1.5">
          {todos.map((todo) => (
            <li key={todo.id}>
              <button
                type="button"
                onClick={() => cycle(todo)}
                title={`${STATUS_LABEL[todo.status]} — click to change`}
                className="hover-surface flex w-full items-start gap-2.5 rounded-row px-2 py-1.5 text-left"
              >
                <span
                  className={cn(
                    "mt-[2px] grid h-4 w-4 shrink-0 place-items-center",
                    todo.status === "completed" && "text-[var(--accent)]",
                    todo.status === "in_progress" && "text-[var(--accent)]",
                    todo.status === "pending" && "text-faint",
                  )}
                >
                  {todo.status === "completed" ? (
                    <CheckIcon size={13} />
                  ) : todo.status === "in_progress" ? (
                    <HalfCircleIcon size={13} />
                  ) : (
                    <CircleIcon size={13} />
                  )}
                </span>
                <span
                  className={cn(
                    "min-w-0 flex-1 text-[13px] leading-5",
                    todo.status === "completed"
                      ? "text-faint line-through"
                      : todo.status === "in_progress"
                        ? "text-[var(--ink)]"
                        : "text-soft",
                  )}
                >
                  {todo.content}
                </span>
                {todo.status === "in_progress" && activeId && (
                  <span className="mt-[3px] shrink-0 text-[10.5px] tracking-wide text-[var(--accent)] uppercase">
                    now
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
    </LiquidSurface>
  );
}
