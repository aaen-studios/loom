/**
 * Monaco, loaded on demand and dressed in Loom's own colours.
 *
 * ## Why this module exists at all
 *
 * Three things have to be true before an editor can be shown, and all three are
 * easy to get wrong in a way that fails *silently*:
 *
 * 1. **Workers.** Monaco does its tokenising, linting and TypeScript analysis in
 *    web workers. Without `self.MonacoEnvironment.getWorker` it does not error —
 *    it falls back to doing the work on the main thread and behaves as though
 *    everything is fine, only slower and with language features quietly missing.
 *    Under Loom's CSP (`script-src 'self'`) a worker loaded from a blob URL is
 *    refused, and the refusal is a console message nobody reads. So the workers
 *    are imported through Vite's `?worker` form, which emits real same-origin
 *    files, and `worker-src 'self' blob:` is declared in `tauri.conf.json`.
 *
 * 2. **The theme.** A stock Monaco theme inside Loom looks like a different
 *    application pasted into a panel, and a Loom-coloured theme with stock
 *    *tokens* would have no syntax colours worth reading. So the split is:
 *    **surfaces from Loom, tokens from Monaco.** The backgrounds, gutters,
 *    cursor, selection and diff washes are resolved from the same CSS variables
 *    everything else uses — so they track the palette, the theme toggle and a
 *    custom accent, automatically, since `applyTheme` re-runs rather than
 *    running once — while `vs`/`vs-dark` keep supplying the language colours,
 *    which are tuned far better than four hues derived from one accent.
 *
 *    See `readTheme` for why the token array is deliberately empty, and for the
 *    bug that made it empty the hard way.
 *
 * 3. **The bundle.** `monaco-editor` is around two megabytes of JavaScript. It is
 *    imported through `loadMonaco()` rather than at the top of a module, so a
 *    user who never opens the editor never downloads it.
 *
 * This mirrors `lib/terminals.ts`: the same reasoning about a large,
 * non-React-owned object, and the same conclusion.
 */

/** The subset of monaco's surface this app uses, so the import stays lazy. */
type Monaco = typeof import("monaco-editor");

let loading: Promise<Monaco> | null = null;

/**
 * Loads Monaco, once.
 *
 * Memoised on the promise rather than on the result, so two panels mounting in
 * the same tick join one import instead of racing two. `StrictMode` mounts every
 * effect twice, so this is not a hypothetical: without it, a development launch
 * downloads and instantiates Monaco twice.
 *
 * The failure here is worth naming, because it is the one that motivated the
 * `LOOM_MONACO_WORKER` probe: if the worker files cannot be fetched under the
 * CSP, Monaco still *resolves* and still renders — it just quietly loses
 * language features. A probe that only checks "did an editor appear" would pass
 * while the feature was broken.
 */
export function loadMonaco(): Promise<Monaco> {
  if (loading) return loading;
  loading = (async () => {
    // Ordered deliberately: `MonacoEnvironment` has to exist before Monaco is
    // imported, or the first tokenisation on the main thread wins and the
    // workers are never used.
    configureWorkers();
    const monaco = await import("monaco-editor");
    defineTheme(monaco);
    // Applied globally, not merely defined. Without this an editor created
    // without an explicit `theme` — which is every diff view, and any call site
    // added later — fell back to Monaco's stock light `vs`. A diff opened in
    // dark mode was a white editor with Loom's chrome around it.
    monaco.editor.setTheme(THEME_NAME);
    return monaco;
  })().catch((error) => {
    // Clear the memo so a retry is possible: a transient chunk failure should
    // not make the editor permanently unavailable for the session.
    loading = null;
    throw error;
  });
  return loading;
}

/**
 * Points Monaco at Vite-emitted worker files.
 *
 * `?worker` rather than `?worker&inline`: the inline form becomes a blob URL,
 * which is exactly what `script-src 'self'` refuses. Vite emits a real file for
 * each, and those are same-origin.
 *
 * Each language gets its own worker because that is how Monaco splits them — the
 * TypeScript worker is by far the largest and is only needed for JS/TS files.
 */
