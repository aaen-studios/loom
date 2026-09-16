/**
 * Voice mode: speaking replies aloud, and previewing voices.
 *
 * ## How audio gets here
 *
 * The Rust side synthesises one sentence at a time and emits each as
 * `loom://voice` with a base64 WAV. Nothing is written to disk, so playback
 * needs no temp files and no asset-protocol scope over a directory the user
 * never chose.
 *
 * ## Why a queue rather than one `Audio` per sentence
 *
 * Sentences are synthesised faster than they are spoken, so they arrive ahead
 * of playback. Handing each to a fresh `Audio` element would overlap them; the
 * queue plays them strictly in order and keeps one sentence of lookahead, which
 * is what makes the reply sound continuous rather than stuttered.
 *
 * Cancellation is the other half: stopping means clearing the queue *and*
 * releasing the element that is currently playing, which is why `stop()`
 * exists rather than a `pause()`.
 */
import { call, isTauri } from "./tauri";

/** One voice the model can speak with. */
export interface VoiceInfo {
  id: string;
  /** The espeak-ng language name, or null when unmapped. */
  language: string | null;
  accent: string | null;
  gender: string | null;
  isDefault: boolean;
}

/** One installable component and whether it is present. */
export interface VoiceComponentStatus {
  id: string;
  label: string;
  licence: string;
  installed: boolean;
  bytes: number;
}

/** What the Voice settings screen shows. */
export interface VoiceStatus {
  enabled: boolean;
  ready: boolean;
  blocking: string | null;
  components: VoiceComponentStatus[];
  defaultVoice: string;
  speed: number;
  autoplay: boolean;
  /** Whether a finished transcript sends itself. */
  autoSend: boolean;
  home: string;
}

/** The config fields the Voice settings screen owns. */
export interface VoiceSettings {
  enabled?: boolean;
  autoplay?: boolean;
  defaultVoice?: string;
  speed?: number;
  speakCode?: boolean;
  /** Send a finished transcript as a message rather than leaving it to edit. */
  autoSend?: boolean;
}

/** Events the voice service emits. */
export type VoiceEvent =
  | { type: "loading"; utterance: number }
  | {
      type: "started";
      utterance: number;
      voice: string;
      /** The sentences that will be spoken, in order. */
      lines: string[];
    }
  | {
      type: "chunk";
      utterance: number;
      index: number;
      /** Base64 WAV. */
      audio: string;
      seconds: number;
    }
  | {
      type: "done";
      utterance: number;
      chunks: number;
      seconds: number;
      realTimeFactor: number;
    }
  | { type: "cancelled"; utterance: number }
  | { type: "error"; utterance: number; message: string };

/** Install progress, emitted on its own channel. */
export interface VoiceInstallEvent {
  component: string;
  label: string;
  step: string;
  detail: string;
  done: boolean;
  percent: number;
}

/** Events the recognition service emits on `loom://voice-listen`. */
export type ListenEvent =
  | { type: "loading" }
  | { type: "level"; level: number }
  | { type: "speech"; speaking: boolean }
  | {
      type: "transcript";
      text: string;
      seconds: number;
      realTimeFactor: number;
      truncated: boolean;
    }
  | { type: "stopped" }
  | { type: "error"; message: string };

export const voiceIpc = {
  status: () => call<VoiceStatus>("voice_status"),
  listVoices: () => call<VoiceInfo[]>("voice_list_voices"),
  speak: (text: string, voice?: string, speed?: number) =>
    call<number>("voice_speak", { text, voice: voice ?? null, speed: speed ?? null }),
  preview: (voice: string) => call<number>("voice_preview", { voice }),
  cancel: () => call<boolean>("voice_cancel"),
  install: () => call<void>("voice_install"),
  saveSettings: (settings: VoiceSettings) =>
    call<unknown>("save_voice_settings", { settings }),
  setPersonaVoice: (id: string, voice: string | null) =>
    call<unknown>("set_persona_voice", { id, voice }),

  /**
   * Starts a dictation session: loads the models and begins accepting audio.
   *
   * The microphone itself is opened in the webview, so this only prepares the
   * Rust side. Blocking on it matters once — the first call loads Whisper —
   * and the caller should show that it is working rather than dropping audio
   * into a channel nobody is reading yet.
   */
  listenStart: () => call<void>("voice_listen_start"),
  /**
   * Feeds one block of microphone audio.
   *
   * Resolves as soon as the block is queued. Recognition happens on its own
   * thread and arrives as a `loom://voice-listen` event, so awaiting this never
   * waits for a transcript.
   */
  listenAudio: (samples: number[], rate: number, channels: number) =>
    call<void>("voice_listen_audio", { samples, rate, channels }),
  /** Ends the session, transcribing anything still open. */
  listenStop: () => call<void>("voice_listen_stop"),
  /** Whether a session is running, so a reload can recover the state. */
  listenStatus: () => call<boolean>("voice_listen_status"),
};

/** Turns a base64 WAV into something an `Audio` element can play. */
function toBlobUrl(base64: string): string {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
}

/** Average volume of a playing element, for a level meter. */
export type LevelListener = (level: number) => void;

/**
 * Plays synthesised sentences in order.
 *
 * Deliberately not a Web Audio graph: an `AudioBufferSourceNode` would need the
 * PCM decoded here rather than in Rust, and the WAV header is already written
 * by the time audio reaches this side. An `Audio` element per sentence, played
 * in sequence, is less code for the same result.
 *
 * The level meter is the one thing that does need Web Audio, so it is attached
 * lazily and only while something is playing.
 */
