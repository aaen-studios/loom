/**
 * Voice mode's UI state: what is installed, what is speaking, and how loudly.
 *
 * The audio queue lives here rather than in a component because playback
 * outlives any one view — a reply keeps being spoken while the user scrolls,
 * opens settings, or switches chats.
 */
import { create } from "zustand";
import { subscribeTauri } from "../lib/listen";
import {
  SpeechQueue,
  voiceIpc,
  type ListenEvent,
  type VoiceInfo,
  type VoiceInstallEvent,
  type VoiceSettings,
  type VoiceStatus,
} from "../lib/voice";
import { Microphone, MicrophoneError, canCapture } from "../lib/microphone";
import { pushSample, type DictationPhase } from "../lib/voiceActivity";

/** What the service is doing, for the UI to render. */
export type VoicePhase =
  | "idle"
  | "loading"
  | "speaking"
  | "installing"
  | "error";

/** Install progress for one component. */
export interface InstallStep {
  component: string;
  label: string;
  detail: string;
  done: boolean;
  percent: number;
}

/**
 * Whether the microphone is open, and whether it is working.
 *
 * Defined in `lib/voiceActivity` and re-exported here rather than declared
 * twice, so the surface's status line and the store's state cannot drift into
 * two unions that are *nearly* the same shape — which is exactly the kind of
 * difference a `switch` stops narrowing on without anybody noticing.
 */
export type { DictationPhase };

interface VoiceState {
  /** Whether the app is speaking, and roughly how far through. */
  phase: VoicePhase;
  /** The message being spoken, so the UI can mark it. */
  speakingId: string | null;
  /** Utterance id of the current request, for matching events. */
  utterance: number | null;
  /** Current output level, 0–1, for a meter. */
  level: number;
  /** Sentences spoken so far, and how many were expected. */
  spoken: number;
  expected: number;
  /** The last error, cleared when the next utterance starts. */
  error: string | null;

  /** Settings, and whether the assets are installed. */
  status: VoiceStatus | null;
  voices: VoiceInfo[];
  install: InstallStep[];
  installing: boolean;

  // -- dictation ----------------------------------------------------------

  /** Whether the microphone is open. */
  dictation: DictationPhase;
  /** Why it is not, phrased for a person. */
  dictationError: string | null;
  /** Input level, 0–1, for the meter. */
  inputLevel: number;
  /** Whether the detector currently hears speech. */
  hearing: boolean;
  /**
   * The last finished transcript.
   *
   * Paired with `transcriptSeq` rather than being consumed once: a component
   * appends it when the counter changes, so a re-render cannot append the same
   * sentence twice and a fast second utterance cannot be dropped.
   */
  transcript: string | null;
  transcriptSeq: number;
  /**
   * Everything recognised this session, oldest first.
   *
   * Kept so the voice surface can show what it has heard. Session-scoped on
   * purpose: this is a record of a conversation being had out loud, not chat
   * history, and persisting it would mean two places claiming to be the
   * transcript.
   */
  heard: string[];
  /**
   * Whether the most recent utterance hit the length cap.
   *
   * Surfaced because a truncated transcript is missing its last word, and a
   * user reading it deserves to know that rather than concluding the
   * recogniser misheard.
   */
  lastTruncated: boolean;
  clearHeard: () => void;
  /**
   * How many audio blocks Rust has accepted this session.
   *
   * The only way to tell "listening and nobody is talking" from "listening and
   * no audio is arriving" — otherwise both are silence.
   */
  blocks: number;

  // -- read-along ---------------------------------------------------------

  /** The sentences of the reply currently being spoken, in order. */
  lines: string[];
  /** Which of them is playing. -1 when nothing is. */
  lineIndex: number;
  /** The voice the current utterance is using. */
  lineVoice: string | null;

  /** The meter's recent levels, oldest first. */
  inputHistory: number[];

  startListening: () => Promise<void>;
  stopListening: () => Promise<void>;
  clearTranscript: () => void;

  load: () => Promise<void>;
  loadVoices: () => Promise<void>;
  installAll: () => Promise<void>;
  /**
   * Saves the Voice settings.
   *
   * Takes `VoiceSettings` from the IPC layer rather than a second local shape:
   * an inline copy is what let `autoSend` be added to one and not the other,
   * and the failure was a type error in the panel rather than anything that
   * pointed at the duplication.
   */
  save: (settings: VoiceSettings) => Promise<void>;

  /** Speaks `text`, tagging it with the message it came from. */
  speak: (text: string, messageId?: string, voice?: string) => Promise<void>;
  preview: (voice: string) => Promise<void>;
  stop: () => void;

  /** Subscribes to the engine's voice events. Called once, at app start. */
  attach: () => () => void;
}

