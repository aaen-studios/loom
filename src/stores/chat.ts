import { create } from "zustand";
import type { ChatMessage } from "../types";

let counter = 0;
const nextId = () => `m${Date.now().toString(36)}-${(counter++).toString(36)}`;

const M0_REPLY =
  "This is the M0 shell — the window, glass, theme, and config persistence are real. The engine (providers, streaming, sessions) lands in M1.";

interface ChatState {
  messages: ChatMessage[];
  send: (text: string) => void;
  reset: () => void;
}

/**
 * Local-only chat state for M0. Replaced by Rust-owned sessions in M1, where
 * streams survive window switches and app backgrounding.
 */
export const useChat = create<ChatState>((set) => ({
  messages: [],

  send: (text) => {
    const trimmed = text.trim();
    if (!trimmed) return;

    const replyId = nextId();
    set((state) => ({
      messages: [
        ...state.messages,
        { id: nextId(), role: "user", content: trimmed },
        { id: replyId, role: "assistant", content: M0_REPLY, pending: true },
      ],
    }));

    window.setTimeout(() => {
      set((state) => ({
        messages: state.messages.map((message) =>
          message.id === replyId ? { ...message, pending: false } : message,
        ),
      }));
    }, 900);
  },

  reset: () => set({ messages: [] }),
}));