export class SpeechQueue {
  private queue: Array<{ url: string; seconds: number; index: number }> = [];
  private current: HTMLAudioElement | null = null;
  private playing = false;
  private levelListeners = new Set<LevelListener>();
  private context: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private source: MediaElementAudioSourceNode | null = null;
  private meterFrame: number | null = null;

  /** Called when the queue drains on its own, not when stopped. */
  onDrained?: () => void;

  /**
   * Called when a sentence *starts playing*, with the index the engine gave it.
   *
   * Deliberately on playback rather than on arrival. Sentences are synthesised
   * ahead of being heard, so highlighting on arrival would run the highlight
   * ahead of the voice by up to a whole sentence — the read-along would point
   * at words that have not been said yet.
   */
  onSentence?: (index: number) => void;

  /** Subscribes to level updates. Returns an unsubscribe function. */
  onLevel(listener: LevelListener): () => void {
    this.levelListeners.add(listener);
    return () => this.levelListeners.delete(listener);
  }

  /** How many sentences are waiting. */
  get depth(): number {
    return this.queue.length;
  }

  /** Whether anything is being spoken. */
  get isPlaying(): boolean {
    return this.playing;
  }

  /** Adds a sentence to the queue, starting playback if idle. */
  enqueue(base64: string, seconds: number, index = 0): void {
    this.queue.push({ url: toBlobUrl(base64), seconds, index });
    if (!this.playing) void this.advance();
  }

  /** Stops and discards everything queued. */
  stop(): void {
    for (const item of this.queue) URL.revokeObjectURL(item.url);
    this.queue = [];
    this.stopMeter();

    if (this.current) {
      // Clearing `src` matters: without it the element may keep decoding the
      // buffer it already has, which is exactly the audio the user asked to
      // stop.
      this.current.pause();
      this.current.removeAttribute("src");
      this.current.load();
      this.current = null;
    }
    this.playing = false;
  }

  /** Plays the next queued sentence. */
  private async advance(): Promise<void> {
    const next = this.queue.shift();
    if (!next) {
      this.playing = false;
      this.stopMeter();
      this.onDrained?.();
      return;
    }

    this.playing = true;
    // Before the element is created, so a slow `load` cannot delay the
    // highlight past the point where the audio starts.
    this.onSentence?.(next.index);

    const audio = new Audio(next.url);
    this.current = audio;

    // Revoke once decoded, so a long reply does not hold every sentence's
    // bytes in memory until the app closes.
    audio.addEventListener(
      "loadeddata",
      () => URL.revokeObjectURL(next.url),
      { once: true },
    );

    try {
      this.attachMeter(audio);
    } catch {
      // A missing AudioContext is not worth failing playback over; the meter
      // is decoration.
    }

    // `release` is assigned synchronously by the executor, so it is set by the
    // time the `catch` below runs. The definite-assignment assertion is what
    // says so to the compiler.
    let release!: () => void;
    const done = new Promise<void>((resolve) => {
      release = resolve;
      audio.addEventListener("ended", () => resolve(), { once: true });
      audio.addEventListener("error", () => resolve(), { once: true });
    });

    try {
      await audio.play();
    } catch {
      // Autoplay can be refused when playback did not begin from a user
      // gesture. Settling here keeps the queue moving rather than waiting
      // forever on an element that will never start.
      //
      // This used to call a function named `resolveImmediately` whose body was
      // empty — a comment claiming a resolution that did not happen. One
      // refusal therefore stopped every later sentence from ever being heard,
      // because `advance` awaited a promise nothing could settle any more.
      release();
    }

    await done;
    this.current = null;
    void this.advance();
  }

  /** Wires an analyser to the element so the UI can show a level. */
  private attachMeter(audio: HTMLAudioElement): void {
    const Ctor =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext })
        .webkitAudioContext;
    if (!Ctor) return;

    if (!this.context) this.context = new Ctor();
    if (this.context.state === "suspended") void this.context.resume();

    // One source per element, and an element cannot be re-routed, so these are
    // rebuilt for each sentence.
    this.source?.disconnect();
    this.analyser?.disconnect();

    this.analyser = this.context.createAnalyser();
    this.analyser.fftSize = 256;
    this.source = this.context.createMediaElementSource(audio);
    this.source.connect(this.analyser);
    this.analyser.connect(this.context.destination);

    this.startMeter();
  }

  private startMeter(): void {
    if (!this.analyser) return;
    if (this.meterFrame !== null) return;

    const buffer = new Uint8Array(this.analyser.frequencyBinCount);

    const tick = () => {
      if (!this.analyser) return;
      this.analyser.getByteFrequencyData(buffer);
      let sum = 0;
      for (const value of buffer) sum += value;
      const level = buffer.length ? sum / buffer.length / 255 : 0;
      for (const listener of this.levelListeners) listener(level);
      this.meterFrame = requestAnimationFrame(tick);
    };

    this.meterFrame = requestAnimationFrame(tick);
  }

  private stopMeter(): void {
    if (this.meterFrame !== null) {
      cancelAnimationFrame(this.meterFrame);
      this.meterFrame = null;
    }
    this.source?.disconnect();
    this.analyser?.disconnect();
    this.source = null;
    this.analyser = null;
    for (const listener of this.levelListeners) listener(0);
  }
}

/** True when the app is running inside Tauri and voice mode can work. */
export function voiceAvailable(): boolean {
  return isTauri && typeof Audio !== "undefined";
}
