import type { TreeEntry } from "./ipc";

/**
 * The file tree's pure parts.
 *
 * The tree is built **lazily, one directory at a time**, by `dir_list`, rather
 * than from a flat list of every path. That is a deliberate departure from what
 * the composer's `@` picker does, and the two want different things: the picker
 * needs to *search* the tree, so it wants every path up front, while a tree that
 * can be expanded needs only what is on screen. A collapsed folder then costs
 * nothing, and a repository with two hundred thousand files opens instantly
 * instead of walking all of them to draw one level.
 *
 * So what lives here is not a tree builder — it is the small set of path
 * questions the component has to answer repeatedly, each of which is easy to get
 * subtly wrong and trivial to test.
 */

/** The parent folder of a `/`-separated path, or `""` for a top-level entry. */
export function parentOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index === -1 ? "" : path.slice(0, index);
}

/** The last segment of a path. */
export function nameOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index === -1 ? path : path.slice(index + 1);
}

/**
 * Every folder between the root and a path, outermost first.
 *
 * This is what "reveal in the tree" expands. The path itself is excluded — a
 * file is not a folder to expand, and including it would make the caller filter
 * it out again.
 */
export function ancestorsOf(path: string): string[] {
  const parts = path.split("/").filter(Boolean);
  const out: string[] = [];
  // The last segment is the entry itself, so only the ones before it are
  // folders on the way to it.
  for (let i = 0; i < parts.length - 1; i += 1) {
    out.push(parts.slice(0, i + 1).join("/"));
  }
  return out;
}

/** Whether `path` is at or inside `folder`. */
export function isInside(folder: string, path: string): boolean {
  if (folder === "") return true;
  return path === folder || path.startsWith(`${folder}/`);
}

/**
 * Sorts a directory listing the way every file tree does: folders first, then
 * files, each case-insensitively.
 *
 * Rust already sorts this way on the way out, and this is not redundant — the
 * component merges a listing with **local** entries it knows about (a file being
 * created, a rename in flight) that git has not reported yet, and those arrive
 * in insertion order.
 */
export function sortEntries(entries: TreeEntry[]): TreeEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  });
}

/**
 * A default name for a new file or folder, not colliding with what is there.
 *
 * `untitled`, `untitled-2`, `untitled-3` — the same convention every editor uses,
 * and the reason to do it rather than asking the user to type a name first is
 * that the common case is "create one, then decide", and a modal in front of that
 * is a modal in the way.
 */
export function unusedName(entries: TreeEntry[], base = "untitled"): string {
  const taken = new Set(entries.map((entry) => entry.name.toLowerCase()));
  if (!taken.has(base.toLowerCase())) return base;
  for (let n = 2; n < 1000; n += 1) {
    const candidate = `${base}-${n}`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
  // Practically unreachable, and a name that is at worst ugly.
  return `${base}-${Date.now()}`;
}

/**
 * The language id for a path, for Monaco.
 *
 * A hand-written map rather than Monaco's own registry lookup, because Monaco's
 * extension table is internal and reaching into it couples this to a version.
 * What is here covers what Loom's own workspace contains — this is a coding
 * workspace, and the file set is not arbitrary.
 *
 * Unknown extensions fall back to `plaintext` rather than being guessed at: a
 * wrong grammar produces confidently wrong colours, which is worse than none.
 */
const LANGUAGES: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  rs: "rust",
  toml: "ini",
  json: "json",
  jsonc: "json",
  css: "css",
  scss: "scss",
  less: "less",
  html: "html",
  htm: "html",
  vue: "html",
  svg: "xml",
  xml: "xml",
  md: "markdown",
  markdown: "markdown",
  yml: "yaml",
  yaml: "yaml",
  py: "python",
  rb: "ruby",
  go: "go",
  java: "java",
  kt: "kotlin",
  kts: "kotlin",
  c: "c",
  h: "c",
  cpp: "cpp",
  cc: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  cs: "csharp",
  php: "php",
  swift: "swift",
  sh: "shell",
  bash: "shell",
  zsh: "shell",
  ps1: "powershell",
  psm1: "powershell",
  bat: "bat",
  cmd: "bat",
  sql: "sql",
  graphql: "graphql",
  gql: "graphql",
  dockerfile: "dockerfile",
  ini: "ini",
  cfg: "ini",
  env: "ini",
  lock: "plaintext",
  txt: "plaintext",
  log: "plaintext",
  diff: "diff",
  patch: "diff",
};

/** A special filename with no extension that still has a grammar. */
const FILENAMES: Record<string, string> = {
  dockerfile: "dockerfile",
  makefile: "makefile",
  ".gitignore": "plaintext",
  ".gitattributes": "plaintext",
  ".editorconfig": "ini",
  ".env": "ini",
  "cargo.lock": "toml",
};

export function languageFor(path: string): string {
  const name = nameOf(path).toLowerCase();
  const byName = FILENAMES[name];
  if (byName) return byName;

  const dot = name.lastIndexOf(".");
  if (dot === -1 || dot === name.length - 1) return "plaintext";
  const extension = name.slice(dot + 1);
  return LANGUAGES[extension] ?? "plaintext";
}

/**
 * The pseudo-path a diff tab uses.
 *
 * Diffs are tabs in the editor rather than a mode of the panel, because that is
 * what keeps the git panel free of any diff rendering of its own — clicking a
 * changed file opens one of these and the editor draws it. The prefix makes a
 * diff tab impossible to confuse with a real file, which matters because the
 * rest of the editor keys everything off the path: a distinct shape means a
 * dirty diff can never be written back to disk.
 */
export const DIFF_PREFIX = "diff:";

/** Whether a tab path is a diff rather than a file. */
export function isDiffPath(path: string): boolean {
  return path.startsWith(DIFF_PREFIX);
}

/**
 * The diff tab's path for a file.
 *
 * `staged` is part of the identity: the staged and unstaged diffs of one file are
 * different things, and sharing a tab between them would show the wrong one the
 * moment both are open.
 */
export function diffPath(file: string, staged: boolean): string {
  return `${DIFF_PREFIX}${staged ? "staged/" : "worktree/"}${file}`;
}

/** The file and side a diff path refers to. */
export function parseDiffPath(path: string): { file: string; staged: boolean } | null {
  if (!isDiffPath(path)) return null;
  const rest = path.slice(DIFF_PREFIX.length);
  if (rest.startsWith("staged/")) {
    return { file: rest.slice("staged/".length), staged: true };
  }
  if (rest.startsWith("worktree/")) {
    return { file: rest.slice("worktree/".length), staged: false };
  }
  return null;
}

/** The label a tab shows for a path. */
export function tabLabel(path: string): string {
  if (isDiffPath(path)) {
    const parsed = parseDiffPath(path);
    return parsed ? `${nameOf(parsed.file)} (diff)` : "diff";
  }
  return nameOf(path);
}

/** A name for a new entry in a folder, avoiding what is already there. */
export function suggestedNameFor(entries: TreeEntry[], isDir: boolean): string {
  return unusedName(entries, isDir ? "new-folder" : "untitled");
}
