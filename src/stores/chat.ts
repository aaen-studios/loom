import { create } from "zustand";
import type {
  Attachment,
  EngineEvent,
  Message,
  ModelRef,
  PendingPermission,
  PendingQuestion,
  QuestionAnswer,
  QueuedMessage,
  Session,
  Todo,
  ToolCallRecord,
} from "../types";
import { ipc } from "../lib/ipc";
import { parseAttachments, withCondensed } from "../lib/messageExtra";
import type { ReasoningBlock } from "../lib/messageExtra";
import { useSettings } from "./settings";
import { useSkills } from "./skills";

interface LiveBuffer {
  messageId: string;
  content: string;
  reasoning: string;
  /** Thinking spells with reply-text offsets, for in-place rendering. */
  reasoningBlocks: ReasoningBlock[];
}

interface ChatState {
  sessions: Session[];
  activeId: string | null;
  messages: Message[];
  /** sessionId → streaming text not yet persisted (engine writes on done). */
  live: Record<string, LiveBuffer>;
  /** messageId → tool calls still in flight this turn. */
  liveTools: Record<string, ToolCallRecord[]>;
  busy: Record<string, boolean>;
  /** sessionId → tool call waiting for approval in that chat. Prompts are kept
   *  with their chat so a request in the background never hijacks the one you
   *  are looking at. */
  permissions: Record<string, PendingPermission>;
  /** sessionId → question waiting for an answer in that chat. */
  questions: Record<string, PendingQuestion>;
  /** sessionId → the last failure in that chat, shown there rather than in
   *  whichever chat happens to be open. */
  errors: Record<string, string>;
  /** sessionId → a reply or failure landed while you were in another chat. */
  unread: Record<string, boolean>;
  /** sessionId → this chat's live task list. */
  todos: Record<string, Todo[]>;
  /** sessionId → its standing goal, set with /goal. */
  goals: Record<string, string | null>;
  /** sessionId → its computer turn is paused because the user took over. */
  computerPaused: Record<string, boolean>;
  /** The one chat currently driving the machine, for the chip's Stop. */
  computerDriver: string | null;
  /** Whether the goal/task panel above the composer is expanded. */
  taskPanelOpen: boolean;
  /** Persona that should answer the next message in a multi-persona chat. */
  speakerId: string | null;
  /** Persona ids currently in the active chat's cast, for transcript labels. */
  castIds: string[];
  /** sessionId → messages waiting to send when the current turn finishes. */
  queues: Record<string, QueuedMessage[]>;
  /** sessionId → a queued message the user sent now, waiting for the turn to
   *  stop before it goes out. */
  boosts: Record<string, QueuedMessage | null>;
  /** sessionId → the user pressed Stop; the queue waits for them. */
  halted: Record<string, boolean>;
  loaded: boolean;

  loadSessions: () => Promise<void>;
  openSession: (id: string) => Promise<void>;
  newSession: (personaId?: string | null, workdir?: string | null) => Promise<Session | null>;
  deleteSession: (id: string) => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  /** Adopts a hand-placed sidebar order: index becomes the chat's `position`. */
  applySessionOrder: (orderedIds: string[]) => void;
  send: (
    text: string,
    options?: {
      model?: ModelRef | null;
      attachments?: Attachment[];
      /** Target a specific chat instead of the active one (queue drain). */
      sessionId?: string;
    },
  ) => Promise<void>;
  enqueue: (text: string, attachments: Attachment[]) => Promise<void>;
  removeQueued: (id: string) => void;
  reorderQueue: (fromId: string, toId: string) => void;
  /** Interrupts the running turn and sends this queued item next. */
  sendQueuedNow: (id: string) => Promise<void>;
  advanceQueue: (sessionId: string, allowQueued: boolean) => Promise<void>;
  ensureSession: () => Promise<string | null>;
  stop: () => Promise<void>;
  setModel: (model: ModelRef, variant?: string | null) => Promise<void>;
  setPersona: (personaId: string | null, systemPrompt: string | null) => Promise<void>;
  setSpeaker: (personaId: string | null) => void;
  setCastIds: (personaIds: string[]) => void;
  setWorkdir: (workdir: string | null) => Promise<void>;
  setPermissionMode: (mode: Session["permissionMode"]) => Promise<void>;
  setAgentMode: (mode: Session["agentMode"]) => Promise<void>;
  setComputerAccess: (enabled: boolean) => Promise<void>;
  setBrowserAccess: (enabled: boolean) => Promise<void>;
  /** Polls the engine for who holds the computer, and updates the chip. */
  refreshComputerDriver: () => Promise<void>;
  /** The pill or the panic key stopped a turn: drop its pause/driver markers. */
  noteComputerStopped: (sessionId: string | null) => void;
  setGoal: (goal: string | null) => Promise<void>;
  setTodos: (todos: Todo[]) => Promise<void>;
  setTaskPanelOpen: (open: boolean) => void;
  answerPermission: (
    permission: PendingPermission,
    allow: boolean,
    remember?: "read-only" | "all" | null,
  ) => Promise<void>;
  answerQuestion: (question: PendingQuestion, answer: QuestionAnswer) => Promise<void>;
  retryLast: () => Promise<void>;
  /** Drops this reply and asks again from the user message before it. */
  regenerate: (messageId: string) => Promise<void>;
  /** Drops this user message and everything after it, returning its text so
   *  the composer can take over. */
  editFrom: (messageId: string) => Promise<string | null>;
  removeMessage: (messageId: string) => Promise<void>;
  /** Text the composer should adopt (set by editFrom). */
  draft: string | null;
  setDraft: (draft: string | null) => void;
  truncateFrom: (index: number) => Promise<Message | null>;
  applyEvent: (event: EngineEvent) => void;
  clearError: () => void;
}

