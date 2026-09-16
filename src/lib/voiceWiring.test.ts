/**
 * Wiring tests: does anything actually connect the voice machinery to the app?
 *
 * These read source files rather than running code, which is a weaker kind of
 * test and is used deliberately. What it catches is a whole class of bug that
 * unit tests cannot see, because every unit is correct:
 *
 *   `useVoice.attach()` was written, documented, and never called by anything.
 *
 * Nothing was broken. `SpeakButton` invoked the command, Rust synthesised
 * sentences and emitted them, and no listener existed — so read-aloud was
 * silent, and a dictation transcript arrived as an event nobody was subscribed
 * to, which made the microphone look like it did nothing at all. Every test
 * passed, because the only thing that was wrong was an absence.
 *
 * The same shape of gap produced a second bug: the composer's microphone
 * reached Rust but the *surface* had no entry point, so the feature existed and
 * was unreachable.
 *
 * So these assertions are about presence, not behaviour. They are cheap, they
 * fail loudly, and they cover the one thing the rest of the suite structurally
 * cannot.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const read = (relative: string) => readFileSync(`${ROOT}${relative}`, "utf8");

describe("the voice store is subscribed", () => {
  it("is attached by an app-level hook", () => {
    // The regression. `attach` is the only subscriber to `loom://voice` and
    // `loom://voice-listen`; if nothing calls it, both directions are silent.
    const events = read("src/lib/events.ts");
    expect(events, "events.ts no longer exports useVoiceEvents").toMatch(
      /export function useVoiceEvents/,
    );
    expect(events, "useVoiceEvents does not call attach").toMatch(
      /return attach\(\)/,
    );
  });

  it("is called from the shell, not from the surface", () => {
    // Being called from the surface would be the same bug in a different
    // place: transcripts arrive while the surface is closed, because the
    // composer's microphone button is where dictation starts.
    const app = read("src/App.tsx");
    expect(app, "App.tsx does not subscribe the voice store").toContain(
      "useVoiceEvents()",
    );
  });

  it("subscribes to both channels", () => {
    const store = read("src/stores/voice.ts");
    expect(store, "nothing listens for playback events").toContain(
      '"loom://voice"',
    );
    expect(store, "nothing listens for transcripts").toContain(
      '"loom://voice-listen"',
    );
  });
});

describe("voice mode is reachable", () => {
  it("is rendered by the shell", () => {
    expect(read("src/App.tsx")).toMatch(/<VoiceMode\s*\/>/);
  });

  it("has a button in the title bar", () => {
    // A feature with no way in is a feature nobody has. This was the second
    // absence-shaped bug: the surface existed and nothing opened it.
    const titleBar = read("src/components/TitleBar.tsx");
    expect(titleBar, "no control opens voice mode").toContain("setVoiceOpen");
  });

  it("has a keyboard shortcut", () => {
    const shortcuts = read("src/lib/shortcuts.ts");
    expect(shortcuts, "voice mode has no shortcut").toMatch(
      /shiftKey && key === "v"/,
    );
    // And the sheet lists it, or the shortcut is undiscoverable.
    expect(read("src/components/ShortcutsSheet.tsx")).toContain("Ctrl+Shift+V");
  });

  it("closes on Escape, from its own listener", () => {
    const surface = read("src/components/VoiceMode.tsx");
    expect(surface).toMatch(/event\.key === "Escape"/);
    expect(surface).toMatch(/setOpen\(false\)/);
  });
});

describe("the microphone chain reaches Rust", () => {
  it("keeps the CSP-safe worklet as a real file", () => {
    // `script-src 'self'` has no `blob:`, so an `addModule` from a blob URL is
    // refused. The file has to exist and be served from the app's own origin.
    expect(read("public/worklets/loom-mic.js")).toContain(
      'registerProcessor("loom-mic"',
    );
    expect(read("src/lib/microphone.ts")).toContain(
      'addModule("/worklets/loom-mic.js")',
    );
  });

  it("asks for echo cancellation", () => {
    // Without it the microphone hears Loom's own voice, the detector fires on
    // the reply being played, and the app interrupts itself — which presents
    // as "voice mode randomly stops talking".
    expect(read("src/lib/microphone.ts")).toMatch(/echoCancellation:\s*true/);
  });

  it("reads the worklet's message, not a Tauri event's payload", () => {
    // `MessagePort` messages arrive on `.data`. Reading `.payload` yields
    // `undefined` for every one of them: the session starts, reports no
    // errors, and never sends any audio.
    const microphone = read("src/lib/microphone.ts");
    expect(microphone).toMatch(/event\.data/);
    expect(microphone).not.toMatch(/event\.payload/);
  });

  it("resumes a suspended audio context", () => {
    // A context created before a user gesture starts suspended, and a
    // suspended context delivers silence.
    expect(read("src/lib/microphone.ts")).toMatch(/context\.resume\(\)/);
  });

  it("registers every command the frontend calls", () => {
    // `invoke` rejects for an unregistered command, and the failure surfaces
    // as a generic error rather than as a missing handler.
    const rust = read("src-tauri/src/lib.rs");
    const frontend = read("src/lib/voice.ts");
    for (const command of [
      "voice_listen_start",
      "voice_listen_audio",
      "voice_listen_stop",
      "voice_listen_status",
    ]) {
      expect(frontend, `${command} is never called`).toContain(command);
      expect(rust, `${command} is not registered`).toContain(`voice::${command}`);
    }
  });
});

describe("the playback queue cannot wedge", () => {
  it("has no unreachable resolution path", () => {
    // `resolveImmediately()` was a named no-op whose comment claimed it
    // resolved the promise. One refused `play()` — autoplay blocked before any
    // gesture — therefore stopped every later sentence from ever being heard,
    // because `advance` awaited a promise nothing could settle any more.
    //
    // This asserts on the *definition*, not on the name. Searching the whole
    // file for the identifier also matches the comment explaining the bug,
    // which is how the first version of this test failed against correct code.
    const voice = read("src/lib/voice.ts");
    expect(
      voice,
      "the no-op resolver came back — a refused play() would wedge the queue",
    ).not.toMatch(/function\s+resolveImmediately/);

    // The refusal path must settle the promise. Both halves are checked: the
    // resolver captured from the executor, and a call to it in the `catch`.
    expect(voice, "the resolver is no longer captured").toMatch(
      /let release!: \(\) => void/,
    );
    const catchBlock = /catch\s*\{[^}]*\}/s.exec(
      voice.slice(voice.indexOf("await audio.play()")),
    );
    expect(catchBlock, "the play() refusal is no longer handled").not.toBeNull();
    expect(
      catchBlock![0],
      "a refused play() no longer settles the promise, so the queue wedges",
    ).toContain("release()");
  });
});