function configureWorkers(): void {
  const environment = globalThis as unknown as {
    MonacoEnvironment?: { getWorker: (id: string, label: string) => Worker };
  };
  if (environment.MonacoEnvironment?.getWorker) return;

  environment.MonacoEnvironment = {
    getWorker(_id: string, label: string) {
      // These are static imports inside a switch rather than a dynamic map,
      // because Vite has to see each specifier literally to emit the worker.
      switch (label) {
        case "json": {
          return new JsonWorker();
        }
        case "css":
        case "scss":
        case "less": {
          return new CssWorker();
        }
        case "html":
        case "handlebars":
        case "razor": {
          return new HtmlWorker();
        }
        case "typescript":
        case "javascript": {
          return new TsWorker();
        }
        default: {
          return new EditorWorker();
        }
      }
    },
  };
}

/* The workers, imported at module scope so Vite sees them.
 *
 * They are separate chunks, so importing them here does not pull them into the
 * entry bundle — they are fetched only when a worker is actually constructed,
 * which is the first time a file is opened.
 *
 * ## Why the specifier is `monaco-editor/editor/editor.worker` and not the
 *    `monaco-editor/esm/vs/editor/editor.worker` every guide shows
 *
 * Monaco 0.56 ships an `exports` map that already contains the `esm/vs` prefix
 * and the extension:
 *
 *     "./*.js": "./esm/vs/*.js",
 *     "./*":    "./esm/vs/*.js"
 *
 * so the long form — which was correct while the package had no `exports` field
 * and is still what almost every tutorial and Stack Overflow answer says — is
 * rewritten to `./esm/vs/esm/vs/editor/editor.worker.js`. That file does not
 * exist, and the resolver's error names the *original* specifier, so it reads as
 * "cannot resolve monaco-editor/esm/vs/editor/editor.worker" and looks like a
 * missing-extension problem. Adding `.js` does not help: `"./*.js"` then captures
 * the extensionless part and produces the same doubled path.
 *
 * The short form is what the map is designed for. This is worth the paragraph
 * because the failure is a *build* failure rather than a runtime one — the
 * opposite of the silent-worker problem documented at the top of this file — and
 * the two are easy to conflate when the message mentions a worker path. */
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import CssWorker from "monaco-editor/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/language/html/html.worker?worker";
import TsWorker from "monaco-editor/language/typescript/ts.worker?worker";

/** Loom's theme name inside Monaco, for `setTheme`. */
export const THEME_NAME = "loom";

/**
 * Resolves a CSS custom property to a hex colour Monaco will accept.
 *
 * Monaco's theme parser wants hex or a small set of named colours; it does not
 * accept `color-mix()` or arbitrary CSS. Loom's tokens are written with
 * `color-mix()` in several places, so reading the variable and passing it through
 * would produce a theme Monaco silently ignores and falls back to its default
 * for — the "every computed style looked correct and it rendered nothing" failure
 * this codebase already has a probe for, in the glass.
 *
 * The trick is to let the *browser* resolve it: assign the value to a real
 * element's `color` and read back `getComputedStyle().color`, which is always
 * `rgb(...)`. That handles `color-mix`, `oklch`, a custom property chain, and
 * anything else the platform understands, without this module having to know any
 * of it.
 */