/**
 * The computer tools, mirrored from `loom_core::computer::is_computer_tool`.
 * Used only to keep the chip's Stop button pointed at the right chat, so a
 * name missing here degrades to a missing button rather than a wrong action.
 */
const COMPUTER_TOOLS = new Set([
  "screenshot",
  "ui",
  "mouse",
  "keyboard",
  "clipboard",
  "list_windows",
  "window",
  "list_processes",
  "launch_app",
  "kill_process",
  "wait",
]);

function isComputerTool(name: string): boolean {
  return COMPUTER_TOOLS.has(name);
}

/** A copy of `record` without `key`, keeping the object stable when absent. */
function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  if (!(key in record)) return record;
  const next = { ...record };
  delete next[key];
  return next;
}

/** Diagnostics: enable with `localStorage.setItem("loomDebug","1")`. */
function debugLog(message: string): void {
  try {
    if (localStorage.getItem("loomDebug") === "1") {
      console.debug(`[loom] ${message}`);
    }
  } catch {
    // no storage (tests)
  }
}

function placeholderFor(live: LiveBuffer, sessionId: string): Message {
  return {
    id: live.messageId,
    sessionId,
    role: "assistant",
    content: live.content,
    reasoning: live.reasoning || null,
    extra: null,
    personaId: null,
    createdAt: Date.now(),
  };
}

/** Appends a reasoning delta to the current spell, or starts a new one once
 *  the reply text (or the turn order) has moved on. */
function appendReasoning(
  blocks: ReasoningBlock[],
  text: string,
  after: number,
  seq: number,
): ReasoningBlock[] {
  const last = blocks[blocks.length - 1];
  if (last && (last.after ?? 0) === after && (last.seq ?? 0) === seq) {
    return [...blocks.slice(0, -1), { ...last, text: last.text + text }];
  }
  return [...blocks, { text, after, seq }];
}

/**
 * The composer's per-chat controls (mode, approvals, persona, computer use)
 * are usable before the first message exists. Choosing one is intent for a
 * chat, so it starts one — the same way `/goal` and the workspace chip do.
 * A `null` choice only clears an override, which with no chat is already the
 * state, so it never starts one.
 */
async function sessionForChoice(
  get: () => ChatState,
  hasChoice: boolean,
): Promise<string | null> {
  const activeId = get().activeId;
  if (activeId || !hasChoice) return activeId;
  return get().ensureSession();
}



/**
 * Sessions + messages + streaming state. Streams live in Rust; this store
 * mirrors their events and keeps buffers for chats you switch away from.
 */
