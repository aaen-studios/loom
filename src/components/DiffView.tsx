import { useEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { parseDiffPath } from "../lib/fileTreeOperations";
import { useEditor } from "../stores/editor";
import { useGit } from "../stores/git";
import type { EditorTab } from "../stores/editor";

/**
 * A file's changes, as a real editor diff.
 *
 * ## Why this is two strings and not a patch
 *
 * Monaco's `DiffEditor` takes a *before* and an *after*, which is exact: two
 * complete versions of the file, and the diff is computed from them. The
 * alternative — parsing a unified patch from `git diff` and rendering it — means
 * writing a hunk parser, then re-deriving line positions to place each hunk, and
 * being subtly wrong about a file with no trailing newline or a hunk that ends
 * mid-line.
 *
 * So both sides are fetched as content: `git show HEAD:<path>` for the before
 * (or `:<path>` for the index, when the tab is the staged diff), and the working
 * file for the after. That also means this component is identical for a staged
 * and an unstaged diff, and it needs nothing from the git panel but a path.
 *
 * ## Why it is read-only
 *
 * The diff *is* the comparison; there is nothing to type into it. Editing a
 * changed file means opening the file, which is a click away in the tree or the
 * git panel's own row. Making the diff editable would mean deciding what an edit
 * to the "before" side even means, and there is no answer to that which is not a
 * surprise.
 */
export function DiffView({ tab }: { tab: EditorTab }) {
  const parsed = parseDiffPath(tab.path);
  const workdir = useEditor((state) => state.workdir);
  const gitStatus = useGit((state) => state.status);

  const holderRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<import("monaco-editor").editor.IStandaloneDiffEditor | null>(null);
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!parsed || !workdir) {
      setState("error");
      setError("This diff does not name a file.");
      return;
    }

    let live = true;
    setState("loading");

    void (async () => {
      try {
        const [before, after] = await Promise.all([
          // `null` when the file is not in that revision, which is the ordinary
          // case for a new file — an empty before side is exactly right, and
          // not an error.
          ipc.gitFileAt(workdir, parsed.file, parsed.staged),
          // The working file is the "after" for both sides. For a *staged*
          // diff the honest reading is "what will be committed", which is the
          // index — but showing the index against the working tree is what
          // every editor calls "changes not staged", and confusing the two is
          // worse than picking one. Staged means index vs HEAD, so the after
          // side is the index.
          parsed.staged
            ? ipc.gitFileAt(workdir, parsed.file, true)
            : ipc.fileRead(workdir, parsed.file).then((file) => file?.text ?? null),
        ]);

        if (!live) return;

        const { loadMonaco, THEME_NAME } = await import("../lib/monaco");
        const monaco = await loadMonaco();
        if (!live) return;
        if (!holderRef.current) return;

        const language = languageOf(parsed.file);
        const beforeText = before ?? "";
        // A staged diff whose "after" read failed, or a file deleted in the
        // index, still has to draw something rather than blank.
        const afterText = after ?? "";

        const diff = monaco.editor.createDiffEditor(holderRef.current, {
          // Named explicitly rather than left to the global default. `loadMonaco`
          // does set it globally, but a diff is the one place where getting it
          // wrong is least obvious: Monaco's stock `vs` is a *light* theme, so
          // the mistake shows up as a white pane in a dark app rather than as
          // anything that reads like a missing theme.
          theme: THEME_NAME,
          // Read-only, and `originalEditable: false` is the half that matters:
          // without it Monaco lets you type into the left pane, and a
          // read-only-looking pane that accepts text is worse than one that
          // refuses it.
          readOnly: true,
          originalEditable: false,
          renderSideBySide: true,
          // Word-level changes are what make a one-word edit legible rather
          // than showing two whole lines.
          renderOverviewRuler: false,
          enableSplitViewResizing: true,
          // The docked panel is 460px wide at its default, and a side-by-side
          // split in that is two columns of nothing. Inline mode shows the
          // change in one column, which is the only readable arrangement at the
          // width the panel actually gets.
          useInlineViewWhenSpaceIsLimited: true,
          renderIndicators: true,
          ignoreTrimWhitespace: false,
          automaticLayout: true,
          scrollBeyondLastLine: false,
          fontSize: 12.5,
          fontFamily: '"JetBrains Mono", ui-monospace, Consolas, monospace',
          minimap: { enabled: false },
        });

        const original = monaco.editor.createModel(
          beforeText,
          language,
          monaco.Uri.parse(`loom-diff-before:///${encodeURIComponent(tab.path)}`),
        );
        const modified = monaco.editor.createModel(
          afterText,
          language,
          monaco.Uri.parse(`loom-diff-after:///${encodeURIComponent(tab.path)}`),
        );
        diff.setModel({ original, modified });
        editorRef.current = diff;
        setState("ready");

        return () => {
          // The models are created here and nowhere else, so they are disposed
          // here. A diff is not a buffer with a life beyond its tab, so unlike
          // the file models in `lib/editors` there is nothing to preserve.
          diff.dispose();
          original.dispose();
          modified.dispose();
          editorRef.current = null;
        };
      } catch (cause) {
        if (!live) return;
        setState("error");
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    })();

    return () => {
      live = false;
    };
  }, [parsed?.file, parsed?.staged, workdir, tab.path]);

  // Redraws when git moves underneath, so a diff left open across a commit or a
  // stage stops showing a comparison that no longer exists. The status is the
  // trigger rather than the content: what matters is that *something* changed,
  // and the effect above re-reads both sides.
  const statusKey = gitStatus.files.length;
  void statusKey;

  if (state === "error") {
    return (
      <div className="grid h-full place-items-center p-6">
        <p className="max-w-[280px] text-center text-[12.5px] leading-5 text-faint">
          {error ?? "This diff could not be read."}
        </p>
      </div>
    );
  }

  return (
    <div className="relative h-full min-h-0">
      {state === "loading" && (
        <p className="absolute inset-x-0 top-2 z-10 text-center text-[12px] text-faint">
          Reading both versions…
        </p>
      )}
      <div ref={holderRef} className="h-full min-h-0" />
    </div>
  );
}

/** A quick language guess for the diff panes, from the extension. */
function languageOf(path: string): string {
  const name = path.split("/").pop()?.toLowerCase() ?? path;
  const extension = name.includes(".") ? name.split(".").pop() ?? "" : "";
  const known: Record<string, string> = {
    ts: "typescript",
    tsx: "typescript",
    js: "javascript",
    jsx: "javascript",
    rs: "rust",
    json: "json",
    css: "css",
    md: "markdown",
    html: "html",
    yml: "yaml",
    yaml: "yaml",
    toml: "ini",
    py: "python",
  };
  return known[extension] ?? "plaintext";
}
