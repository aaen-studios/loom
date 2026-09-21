/**
 * The live Monaco models, held outside React on purpose.
 *
 * ## Why this file exists
 *
 * `PanelBody` in `DockShell.tsx` mounts **only the active tab** of a zone. That
 * is right for a terminal and right for a file list, but it means switching away
 * from the editor unmounts it — and a Monaco model held in component state is
 * destroyed when that happens. The buffer, the undo stack, the cursor, the
 * folding: all of it, while the user's unsaved edits are in it.
 *
 * So the models live here, keyed by path, and are moved into whichever React
 * node is currently showing them. This is exactly the arrangement
 * `lib/terminals.ts` uses for xterm, and for exactly the same reason — its
 * comment says "putting one in a store would re-render whatever subscribes on
 * every prompt redraw, and putting it in component state would destroy it on
 * every tab switch, losing the scrollback, which is the thing a terminal is
 * for". A model is that same kind of object: large, stateful, and owned by
 * something that outlives the component tree.
 *
 * ## What is deliberately NOT here
 *
 * Only the model and the view state. Everything else about an open file — the
 * hash it was loaded at, the line ending, whether it is dirty, whether it
 * conflicts — is small, plain data, and belongs in the store where React can
 * read it without subscribing to a Monaco object.
 *
 * The split matters because `terminals.ts` learned it the hard way: the less
 * that lives in the module-level map, the less there is to keep in step when a
 * buffer closes.
 */

/**
 * Monaco's own types, imported by name.
 *
 * `Monaco["editor"]["ITextModel"]` does not work: `typeof import(...)` describes
 * the module's *values*, and `ITextModel` is a type-only member of the `editor`
 * namespace, so an indexed access on the value type cannot reach it. Importing
 * the namespace as a type is the way to name those members.
 */
import type { editor as MonacoEditor } from "monaco-editor";

type Monaco = typeof import("monaco-editor");
type ViewState = MonacoEditor.ICodeEditorViewState | null;

interface Live {
  model: MonacoEditor.ITextModel;
  /** Where the cursor was, and what was folded, when it was last shown. */
  viewState: ViewState;
  /** The language the model was created with, so a rename can re-check it. */
  language: string;
}

const live = new Map<string, Live>();

/** The model for a path, or null when it is not open. */
export function modelFor(path: string): Live["model"] | null {
  return live.get(path)?.model ?? null;
}

/** Whether a path already has a model, so a reopen need not re-read the file. */
export function has(path: string): boolean {
  return live.has(path);
}

/**
 * The model for a path, creating it if needed.
 *
 * `monaco.Uri.file`-style URIs rather than bare strings: Monaco keys its own
 * registries by URI, and two paths that differ only in case are genuinely
 * different files on Linux and genuinely the same one on Windows. Using the URI
 * form keeps Monaco's view of identity consistent with the backend's, which
 * resolves paths the same way the OS does.
 *
 * `keepUndo` is the parameter that matters. Re-creating a model on a reload
 * would throw away the undo stack, so the caller says whether the text it is
 * supplying is the same buffer — a conflict resolution where the user chose to
 * keep their version — or a genuinely new one. `setValue` on an existing model
 * keeps the stack; `createModel` does not.
 */
export function ensure(
  monaco: Monaco,
  path: string,
  text: string,
  language: string,
  keepUndo = false,
): Live["model"] {
  const existing = live.get(path);
  if (existing) {
    if (existing.model.getValue() !== text) {
      // `pushEditOperations` would be needed to make this undoable as one step;
      // `setValue` clears the stack, which is the honest behaviour for "the file
      // changed underneath you" — undoing back into the version you just
      // discarded would be a lie.
      existing.model.setValue(text);
    }
    if (existing.language !== language) {
      monaco.editor.setModelLanguage(existing.model, language);
      existing.language = language;
    }
    void keepUndo;
    return existing.model;
  }

  const uri = monaco.Uri.parse(modelUri(path));
  // A model may already exist under this URI if a previous `ensure` created one
  // and the map lost track — StrictMode can do this. Reusing it is what keeps
  // "one model per path" true, which Monaco requires: two models on one URI
  // throws.
  const orphan = monaco.editor.getModel(uri);
  const model = orphan ?? monaco.editor.createModel(text, language, uri);
  live.set(path, { model, viewState: null, language });
  return model;
}