export const useChat = create<ChatState>((set, get) => ({
  sessions: [],
  activeId: null,
  messages: [],
  live: {},
  liveTools: {},
  busy: {},
  permissions: {},
  questions: {},
  errors: {},
  unread: {},
  todos: {},
  goals: {},
  computerPaused: {},
  computerDriver: null,
  taskPanelOpen: true,
  speakerId: null,
  castIds: [],
  queues: {},
  boosts: {},
  halted: {},
  loaded: false,
  draft: null,

  loadSessions: async () => {
    // Chats that were created but never used are not worth keeping; the open
    // one is protected so a chat being composed is never pulled away.
    await ipc.pruneEmptySessions(get().activeId);
    const sessions = (await ipc.listSessions()) ?? [];
    // Launching never reopens old history: the canvas starts blank and the
    // session is created on the first send (or picked from the chats popup).
    set({ sessions, loaded: true });
  },

  openSession: async (id) => {
    const [storedMessages, todos, cast, goal] = await Promise.all([
      ipc.sessionMessages(id),
      ipc.sessionTodos(id),
      ipc.sessionCast(id),
      ipc.sessionGoal(id),
    ]);
    set({ castIds: (cast ?? []).map((member) => member.id) });
    const stored = storedMessages ?? [];
    const live = get().live[id];
    const withLive = live
      ? stored.some((message) => message.id === live.messageId)
        ? stored.map((message) =>
            message.id === live.messageId
              ? {
                  ...message,
                  content: live.content,
                  reasoning: live.reasoning || null,
                }
              : message,
          )
        : [...stored, placeholderFor(live, id)]
      : stored;

    set((state) => {
      // Opening the chat is what "reads" whatever landed there.
      const next = {
        activeId: id,
        messages: withLive,
        todos: { ...state.todos, [id]: todos ?? [] },
        goals: { ...state.goals, [id]: goal ?? null },
      };
      if (!state.unread[id]) return next;
      const unread = { ...state.unread };
      delete unread[id];
      return { ...next, unread };
    });
  },

  newSession: async (personaId = null, workdir = null) => {
    const session = await ipc.createSession(null, personaId, workdir);
    if (!session) return null;
    set((state) => ({
      sessions: [session, ...state.sessions],
      activeId: session.id,
      messages: [],
    }));
    // Listing prunes the chats that were never used, including the empty one
    // this replaced.
    void get().loadSessions();
    return session;
  },

  deleteSession: async (id) => {
    await ipc.deleteSession(id);
    set((state) => {
      const sessions = state.sessions.filter((session) => session.id !== id);
      const activeId = state.activeId === id ? (sessions[0]?.id ?? null) : state.activeId;
      const permissions = { ...state.permissions };
      delete permissions[id];
      const questions = { ...state.questions };
      delete questions[id];
      const errors = { ...state.errors };
      delete errors[id];
      const unread = { ...state.unread };
      delete unread[id];
      const todos = { ...state.todos };
      delete todos[id];
      const goals = { ...state.goals };
      delete goals[id];
      const queues = { ...state.queues };
      delete queues[id];
      const boosts = { ...state.boosts };
      delete boosts[id];
      const halted = { ...state.halted };
      delete halted[id];
      return {
        sessions,
        activeId,
        permissions,
        questions,
        errors,
        unread,
        todos,
        goals,
        queues,
        boosts,
        halted,
        messages: activeId ? state.messages : [],
      };
    });
    const activeId = get().activeId;
    if (activeId) await get().openSession(activeId);
  },

  renameSession: async (id, title) => {
    await ipc.renameSession(id, title);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === id ? { ...session, title } : session,
      ),
    }));
  },

  applySessionOrder: (orderedIds) => {
    // The array itself is left alone: it stays newest-first, which is what the
    // flat list and loadSessions hand back. Only the `position` column moves,
    // and the sidebar's group order is derived from that.
    const rank = new Map(orderedIds.map((id, index) => [id, index]));
    set((state) => ({
      sessions: state.sessions.map((session) => {
        const position = rank.get(session.id);
        return position === undefined ? session : { ...session, position };
      }),
    }));
  },

  send: async (text, options) => {
    const trimmed = text.trim();
    const attachments = options?.attachments ?? [];
    if (!trimmed && attachments.length === 0) return;

    let targetId = options?.sessionId ?? get().activeId;
    if (!targetId) {
      const session = await get().newSession();
      if (!session) return;
      targetId = session.id;
    }
    if (get().busy[targetId]) return;

    const optimistic: Message = {
      id: `local-${crypto.randomUUID()}`,
      sessionId: targetId,
      role: "user",
      content: trimmed,
      reasoning: null,
      extra: attachments.length ? JSON.stringify(attachments) : null,
      personaId: null,
      createdAt: Date.now(),
    };
    set((state) => {
      const errors = { ...state.errors };
      delete errors[targetId as string];
      return {
        messages:
          state.activeId === targetId
            ? [...state.messages, optimistic]
            : state.messages,
        busy: { ...state.busy, [targetId as string]: true },
        errors,
      };
    });

    try {
      await ipc.sendMessage(
        targetId,
        trimmed,
        attachments,
        options?.model ?? null,
        null,
        null,
        get().speakerId,
      );
      // The chosen speaker is for one turn only; the next reply goes back to
      // the chat's default persona unless the user picks again.
      if (get().speakerId) set({ speakerId: null });
      void get().loadSessions();
    } catch (error) {
      set((state) => ({
        busy: { ...state.busy, [targetId as string]: false },
        errors: {
          ...state.errors,
          [targetId as string]: error instanceof Error ? error.message : String(error),
        },
      }));
    }
  },

  enqueue: async (text, attachments) => {
    const trimmed = text.trim();
    if (!trimmed && attachments.length === 0) return;
    const sessionId = await get().ensureSession();
    if (!sessionId) return;
    const item: QueuedMessage = {
      id: crypto.randomUUID(),
      text: trimmed,
      attachments,
    };
    set((state) => ({
      queues: {
        ...state.queues,
        [sessionId]: [...(state.queues[sessionId] ?? []), item],
      },
    }));
  },

  removeQueued: (id) => {
    const activeId = get().activeId;
    if (!activeId) return;
    set((state) => ({
      queues: {
        ...state.queues,
        [activeId]: (state.queues[activeId] ?? []).filter((item) => item.id !== id),
      },
    }));
  },

  reorderQueue: (fromId, toId) => {
    const activeId = get().activeId;
    if (!activeId || fromId === toId) return;
    set((state) => {
      const queue = state.queues[activeId] ?? [];
      const from = queue.findIndex((item) => item.id === fromId);
      const to = queue.findIndex((item) => item.id === toId);
      if (from < 0 || to < 0) return state;
      const next = [...queue];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return { queues: { ...state.queues, [activeId]: next } };
    });
  },

  sendQueuedNow: async (id) => {
    const activeId = get().activeId;
    if (!activeId) return;
    // One interrupt at a time; the pending one is already on its way out.
    if (get().boosts[activeId]) return;
    const item = (get().queues[activeId] ?? []).find((entry) => entry.id === id);
    if (!item) return;
    set((state) => ({
      queues: {
        ...state.queues,
        [activeId]: (state.queues[activeId] ?? []).filter((entry) => entry.id !== id),
      },
      boosts: { ...state.boosts, [activeId]: item },
      halted: { ...state.halted, [activeId]: false },
    }));
    // Stopping the turn produces a terminal event; the boost goes out then.
    if (get().busy[activeId]) {
      await ipc.cancelStream(activeId);
    } else {
      await get().advanceQueue(activeId, true);
    }
  },

  /** Sends the pending boost, then the next queued message, after a turn. */
  advanceQueue: async (sessionId, allowQueued) => {
    const boost = get().boosts[sessionId];
    if (boost) {
      set((state) => ({ boosts: { ...state.boosts, [sessionId]: null } }));
      await get().send(boost.text, {
        attachments: boost.attachments,
        sessionId,
      });
      return;
    }
    if (!allowQueued) return;
    const queue = get().queues[sessionId] ?? [];
    if (queue.length === 0) return;
    const [next, ...rest] = queue;
    set((state) => ({ queues: { ...state.queues, [sessionId]: rest } }));
    await get().send(next.text, { attachments: next.attachments, sessionId });
  },

  ensureSession: async () => {
    const activeId = get().activeId;
    if (activeId) return activeId;
    const session = await get().newSession();
    return session?.id ?? null;
  },

  stop: async () => {
    const activeId = get().activeId;
    if (!activeId) return;
    // Stopping means stop: the queue waits for the user rather than firing
    // the next message the moment the cancelled turn lands.
    set((state) => ({ halted: { ...state.halted, [activeId]: true } }));
    await ipc.cancelStream(activeId);
  },

  setModel: async (model, variant = null) => {
    const activeId = get().activeId;
    if (activeId) {
      await ipc.setSessionModel(activeId, model.providerId, model.modelId, variant);
      set((state) => ({
        sessions: state.sessions.map((session) =>
          session.id === activeId
            ? {
                ...session,
                providerId: model.providerId,
                modelId: model.modelId,
                variant,
              }
            : session,
        ),
      }));
    }

    // Keep the titles model: choosing a chat model must not clear it.
    const lite = useSettings.getState().config.chat.lite;
    const updated = await ipc.setDefaultModel({
      providerId: model.providerId,
      modelId: model.modelId,
      variant,
      liteProviderId: lite?.providerId ?? null,
      liteModelId: lite?.modelId ?? null,
    });
    if (updated) useSettings.getState().applyRemote(updated);
    void get().loadSessions();
  },

  setSpeaker: (personaId) => {
    set({ speakerId: personaId });
  },

  setCastIds: (personaIds) => {
    set({ castIds: personaIds });
  },

  setPersona: async (personaId, systemPrompt) => {
    const sessionId = await sessionForChoice(get, personaId !== null);
    if (!sessionId) return;
    await ipc.setSessionPersona(sessionId, personaId, systemPrompt);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === sessionId ? { ...session, personaId, systemPrompt } : session,
      ),
    }));
  },

  setWorkdir: async (workdir) => {
    const activeId = get().activeId;
    if (!activeId) return;
    await ipc.setSessionWorkdir(activeId, workdir);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === activeId ? { ...session, workdir } : session,
      ),
    }));
  },

  setPermissionMode: async (mode) => {
    const sessionId = await sessionForChoice(get, mode !== null);
    if (!sessionId) return;
    await ipc.setSessionPermissionMode(sessionId, mode);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === sessionId ? { ...session, permissionMode: mode } : session,
      ),
    }));
  },

  setAgentMode: async (mode) => {
    const sessionId = await sessionForChoice(get, mode !== null);
    if (!sessionId) return;
    await ipc.setSessionAgentMode(sessionId, mode);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === sessionId ? { ...session, agentMode: mode } : session,
      ),
    }));
  },

  setComputerAccess: async (enabled) => {
    const sessionId = await sessionForChoice(get, enabled);
    if (!sessionId) return;
    await ipc.setSessionComputerAccess(sessionId, enabled);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === sessionId ? { ...session, computerAccess: enabled } : session,
      ),
      // Switching the chip off revokes consent: the engine stops the turn, so
      // the pause marker this chat may be carrying is no longer true.
      computerPaused: enabled
        ? state.computerPaused
        : withoutKey(state.computerPaused, sessionId),
    }));
  },

  setBrowserAccess: async (enabled) => {
    const sessionId = await sessionForChoice(get, enabled);
    if (!sessionId) return;
    await ipc.setSessionBrowserAccess(sessionId, enabled);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === sessionId ? { ...session, browserAccess: enabled } : session,
      ),
    }));
  },

  refreshComputerDriver: async () => {
    const status = await ipc.computerStatus();
    if (!status) return;
    set({ computerDriver: status.sessionId ?? null });
  },

  noteComputerStopped: (sessionId) => {
    set((state) => {
      // A stop from the pill or the panic key does not go through this chat's
      // store, so the notice is the only thing that clears the markers. A stop
      // with no session named (nothing was driving) clears them all: no chat is
      // holding the computer any more.
      if (!sessionId) return { computerPaused: {}, computerDriver: null };
      return {
        computerPaused: withoutKey(state.computerPaused, sessionId),
        computerDriver:
          state.computerDriver === sessionId ? null : state.computerDriver,
      };
    });
  },

  setGoal: async (goal) => {
    const activeId = get().activeId;
    if (!activeId) return;
    set((state) => ({ goals: { ...state.goals, [activeId]: goal } }));
    await ipc.setSessionGoal(activeId, goal);
  },

  setTodos: async (todos) => {
    const activeId = get().activeId;
    if (!activeId) return;
    set((state) => ({ todos: { ...state.todos, [activeId]: todos } }));
    const stored = await ipc.setTodos(activeId, todos);
    if (stored) {
      set((state) => ({ todos: { ...state.todos, [activeId]: stored } }));
    }
  },

  setTaskPanelOpen: (open) => set({ taskPanelOpen: open }),

  answerPermission: async (permission, allow, remember = null) => {
    set((state) => {
      const permissions = { ...state.permissions };
      delete permissions[permission.sessionId];
      return { permissions };
    });
    await ipc.respondToolPermission(permission.callId, allow, remember);
  },

  answerQuestion: async (question, answer) => {
    set((state) => {
      const questions = { ...state.questions };
      delete questions[question.sessionId];
      return { questions };
    });
    await ipc.respondQuestion(question.callId, answer);
  },

  /**
   * Drops everything from `index` onwards, in the database and in memory.
   * Returns the user message that started the tail, when there is one.
   */
  truncateFrom: async (index) => {
    const messages = get().messages;
    if (index < 0 || index >= messages.length) return null;

    for (const message of messages.slice(index)) {
      if (!message.id.startsWith("local-")) {
        await ipc.deleteMessage(message.id);
      }
    }
    set((state) => ({ messages: state.messages.slice(0, index) }));
    return messages[index];
  },

  /**
   * Drops the failed turn and re-sends the last user message, so a retry does
   * not leave half a conversation behind.
   */
  retryLast: async () => {
    const { messages, activeId } = get();
    if (!activeId || get().busy[activeId]) return;

    const lastUserIndex = messages
      .map((message) => message.role)
      .lastIndexOf("user");
    if (lastUserIndex < 0) return;

    const user = messages[lastUserIndex];
    const attachments = parseAttachments(user.extra);
    await get().truncateFrom(lastUserIndex);
    get().clearError();
    await get().send(user.content, { attachments });
  },

  /** Ask again for one reply, keeping everything before it. */
  regenerate: async (messageId) => {
    const { messages, activeId } = get();
    if (!activeId || get().busy[activeId]) return;

    const index = messages.findIndex((message) => message.id === messageId);
    if (index < 0) return;

    let userIndex = -1;
    for (let cursor = index - 1; cursor >= 0; cursor -= 1) {
      if (messages[cursor].role === "user") {
        userIndex = cursor;
        break;
      }
    }
    if (userIndex < 0) return;

    const user = messages[userIndex];
    const attachments = parseAttachments(user.extra);
    await get().truncateFrom(userIndex);
    get().clearError();
    await get().send(user.content, { attachments });
  },

  /** Take a message back into the composer, dropping it and everything after. */
  editFrom: async (messageId) => {
    const index = get().messages.findIndex((message) => message.id === messageId);
    if (index < 0) return null;
    const message = get().messages[index];
    if (message.role !== "user") return null;

    await get().truncateFrom(index);
    return message.content;
  },

  removeMessage: async (messageId) => {
    if (!messageId.startsWith("local-")) {
      await ipc.deleteMessage(messageId);
    }
    set((state) => ({
      messages: state.messages.filter((message) => message.id !== messageId),
    }));
  },

  setDraft: (draft) => set({ draft }),


  applyEvent: (event) => {
    // A payload missing its ids would otherwise be filed under "undefined"
    // and silently strand the UI in a busy state. `title` and
    // `harnessChanged` carry no message id.
    if (
      event.type === "title" ||
      event.type === "harnessChanged" ||
      event.type === "todosChanged" ||
      event.type === "computerPaused" ||
      event.type === "computerResumed"
    ) {
      if (!event.sessionId) return;
    } else if (event.type === "notice") {
      // A note with no message behind it still has to clear the turn, so the
      // id is not required — only the chat it belongs to.
      if (!event.sessionId) return;
    } else if (
      event.type === "taskChanged" ||
      event.type === "jobChanged" ||
      event.type === "commandChanged"
    ) {
      // Runs, jobs, and shell commands belong to their own store; events.ts
      // routes them there, and none of them is about this chat's transcript.
      return;
    } else if (event.type === "memoryChanged") {
      if (!event.sessionId) return;
    } else if (!event.sessionId || !event.messageId) {
      debugLog(`dropped malformed ${event.type} event`);
      return;
    }

    debugLog(`apply ${event.type}`);
    const { activeId } = get();
    const isActive = event.sessionId === activeId;

    switch (event.type) {
      case "started": {
        const live: LiveBuffer = {
          messageId: event.messageId,
          content: "",
          reasoning: "",
          reasoningBlocks: [],
        };
        set((state) => ({
          busy: { ...state.busy, [event.sessionId]: true },
          live: { ...state.live, [event.sessionId]: live },
          messages:
            state.activeId === event.sessionId &&
            !state.messages.some((message) => message.id === event.messageId)
              ? [...state.messages, placeholderFor(live, event.sessionId)]
              : state.messages,
        }));
        break;
      }

      case "delta":
      case "reasoning": {
        const live = get().live[event.sessionId];
        const base: LiveBuffer = live ?? {
          messageId: event.messageId,
          content: "",
          reasoning: "",
          reasoningBlocks: [],
        };
        const key = event.type === "delta" ? "content" : "reasoning";
        const nextLive: LiveBuffer =
          event.type === "delta"
            ? { ...base, content: base.content + event.text }
            : {
                ...base,
                reasoning: base.reasoning + event.text,
                reasoningBlocks: appendReasoning(
                  base.reasoningBlocks,
                  event.text,
                  event.after,
                  event.seq,
                ),
              };

        set((state) => ({
          live: { ...state.live, [event.sessionId]: nextLive },
          messages: state.activeId === event.sessionId
            ? state.messages.map((message) =>
                message.id === event.messageId
                  ? { ...message, [key]: nextLive[key] }
                  : message,
              )
            : state.messages,
        }));
        break;
      }

      case "toolCallStarted": {
        set((state) => ({
          // The pill follows computer tools; so does the chip's Stop button,
          // which should only be there for the chat that owns the mouse.
          computerDriver: isComputerTool(event.name)
            ? event.sessionId
            : state.computerDriver,
          liveTools: {
            ...state.liveTools,
            [event.messageId]: [
              ...(state.liveTools[event.messageId] ?? []),
              {
                id: event.callId,
                name: event.name,
                arguments: event.arguments,
                status: "running",
                output: "",
                // The engine flushes all text before starting a call, so the
                // live buffer is exactly the text that came before it.
                after: [...(state.live[event.sessionId]?.content ?? "")].length,
                seq: event.seq,
              },
            ],
          },
        }));
        break;
      }

      case "toolCallFinished": {
        set((state) => ({
          liveTools: {
            ...state.liveTools,
            [event.messageId]: (state.liveTools[event.messageId] ?? []).map((call) =>
              call.id === event.callId
                ? {
                    ...call,
                    status: event.ok ? "ok" : "error",
                    output: event.output,
                    images: event.images ?? [],
                  }
                : call,
            ),
          },
        }));
        break;
      }

      case "toolPermissionRequest": {
        set((state) => ({
          permissions: {
            ...state.permissions,
            [event.sessionId]: {
              sessionId: event.sessionId,
              messageId: event.messageId,
              callId: event.callId,
              name: event.name,
              arguments: event.arguments,
              readOnly: event.readOnly,
              reason: event.reason ?? null,
            },
          },
        }));
        break;
      }

      case "questionRequest": {
        set((state) => ({
          questions: {
            ...state.questions,
            [event.sessionId]: {
              sessionId: event.sessionId,
              messageId: event.messageId,
              callId: event.callId,
              question: event.question,
            },
          },
        }));
        break;
      }

      case "done": {
        debugLog(`done for active=${isActive}, busy before=${Object.keys(get().busy).join(",")}`);
        set((state) => {
          const live = { ...state.live };
          delete live[event.sessionId];
          const busy = { ...state.busy };
          delete busy[event.sessionId];
          const liveTools = { ...state.liveTools };
          delete liveTools[event.messageId];
          // A prompt for this turn can no longer be answered once the turn
          // ends (timeout, stop): clearing it gives the composer back.
          const questions = { ...state.questions };
          delete questions[event.sessionId];
          const permissions = { ...state.permissions };
          delete permissions[event.sessionId];
          // A paused computer turn cannot outlive its turn either: a stale
          // Resume/Stop pair in the chip (or a chip still tinted red) is what
          // this marker would otherwise leave behind for ever. The driver
          // marker goes with it, so another armed chat's chip does not keep
          // offering a Stop for a turn that has finished.
          const computerPaused = withoutKey(state.computerPaused, event.sessionId);
          const computerDriver =
            state.computerDriver === event.sessionId ? null : state.computerDriver;
          // A reply that lands in a chat you are not looking at stays marked
          // until you open it.
          const unread =
            state.activeId === event.sessionId
              ? state.unread
              : { ...state.unread, [event.sessionId]: true };
          return {
            live,
            busy,
            liveTools,
            questions,
            permissions,
            computerPaused,
            computerDriver,
            unread,
            messages:
              state.activeId === event.sessionId
                ? state.messages.map((message) =>
                    message.id === event.messageId
                      ? {
                          ...message,
                          content: event.content,
                          reasoning: event.reasoning ?? null,
                          // A reply answered from a condensed view carries the
                          // fold on the message, so the faint line under it is
                          // there the moment the turn ends rather than a
                          // reload later.
                          extra: withCondensed(message.extra, event.condensed),
                        }
                      : message,
                  )
                : state.messages,
          };
        });
        if (isActive) void get().openSession(event.sessionId);
        void get().loadSessions();
        // The engine records recent models and titles while replying, so pull
        // the config back to keep the picker and settings in step.
        void useSettings.getState().load();
        // The chat is free again: send whatever the user queued while it ran.
        // A manual Stop parks the queue (the halt flag), and the send goes out
        // after the reload so the optimistic bubble is not overwritten by it.
        const releaseQueue = () => {
          const wasHalted = get().halted[event.sessionId] ?? false;
          if (wasHalted) {
            set((state) => ({
              halted: { ...state.halted, [event.sessionId]: false },
            }));
          }
          void get().advanceQueue(event.sessionId, !wasHalted);
        };
        if (isActive) {
          void get().openSession(event.sessionId).then(releaseQueue, releaseQueue);
        } else {
          releaseQueue();
        }
        break;
      }

      case "notice": {
        // The turn ended early: a limit, a provider refusal, a loop. Everything
        // below is what hands the chat back — clearing `busy` frees the
        // composer, and `advanceQueue` drains whatever was queued behind the
        // turn. Without this arm (which is what a rename from `error` left
        // behind) a stopped turn stayed busy for good.
        debugLog(`notice for active=${isActive}: ${event.text}`);
        set((state) => {
          const busy = { ...state.busy };
          delete busy[event.sessionId];
          const live = { ...state.live };
          delete live[event.sessionId];
          const liveTools = { ...state.liveTools };
          if (event.messageId) delete liveTools[event.messageId];
          const questions = { ...state.questions };
          delete questions[event.sessionId];
          const permissions = { ...state.permissions };
          delete permissions[event.sessionId];
          const unread =
            state.activeId === event.sessionId
              ? state.unread
              : { ...state.unread, [event.sessionId]: true };
          return {
            busy,
            live,
            liveTools,
            // A stop ends a paused computer turn too (the 15-minute takeover
            // timeout arrives as a notice), so the marker goes with it.
            computerPaused: withoutKey(state.computerPaused, event.sessionId),
            computerDriver:
              state.computerDriver === event.sessionId ? null : state.computerDriver,
            errors: { ...state.errors, [event.sessionId]: event.text },
            questions,
            permissions,
            unread,
          };
        });
        // Reload so the reason recorded on the message shows (and survives a
        // restart) rather than leaving an empty bubble.
        if (isActive) void get().openSession(event.sessionId);
        // A stop parks the queue (fix and resend), but an explicit "send now"
        // still goes out — the user asked for it.
        void get().advanceQueue(event.sessionId, false);
        break;
      }

      case "title": {
        set((state) => ({
          sessions: state.sessions.map((session) =>
            session.id === event.sessionId
              ? { ...session, title: event.title }
              : session,
          ),
        }));
        break;
      }

      case "harnessChanged": {
        // A harness tool changed the config (or wrote a skill on disk). The
        // engine has already saved; pull both stores back so the UI stops
        // holding a stale copy that the next debounced save could revert.
        void useSettings.getState().load();
        void useSkills.getState().load();
        break;
      }

      case "todosChanged": {
        set((state) => ({
          todos: { ...state.todos, [event.sessionId]: event.todos },
        }));
        break;
      }

      case "computerPaused": {
        set((state) => ({
          computerPaused: { ...state.computerPaused, [event.sessionId]: true },
          // The chat that can pause is by definition the one holding the mouse.
          computerDriver: event.sessionId,
        }));
        break;
      }

      case "computerResumed": {
        set((state) => ({
          computerPaused: withoutKey(state.computerPaused, event.sessionId),
        }));
        break;
      }
    }

    debugLog(`  -> busy=[${Object.keys(get().busy).join(",")}] live=[${Object.keys(get().live).join(",")}] messages=${get().messages.length}`);
  },

  clearError: () => {
    const activeId = get().activeId;
    if (!activeId) return;
    set((state) => {
      const errors = { ...state.errors };
      delete errors[activeId];
      return { errors };
    });
  },
}));

