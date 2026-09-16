"""Confirms the listening chain is wired end to end, by reading the source.

A checklist that a human ticks is a checklist that drifts. This reads the actual
call chain from the files, so "the microphone is connected to the recogniser" is
a claim with evidence rather than a claim.

It is a structural check, not a behavioural one: it proves each link names the
next, not that the audio sounds right. The behavioural evidence is the test
suite, which is why `check-tests.py` is the thing to run afterwards.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Each link: what must appear in which file, and what it means.
LINKS = [
    (
        "crates/loom-core/src/voice/audio.rs",
        r"pub fn for_vad",
        "the framer produces the VAD's exact window size",
    ),
    (
        "crates/loom-core/src/voice/vad.rs",
        r"self\.context",
        "the 64-sample context is threaded, not just the 512-sample window",
    ),
    (
        "crates/loom-core/src/voice/listen.rs",
        r"impl Detector for Vad",
        "the listener drives the real detector",
    ),
    (
        "crates/loom-core/src/voice/listen.rs",
        r"MIN_UTTERANCE|min_quiet_windows",
        "the hysteresis comes from the reference's thresholds",
    ),
    (
        "crates/loom-core/src/voice/dictate.rs",
        r"Whisper::load",
        "dictation owns a recogniser",
    ),
    (
        "crates/loom-core/src/voice/dictate.rs",
        r"Listener::new",
        "dictation owns a listener",
    ),
    (
        "src-tauri/src/voice.rs",
        r"Dictation::load_from|load_dictation",
        "the command layer loads both models",
    ),
    (
        "src-tauri/src/voice.rs",
        r"engine\.push",
        "microphone blocks reach the dictation engine",
    ),
    (
        "src-tauri/src/voice.rs",
        r"LISTEN_EVENT",
        "transcripts are emitted to the frontend",
    ),
    (
        "src-tauri/src/commands.rs",
        r"dictation:",
        "the service is in app state",
    ),
    (
        "src-tauri/src/lib.rs",
        r"voice::voice_listen_audio",
        "the listen commands are registered",
    ),
    (
        "src/lib/microphone.ts",
        r"getUserMedia",
        "the webview opens the microphone",
    ),
    (
        "src/lib/microphone.ts",
        r"echoCancellation",
        "echo cancellation is requested, so Loom does not hear itself",
    ),
    (
        "src/lib/microphone.ts",
        r'addModule\("/worklets/loom-mic\.js"\)',
        "the worklet is loaded from a real file, as the CSP requires",
    ),
    (
        "public/worklets/loom-mic.js",
        r"registerProcessor",
        "the worklet exists and registers",
    ),
    (
        "src/lib/microphone.ts",
        r"listenAudio",
        "blocks are sent to Rust",
    ),
    (
        "src/stores/voice.ts",
        r'"loom://voice-listen"',
        "the store listens for transcripts",
    ),
    (
        "src/stores/voice.ts",
        r"get\(\)\.stop\(\)",
        "a speech event stops playback — this is barge-in",
    ),
    (
        "src/components/Composer.tsx",
        r"startListening",
        "the composer can start a session",
    ),
    (
        "src/components/Composer.tsx",
        r"transcriptSeq",
        "a transcript reaches the composer exactly once",
    ),
    # The two links that were missing, and that this check could not see.
    (
        "src/App.tsx",
        r"useVoiceEvents\(\)",
        "the voice store is subscribed for the whole session",
    ),
    (
        "src/App.tsx",
        r"<VoiceMode\s*/>",
        "the voice mode surface is mounted",
    ),
    (
        "src/components/VoiceMode.tsx",
        r"startListening",
        "the surface can start a session",
    ),
    (
        "src/lib/events.ts",
        r"return attach\(\)",
        "attach is called — without this, both directions are silent",
    ),
]

missing = []
print("=== the listening chain ===")
for name, pattern, meaning in LINKS:
    path = ROOT / name
    if not path.exists():
        print(f"  MISSING  {name}")
        missing.append(f"{name} does not exist")
        continue
    text = path.read_text(encoding="utf-8", errors="replace")
    if re.search(pattern, text):
        print(f"  ok       {meaning}")
    else:
        print(f"  MISSING  {meaning}")
        print(f"           ({name} has no match for {pattern!r})")
        missing.append(meaning)

print()
if missing:
    print(f"{len(missing)} link(s) missing:")
    for item in missing:
        print(f"  - {item}")
    sys.exit(1)

print(f"all {len(LINKS)} links are present.")
print()
print("This is a structural check. The behavioural evidence is `check-tests.py`,")
print("and the honest gap that neither covers is a real microphone.")
