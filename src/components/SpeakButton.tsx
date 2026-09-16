/**
 * A speak/stop control for one message.
 *
 * Kept as its own component because the state it reads is per-message: whether
 * *this* one is being spoken, and how far through. Putting that in the parent
 * would re-render the whole canvas on every utterance tick.
 */
import { useVoice } from "../stores/voice";
import { cn } from "../lib/cn";

export function SpeakButton({
  text,
  messageId,
  voice,
  className,
}: {
  /** The message's text. Markdown is stripped on the Rust side. */
  text: string;
  messageId: string;
  /** Overrides the default voice, for a persona with its own. */
  voice?: string;
  className?: string;
}) {
  const speakingId = useVoice((state) => state.speakingId);
  const phase = useVoice((state) => state.phase);
  const ready = useVoice((state) => state.status?.ready ?? false);
  const enabled = useVoice((state) => state.status?.enabled ?? false);
  const speak = useVoice((state) => state.speak);
  const stop = useVoice((state) => state.stop);

  // Nothing to show until the assets are in place: a button that always fails
  // is worse than no button, and Settings → Voice is where that gets fixed.
  if (!enabled || !ready) return null;

  const isSpeaking = speakingId === messageId;
  const busy = isSpeaking && (phase === "loading" || phase === "speaking");

  return (
    <button
      type="button"
      title={busy ? "Stop speaking" : "Read aloud"}
      aria-label={busy ? "Stop speaking" : "Read aloud"}
      onClick={() => {
        if (busy) stop();
        else void speak(text, messageId, voice);
      }}
      className={cn(
        "rounded-[var(--radius-control)] p-1 transition-colors hover:bg-[var(--hover-bg)]",
        busy ? "text-[var(--accent)]" : "text-[var(--ink-faint)] hover:text-[var(--ink)]",
        className,
      )}
    >
      {busy ? <StopGlyph /> : <SpeakGlyph />}
    </button>
  );
}

/** A speaker cone with waves. */
function SpeakGlyph() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8 2.5 4.5 5.5H2v5h2.5L8 13.5z" />
      <path d="M10.5 6a3 3 0 0 1 0 4" />
      <path d="M12.5 4a5.5 5.5 0 0 1 0 8" />
    </svg>
  );
}

/** A filled square, the conventional stop mark. */
function StopGlyph() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="currentColor"
      aria-hidden="true"
    >
      <rect x="4" y="4" width="8" height="8" rx="1.5" />
    </svg>
  );
}