/**
 * One queue for the whole app.
 *
 * Module scope rather than store state: it owns live `Audio` elements, and
 * putting them in a store would make every level tick look like a state change
 * worth re-rendering.
 */
const queue = new SpeechQueue();

/**
 * One microphone for the whole app.
 *
 * Module scope for the same reason as the queue, and one more: `getUserMedia`
 * holds an OS-level permission. Two instances would mean two prompts and two
 * device handles for one physical microphone.
 */
const microphone = new Microphone();

export const useVoice = create<VoiceState>((set, get) => ({
  phase: "idle",
  speakingId: null,
  utterance: null,
  level: 0,
  spoken: 0,
  expected: 0,
  error: null,

  status: null,
  voices: [],
  install: [],
  installing: false,

  dictation: "off",
  dictationError: null,
  inputLevel: 0,
  hearing: false,
  transcript: null,
  transcriptSeq: 0,
  heard: [],
  lastTruncated: false,
  blocks: 0,

  lines: [],
  lineIndex: -1,
  lineVoice: null,
  inputHistory: [],

  load: async () => {
    const status = await voiceIpc.status();
    if (status) set({ status });
  },

  loadVoices: async () => {
    const voices = await voiceIpc.listVoices();
    if (voices) set({ voices });
  },

  installAll: async () => {
    if (get().installing) return;
    set({ installing: true, phase: "installing", install: [] });
    try {
      await voiceIpc.install();
    } catch (error) {
      set({
        installing: false,
        phase: "error",
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  save: async (settings) => {
    const config = await voiceIpc.saveSettings(settings);
    if (config) {
      // The command returns the whole config, so the panel's own copy stays in
      // step without a second round trip.
      await get().load();
    }
  },

  speak: async (text, messageId, voice) => {
    const { status } = get();
    if (status && !status.enabled) return;
    if (!text.trim()) return;

    // A second request supersedes the first, which is what clicking speak on
    // another message should do.
    queue.stop();

    set({
      phase: "loading",
      speakingId: messageId ?? null,
      spoken: 0,
      expected: 0,
      error: null,
      level: 0,
      // Cleared here rather than on `started`, so the previous reply's
      // sentences are not shown as if they were this one's while it loads.
      lines: [],
      lineIndex: -1,
      lineVoice: null,
    });

    try {
      const utterance = await voiceIpc.speak(text, voice, undefined);
      set({ utterance: utterance ?? null });
    } catch (error) {
      set({
        phase: "error",
        speakingId: null,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  preview: async (voice) => {
    queue.stop();
    set({ phase: "loading", spoken: 0, expected: 0, error: null });
    try {
      const utterance = await voiceIpc.preview(voice);
      set({ utterance: utterance ?? null });
    } catch (error) {
      set({
        phase: "error",
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  stop: () => {
    queue.stop();
    void voiceIpc.cancel();
    // The read-along highlight goes with the voice. Leaving it on the sentence
    // that was interrupted would point at words nobody is saying any more.
    set({
      phase: "idle",
      speakingId: null,
      utterance: null,
      level: 0,
      lineIndex: -1,
    });
  },

  startListening: async () => {
    if (get().dictation === "listening" || get().dictation === "starting") return;
    if (!canCapture()) {
      set({
        dictation: "error",
        dictationError: "Voice input needs the desktop app and a microphone.",
      });
      return;
    }

    // Speaking and listening at once is what echo cancellation is for, but
    // starting a session mid-reply is still confusing: the first thing the
    // detector hears is Loom. Stopping first makes the transition explicit.
    if (get().phase === "speaking" || get().phase === "loading") get().stop();

    set({ dictation: "starting", dictationError: null, blocks: 0, inputHistory: [] });

    try {
      // Assigned before `start`, because `start` begins posting blocks as soon
      // as the graph is built and the first ones would otherwise be uncounted.
      microphone.onBlock = (blocks) => {
        // Only the count is stored, not the samples — the audio itself goes
        // straight to Rust, and holding a copy here would double the memory for
        // no benefit.
        if (blocks !== get().blocks) set({ blocks });
      };

      await microphone.start((error) => {
        // A failure after capture began — the device was unplugged — cannot be
        // thrown at the caller that started it.
        set({
          dictation: error.denied ? "error" : "off",
          dictationError: error.message,
          hearing: false,
          inputLevel: 0,
          inputHistory: [],
        });
      });
      set({ dictation: "listening" });
    } catch (error) {
      const microphoneError =
        error instanceof MicrophoneError ? error : null;
      set({
        dictation: "error",
        dictationError:
          microphoneError?.message ??
          (error instanceof Error ? error.message : String(error)),
        hearing: false,
      });
    }
  },

  stopListening: async () => {
    microphone.onBlock = undefined;
    await microphone.stop();
    set({ dictation: "off", hearing: false, inputLevel: 0, inputHistory: [] });
  },

  clearTranscript: () => set({ transcript: null }),

  clearHeard: () => set({ heard: [] }),

  attach: () => {
    queue.onDrained = () => {
      // Only clear if nothing newer has started, or a fast follow-up would be
      // marked idle while it is still speaking.
      if (get().phase === "speaking") {
        set({ phase: "idle", speakingId: null, level: 0 });
      }
    };

    const unsubscribeLevel = queue.onLevel((level) => set({ level }));

    // The read-along's whole mechanism: the queue reports a sentence when it
    // starts *playing* it, not when it arrives, so the highlight tracks the
    // voice rather than running ahead of it by a sentence.
    queue.onSentence = (index) => set({ lineIndex: index });

    const disposers: Array<() => void> = [unsubscribeLevel];

    disposers.push(subscribeTauri<import("../lib/voice").VoiceEvent>("loom://voice", (payload) => {
      // A superseded utterance's events must not drive the UI: a late "done"
      // from a cancelled request would clear the new one's speaking mark.
      const current = get().utterance;
      if (current !== null && payload.utterance !== current) return;

      switch (payload.type) {
        case "loading":
          set({ phase: "loading" });
          break;
        case "started":
          set({
            phase: "speaking",
            expected: payload.lines.length,
            spoken: 0,
            lines: payload.lines,
            lineVoice: payload.voice,
            lineIndex: -1,
          });
          break;
        case "chunk":
          set((state) => ({ spoken: state.spoken + 1 }));
          // The engine's own index, carried through the queue, so the
          // highlight matches the sentence being heard rather than the order
          // this side happened to receive them in.
          queue.enqueue(payload.audio, payload.seconds, payload.index);
          break;
        case "done":
          // Synthesis finished; playback may lag. The queue's drain callback
          // clears the phase, so nothing is marked idle while still audible.
          if (payload.chunks === 0) {
            set({ phase: "idle", speakingId: null, lines: [], lineIndex: -1 });
          }
          break;
        case "cancelled":
        case "error":
          queue.stop();
          set({
            phase: payload.type === "error" ? "error" : "idle",
            speakingId: null,
            utterance: null,
            level: 0,
            lineIndex: -1,
            lines: payload.type === "error" ? get().lines : [],
            error: payload.type === "error" ? payload.message : null,
          });
          break;
      }
    }));

    disposers.push(subscribeTauri<VoiceInstallEvent>("loom://voice-install", (payload) => {
      if (payload.component === "all") {
        set({ installing: !payload.done, phase: payload.done ? "idle" : "installing" });
        if (payload.done) {
          // Re-read rather than assuming: a component can finish installed
          // while another failed, and the screen should show which.
          void get().load();
          void get().loadVoices();
        }
        return;
      }

      set((state) => {
        const next = state.install.filter(
          (step) => step.component !== payload.component,
        );
        next.push({
          component: payload.component,
          label: payload.label,
          detail: payload.detail,
          done: payload.done,
          percent: payload.percent,
        });
        return { install: next };
      });
    }));

    disposers.push(subscribeTauri<ListenEvent>("loom://voice-listen", (payload) => {
      switch (payload.type) {
        case "loading":
          // Only reached on the very first session, while Whisper loads.
          set({ dictation: "starting" });
          break;

        case "level":
          // History as well as the current value: a single number is a bar that
          // jumps, and a strip of them is a meter that shows the shape of
          // speech, which is what makes a paused sentence look like a pause
          // rather than like a failure.
          set((state) => ({
            inputLevel: payload.level,
            inputHistory: pushSample(state.inputHistory, payload.level),
          }));
          break;

        case "speech":
          // The whole of barge-in. `speaking` is true on the first window above
          // the speech threshold, not when a transcript arrives — waiting for a
          // transcript would mean talking over the user for as long as it takes
          // them to finish a sentence.
          if (payload.speaking) {
            const { phase } = get();
            if (phase === "speaking" || phase === "loading") get().stop();
          }
          set({ hearing: payload.speaking });
          break;

        case "transcript":
          set((state) => ({
            transcript: payload.text,
            transcriptSeq: state.transcriptSeq + 1,
            // Capped: an hour of dictation is thousands of utterances, and the
            // surface only ever shows the recent ones.
            heard: [...state.heard, payload.text].slice(-30),
            lastTruncated: payload.truncated,
          }));
          break;

        case "stopped":
          set({ dictation: "off", hearing: false, inputLevel: 0, inputHistory: [] });
          break;

        case "error":
          set({
            dictation: "error",
            dictationError: payload.message,
            hearing: false,
          });
          break;
      }
    }));

    return () => disposers.forEach((dispose) => dispose());
  },
}));

/** Which message is currently being spoken, if any. */
export function useSpeakingId(): string | null {
  return useVoice((state) => state.speakingId);
}
