/**
 * Voice mode: the speaking-and-listening surface.
 *
 * ## Why this is a surface and not a button
 *
 * The composer's microphone button dictates *into the composer*: it is for
 * writing a message by talking. This is the other thing — a conversation held
 * out loud, where a reply is heard rather than read and interrupting means
 * simply talking. The two want different layouts, so they are different screens.
 *
 * ## What the layout has to answer
 *
 * Five questions, in the order a person asks them:
 *
 * 1. *Is it hearing me?* — the dial, the meter, and the status line. A meter
 *    that does not move means the wrong input device, and that is worth knowing
 *    before finishing a sentence rather than after.
 * 2. *What did it hear?* — left pane, appended as utterances end.
 * 3. *What is it saying?* — right pane, with the sentence being played
 *    highlighted so a fast reply is followable.
 * 4. *How do I stop it?* — talk. The detector fires on the first window above
 *    the speech threshold and the reply stops there, without a click.
 * 5. *What if it is wrong?* — with auto-send off, a transcript waits in the
 *    composer to be corrected. The footer says so, because a button that
 *    silently does nothing is worse than no button.
 *
 * ## Why nothing here is a second implementation
 *
 * The level scaling, the status wording and the read-along progress are in
 * `lib/voiceActivity`, and the sentence list comes from Rust rather than being
 * re-split here. A UI that re-derives the engine's decisions is a UI that
 * eventually contradicts them.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { canCapture } from "../lib/microphone";
import {
  LEVEL_HISTORY,
  barHeight,
  describeListening,
  readAlongProgress,
  silenceHint,
  voiceLabel,
} from "../lib/voiceActivity";
import { useUi } from "../stores/ui";
import { useVoice } from "../stores/voice";
import {
  CloseIcon,
  MicIcon,
  MicOffIcon,
  SoundIcon,
  StopIcon,
  TrashIcon,
} from "./icons";
import { LiquidSurface } from "./LiquidSurface";
import { EmptyState, IconButton, fieldBase } from "./ui";

export function VoiceMode() {
  const open = useUi((state) => state.voiceOpen);
  const setOpen = useUi((state) => state.setVoiceOpen);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const setSettingsCategory = useUi((state) => state.setSettingsCategory);

  const phase = useVoice((state) => state.phase);
  const dictation = useVoice((state) => state.dictation);
  const dictationError = useVoice((state) => state.dictationError);
  const hearing = useVoice((state) => state.hearing);
  const inputLevel = useVoice((state) => state.inputLevel);
  const inputHistory = useVoice((state) => state.inputHistory);
  const blocks = useVoice((state) => state.blocks);
  const heard = useVoice((state) => state.heard);
  const lastTruncated = useVoice((state) => state.lastTruncated);
  const lines = useVoice((state) => state.lines);
  const lineIndex = useVoice((state) => state.lineIndex);
  const lineVoice = useVoice((state) => state.lineVoice);
  const spoken = useVoice((state) => state.spoken);
  const expected = useVoice((state) => state.expected);
  // Read at the top, not next to the element that draws it. Putting the hook
  // inside the `speaking && …` expression below would call it conditionally,
  // which React forbids — and the failure is an unpredictable hook order
  // rather than anything that points at the meter.
  const outputLevel = useVoice((state) => state.level);
  const error = useVoice((state) => state.error);
  const status = useVoice((state) => state.status);
  const voices = useVoice((state) => state.voices);
  const startListening = useVoice((state) => state.startListening);
  const stopListening = useVoice((state) => state.stopListening);
  const stop = useVoice((state) => state.stop);
  const clearHeard = useVoice((state) => state.clearHeard);
  const save = useVoice((state) => state.save);

  const [secondsListening, setSecondsListening] = useState(0);
  const heardEnd = useRef<HTMLDivElement | null>(null);

  // Escape closes the surface, and it is handled here rather than in the global
  // shortcut hook so the surface owns its own dismissal — including the case
  // where it is open over something else that also wants Escape.
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  // The clock behind `silenceHint`. Counting from when listening began is what
  // makes "no audio is arriving" distinguishable from "you have not spoken
  // yet" — without it, both are silence and the surface cannot tell them apart.
  useEffect(() => {
    if (!open || dictation !== "listening") return;
    setSecondsListening(0);
    const timer = window.setInterval(
      () => setSecondsListening((value) => value + 1),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [open, dictation]);

  // Keep the newest utterance in view as it is added.
  useEffect(() => {
    heardEnd.current?.scrollIntoView({ block: "end" });
  }, [heard.length]);

  // Left-padded to a fixed width so the meter does not reflow as it fills:
  // bars growing in from the right looks like a scrolling chart rather than a
  // level.
  const bars = useMemo(() => {
    const missing = Math.max(0, LEVEL_HISTORY - inputHistory.length);
    return [
      ...(new Array(missing).fill(0) as number[]),
      ...inputHistory,
    ].slice(-LEVEL_HISTORY);
  }, [inputHistory]);

  if (!open) return null;

  const listening = dictation === "listening";
  const speaking = phase === "speaking" || phase === "loading";
  const hint = silenceHint(blocks, secondsListening);
  const progress = readAlongProgress(lineIndex, lines.length);

  const toggle = () => {
    if (listening) void stopListening();
    else if (dictation !== "starting") void startListening();
  };

  return (
    <div className="absolute inset-0 z-40 flex items-center justify-center p-4">
      <button
        type="button"
        aria-label="Close voice mode"
        onClick={() => setOpen(false)}
        className="absolute inset-0 cursor-default bg-black/25"
      />

      <LiquidSurface
        surface="overlays"
        layout="block"
        className="animate-summon relative flex h-full w-full max-w-3xl flex-col rounded-window"
        tint="var(--panel-bg-strong)"
      >
        <header className="flex items-center gap-2 px-4 pt-3 pb-2">
          <SoundIcon size={16} className="text-[var(--accent)]" />
          <h2 className="text-[14.5px] font-semibold">Voice mode</h2>
          {lineVoice && (
            <span className="chip px-2 py-0.5 text-[11px]">{lineVoice}</span>
          )}
          <div className="ml-auto flex items-center gap-1">
            <IconButton
              label="Clear what was heard"
              onClick={clearHeard}
              disabled={heard.length === 0}
            >
              <TrashIcon size={15} />
            </IconButton>
            <IconButton label="Close voice mode" onClick={() => setOpen(false)}>
              <CloseIcon size={16} />
            </IconButton>
          </div>
        </header>

        {status && !status.ready && (
          <div className="mx-4 mb-2 rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)] px-3 py-2.5">
            <p className="text-[12.5px] text-soft">
              Voice mode is missing something it needs.
            </p>
            <p className="mt-0.5 text-[11.5px] leading-4 text-faint">
              {status.blocking ?? "Some components are not installed yet."}
            </p>
            <button
              type="button"
              className="btn-primary mt-2"
              onClick={() => {
                setSettingsCategory("voice");
                setSettingsOpen(true);
              }}
            >
              Open Voice settings
            </button>
          </div>
        )}

        <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 overflow-y-auto px-4 lg:grid-cols-2 lg:overflow-hidden">
          {/* --- what was heard --- */}
          <section className="flex min-h-0 flex-col rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)]">
            <h3 className="px-3 pt-2.5 pb-1 text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
              You said
            </h3>
            <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
              {heard.length === 0 ? (
                <EmptyState
                  title={listening ? "Waiting for you to speak" : "Nothing heard yet"}
                  hint={
                    listening
                      ? "Speak normally. A pause of about a quarter of a second ends a sentence."
                      : "Start listening, then talk."
                  }
                />
              ) : (
                <ul className="flex flex-col gap-1.5">
                  {heard.map((text, index) => (
                    <li
                      key={`${index}-${text.slice(0, 12)}`}
                      className="ml-6 rounded-row border border-[var(--glass-border)] bg-[var(--hover-bg)] px-2.5 py-1.5 text-[13px] leading-5"
                    >
                      {text}
                    </li>
                  ))}
                  <div ref={heardEnd} />
                </ul>
              )}
              {lastTruncated && heard.length > 0 && (
                <p className="mt-2 px-1 text-[11.5px] leading-4 text-faint">
                  That last one ran past a minute and was cut short, so its final
                  word may be missing.
                </p>
              )}
            </div>
          </section>

          {/* --- what is being said --- */}
          <section className="flex min-h-0 flex-col rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)]">
            <div className="flex items-center gap-2 px-3 pt-2.5 pb-1">
              <h3 className="text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
                Loom
              </h3>
              {speaking && expected > 0 && (
                <span className="text-[11px] text-faint">
                  {Math.min(spoken, expected)} of {expected}
                </span>
              )}
              {speaking && (
                <button
                  type="button"
                  onClick={stop}
                  className="ml-auto flex items-center gap-1 rounded-capsule px-2 py-0.5 text-[11.5px] text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
                >
                  <StopIcon size={12} />
                  Stop
                </button>
              )}
            </div>

            {speaking && expected > 0 && (
              <div className="mx-3 h-0.5 overflow-hidden rounded-capsule bg-[var(--ink-ghost)]">
                <div
                  className="h-full rounded-capsule bg-[var(--accent)] transition-[width] duration-200"
                  style={{ width: `${Math.round(progress * 100)}%` }}
                />
              </div>
            )}

            <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
              {lines.length === 0 ? (
                <EmptyState
                  title={speaking ? "Getting ready to speak" : "Nothing being said"}
                  hint={
                    speaking
                      ? undefined
                      : "Turn on “Speak replies automatically”, or press the speaker on any message."
                  }
                />
              ) : (
                <ul className="flex flex-col gap-1.5">
                  {lines.map((line, index) => (
                    <li
                      key={`${index}-${line.slice(0, 12)}`}
                      aria-current={index === lineIndex ? "true" : undefined}
                      className={cn(
                        "rounded-row px-2.5 py-1.5 text-[13px] leading-5 transition-colors",
                        index === lineIndex
                          ? "bg-[var(--accent-soft)] text-[var(--ink)]"
                          : index < lineIndex
                            ? "text-faint"
                            : "text-soft",
                      )}
                    >
                      {line}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </section>
        </div>

        {/* --- the dial, and the controls --- */}
        <footer className="flex shrink-0 flex-col items-center gap-2 px-4 pt-3 pb-4">
          {error && (
            <p className="max-w-md text-center text-[12px] text-[var(--danger)]">
              {error}
            </p>
          )}
          {dictationError && (
            <p className="max-w-md text-center text-[12px] text-[var(--danger)]">
              {dictationError}
            </p>
          )}
          {hint && (
            <p className="max-w-md text-center text-[11.5px] leading-4 text-faint">
              {hint}
            </p>
          )}

          <div className="flex items-center gap-4">
            {/* Playback level, so it is visible that Loom is talking even with
                the sound off. */}
            {speaking && (
              <div className="h-1.5 w-16 overflow-hidden rounded-capsule bg-[var(--ink-ghost)]">
                <div
                  className="h-full rounded-capsule bg-[var(--accent)] transition-[width] duration-100"
                  style={{ width: `${Math.round(barHeight(outputLevel) * 100)}%` }}
                />
              </div>
            )}

            <div className="flex flex-col items-center gap-1.5">
              <button
                type="button"
                onClick={toggle}
                disabled={dictation === "starting" || !canCapture()}
                aria-label={listening ? "Stop listening" : "Start listening"}
                aria-pressed={listening}
                className={cn(
                  "relative grid h-20 w-20 place-items-center rounded-capsule border transition-colors",
                  "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]",
                  listening
                    ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                    : "border-[var(--glass-border)] bg-[var(--hover-bg)] text-soft",
                  (dictation === "starting" || !canCapture()) && "opacity-50",
                )}
              >
                {/* The halo. Driven by the level rather than an animation, so
                    what moves is the actual microphone rather than a loop that
                    would keep breathing while nothing was arriving. */}
                <span
                  aria-hidden="true"
                  className="absolute inset-0 rounded-capsule border border-[var(--accent)] transition-transform duration-100"
                  style={{
                    transform: `scale(${1 + barHeight(inputLevel) * 0.3})`,
                    opacity: listening ? 0.45 - barHeight(inputLevel) * 0.25 : 0,
                  }}
                />
                {listening ? <MicOffIcon size={26} /> : <MicIcon size={26} />}
              </button>

              <p className="text-[12px] text-soft">
                {describeListening(dictation, hearing)}
              </p>
            </div>

            {/* Input level, as a strip. Ten a second, so a pause is a gap. */}
            <div
              aria-hidden="true"
              className="flex h-6 w-16 items-end justify-end gap-[2px]"
            >
              {bars.map((value, index) => (
                <span
                  key={index}
                  className={cn(
                    "w-[3px] rounded-capsule transition-[height] duration-100",
                    hearing ? "bg-[var(--accent)]" : "bg-[var(--ink-faint)]",
                  )}
                  style={{ height: `${Math.max(6, barHeight(value) * 100)}%` }}
                />
              ))}
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-center gap-2 pt-1">
            <select
              value={status?.defaultVoice ?? ""}
              onChange={(event) =>
                void save({ defaultVoice: event.currentTarget.value })
              }
              aria-label="Voice"
              className={cn(fieldBase, "max-w-[15rem] text-[12px]")}
            >
              {voices.length === 0 && (
                <option value={status?.defaultVoice ?? ""}>
                  {status?.defaultVoice ?? "no voices installed"}
                </option>
              )}
              {voices.map((voice) => (
                <option key={voice.id} value={voice.id}>
                  {voiceLabel(voice)}
                </option>
              ))}
            </select>

            {/* The one setting that decides what happens to a transcript, and
                the only one worth having here — everything else is in Settings
                → Voice. */}
            <button
              type="button"
              role="switch"
              aria-checked={status?.autoSend ?? false}
              onClick={() => void save({ autoSend: !(status?.autoSend ?? false) })}
              className={cn(
                "flex items-center gap-2 rounded-capsule border px-2.5 py-1 text-[12px] transition-colors",
                status?.autoSend
                  ? "border-transparent bg-[var(--accent)] text-white"
                  : "border-[var(--glass-border)] text-soft hover:text-[var(--ink)]",
              )}
            >
              Send what I say
            </button>
          </div>

          {!status?.autoSend && heard.length > 0 && (
            // Said out loud, because the alternative is a user believing the
            // sentence vanished when it is sitting in the composer behind this
            // surface.
            <p className="text-[11.5px] text-faint">
              Waiting in the composer — press Enter to send.
            </p>
          )}
        </footer>
      </LiquidSurface>
    </div>
  );
}

/* The playback level is read at the top of the component with the other
 * selectors. It was briefly a `useVoiceLevel()` helper called from inside the
 * JSX, which is a hook called conditionally — legal-looking, and a hook-order
 * bug that would have surfaced as an unrelated component misbehaving. */
