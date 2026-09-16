import { create } from "zustand";
import { ipc, type Skill } from "../lib/ipc";

interface SkillsState {
  skills: Skill[];
  loaded: boolean;
  load: () => Promise<void>;
}

/**
 * Skills live as markdown files on disk, not in config, so they need their own
 * store: the composer's slash menu and the Settings editor both read it, and a
 * `harnessChanged` event (the model wrote a skill) reloads it.
 */
export const useSkills = create<SkillsState>((set) => ({
  skills: [],
  loaded: false,

  load: async () => {
    const skills = (await ipc.listSkills()) ?? [];
    set({ skills, loaded: true });
  },
}));
