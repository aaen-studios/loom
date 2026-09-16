/**
 * Microphone capture: the browser side of dictation.
 *
 * ## Why capture lives here rather than in Rust
 *
 * `getUserMedia` is the only portable way to open a microphone from this process,
 * and it comes with the platform's permission flow, device picker and echo
 * cancellation for free. Doing it in Rust would mean a second permission
 * dialogue, a second device list, and hand-written platform code for three
 * operating systems — for audio that then has to be sent *to* the webview
 * anyway to be played.
 *
 * So the webview captures and the Rust side understands. What crosses is raw
 * samples: no encoding, no files, no temporary directories.
 *
 * ## Why a worklet and not a `ScriptProcessorNode`
 *
 * `ScriptProcessorNode` is deprecated and runs on the main thread, so a long
 * frame — a re-render, a big paste — drops audio and it arrives as a gap in a
 * transcript. An `AudioWorklet` runs on the audio thread and cannot be starved
 * that way.
 *
 * The worklet itself is a real file under `public/worklets/`, not a blob: the
 * app's CSP is `script-src 'self'` with no `blob:`, so `addModule` refuses a
 * blob URL. That is a deliberate restriction, and it is why this is not the
 * one-liner it usually is.
 *
 * ## Echo cancellation matters more than it looks
 *
 * `echoCancellation` is requested explicitly. Without it the microphone hears
 * Loom's own voice, so the detector fires on the reply being spoken and the app
 * interrupts itself — which presents as "voice mode randomly stops talking".
 */
import { isTauri } from "./tauri";
import { voiceIpc } from "./voice";

/** Why a microphone session could not start, phrased for a person. */
export class MicrophoneError extends Error {
  constructor(
    message: string,
    /** Whether the user's OS refused permission, which is worth saying plainly. */
    readonly denied: boolean,
  ) {
    super(message);
    this.name = "MicrophoneError";
  }
}

/** Turns a `getUserMedia` failure into something a person can act on. */
function describe(error: unknown): MicrophoneError {
  const name =
    error && typeof error === "object" && "name" in error
      ? String((error as { name: unknown }).name)
      : "";

  switch (name) {
    case "NotAllowedError":
    case "SecurityError":
      return new MicrophoneError(
        "Loom does not have permission to use the microphone. Allow it in your " +
          "operating system's privacy settings, then try again.",
        true,
      );
    case "NotFoundError":
    case "OverconstrainedError":
      return new MicrophoneError(
        "No microphone was found. Connect one and try again.",
        false,
      );
    case "NotReadableError":
      return new MicrophoneError(
        "The microphone is in use by another application.",
        false,
      );
    default:
      return new MicrophoneError(
        error instanceof Error ? error.message : String(error),
        false,
      );
  }
}

/**
 * An open microphone feeding Loom's recogniser.
 *
 * Owns the audio graph and the permission, so [`stop`](Microphone.stop) is a
 * real release: the track is stopped, not merely paused, which is what turns the
 * OS's "recording" indicator off. Leaving it paused would leave Loom holding the
 * microphone while the UI said it was not listening.
 */
export class Microphone {
  private stream: MediaStream | null = null;
  private context: AudioContext | null = null;
  private node: AudioWorkletNode | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private stopping = false;

  /** How many blocks have been posted, for a diagnostic readout. */
  private posted = 0;

  /** Called with the running block count as audio is sent. */
  onBlock?: (blocks: number) => void;