/**
 * A Monaco URI for a workspace-relative path.
 *
 * Prefixed with a synthetic scheme rather than being a real `file://` URL. A
 * relative path is not an absolute file URL, and inventing one would make Monaco
 * believe it knows where the file is — which it does not, and which the
 * TypeScript worker would then use to resolve imports against the wrong root. A
 * neutral scheme keeps the URI purely an identity.
 */
function modelUri(path: string): string {
  // `/`-separated and encoded, because a path can contain a space or a `#` and
  // an unencoded one produces a URI that does not round-trip.
  const encoded = path
    .split("/")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return `loom-file:///${encoded}`;
}

/** Remembers where the cursor was, so a remount reopens where you left off. */
export function saveViewState(path: string, state: ViewState): void {
  const entry = live.get(path);
  if (entry) entry.viewState = state;
}

/** The saved view state for a path, or null. */
export function viewStateFor(path: string): ViewState {
  return live.get(path)?.viewState ?? null;
}

/**
 * Throws a buffer away. Called when a tab is actually closed, and nowhere else.
 *
 * Nothing unmounting is a reason to dispose a model — the same rule the terminal
 * follows, and for the same reason. A closed tab, on the other hand, is the user
 * saying they are done with it.
 */
export function dispose(path: string): void {
  const entry = live.get(path);
  if (!entry) return;
  entry.model.dispose();
  live.delete(path);
}

/** Every path with a live model, for diagnostics and for a "save all". */
export function paths(): string[] {
  return [...live.keys()];
}

/**
 * Moves a buffer to a new path, for a rename.
 *
 * The model's URI is not changed — Monaco cannot rebind one — so this is a
 * remove-and-recreate under the new name, carrying the text and the view state
 * across. The undo stack is lost, which is the honest cost of a rename: the
 * buffer is now a different file, and undoing into the old one would offer to
 * write the old content to a path that no longer has it.
 */
export function rename(
  monaco: Monaco,
  from: string,
  to: string,
  language: string,
): void {
  const entry = live.get(from);
  if (!entry) return;
  const text = entry.model.getValue();
  entry.model.dispose();
  live.delete(from);
  const model = monaco.editor.createModel(text, language, monaco.Uri.parse(modelUri(to)));
  live.set(to, { model, viewState: entry.viewState, language });
}

/** How many buffers are open. Cheap enough for a probe to read. */
export function size(): number {
  return live.size;
}

/**
 * Replaces a model's text without a Monaco instance.
 *
 * Exists for exactly one caller: the conflict banner's "Reload", which has to
 * put the file's new contents into a buffer the store does not hold the text of.
 * The store knows the path; the model — the only object holding that text — is
 * here. So this is the seam between them, in the same spirit as `saveAll`
 * reaching in for the text to write.
 *
 * A no-op when there is no model, which is the ordinary case for a tab that was
 * never mounted: the pane reads the file on mount anyway, so there is nothing to
 * update.
 */
export function setText(path: string, text: string): void {
  const entry = live.get(path);
  if (!entry) return;
  if (entry.model.getValue() === text) return;
  // `pushEditOperations` would make this one undoable step, and that is wrong
  // here: undoing back into the version that was just discarded would offer to
  // write text the user has already decided against.
  entry.model.setValue(text);
}

/** The current text of a buffer, or null when there is no model. */
export function textFor(path: string): string | null {
  return live.get(path)?.model.getValue() ?? null;
}
