import { beforeEach, describe, expect, it, vi } from "vitest";

// `vi.hoisted` runs before the imports, so the mock factory can close over it.
const calls = vi.hoisted(() => ({
  setSessionModel: vi.fn(async () => null),
  setDefaultModel: vi.fn(async () => null),
  sessionMessages: vi.fn(async (): Promise<unknown[]> => []),
  listSessions: vi.fn(async (): Promise<unknown[]> => []),
  pruneEmptySessions: vi.fn(async () => 0),
  respondToolPermission: vi.fn(async () => null),
  setSessionAgentMode: vi.fn(async () => null),
  deleteMessage: vi.fn(async () => null),
  sendMessage: vi.fn(async () => null),
}));

vi.mock("../lib/ipc", () => ({ ipc: calls }));

import { useChat } from "./chat";
import { useSettings } from "./settings";

describe("choosing a model", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useChat.setState({
      sessions: [],
      activeId: null,
      messages: [],
      live: {},
      liveTools: {},
      busy: {},
      permissions: {},
      questions: {},
      errors: {},
      loaded: true,
    });
    useSettings.setState({
      config: {
        ...useSettings.getState().config,
        chat: { ...useSettings.getState().config.chat, lite: null },
      },
    });
  });

  it("sets the app default when no chat is open yet", async () => {
    await useChat.getState().setModel({ providerId: "a", modelId: "model-a" });

    // Without a session there is nothing session-scoped to write.
    expect(calls.setSessionModel).not.toHaveBeenCalled();
    expect(calls.setDefaultModel).toHaveBeenCalledWith({
      providerId: "a",
      modelId: "model-a",
      variant: null,
      liteProviderId: null,
      liteModelId: null,
    });
  });

  it("also writes the choice onto the active chat", async () => {
    // The reload after a change reads back what the store currently holds
    // (which is what the database would return).
    calls.listSessions.mockImplementation(async () => useChat.getState().sessions);

    useChat.setState({
      sessions: [
        {
          id: "s1",
          title: "",
          providerId: null,
          modelId: null,
          variant: null,
          personaId: null,
          systemPrompt: null,
          workdir: null,
          permissionMode: null,
          agentMode: null,
          computerAccess: false,
          createdAt: 0,
          updatedAt: 0,
        },
      ],
      activeId: "s1",
    });

    await useChat.getState().setModel({ providerId: "b", modelId: "model-b" }, "high");

    expect(calls.setSessionModel).toHaveBeenCalledWith("s1", "b", "model-b", "high");
    expect(useChat.getState().sessions[0].modelId).toBe("model-b");
    expect(useChat.getState().sessions[0].variant).toBe("high");
  });

  it("keeps the titles model when switching chat models", async () => {
    useSettings.setState({
      config: {
        ...useSettings.getState().config,
        chat: {
          ...useSettings.getState().config.chat,
          lite: { providerId: "a", modelId: "cheap-model" },
        },
      },
    });

    await useChat.getState().setModel({ providerId: "b", modelId: "model-b" });

    expect(calls.setDefaultModel).toHaveBeenCalledWith(
      expect.objectContaining({
        liteProviderId: "a",
        liteModelId: "cheap-model",
      }),
    );
  });

  it("writes the agent mode onto the active chat", async () => {
    useChat.setState({
      sessions: [
        {
          id: "s1",
          title: "",
          providerId: null,
          modelId: null,
          variant: null,
          personaId: null,
          systemPrompt: null,
          workdir: null,
          permissionMode: null,
          agentMode: null,
          computerAccess: false,
          createdAt: 0,
          updatedAt: 0,
        },
      ],
      activeId: "s1",
    });

    await useChat.getState().setAgentMode("plan");

    expect(calls.setSessionAgentMode).toHaveBeenCalledWith("s1", "plan");
    expect(useChat.getState().sessions[0].agentMode).toBe("plan");
  });
});