  /**
   * Starts capturing.
   *
   * `onError` is called for a failure that happens *after* capture has begun —
   * a device unplugged mid-session — because those cannot be thrown at the
   * caller that started it.
   */
  async start(onError?: (error: MicrophoneError) => void): Promise<void> {
    if (!isTauri) {
      throw new MicrophoneError("voice input needs the desktop app", false);
    }
    if (this.stream) return;
    this.posted = 0;

    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          // Asked for explicitly. Without it the recogniser hears Loom's own
          // speech and interrupts the reply that is being played.
          echoCancellation: true,
          // Noise suppression helps a recogniser and costs nothing here: this
          // audio is never heard by a person.
          noiseSuppression: true,
          autoGainControl: true,
          channelCount: 1,
        },
      });
    } catch (error) {
      throw describe(error);
    }

    try {
      // 16 kHz, because that is what the models want. `getUserMedia` treats this
      // as a hint and usually honours it, but the rate is *reported* to Rust
      // rather than assumed — the worklet posts it once — so a browser that
      // ignores the request still produces correct audio rather than
      // three-times-speed audio.
      const context = new AudioContext({ sampleRate: 16_000 });

      // A context created before a user gesture starts suspended, and a
      // suspended context delivers silence — the graph is built, the worklet
      // runs, and no audio ever arrives. Starting from a click normally avoids
      // it, but a session restarted by anything else does not.
      if (context.state === "suspended") await context.resume();

      await context.audioWorklet.addModule("/worklets/loom-mic.js");

      const source = context.createMediaStreamSource(stream);
      const node = new AudioWorkletNode(context, "loom-mic", {
        numberOfInputs: 1,
        numberOfOutputs: 0,
        channelCount: 1,
        channelCountMode: "explicit",
      });

      // The Rust side needs the models before it can score anything, so the
      // session is started first and the first block may queue behind a load.
      await voiceIpc.listenStart();

      node.port.onmessage = (event: MessageEvent) => {
        // `.data`, not `.payload` — this is a `MessagePort`, not a Tauri event.
        // Reading the wrong property yields `undefined` for every message, so
        // the session starts, reports no errors, and never sends any audio.
        const data = event.data;
        if (!data || typeof data !== "object") return;

        if (data.type === "sampleRate") {
          // Reported for diagnostics only. The rate that describes the samples
          // is the context's, which the worklet is running under, and that is
          // what is passed with each block.
          return;
        }

        if (data.type === "block" && Array.isArray(data.samples)) {
          this.posted += 1;
          // Reported so the surface can tell "listening and silent" from
          // "listening and nothing is arriving", which look identical
          // otherwise.
          this.onBlock?.(this.posted);
          // Fire and forget: awaiting here would stall the message handler and
          // back up the port, and a dropped block is better than a stalled
          // audio thread.
          void voiceIpc
            .listenAudio(data.samples as number[], context.sampleRate, 1)
            .catch((error) => {
              // A failure here means the Rust session ended — most likely
              // because the user stopped it. Reporting an error for every
              // remaining block would be noise, so the session ends instead.
              if (!this.stopping) {
                onError?.(
                  new MicrophoneError(
                    error instanceof Error ? error.message : String(error),
                    false,
                  ),
                );
                void this.stop();
              }
            });
        }
      };

      // A device change — headphones unplugged, a USB microphone removed —
      // ends the stream rather than silently delivering nothing.
      stream.getAudioTracks().forEach((track) => {
        track.onended = () => {
          if (this.stopping) return;
          onError?.(
            new MicrophoneError("The microphone disconnected.", false),
          );
          void this.stop();
        };
      });

      source.connect(node);

      this.stream = stream;
      this.context = context;
      this.node = node;
      this.source = source;
      this.stopping = false;
    } catch (error) {
      // Whatever went wrong, the permission must not be left open.
      stream.getTracks().forEach((track) => track.stop());
      throw error instanceof MicrophoneError ? error : describe(error);
    }
  }

  /** Whether the microphone is open. */
  get active(): boolean {
    return this.stream !== null;
  }

  /** How many blocks have been sent, for a diagnostic readout. */
  get blocks(): number {
    return this.posted;
  }

  /**
   * Closes the microphone and ends the Rust session.
   *
   * The order matters: the Rust side is told to stop *first*, so it transcribes
   * whatever is still open rather than having the audio cut off under it.
   */
  async stop(): Promise<void> {
    if (!this.stream) return;
    this.stopping = true;

    // Before the graph is torn down, so a sentence in progress is not lost.
    try {
      await voiceIpc.listenStop();
    } catch {
      // Already stopped from the Rust side, which is the outcome wanted.
    }

    this.node?.port.close();
    this.node?.disconnect();
    this.source?.disconnect();
    this.stream.getTracks().forEach((track) => track.stop());
    await this.context?.close();

    this.stream = null;
    this.context = null;
    this.node = null;
    this.source = null;
    this.stopping = false;
    this.posted = 0;
  }
}

/** Whether this machine can capture at all. */
export function canCapture(): boolean {
  return (
    isTauri &&
    typeof navigator !== "undefined" &&
    typeof navigator.mediaDevices?.getUserMedia === "function"
  );
}
