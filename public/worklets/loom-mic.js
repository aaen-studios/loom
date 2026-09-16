// Loom's microphone worklet.
//
// Runs on the audio thread, so it does as little as possible: downmix to mono,
// accumulate into fixed blocks, and post a copy. Everything else — converting
// to 16-bit, base64, IPC — happens on the main thread, where taking a
// millisecond does not cause a dropout.
//
// ## Why this is a real file and not a blob
//
// `addModule` refuses a `blob:` URL under the app's CSP, which is
// `script-src 'self'`. A worklet from a blob is the usual trick and it is
// blocked here, so this is served from `public/` alongside the rest of the app.
//
// ## Why the block is 100 ms
//
// Three competing costs. Smaller blocks mean lower latency between the user
// starting to speak and barge-in firing, but more IPC calls and more per-call
// overhead. Larger blocks mean fewer calls but a longer wait. 100 ms is 1600
// samples at 16 kHz: small enough that barge-in feels immediate, large enough
// that ten IPC calls a second is nothing.

/** Samples per posted block. 100 ms at the target rate. */
const BLOCK = 1600;

class LoomMicProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.block = new Float32Array(BLOCK);
    this.filled = 0;
    // Sent once, so the Rust side knows what it is being given. The context is
    // asked for 16 kHz and usually gets it, but the rate is reported rather
    // than assumed: if the browser ignores the request, resampling happens on
    // the Rust side instead of silently misreading the audio.
    this.port.postMessage({ type: "sampleRate", sampleRate });
  }

  process(inputs) {
    const input = inputs[0];
    // No input yet, or silence: nothing to do, and returning true keeps the
    // node alive.
    if (!input || input.length === 0 || !input[0]) return true;

    const channels = input.length;
    const frames = input[0].length;

    for (let i = 0; i < frames; i += 1) {
      let sum = 0;
      for (let c = 0; c < channels; c += 1) {
        const channel = input[c];
        if (channel) sum += channel[i];
      }
      this.block[this.filled] = sum / channels;
      this.filled += 1;

      if (this.filled === BLOCK) {
        // A copy, not a transfer: this buffer is reused immediately.
        this.port.postMessage({ type: "block", samples: this.block.slice() });
        this.filled = 0;
      }
    }

    return true;
  }
}

registerProcessor("loom-mic", LoomMicProcessor);
