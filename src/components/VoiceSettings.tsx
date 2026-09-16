/**
 * The Voice settings section: components, the default voice, and playback.
 *
 * Everything here runs locally. The one fact worth stating in the UI rather
 * than burying in a credits file is that espeak-ng is GPL-3.0, so it appears
 * next to the download button — it describes what is about to be put on the
 * machine.
 *
 * The screen also has to work while half-installed: the voice picker is useful
 * as soon as the 28 MB voices file is present, long before the 325 MB model is,
 * so nothing is gated on the whole set being complete.
 */
import { useEffect, useMemo } from "react";
import { cn } from "../lib/cn";
import type { VoiceInfo } from "../lib/voice";
import { useVoice } from "../stores/voice";
import { Row, Section, Toggle } from "./ui";

/** Bytes, phrased for a person. */
function megabytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}

/** Groups voices by accent, so the picker reads as a list rather than a wall. */
function groupVoices(voices: VoiceInfo[]): Array<{ accent: string; items: VoiceInfo[] }> {
  const groups = new Map<string, VoiceInfo[]>();
  for (const voice of voices) {
    const key = voice.accent ?? "Other";
    const list = groups.get(key);
    if (list) list.push(voice);
    else groups.set(key, [voice]);
  }
  return [...groups.entries()]
    .map(([accent, items]) => ({ accent, items }))
    .sort((a, b) => a.accent.localeCompare(b.accent));
}

