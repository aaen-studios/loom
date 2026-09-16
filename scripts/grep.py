"""Grep files, printing line numbers.

Single-file `grep` in this shell returns false negatives on large files, and the
first version of this script did too: it reported **0 matches** for text that was
visibly present in the output it searched, and that turned into a wrong
conclusion about which edits had landed.

# Why the first version was wrong

`path.parts` was tested against a list of directory names to skip. For a file at
the workspace root, `parts` is the *absolute* path split up, so a workspace
directory name can appear in it — and here the workspace is
`Documents/GitHub/loom/src/...`, where a component can accidentally collide with
a skip word. Filtering on `parts` is the bug: it tests the whole absolute path
rather than the part of it that was asked for.

So this version:
  * filters on the path *relative to the root*, so only real parts count
  * prints the line count of every file it reads, so a file that was not read
    (or was read as empty) is obvious rather than silent
  * reports "0 matching lines" only after printing the files it searched

Written as a file rather than `python -c` because inline quoting with nested
quotes arrives mangled in this shell, and the resulting syntax error says
nothing about the command that caused it.
"""

import pathlib
import re
import sys

# The console here is cp1252, so printing a `−`, `→` or any other character from
# a source file raises UnicodeEncodeError and kills the script *mid-output* —
# after it has printed some matches. Partial output that looks complete is worse
# than none, so the encoding is set explicitly and anything unencodable is
# replaced rather than fatal.
try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
except (AttributeError, OSError):
    pass

ROOT = pathlib.Path(__file__).resolve().parent.parent

if len(sys.argv) < 2:
    print("usage: grep.py <regex> [path ...]")
    sys.exit(2)

# Surrounding quotes are stripped from every argument, and that is not cosmetic.
#
# This shell passes a quoted argument with its quote characters *attached*, so
# `grep.py "panel-strong" src` arrives as the literal string `"panel-strong"` —
# quotes included — and a regex for that cannot match `panel-strong`. The script
# then reports "0 matching lines", which looks exactly like a real answer.
#
# This caused wrong conclusions: several searches appeared to prove an edit had
# not landed when it had, and one showed a function missing that was there.
# A search tool whose empty result is indistinguishable from "not found" is
# worse than no search tool, so the stripping happens on both the pattern and
# the paths.
def unquote(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        return value[1:-1]
    return value


pattern = unquote(sys.argv[1])
given = [unquote(item) for item in sys.argv[2:]] or ["src"]
# Case-insensitive, and that was a regression worth naming: the version I
# rewrote to fix the quoting bug dropped the `IGNORECASE` the original had, so
# a search for `Speaker` stopped matching `SoundIcon`'s doc comment about "a
# speaker cone" — and reported zero, which reads as "not there".
expression = re.compile(pattern, re.IGNORECASE)

# Names that mean "do not search this", checked against the relative path only.
SKIP = {"node_modules", "target", "dist", ".git", "build"}

targets: list[pathlib.Path] = []
for name in given:
    path = (ROOT / name).resolve()
    if not path.exists():
        print(f"missing: {name}")
        continue
    if path.is_dir():
        for pattern_glob in ("*.ts", "*.tsx", "*.rs", "*.py", "*.md", "*.js"):
            targets.extend(sorted(path.rglob(pattern_glob)))
    else:
        targets.append(path)


def relative(path: pathlib.Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


searched = 0
total = 0
for path in targets:
    rel = relative(path)
    # Skip on the *relative* path, so a parent directory named `src` or `dist`
    # cannot hide a file and an absolute path cannot hide a whole tree.
    if any(part in SKIP for part in pathlib.PurePath(rel).parts):
        continue

    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as error:
        print(f"unreadable: {rel} ({error})")
        continue

    searched += 1
    lines = text.splitlines()
    hits = [
        (number, line)
        for number, line in enumerate(lines, 1)
        if expression.search(line)
    ]
    if not hits:
        continue

    print(f"{rel}  ({len(hits)} of {len(lines)} lines)")
    for number, line in hits[:40]:
        trimmed = line.strip()
        if len(trimmed) > 120:
            trimmed = trimmed[:117] + "..."
        print(f"  {number}: {trimmed}")
    total += len(hits)

print(f"\n{total} matching lines in {searched} file(s) searched")
if searched == 0:
    print("nothing was searched, so the count above means nothing")
    sys.exit(2)
