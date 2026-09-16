import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { isTauri } from "../lib/tauri";
import type { Job, Task } from "../types";

interface TasksState {
  tasks: Task[];
  jobs: Job[];
  loaded: boolean;
  /** Set while a list refresh is in flight, so the panel can stay quiet. */
  loading: boolean;
  /** Newest facts saved by the memory pass, for the optional chip. */
  memoryNotice: { scope: string; added: number; sessionId: string } | null;
  load: () => Promise<void>;
  applyTask: (task: Task) => void;
  applyJob: (job: Job) => void;
  setMemoryNotice: (notice: TasksState["memoryNotice"]) => void;
}

export const useTasks = create<TasksState>((set, get) => ({
  tasks: [],
  jobs: [],
  loaded: false,
  loading: false,
  memoryNotice: null,

  load: async () => {
    if (!isTauri || get().loading) return;
    set({ loading: true });
    try {
      const [tasks, jobs] = await Promise.all([ipc.listTasks(), ipc.listJobs()]);
      set({ tasks: tasks ?? [], jobs: jobs ?? [], loaded: true });
    } finally {
      set({ loading: false });
    }
  },

  applyTask: (task) =>
    set((state) => {
      const index = state.tasks.findIndex((existing) => existing.id === task.id);
      const tasks =
        index === -1
          ? [task, ...state.tasks]
          : state.tasks.map((existing) => (existing.id === task.id ? task : existing));
      tasks.sort((left, right) => right.createdAt - left.createdAt);
      return { tasks };
    }),

  applyJob: (job) =>
    set((state) => {
      const index = state.jobs.findIndex((existing) => existing.id === job.id);
      return {
        jobs:
          index === -1
            ? [...state.jobs, job]
            : state.jobs.map((existing) => (existing.id === job.id ? job : existing)),
      };
    }),

  setMemoryNotice: (memoryNotice) => set({ memoryNotice }),
}));

/** Number of runs that are queued or running (the titlebar badge). */
export function activeTaskCount(tasks: Task[]): number {
  return tasks.filter((task) => task.status === "queued" || task.status === "running")
    .length;
}
