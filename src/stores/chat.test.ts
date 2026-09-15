import { beforeEach, describe, expect, it } from "vitest";
import { useChat } from "./chat";

const reset = () => {
  useChat.setState({
    sessions: [],
    activeId: "s1",
    messages: [],
    live: {},
    liveTools: {},
    busy: {},
    permission: null,
    error: null,
    loaded: true,
  });
};

describe("streaming state machine", () => {
  beforeEach(reset);

  it("streams text and reasoning into the active message", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "s1", messageId: "m1" });
    expect(useChat.getState().busy.s1).toBe(true);
    expect(useChat.getState().messages).toHaveLength(1);

    useChat.getState().applyEvent({ type: "delta", sessionId: "s1", messageId: "m1", text: "Hel" });
    useChat.getState().applyEvent({ type: "delta", sessionId: "s1", messageId: "m1", text: "lo" });
    useChat.getState().applyEvent({
      type: "reasoning",
      sessionId: "s1",
      messageId: "m1",
      text: "thinking",
    });

    const message = useChat.getState().messages[0];
    expect(message.content).toBe("Hello");
    expect(message.reasoning).toBe("thinking");
  });

  it("keeps buffering text for chats that are not open", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "other", messageId: "m2" });
    useChat.getState().applyEvent({ type: "delta", sessionId: "other", messageId: "m2", text: "off-screen" });

    expect(useChat.getState().messages).toHaveLength(0);
    expect(useChat.getState().live.other.content).toBe("off-screen");
    expect(useChat.getState().busy.other).toBe(true);
  });

  it("finalises content on done and clears buffers", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "s1", messageId: "m1" });
    useChat.getState().applyEvent({ type: "delta", sessionId: "s1", messageId: "m1", text: "part" });
    useChat.getState().applyEvent({
      type: "done",
      sessionId: "s1",
      messageId: "m1",
      content: "complete answer",
      reasoning: null,
      usage: { inputTokens: 3, outputTokens: 4 },
    });

    const state = useChat.getState();
    expect(state.busy.s1).toBeUndefined();
    expect(state.live.s1).toBeUndefined();
    expect(state.messages[0].content).toBe("complete answer");
  });

  it("surfaces errors and stops the spinner", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "s1", messageId: "m1" });
    useChat.getState().applyEvent({
      type: "error",
      sessionId: "s1",
      messageId: "m1",
      error: "rate limited",
    });

    const state = useChat.getState();
    expect(state.error).toBe("rate limited");
    expect(state.busy.s1).toBeUndefined();
    // The empty placeholder is removed so no blank bubble lingers.
    expect(state.messages.filter((message) => message.content === "")).toHaveLength(0);
  });

  it("tracks tool calls from start to finish", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "s1", messageId: "m1" });
    useChat.getState().applyEvent({
      type: "toolCallStarted",
      sessionId: "s1",
      messageId: "m1",
      callId: "c1",
      name: "read_file",
      arguments: '{"path":"a.txt"}',
    });
    expect(useChat.getState().liveTools.m1[0].status).toBe("running");

    useChat.getState().applyEvent({
      type: "toolCallFinished",
      sessionId: "s1",
      messageId: "m1",
      callId: "c1",
      ok: true,
      output: "contents",
    });
    expect(useChat.getState().liveTools.m1[0]).toMatchObject({
      status: "ok",
      output: "contents",
    });

    useChat.getState().applyEvent({
      type: "done",
      sessionId: "s1",
      messageId: "m1",
      content: "done",
      reasoning: null,
      usage: { inputTokens: null, outputTokens: null },
    });
    expect(useChat.getState().liveTools.m1).toBeUndefined();
  });

  it("queues permission prompts and clears them when answered", async () => {
    useChat.getState().applyEvent({
      type: "toolPermissionRequest",
      sessionId: "s1",
      messageId: "m1",
      callId: "c1",
      name: "run_command",
      arguments: '{"command":"dir"}',
      readOnly: false,
    });
    expect(useChat.getState().permission?.name).toBe("run_command");

    // In the browser (no Tauri) the IPC call resolves to null; the prompt must
    // still clear so the UI is never stuck.
    await useChat.getState().answerPermission(true);
    expect(useChat.getState().permission).toBeNull();
  });

  it("renames sessions when a title event arrives", () => {
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
          createdAt: 0,
          updatedAt: 0,
        },
      ],
    });
    useChat.getState().applyEvent({ type: "title", sessionId: "s1", title: "Rust help" });
    expect(useChat.getState().sessions[0].title).toBe("Rust help");
  });
});