export function VoiceSettings() {
  const status = useVoice((state) => state.status);
  const voices = useVoice((state) => state.voices);
  const install = useVoice((state) => state.install);
  const installing = useVoice((state) => state.installing);

  const load = useVoice((state) => state.load);
  const loadVoices = useVoice((state) => state.loadVoices);
  const installAll = useVoice((state) => state.installAll);
  const save = useVoice((state) => state.save);
  const preview = useVoice((state) => state.preview);
  const stop = useVoice((state) => state.stop);

  useEffect(() => {
    void load();
    void loadVoices();
  }, [load, loadVoices]);

  const groups = useMemo(() => groupVoices(voices), [voices]);

  if (!status) {
    return <p className="px-1 py-4 text-[12.5px] text-faint">Reading voice settings…</p>;
  }

  const missing = status.components.filter((component) => !component.installed);
  const totalMissing = missing.reduce((sum, component) => sum + component.bytes, 0);
  const percent = install.length
    ? Math.max(...install.map((step) => step.percent), 0)
    : 0;

  return (
    <>
      <Section
        title="Playback"
        description="Loom can read replies aloud. Everything runs on this machine: nothing is sent anywhere."
      >
        <Toggle
          label="Enable voice"
          hint="Read-aloud controls appear on replies while this is on."
          checked={status.enabled}
          onChange={(enabled) => void save({ enabled })}
        />
        <Toggle
          label="Speak replies automatically"
          hint="Off means nothing is spoken until you ask for it."
          checked={status.autoplay}
          onChange={(autoplay) => void save({ autoplay })}
        />
        <Toggle
          label="Send what I say"
          hint={
            status.ready
              ? "Dictate with the microphone button beside the composer. Off means a " +
                "transcript waits in the composer to be corrected first — recognition " +
                "does make mistakes."
              : "Needs the speech-to-text components below."
          }
          checked={status.autoSend}
          onChange={(autoSend) => void save({ autoSend })}
        />
        <Row label="Speed" hint={`${status.speed.toFixed(2)}× playback rate`}>
          <input
            type="range"
            min={0.5}
            max={2}
            step={0.05}
            value={status.speed}
            onChange={(event) => void save({ speed: Number(event.currentTarget.value) })}
            className="w-36 accent-[var(--accent)]"
          />
        </Row>
      </Section>

      <Section
        title="Components"
        description="Nothing ships in the installer. These are fetched on first use and kept in the Loom home folder."
      >
        {status.components.map((component) => {
          const step = install.find((item) => item.component === component.id);
          return (
            <Row
              key={component.id}
              label={component.label}
              hint={`${megabytes(component.bytes)} · ${component.licence}${
                step && !component.installed ? ` · ${step.detail}` : ""
              }`}
            >
              <span
                className={cn(
                  "text-[11.5px]",
                  component.installed ? "text-[var(--accent)]" : "text-faint",
                )}
              >
                {component.installed ? "installed" : "not installed"}
              </span>
            </Row>
          );
        })}

        {installing && (
          <div className="px-1 py-2">
            <div className="h-1 w-full overflow-hidden rounded-capsule bg-[var(--ink-ghost)]">
              <div
                className="h-full rounded-capsule bg-[var(--accent)] transition-[width]"
                style={{ width: `${percent}%` }}
              />
            </div>
            <p className="mt-1 text-[11.5px] text-faint">{percent}% of the total</p>
          </div>
        )}

        {missing.length > 0 && !installing && (
          <Row
            label={`Download ${missing.length} component${missing.length === 1 ? "" : "s"}`}
            hint={`${megabytes(totalMissing)} · espeak-ng is GPL-3.0, the rest is permissive`}
          >
            <button
              type="button"
              onClick={() => void installAll()}
              className="shrink-0 rounded-control bg-[var(--accent)] px-2.5 py-1 text-[12px] text-white transition-opacity hover:opacity-90"
            >
              Download
            </button>
          </Row>
        )}

        <Row label="Location" hint={status.home} />
      </Section>

      <Section
        title="Default voice"
        description={
          voices.length === 0
            ? "The voices file is not installed yet. Download the components above."
            : `${voices.length} voices. Used by every persona that has not chosen its own. Click one to hear it.`
        }
      >
        {groups.map((group) => (
          <div key={group.accent} className="px-1 py-2">
            <p className="text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
              {group.accent}
            </p>
            <div className="mt-1.5 flex flex-wrap gap-1">
              {group.items.map((voice) => (
                <button
                  key={voice.id}
                  type="button"
                  title={`${voice.gender ?? "unknown"} · ${voice.language ?? "no phonemizer"}`}
                  onClick={() => {
                    void save({ defaultVoice: voice.id });
                    void preview(voice.id);
                  }}
                  className={cn(
                    "rounded-capsule border px-2 py-0.5 font-mono text-[11px] transition-colors",
                    voice.id === status.defaultVoice
                      ? "border-[var(--accent)] text-[var(--accent)]"
                      : "border-[var(--glass-border)] text-soft hover:text-[var(--ink)]",
                  )}
                >
                  {voice.id}
                </button>
              ))}
            </div>
          </div>
        ))}

        {voices.length > 0 && (
          <Row label="Stop" hint="Silences whatever is speaking.">
            <button
              type="button"
              onClick={stop}
              className="shrink-0 rounded-control border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-soft transition-colors hover:text-[var(--ink)]"
            >
              Stop
            </button>
          </Row>
        )}

        {voices.length === 0 && !installing && (
          <p className="px-1 py-3 text-center text-[12.5px] text-faint">
            No voices yet. They arrive with the components above.
          </p>
        )}
      </Section>

      {status.blocking && (
        <Section title="Not ready">
          <Row label="Voice mode is unavailable" hint={status.blocking} />
        </Section>
      )}

      <Section
        title="Credits"
        description="Attributions the models and libraries require."
      >
        <p className="px-1 py-2 text-[11.5px] leading-4 text-faint">
          Kokoro-82M, Apache-2.0, by hexgrad. Trained partly on CC BY audio that
          requires attribution: Koniwa (CC BY 3.0) and SIWIS (CC BY 4.0).
          Phonemization uses espeak-ng, GPL-3.0. ONNX Runtime is MIT.
        </p>
      </Section>
    </>
  );
}