function resolveColour(variable: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const probe = document.createElement("span");
  probe.style.color = "var(" + variable + ")";
  probe.style.display = "none";
  document.body.appendChild(probe);
  const computed = getComputedStyle(probe).color;
  probe.remove();

  // Alpha is captured, and preserving it is the whole point.
  //
  // An earlier version of this regex matched only the three channel numbers, so
  // `rgb(9 11 17 / 0.82)` and `rgb(142 162 255 / 0.16)` both came back fully
  // opaque. That is not a rounding error — it is the difference between a
  // sixteen-percent wash and a solid slab. `--hover-bg` is a few percent by
  // design, and painted opaque it covered the line it was meant to tint; the
  // visible symptom was a dark rectangle where the current line should have had
  // a barely-there highlight.
  //
  // Monaco accepts `#RRGGBBAA`, so the alpha survives as a fourth byte rather
  // than being approximated away.
  const match =
    /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?/.exec(
      computed,
    );
  if (!match) return fallback;

  const hex = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, "0");

  const body = `#${hex(Number(match[1]))}${hex(Number(match[2]))}${hex(Number(match[3]))}`;
  const raw = match[4];
  if (raw === undefined) return body;

  const alpha = raw.endsWith("%") ? Number(raw.slice(0, -1)) / 100 : Number(raw);
  if (!Number.isFinite(alpha) || alpha >= 1) return body;
  if (alpha <= 0) return `${body}00`;
  return `${body}${hex(alpha * 255)}`;
}

/** A theme built from Loom's live tokens. */
interface Theme {
  base: "vs" | "vs-dark";
  inherit: boolean;
  rules: { token: string; foreground?: string; fontStyle?: string }[];
  colors: Record<string, string>;
}

/** Derives a Monaco theme from the tokens currently on `<html>`. */
function readTheme(): Theme {
  const dark = document.documentElement.classList.contains("dark");

  const ink = resolveColour("--ink", dark ? "#f4f6fc" : "#141821");
  const soft = resolveColour("--ink-soft", dark ? "#c3c9d8" : "#414a5e");
  const faint = resolveColour("--ink-faint", dark ? "#7b8499" : "#7d859a");
  const accent = resolveColour("--accent", "#8ea2ff");
  const border = resolveColour("--glass-border", dark ? "#2a3040" : "#dcdfe8");
  // The editor sits on the panel surface rather than on the app background: it
  // is a reading surface, and code needs more contrast than artwork does.
  //
  // Resolved through `resolveColour` *with* its alpha, so this is the panel's
  // own 82% — the editor is translucent like every other surface, over a
  // container that is already blurred and tinted. That is deliberate: a solid
  // editor in a glass app reads as a rectangle pasted on top. The one place it
  // would bite is code over moving artwork, which is why `--panel-bg-strong` is
  // the token used and not `--panel-bg`.
  const surface = resolveColour("--panel-bg-strong", dark ? "#11141d" : "#ffffff");
  const gutter = resolveColour("--hover-bg", dark ? "#1b2030" : "#eef0f6");

  return {
    base: dark ? "vs-dark" : "vs",
    inherit: true,
    /*
     * No token rules. Two reasons, and the first one is a bug I shipped.
     *
     * **Monaco's token `foreground` must be hex *without* the `#`.** This array
     * used to carry fourteen rules built from `resolveColour`, which returns
     * `#rrggbb` — so every one of them was malformed and Monaco silently ignored
     * all fourteen. The symptom was an editor with no syntax colours at all and
     * no error anywhere: `# Loom` and `**Status:**` rendered in the same flat
     * grey as everything else.
     *
     * **And they would not have been good even had they parsed.** They were four
     * hues derived from one accent, which is not enough to colour a language
     * readably: keywords, types, strings, numbers and functions all landed on
     * either the accent or the ink, so a TypeScript file would have had about
     * two distinguishable colours. `vs` and `vs-dark` ship palettes tuned by
     * people who do this properly, and `inherit: true` above keeps them.
     *
     * So the tokens come from the base theme and this file only overrides the
     * *surfaces* — which is the part that has to match Loom, because that is
     * what the editor is sitting inside.
     */
    rules: [],
    colors: {
      "editor.background": surface,
      "editor.foreground": ink,
      "editorLineNumber.foreground": faint,
      "editorLineNumber.activeForeground": soft,
      "editorGutter.background": surface,
      "editor.lineHighlightBackground": gutter,
      "editor.selectionBackground": resolveColour("--accent-soft", gutter),
      "editorCursor.foreground": accent,
      "editorIndentGuide.background1": border,
      "editorIndentGuide.activeBackground1": faint,
      "editorWidget.background": surface,
      "editorWidget.border": border,
      "editorSuggestWidget.background": surface,
      "editorSuggestWidget.selectedBackground": gutter,
      "editorHoverWidget.background": surface,
      "editorHoverWidget.border": border,
      "editorBracketMatch.background": gutter,
      "editorBracketMatch.border": accent,
      "scrollbarSlider.background": border,
      "scrollbarSlider.hoverBackground": faint,
      "minimap.background": surface,
      // The diff view. Monaco's defaults are pure red and green, which sit wrong
      // against every palette in this app — the same complaint that made the
      // terminal's ANSI set Loom's own.
      "diffEditor.insertedTextBackground": dark ? "#1e3a2f66" : "#b7f0c966",
      "diffEditor.removedTextBackground": dark ? "#4a202966" : "#f9c2c866",
      "diffEditor.insertedLineBackground": dark ? "#16302330" : "#d6f5e030",
      "diffEditor.removedLineBackground": dark ? "#3a1a2030" : "#fbdfe230",
    },
  };
}

