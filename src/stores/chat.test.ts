import { beforeEach, describe, expect, it, vi } from "vitest";
import { ipc } from "../lib/ipc";
import type { Session } from "../types";
import { useChat } from "./chat";
import { useSettings } from "./settings";
import { useSkills } from "./skills";

const session = (id: string): Session => ({
  id,
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
});

const reset = () => {
  useChat.setState({
    sessions: [],
    activeId: "s1",
    messages: [],
    live: {},
    liveTools: {},
    busy: {},
    permissions: {},
    questions: {},
    errors: {},
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
      after: 5,
      seq: 1,
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

  it("surfaces a stop and hands the chat back", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "s1", messageId: "m1" });
    useChat.getState().applyEvent({
      type: "notice",
      sessionId: "s1",
      messageId: "m1",
      text: "This turn ended early.",
      detail: "http 429",
    });

    const state = useChat.getState();
    expect(state.errors.s1).toBe("This turn ended early.");
    expect(state.busy.s1).toBeUndefined();
    // The message itself is kept: the engine stores the reason on it, so the
    // transcript shows why the turn stopped instead of an empty bubble.
  });

  /// The engine's `Notice` carries no message id when there is no message to
  /// attach it to. Dropping it as "malformed" left the chat busy for ever —
  /// the composer disabled and the queue never draining.
  it("clears the turn for a notice with no message id", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "s1", messageId: "m1" });
    useChat.getState().applyEvent({
      type: "notice",
      sessionId: "s1",
      messageId: null,
      text: "The computer stayed paused for 15 minutes, so Loom stopped.",
      detail: null,
    });

    const state = useChat.getState();
    expect(state.busy.s1).toBeUndefined();
    expect(state.live.s1).toBeUndefined();
    expect(state.errors.s1).toContain("15 minutes");
  });

  it("files a background chat's stop under that chat", () => {
    useChat.getState().applyEvent({ type: "started", sessionId: "other", messageId: "m2" });
    useChat.getState().applyEvent({
      type: "notice",
      sessionId: "other",
      messageId: "m2",
      text: "no connection",
      detail: null,
    });

    expect(useChat.getState().errors.other).toBe("no connection");
    expect(useChat.getState().errors.s1).toBeUndefined();
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
      seq: 2,
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

  it("queues permission prompts per chat and clears them when answered", async () => {
    useChat.getState().applyEvent({
      type: "toolPermissionRequest",
      sessionId: "s1",
      messageId: "m1",
      callId: "c1",
      name: "run_command",
      arguments: '{"command":"dir"}',
      readOnly: false,
    });
    const permission = useChat.getState().permissions.s1;
    expect(permission?.name).toBe("run_command");

    // In the browser (no Tauri) the IPC call resolves to null; the prompt must
    // still clear so the UI is never stuck.
    await useChat.getState().answerPermission(permission!, true);
    expect(useChat.getState().permissions.s1).toBeUndefined();
  });

  it("shows an ask_user question and clears it when answered", async () => {
    useChat.getState().applyEvent({
      type: "questionRequest",
      sessionId: "s1",
      messageId: "m1",
      callId: "c1",
      question: {
        question: "Which database?",
        options: [{ label: "Postgres" }],
        allowMultiple: false,
        allowFreeText: true,
      },
    });
    const question = useChat.getState().questions.s1;
    expect(question?.question.question).toBe("Which database?");

    await useChat.getState().answerQuestion(question!, {
      selected: ["Postgres"],
      text: null,
      cancelled: false,
    });
    expect(useChat.getState().questions.s1).toBeUndefined();
  });

  it("keeps a background chat's question out of the open chat", async () => {
    useChat.getState().applyEvent({
      type: "questionRequest",
      sessionId: "other",
      messageId: "m2",
      callId: "c2",
      question: {
        question: "Which database?",
        options: [],
        allowMultiple: false,
        allowFreeText: true,
      },
    });

    // The prompt is filed under the chat that asked, not the one on screen.
    expect(useChat.getState().questions.s1).toBeUndefined();
    expect(useChat.getState().questions.other?.callId).toBe("c2");

    await useChat.getState().answerQuestion(useChat.getState().questions.other!, {
      selected: [],
      text: "Postgres",
      cancelled: false,
    });
    expect(useChat.getState().questions.other).toBeUndefined();
  });

  it("clears a pending question when its turn ends", () => {
    useChat.getState().applyEvent({
      type: "questionRequest",
      sessionId: "s1",
      messageId: "m1",
      callId: "c1",
      question: {
        question: "Still there?",
        options: [],
        allowMultiple: false,
        allowFreeText: true,
      },
    });
    useChat.getState().applyEvent({
      type: "notice",
      sessionId: "s1",
      messageId: "m1",
      text: "the question timed out",
      detail: null,
    });

    // The card replaces the composer, so it must not outlive the turn.
    expect(useChat.getState().questions.s1).toBeUndefined();
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
          agentMode: null,
          computerAccess: false,
          createdAt: 0,
          updatedAt: 0,
        },
      ],
    });
    useChat.getState().applyEvent({ type: "title", sessionId: "s1", title: "Rust help" });
    expect(useChat.getState().sessions[0].title).toBe("Rust help");
  });

  it("reloads config and skills when a harness change arrives", () => {
    // Config edits (personas, MCP servers, settings) live in the settings
    // store; skills are files on disk in their own store. Both must refresh so
    // the UI does not keep a stale copy a debounced save could revert.
    const settingsLoad = vi
      .spyOn(useSettings.getState(), "load")
      .mockResolvedValue(undefined);
    const skillsLoad = vi
      .spyOn(useSkills.getState(), "load")
      .mockResolvedValue(undefined);

    useChat.getState().applyEvent({
      type: "harnessChanged",
      sessionId: "s1",
      section: "personas",
      summary: 'Created persona "Reviewer"',
    });

    expect(settingsLoad).toHaveBeenCalledTimes(1);
    expect(skillsLoad).toHaveBeenCalledTimes(1);

    settingsLoad.mockRestore();
    skillsLoad.mockRestore();
  });

  it("drops a harness change with no session id", () => {
    const settingsLoad = vi
      .spyOn(useSettings.getState(), "load")
      .mockResolvedValue(undefined);
    useChat.getState().applyEvent({
      type: "harnessChanged",
      sessionId: "",
      section: "settings",
      summary: "Updated settings",
    });
    expect(settingsLoad).not.toHaveBeenCalled();
    settingsLoad.mockRestore();
  });
});

describe("composer choices before the first message", () => {
  beforeEach(() => {
    reset();
    useChat.setState({ activeId: null, sessions: [] });
  });

  it("starts a chat when a mode is chosen on the blank canvas", async () => {
    const created = session("new-1");
    const create = vi.spyOn(ipc, "createSession").mockResolvedValue(created);
    const list = vi.spyOn(ipc, "listSessions").mockResolvedValue([created]);
    const prune = vi
      .spyOn(ipc, "pruneEmptySessions")
      .mockResolvedValue(0);
    const setMode = vi
      .spyOn(ipc, "setSessionAgentMode")
      .mockResolvedValue(undefined);

    await useChat.getState().setAgentMode("plan");

    // The click must not be swallowed: a chat is started, then the mode lands.
    expect(create).toHaveBeenCalledTimes(1);
    expect(setMode).toHaveBeenCalledWith("new-1", "plan");

    create.mockRestore();
    list.mockRestore();
    prune.mockRestore();
    setMode.mockRestore();
  });

  it("does not start a chat just to clear an override", async () => {
    const create = vi.spyOn(ipc, "createSession");

    await useChat.getState().setAgentMode(null);

    expect(create).not.toHaveBeenCalled();
    create.mockRestore();
  });
});
