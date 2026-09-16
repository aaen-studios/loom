"""Splits the working tree into voice-mode changes and everything else.

The tree carries unrelated in-flight work — `engine.rs`, `db.rs`, `harness.rs`,
`tools.rs`, `types.ts`, an untracked `condense.rs` — that predates voice mode. A
commit made from this tree as-is would tangle the two together, so this answers
"what exactly did voice mode touch".

Run: python scripts/voice-changes.py
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).parent.parent

# Files voice mode created, and existing ones it edited.
CREATED_PREFIXES = (
    "crates/loom-core/src/voice/",
    "crates/loom-core/examples/",
)
CREATED_FILES = (
    "docs/voice-mode.md",
    # Frontend.
    "src/lib/voice.ts",
    "src/stores/voice.ts",
    "src/components/SpeakButton.tsx",
    "src/components/VoiceSettings.tsx",
    # The Tauri service layer.
    "src-tauri/src/voice.rs",
)

EDITED_FILES = (
    "crates/loom-core/src/lib.rs",       # one `pub mod voice;` line
    "crates/loom-core/src/paths.rs",     # voice_dir(), and it in ensure_home
    "crates/loom-core/src/config.rs",    # AppConfig::voice
    "crates/loom-core/src/persona.rs",   # Persona::voice
    "crates/loom-core/Cargo.toml",       # ort, libloading
    "Cargo.lock",
    # Frontend. Each change is additive: a new category, a new icon, a new
    # field, a new button in an existing toolbar.
    "src-tauri/Cargo.toml",              # base64
    "src-tauri/src/lib.rs",              # `mod voice`, and the command list
    "src-tauri/src/commands.rs",         # AppState gains two fields
    "src/lib/settingsCategories.ts",     # "voice", and the duplicate fixed
    "src/components/SettingsPanel.tsx",  # category, icon, nav group, section
    "src/components/ChatCanvas.tsx",     # the per-message speak button
    "src/components/icons.tsx",          # SoundIcon
    "src/components/ui.tsx",             # Row children made optional
    "src/types.ts",                      # Persona::voice, AppConfig::voice
    "src/stores/settings.ts",            # the voice defaults
)

# Scripts voice mode added. `cargo-errors.py` is generic but was written here.
SCRIPT_FILES = (
    "scripts/setup-voice.py",
    "scripts/fetch-onnxruntime.py",
    "scripts/fetch-espeak.py",
    "scripts/fetch-libclang.py",
    "scripts/check-wav.py",
    "scripts/check-espeak-constants.py",
    "scripts/check-voice-apis.py",
    "scripts/cargo-errors.py",
    "scripts/typecheck.py",
    "scripts/voice-changes.py",
    "scripts/check-stt-sources.py",
    "scripts/fetch-whisper.py",
    "scripts/probe-tokenizer.py",
    "scripts/check-mel.py",
    "scripts/voice-lines.py",
    "scripts/voice-status.py",
    "scripts/check-ort-versions.py",
    "scripts/probe-ort-api.py",
    "scripts/probe-ort-outlet.py",
    # Examples.
    "crates/loom-core/examples/dump_mel.rs",
    "crates/loom-core/examples/hello_kokoro.rs",
    "crates/loom-core/examples/fetch_voice_assets.rs",
    "crates/loom-core/examples/probe_espeak.rs",
    "crates/loom-core/examples/probe_whisper.rs",
    "crates/loom-core/examples/probe_vad.rs",
    "crates/loom-core/examples/transcribe.rs",
    "crates/loom-core/src/voice/listen.rs",
    "crates/loom-core/src/voice/dictate.rs",
    "public/worklets/loom-mic.js",
    "src/lib/microphone.ts",
    "src/lib/voiceActivity.ts",
    "src/lib/voiceActivity.test.ts",
    "src/lib/voiceWiring.test.ts",
    "src/components/VoiceMode.tsx",
    "src/stores/voice.test.ts",
)


def status():
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    entries = []
    for line in result.stdout.splitlines():
        if not line.strip():
            continue
        entries.append((line[:2].strip(), line[3:].strip().strip('"')))
    return entries


def classify(path):
    if path.startswith(CREATED_PREFIXES) or path in CREATED_FILES:
        return "created"
    if path in SCRIPT_FILES:
        return "created"
    if path in EDITED_FILES:
        return "edited"
    return "other"


def main():
    mine_created = []
    mine_edited = []
    other = []

    for code, path in status():
        kind = classify(path)
        label = f"{code} {path}"
        if kind == "created":
            mine_created.append(label)
        elif kind == "edited":
            mine_edited.append(label)
        else:
            other.append(label)

    print(f"voice mode — created ({len(mine_created)}):")
    for item in sorted(mine_created):
        print(f"  {item}")
    print()

    print(f"voice mode — edited ({len(mine_edited)}):")
    for item in sorted(mine_edited):
        print(f"  {item}")
    print()

    print(f"pre-existing / not voice mode ({len(other)}):")
    for item in sorted(other):
        print(f"  {item}")
    print()

    print("Nothing is staged or committed. To commit only voice mode:")
    print()
    print("  git add crates/loom-core/src/voice crates/loom-core/examples \\")
    print("          docs/voice-mode.md src/lib/voice.ts src/stores/voice.ts \\")
    print("          src/components/SpeakButton.tsx src/components/VoiceSettings.tsx \\")
    print("          src-tauri/src/voice.rs")
    print("  git add crates/loom-core/src/{lib,paths,config,persona}.rs \\")
    print("          src-tauri/src/{lib,commands}.rs \\")
    print("          src/components/{SettingsPanel,ChatCanvas,icons,ui}.tsx \\")
    print("          src/{types.ts,lib/settingsCategories.ts} src/stores/settings.ts \\")
    print("          crates/loom-core/Cargo.toml src-tauri/Cargo.toml Cargo.lock")
    print("  git add scripts/setup-voice.py scripts/fetch-*.py \\")
    print("          scripts/check-*.py scripts/probe-*.py \\")
    print("          scripts/cargo-errors.py scripts/typecheck.py")
    print("  git commit -m 'voice: Kokoro speech pipeline'")
    return 0


if __name__ == "__main__":
    sys.exit(main())