/** Defines (or redefines) Loom's theme from the live tokens. */
export function defineTheme(monaco: Monaco): void {
  monaco.editor.defineTheme(THEME_NAME, readTheme());
}

/**
 * Re-applies the theme, and tells Monaco to repaint.
 *
 * Called when the theme or the palette changes. A theme is read once at
 * definition, so without this an app-wide palette change would leave the editor
 * on the previous colours — the only surface in the app that did not follow.
 */
export function applyTheme(monaco: Monaco): void {
  defineTheme(monaco);
  monaco.editor.setTheme(THEME_NAME);
}

/** Monaco's editor options, from `config.editor`. */
export interface EditorOptions {
  fontSize: number;
  tabSize: number;
  wordWrap: "off" | "on";
  minimap: boolean;
  lineNumbers: boolean;
  renderWhitespace: boolean;
}

export function buildOptions(options: EditorOptions) {
  return {
    fontSize: options.fontSize,
    tabSize: options.tabSize,
    // A docked panel is 460px at its default width, which is about forty
    // characters of code. Wrapping is what makes the panel usable at the width
    // it is actually given, which is why it is on by default.
    wordWrap: options.wordWrap,
    minimap: { enabled: options.minimap },
    lineNumbers: options.lineNumbers ? ("on" as const) : ("off" as const),
    renderWhitespace: options.renderWhitespace ? ("all" as const) : ("none" as const),
    // The panel owns its own scrolling, and a nested scrollbar inside a docked
    // zone is a second thing to aim at.
    scrollBeyondLastLine: false,
    smoothScrolling: true,
    cursorBlinking: "smooth" as const,
    cursorSmoothCaretAnimation: "on" as const,
    // The app's own font stack: JetBrains Mono is bundled for the terminal, so
    // the two monospace surfaces read as one family.
    fontFamily: '"JetBrains Mono", ui-monospace, Consolas, monospace',
    fontLigatures: false,
    automaticLayout: false,
    // Find and replace stay on; the command palette does not. A palette inside
    // Loom would be a second Ctrl+Shift+P competing with the app's own keys,
    // and the panel is not an IDE enough to need one.
    quickSuggestions: { other: true, comments: false, strings: false },
    suggestSelection: "first" as const,
    tabCompletion: "on" as const,
    formatOnPaste: false,
    formatOnType: false,
    // Off, deliberately: Loom has its own autosave with a hash guard, and two
    // save mechanisms racing over one file is how work is lost.
    autoIndent: "full" as const,
    fixedOverflowWidgets: true,
    padding: { top: 8, bottom: 24 },
  };
}

/** Whether Monaco's chunks are already loaded, for a probe or a test. */
export function monacoLoaded(): boolean {
  return loading !== null;
}
