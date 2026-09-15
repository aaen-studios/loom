import { create } from "zustand";
import type {
  Attachment,
  EngineEvent,
  Message,
  ModelRef,
  PendingPermission,
  Session,
  ToolCallRecord,
} from "../types";
import { ipc } from "../lib/ipc";
import { parseAttachments } from "../lib/messageExtra";
import { useSettings } from "./settings";

interface LiveBuffer {
  messageId: string;
  content: string;
  reasoning: string;
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
  permission: PendingPermission | null;
  error: string | null;
  loaded: boolean;

  loadSessions: () => Promise<void>;
  openSession: (id: string) => Promise<void>;
  newSession: (personaId?: string | null) => Promise<Session | null>;
  deleteSession: (id: string) => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  send: (
    text: string,
    options?: { model?: ModelRef | null; attachments?: Attachment[] },
  ) => Promise<void>;
  ensureSession: () => Promise<string | null>;
  stop: () => Promise<void>;
  setModel: (model: ModelRef, variant?: string | null) => Promise<void>;
  setPersona: (personaId: string | null, systemPrompt: string | null) => Promise<void>;
  setWorkdir: (workdir: string | null) => Promise<void>;
  setPermissionMode: (mode: Session["permissionMode"]) => Promise<void>;
  answerPermission: (
    allow: boolean,
    remember?: "read-only" | "all" | null,
  ) => Promise<void>;
  retryLast: () => Promise<void>;
  applyEvent: (event: EngineEvent) => void;
  clearError: () => void;
}

function placeholderFor(live: LiveBuffer, sessionId: string): Message {  return {
    id: live.messageId,
    sessionId,
    role: "assistant",
    content: live.content,
    reasoning: live.reasoning || null,
    extra: null,
    createdAt: Date.now(),
  };
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
  permission: null,
  error: null,
  loaded: false,

  loadSessions: async () => {
    const sessions = (await ipc.listSessions()) ?? [];
    set({ sessions, loaded: true });
    if (!get().activeId && sessions.length > 0) {
      await get().openSession(sessions[0].id);
    }
  },

  openSession: async (id) => {
    const stored = (await ipc.sessionMessages(id)) ?? [];
    const live = get().live[id];
    const messages = live
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

    set({ activeId: id, messages, error: null });
  },

  newSession: async (personaId = null) => {
    const session = await ipc.createSession(null, personaId);
    if (!session) return null;
    set((state) => ({
      sessions: [session, ...state.sessions],
      activeId: session.id,
      messages: [],
      error: null,
    }));
    return session;
  },

  deleteSession: async (id) => {
    await ipc.deleteSession(id);
    set((state) => {
      const sessions = state.sessions.filter((session) => session.id !== id);
      const activeId = state.activeId === id ? (sessions[0]?.id ?? null) : state.activeId;
      return { sessions, activeId, messages: activeId ? state.messages : [] };
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

  send: async (text, options) => {
    const trimmed = text.trim();
    const attachments = options?.attachments ?? [];
    if (!trimmed && attachments.length === 0) return;

    let activeId = get().activeId;
    if (!activeId) {
      const session = await get().newSession();
      if (!session) return;
      activeId = session.id;
    }
    if (get().busy[activeId]) return;

    const optimistic: Message = {
      id: `local-${crypto.randomUUID()}`,
      sessionId: activeId,
      role: "user",
      content: trimmed,
      reasoning: null,
      extra: attachments.length ? JSON.stringify(attachments) : null,
      createdAt: Date.now(),
    };
    set((state) => ({
      messages: [...state.messages, optimistic],
      busy: { ...state.busy, [activeId as string]: true },
      error: null,
    }));

    try {
      await ipc.sendMessage(activeId, trimmed, attachments, options?.model ?? null);
      void get().loadSessions();
    } catch (error) {
      set((state) => ({
        busy: { ...state.busy, [activeId as string]: false },
        error: error instanceof Error ? error.message : String(error),
      }));
    }
  },

  ensureSession: async () => {
    const activeId = get().activeId;
    if (activeId) return activeId;
    const session = await get().newSession();
    return session?.id ?? null;
  },

  stop: async () => {
    const activeId = get().activeId;
    if (activeId) await ipc.cancelStream(activeId);
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

  setPersona: async (personaId, systemPrompt) => {
    const activeId = get().activeId;
    if (!activeId) return;
    await ipc.setSessionPersona(activeId, personaId, systemPrompt);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === activeId
          ? { ...session, personaId, systemPrompt }
          : session,
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
    const activeId = get().activeId;
    if (!activeId) return;
    await ipc.setSessionPermissionMode(activeId, mode);
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === activeId ? { ...session, permissionMode: mode } : session,
      ),
    }));
  },

  answerPermission: async (allow, remember = null) => {
    const permission = get().permission;
    if (!permission) return;
    set({ permission: null });
    await ipc.respondToolPermission(permission.callId, allow, remember);
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
    for (const message of messages.slice(lastUserIndex)) {
      if (!message.id.startsWith("local-")) {
        await ipc.deleteMessage(message.id);
      }
    }

    const attachments = parseAttachments(user.extra);
    set((state) => ({
      messages: state.messages.slice(0, lastUserIndex),
      error: null,
    }));
    await get().send(user.content, { attachments });
  },

  applyEvent: (event) => {
    const { activeId } = get();
    const isActive = event.sessionId === activeId;

    switch (event.type) {
      case "started": {
        const live: LiveBuffer = { messageId: event.messageId, content: "", reasoning: "" };
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
        const key = event.type === "delta" ? "content" : "reasoning";
        const nextLive: LiveBuffer = live
          ? { ...live, [key]: live[key] + event.text }
          : { messageId: event.messageId, content: "", reasoning: "", [key]: event.text };

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
                  }
                : call,
            ),
          },
        }));
        break;
      }

      case "toolPermissionRequest": {
        set({
          permission: {
            sessionId: event.sessionId,
            messageId: event.messageId,
            callId: event.callId,
            name: event.name,
            arguments: event.arguments,
            readOnly: event.readOnly,
          },
        });
        break;
      }

      case "done": {
        set((state) => {
          const live = { ...state.live };
          delete live[event.sessionId];
          const busy = { ...state.busy };
          delete busy[event.sessionId];
          const liveTools = { ...state.liveTools };
          delete liveTools[event.messageId];
          return {
            live,
            busy,
            liveTools,
            messages:
              state.activeId === event.sessionId
                ? state.messages.map((message) =>
                    message.id === event.messageId
                      ? {
                          ...message,
                          content: event.content,
                          reasoning: event.reasoning ?? null,
                        }
                      : message,
                  )
                : state.messages,
          };
        });
        if (isActive) void get().openSession(event.sessionId);
        void get().loadSessions();
        break;
      }

      case "error": {
        set((state) => {
          const busy = { ...state.busy };
          delete busy[event.sessionId];
          const live = { ...state.live };
          delete live[event.sessionId];
          return { busy, live, error: event.error };
        });
        // Reload so the reason recorded on the message shows (and survives a
        // restart) rather than leaving an empty bubble.
        if (isActive) void get().openSession(event.sessionId);
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
    }
  },

  clearError: () => set({ error: null }),
}));

