"""Prints the shapes the voice IPC layer and the frontend voice code already have.

Written as a file rather than a `python -c` one-liner because inline quoting has
been unreliable in this shell: a `-c` with nested quotes arrives mangled and the
syntax error says nothing about the command that caused it.
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent


def show(title):
    print(f"\n=== {title} ===")


# --- the Tauri voice module -------------------------------------------------
voice = ROOT / "src-tauri/src/voice.rs"
if voice.exists():
    text = voice.read_text(encoding="utf-8", errors="replace")
    show(f"src-tauri/src/voice.rs ({len(text.splitlines())} lines)")

    for match in re.finditer(r"#\[tauri::command\]\s*\n((?:\s*///.*\n)*)\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([^)]*)\)\s*(->[^{]*)?", text):
        doc = " ".join(line.strip(" /") for line in match.group(1).splitlines()[:1])
        print(f"  fn {match.group(2)}({' '.join(match.group(3).split())}){match.group(4) or ''}")
        if doc:
            print(f"      {doc[:90]}")

    print("\n  state and job types:")
    for match in re.finditer(r"^(pub struct|pub enum|enum|struct)\s+(\w+)", text, re.M):
        print(f"    {match.group(1)} {match.group(2)}")

    print("\n  variants of the job enum:")
    job = re.search(r"enum Job\s*\{(.*?)\n\}", text, re.S)
    if job:
        for line in job.group(1).splitlines():
            stripped = line.strip()
            if stripped and not stripped.startswith("//") and not stripped.startswith("#"):
                print(f"    {stripped[:100]}")

    print("\n  what the worker thread is named / where it is spawned:")
    for match in re.finditer(r"(thread::spawn|spawn\()", text):
        line_no = text[: match.start()].count("\n") + 1
        line = text.splitlines()[line_no - 1].strip()
        print(f"    line {line_no}: {line[:100]}")

# --- commands.rs ------------------------------------------------------------
commands = ROOT / "src-tauri/src/commands.rs"
if commands.exists():
    text = commands.read_text(encoding="utf-8", errors="replace")
    show(f"src-tauri/src/commands.rs ({len(text.splitlines())} lines)")

    print("  AppState fields:")
    state = re.search(r"pub struct AppState\s*\{(.*?)\n\}", text, re.S)
    if state:
        for line in state.group(1).splitlines():
            stripped = line.strip()
            if stripped.startswith("pub "):
                print(f"    {stripped[:110]}")

    print("\n  voice-related commands:")
    for match in re.finditer(r"#\[tauri::command\]\s*\n\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", text):
        if "voice" in match.group(1) or "speak" in match.group(1) or "audio" in match.group(1):
            print(f"    {match.group(1)}")

# --- lib.rs -----------------------------------------------------------------
lib = ROOT / "src-tauri/src/lib.rs"
if lib.exists():
    text = lib.read_text(encoding="utf-8", errors="replace")
    show(f"src-tauri/src/lib.rs ({len(text.splitlines())} lines)")
    handler = re.search(r"invoke_handler\(tauri::generate_handler!\[(.*?)\]\)", text, re.S)
    if handler:
        names = [n.strip() for n in handler.group(1).split(",") if n.strip()]
        print(f"  {len(names)} registered commands:")
        for name in names:
            print(f"    {name}")
    print("\n  voice service construction:")
    for match in re.finditer(r".*[Vv]oice.*(new|state|manage|start).*", text):
        print(f"    {match.group(0).strip()[:110]}")

# --- the frontend -----------------------------------------------------------
show("frontend voice files")
for name in [
    "src/lib/voice.ts",
    "src/stores/voice.ts",
    "src/components/VoiceSettings.tsx",
    "src/components/SpeakButton.tsx",
]:
    path = ROOT / name
    if not path.exists():
        print(f"  {name}: MISSING")
        continue
    text = path.read_text(encoding="utf-8", errors="replace")
    print(f"  {name}: {len(text.splitlines())} lines")
    if "voice" in name and name.endswith(".ts"):
        for match in re.finditer(r"^export (?:async )?(?:function|const)\s+(\w+)", text, re.M):
            print(f"      {match.group(1)}")

show("frontend voice-related symbols elsewhere")
for path in sorted((ROOT / "src").rglob("*.ts*")):
    if any(part in path.name for part in ("voice", "Voice", "Speak")):
        continue
    text = path.read_text(encoding="utf-8", errors="replace")
    hits = [
        f"{line_no}: {line.strip()[:95]}"
        for line_no, line in enumerate(text.splitlines(), 1)
        if re.search(r"\bvoice\b|\bspeak\b|\btts\b|transcri", line, re.I)
    ]
    if hits:
        print(f"  {path.relative_to(ROOT)}")
        for hit in hits[:8]:
            print(f"    {hit}")
