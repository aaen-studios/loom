"""Checks the voice-mode document for the sections this session added.

Written as a file rather than a command line because this shell attaches quote
characters to quoted arguments and splits on spaces inside them, so a phrase like
"Two absences" arrives as two separate arguments and the search reports a
missing file named `absences"`. That produced two false "the edit did not land"
conclusions before I stopped trusting inline invocations for phrases.
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOC = ROOT / "docs/voice-mode.md"

text = DOC.read_text(encoding="utf-8", errors="replace")

print(f"=== {DOC.relative_to(ROOT)} ({len(text.splitlines())} lines) ===\n")

print("headings:")
for match in re.finditer(r"^#{2,3} .*$", text, re.M):
    print(f"  {match.group(0)}")

print()
# Each item: a phrase that must appear, and what its absence would mean.
REQUIRED = [
    ("## 11. The voice-mode surface", "the surface has its own section"),
    ("VoiceMode.tsx", "the surface is named in the docs"),
    ("voiceActivity", "the shared level logic is named"),
    ("Ctrl+Shift+V", "the shortcut is documented"),
    ("SpeechQueue.onSentence", "the playback-timed highlight is explained"),
    ("Two absences", "the missing-link bugs are recorded"),
    ("attach()", "the never-called subscriber is named"),
    ("11.2 Not built", "the remaining work is listed"),
    ("real microphone", "the unverified gap is stated"),
    ("voice/listen.rs", "the listener is in the module table"),
    ("voice/dictate.rs", "dictation is in the module table"),
]

print("required content:")
missing = []
for phrase, meaning in REQUIRED:
    if phrase in text:
        print(f"  ok       {meaning}")
    else:
        print(f"  MISSING  {meaning}  ({phrase!r})")
        missing.append(phrase)

print()
# Things that must NOT still be there, because they stopped being true.
STALE = [
    "no microphone and no voice UI",
    "getUserMedia not written",
    "Not built\n\n- **Microphone capture.**",
    "The Gemini-style",
]

print("stale claims:")
for phrase in STALE:
    if phrase in text:
        print(f"  STALE    {phrase!r} is still present")
        missing.append(phrase)
    else:
        print(f"  ok       {phrase!r} is gone")

print()
if missing:
    print(f"{len(missing)} problem(s):")
    for item in missing:
        print(f"  - {item}")
else:
    print("the document is consistent with the code.")
