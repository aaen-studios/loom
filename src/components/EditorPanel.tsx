import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { isDiffPath, tabLabel } from "../lib/fileTreeOperations";
import { useEditor } from "../stores/editor";
import { useSettings } from "../stores/settings";
import { CloseIcon, FileIcon, GitDiffIcon } from "./icons";

/**
 * The editor's open files, as a tab strip.
 *
 * Registered as the `editor` panel's `tabStrip`, which is the seam the terminal
 * already uses: when the editor is **alone in its zone**, these tabs *become* the
 * zone's tabs — one row instead of two saying almost the same thing, and the
 * panel's close button sits where this row's would. With a second panel stacked
 * in the same zone the normal zone row returns and this one draws itself.
 *
 * That is why the layout here is a bare `flex-1` row rather than anything with
 * its own chrome: it has to look right in both places, and a second border under
 * the zone's own would show as a doubled line.
 */
export function EditorTabs({ standalone }: { standalone?: boolean }) {
  const tabs = useEditor((state) => state.tabs);
  const activePath = useEditor((state) => state.activePath);
  const setActive = useEditor((state) => state.setActive);
  const close = useEditor((state) => state.close);
  const closeOthers = useEditor((state) => state.closeOthers);
  const saveAll = useEditor((state) => state.saveAll);

  const stripRef = useRef<HTMLDivElement>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);

  // Keeps the active tab in view when a tab is opened or switched by keyboard.
  // `scrollIntoView` with `block: "nearest"` rather than `"center"`: centring
  // moves the whole strip when the tab is already visible, which reads as the
  // tabs jumping for no reason.
  useLayoutEffect(() => {
    if (!activePath) return;
    const node = stripRef.current?.querySelector<HTMLElement>(
      `[data-editor-tab="${CSS.escape(activePath)}"]`,
    );
    node?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activePath]);

  // A middle-click closes a tab, which is the habit everywhere else and costs
  // three lines. `onAuxClick` rather than `onMouseDown` so a stray middle-click
  // during a drag does not close something.
  return (
    <div className="flex min-w-0 flex-1 items-center gap-0.5">
      <div
        ref={stripRef}
        role="tablist"
        aria-label="Open files"
        className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto"
      >
        {tabs.map((tab) => {
          const active = tab.path === activePath;
          return (
            <div key={tab.path} className="group relative flex shrink-0 items-center">
              <button
                type="button"
                data-editor-tab={tab.path}
                role="tab"
                aria-selected={active}
                onClick={() => setActive(tab.path)}
                onAuxClick={(event) => {
                  if (event.button === 1) {
                    event.preventDefault();
                    close(tab.path);
                  }
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  setMenuFor(tab.path);
                }}
                title={tab.error ?? tab.path}
                className={cn(
                  "flex max-w-[190px] items-center gap-1.5 rounded-row py-1 pr-5 pl-2 text-[12px] transition-colors",
                  active
                    ? "bg-[var(--hover-bg)] text-[var(--ink)]"
                    : "text-faint hover:bg-[var(--hover-bg)] hover:text-soft",
                )}
              >
                {isDiffPath(tab.path) ? (
                  <GitDiffIcon size={12} className="shrink-0" />
                ) : (
                  <FileIcon size={12} className="shrink-0" />
                )}
                <span className="truncate">{tab.label}</span>
                {/* The dirty mark is a dot, not an asterisk in the label: the
                    label has to fit in 190px and a suffix shifts the text. */}
                {tab.dirty && !tab.conflict && (
                  <span
                    aria-label="Unsaved changes"
                    className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--accent)]"
                  />
                )}
                {tab.conflict && (
                  <span
                    aria-label="Changed on disk since you opened it"
                    className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
                  />
                )}
                {tab.error && (
                  <span
                    aria-label="This file could not be read"
                    className="text-[10px] text-[var(--danger)]"
                  >
                    !
                  </span>
                )}
              </button>
              <button
                type="button"
                aria-label={`Close ${tab.label}`}
                onClick={() => close(tab.path)}
                className="absolute right-0.5 grid h-5 w-5 place-items-center rounded-control text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-[var(--ink)]"
              >
                <CloseIcon size={11} />
              </button>
            </div>
          );
        })}
        {/* An empty strip, deliberately. It used to say "No file open", which
            then appeared twice — once here and once in the pane below, which
            says the same thing and adds where to find one. The pane is the
            right place for it: this row is chrome, and a sentence in a tab strip
            reads as a tab that failed to load. */}
      </div>

      {tabs.length > 0 && (
        <button
          type="button"
          onClick={() => void saveAll()}
          title="Save all"
          aria-label="Save all"
          className="shrink-0 rounded-control px-1.5 py-0.5 text-[10.5px] text-faint hover:text-[var(--accent)]"
        >
          save all
        </button>
      )}

      {menuFor && (
        <>
          <button
            type="button"
            aria-label="Close the tab menu"
            onClick={() => setMenuFor(null)}
            className="fixed inset-0 z-40 cursor-default"
          />
          <div className="panel-strong absolute top-full right-2 z-50 mt-1 w-[180px] rounded-sheet p-1">
            <MenuItem
              onClick={() => {
                close(menuFor);
                setMenuFor(null);
              }}
            >
              Close
            </MenuItem>
            <MenuItem
              onClick={() => {
                closeOthers(menuFor);
                setMenuFor(null);
              }}
            >
              Close others
            </MenuItem>
          </div>
        </>
      )}

      {/* `standalone` is set when this strip is the panel's own row rather than
          standing in for the zone's, which is only in a torn-off window. Nothing
          extra is drawn for it — the flag exists so the row keeps the same
          height in both arrangements, and a future addition has somewhere to
          branch. */}
      {standalone ? null : null}
    </div>
  );
}

function MenuItem({
  onClick,
  children,
}: {
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center rounded-row px-2 py-1 text-left text-[12.5px] text-soft hover:bg-[var(--hover-bg)]"
    >
      {children}
    </button>
  );
}

/**
 * The editor panel: tabs, then one Monaco pane for the active tab.
 *
 * ## One editor, many models
 *
 * There is a **single** Monaco instance, and switching tabs hands it a different
 * model. That is how Monaco is meant to be used, and it is also the only
 * arrangement that keeps a tab switch cheap: building an editor per tab would
 * mean N DOM trees, N sets of listeners, and a visible pause on every switch.
 *
 * The models themselves live in `lib/editors`, outside React, because this
 * component unmounts whenever the panel is not the active tab — see that file's
 * note. What this component owns is the *editor*, which is cheap to rebuild and
 * whose view state is saved and restored across a remount.
 */
export function EditorPanel({ workdir }: { workdir: string | null }) {
  const tabs = useEditor((state) => state.tabs);
  const activePath = useEditor((state) => state.activePath);
  const setWorkdir = useEditor((state) => state.setWorkdir);
  const active = tabs.find((tab) => tab.path === activePath) ?? null;

  useEffect(() => {
    setWorkdir(workdir);
  }, [setWorkdir, workdir]);

  if (!workdir) {
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[240px] text-center text-[12.5px] leading-5 text-faint">
          This chat has no workspace folder. Pick one from the workspace chip, then
          open a file from the tree.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {!active ? (
        <div className="grid h-full place-items-center p-4">
          <p className="max-w-[260px] text-center text-[12.5px] leading-5 text-faint">
            No file open. Pick one from the Files panel or the tree beside this
            one.
          </p>
        </div>
      ) : (
        <EditorPane tab={active} />
      )}
    </div>
  );
}

/**
 * The Monaco host for one tab.
 *
 * The editor is created once per *mount* and then re-pointed at a different
 * model as the active tab changes, rather than being torn down and rebuilt. That
 * matters more than it looks: disposing and recreating Monaco on every tab click
 * costs a full DOM teardown, and it loses the scroll position and the folding of
 * the tab you are coming back to.
 */
export function EditorPane({ tab }: { tab: import("../stores/editor").EditorTab }) {
  const holderRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<import("monaco-editor").editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof import("monaco-editor") | null>(null);
  const [ready, setReady] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const config = useSettings((state) => state.config.editor);
  const setDirty = useEditor((state) => state.setDirty);
  const save = useEditor((state) => state.save);
  const resolveConflict = useEditor((state) => state.resolveConflict);

  // A debounce timer per mount, reset on every keystroke. A ref rather than
  // state: it is a timer, not something the DOM depends on, and putting it in
  // state would re-render the pane on every character.
  const saveTimer = useRef<number | null>(null);

  /* ---------------------------------------------------------------------------
     Mount: load Monaco, create the editor, attach the model.
  --------------------------------------------------------------------------- */
  useEffect(() => {
    let live = true;
    setLoadError(null);

    void (async () => {
      try {
        const { loadMonaco } = await import("../lib/monaco");
        const monaco = await loadMonaco();
        if (!live || !holderRef.current) return;
        monacoRef.current = monaco;

        const editor = monaco.editor.create(holderRef.current, {
          ...buildOptionsFrom(config),
          // `automaticLayout` is off in the shared options; a ResizeObserver
          // here is the replacement, because `automaticLayout` polls with a
          // timer and this panel resizes on a splitter drag, which is exactly
          // when a poll is visibly laggy.
          automaticLayout: false,
          theme: "loom",
        });
        editorRef.current = editor;

        // Content changes mark the tab dirty and arm the autosave. This is the
        // *only* place the buffer's text enters the store's knowledge — the
        // store never holds the text, so this is a flag rather than a copy.
        editor.onDidChangeModelContent(() => {
          const path = currentPath();
          if (!path || isDiffPath(path)) return;
          setDirty(path, true);
          armAutosave();
        });

        // The autosave fires on the *pause*, not on the timer alone, so a long
        // uninterrupted burst of typing does not save mid-word.
        setReady(true);
      } catch (cause) {
        if (!live) return;
        setLoadError(cause instanceof Error ? cause.message : String(cause));
      }
    })();

    return () => {
      live = false;
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
      const editor = editorRef.current;
      if (editor) {
        // The view state is kept so a remount — a panel switch, or StrictMode in
        // development — reopens where the cursor was. The *editor* is disposed;
        // the models, which live in `lib/editors`, are not.
        const path = currentPath();
        if (path) {
          const view = editor.saveViewState();
          void import("../lib/editors").then((module) =>
            module.saveViewState(path, view),
          );
        }
        editor.dispose();
        editorRef.current = null;
      }
    };
    // Deliberately mounted once. `config` changes are handled by the effect
    // below, which updates options in place — recreating the editor on a font
    // size change would lose the scroll position.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** The path the editor is currently showing, read from the store. */
  function currentPath(): string | null {
    return useEditor.getState().activePath;
  }

  /** Arms the autosave for the active buffer. */
  function armAutosave() {
    if (!config.autosave) return;
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      const path = currentPath();
      if (!path || isDiffPath(path)) return;
      const tab = useEditor.getState().tabs.find((entry) => entry.path === path);
      if (!tab || !tab.dirty) return;
      // The text is read from the model at the moment of saving rather than
      // captured here, so a save is always of what is on screen.
      void import("../lib/editors").then((module) => {
        const model = module.modelFor(path);
        if (model) void save(path, model.getValue());
      });
    }, Math.max(250, config.autosaveDelayMs));
  }

  /* ---------------------------------------------------------------------------
     The active tab: point the editor at its model.
  --------------------------------------------------------------------------- */
  useEffect(() => {
    const monaco = monacoRef.current;
    const editor = editorRef.current;
    if (!monaco || !editor || !ready) return;

    if (isDiffPath(tab.path) || tab.error) {
      // A diff or an unreadable file has no model to show here. The diff has its
      // own component; an unreadable file shows the error instead.
      editor.setModel(null);
      return;
    }

    void (async () => {
      const module = await import("../lib/editors");
      // The text: from the file that was read, or from the model that already
      // exists — which is what makes a tab switch back to a dirty buffer show
      // the edits rather than the file on disk.
      const existing = module.modelFor(tab.path);
      const text = existing ? existing.getValue() : "";
      const placeholder = existing ? text : "";

      let body = placeholder;
      if (!existing) {
        const file = await ipc_read(tab.path);
        if (file === null) return;
        body = file;
      }

      const model = module.ensure(monaco, tab.path, body, tab.language);
      const view = module.viewStateFor(tab.path);
      editor.setModel(model);
      if (view) editor.restoreViewState(view);
      // Focus on arrival so typing works without a click — the common case for
      // opening a file is wanting to be in it.
      if (!isDiffPath(tab.path)) editor.focus();
    })();
  }, [tab.path, tab.language, tab.error, ready]);

  /* ---------------------------------------------------------------------------
     The file changed underneath.

     There is deliberately **no effect here**, and this note is what stops one
     being added back.

     An earlier version watched `tab.hash` and, for a clean buffer, re-read the
     file and pushed it into the model. It looked defensive and was actually
     harmful: three effects ended up writing the same model. The store read the
     file for its metadata, this pane read it again for the initial text, and
     that third effect read it a third time on every hash change. Three reads per
     open, on every save, and two independent writers whose ordering decided
     which text you saw.

     Nothing needs it. A file that changed on disk is detected by `recheck` — on
     `loom://fs`, on a finished tool call, on a pull — and the reload path already
     puts the new text into the model through `editors.setText`. That is the one
     writer, it knows *why* the text changed, and it distinguishes a dirty buffer
     (raise a conflict) from a clean one (adopt it) in a single place. A second
     writer here could only disagree with it.
  --------------------------------------------------------------------------- */

  /* ---------------------------------------------------------------------------
     Settings and theme changes, applied in place.
  --------------------------------------------------------------------------- */
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) return;
    editor.updateOptions(buildOptionsFrom(config));
  }, [config]);

  useEffect(() => {
    // The palette and the theme toggle both land as class or property changes on
    // `<html>`. Monaco reads a theme once at definition, so without this the
    // editor would be the one surface that did not follow a theme change.
    const apply = () => {
      void import("../lib/monaco").then((module) => {
        const monaco = monacoRef.current;
        if (monaco) module.applyTheme(monaco);
      });
    };
    const observer = new MutationObserver(apply);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "style"],
    });
    return () => observer.disconnect();
  }, []);

  /* ---------------------------------------------------------------------------
     A ResizeObserver, replacing `automaticLayout`.
  --------------------------------------------------------------------------- */
  useEffect(() => {
    const node = holderRef.current;
    if (!node) return;
    const observer = new ResizeObserver(() => {
      editorRef.current?.layout();
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  /** Ctrl+S / Cmd+S saves now. */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      if (event.key.toLowerCase() !== "s") return;
      event.preventDefault();
      const path = currentPath();
      if (!path || isDiffPath(path)) return;
      void (async () => {
        const module = await import("../lib/editors");
        const model = module.modelFor(path);
        if (model) void save(path, model.getValue());
      })();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // `save` is stable (a zustand action), so this attaches once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {tab.conflict && (
        <ConflictBanner
          tab={tab}
          onReload={() => void resolveConflict(tab.path, "reload")}
          onKeep={() => void resolveConflict(tab.path, "keep")}
        />
      )}

      {tab.readOnly && (
        <p className="shrink-0 border-b border-[var(--glass-border)] px-3 py-1 text-[11.5px] text-faint">
          Read-only.
        </p>
      )}

      {tab.lossy && (
        <p className="shrink-0 border-b border-[var(--glass-border)] px-3 py-1 text-[11.5px] text-faint">
          This file is not valid UTF-8, so it is shown with replacement characters.
          Saving would not preserve the original bytes.
        </p>
      )}

      {loadError ? (
        <div className="grid flex-1 place-items-center p-4">
          <p className="max-w-[320px] text-center text-[12.5px] leading-5 text-faint">
            The editor could not load. {loadError}
          </p>
        </div>
      ) : tab.error ? (
        <div className="grid flex-1 place-items-center p-4">
          <p className="max-w-[320px] text-center text-[12.5px] leading-5 text-faint">
            {tab.error}
          </p>
        </div>
      ) : (
        <div ref={holderRef} className="min-h-0 flex-1" />
      )}

      <StatusBar tab={tab} />
    </div>
  );
}

/**
 * What changed on disk while you had edits.
 *
 * The one thing an editor must never do is discarding unsaved keystrokes without
 * asking, so this is a question rather than a notification. `Reload` adopts the
 * file; `Keep mine` writes your version over it. There is deliberately no
 * "merge": a three-way merge needs a common ancestor, and the version you loaded
 * is not kept anywhere once you have typed over it.
 */
function ConflictBanner({
  tab,
  onReload,
  onKeep,
}: {
  tab: import("../stores/editor").EditorTab;
  onReload: () => void;
  onKeep: () => void;
}) {
  return (
    <div className="flex shrink-0 items-start gap-2 border-b border-[var(--danger)]/40 bg-[var(--danger)]/10 px-3 py-1.5">
      <span className="min-w-0 flex-1 text-[11.5px] leading-4">
        This file changed on disk
        {tab.conflictAt ? ` (${relativeWhen(tab.conflictAt)})` : ""} while you had
        unsaved edits. Reloading discards them; keeping yours writes over the new
        version.
      </span>
      <button
        type="button"
        onClick={onReload}
        className="shrink-0 rounded-control px-1.5 py-0.5 text-[11px] text-soft hover:text-[var(--ink)]"
      >
        Reload
      </button>
      <button
        type="button"
        onClick={onKeep}
        className="shrink-0 rounded-control px-1.5 py-0.5 text-[11px] font-medium text-[var(--danger)] hover:underline"
      >
        Keep mine
      </button>
    </div>
  );
}

/** A short "3 minutes ago", for the conflict banner. */
function relativeWhen(at: number): string {
  const seconds = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}

/**
 * The path, the language, the line ending, and whether it is saved.
 *
 * Deliberately not a cursor position. The obvious way to show one is
 * `onDidChangeCursorPosition` into `useState`, which repaints this bar on every
 * arrow key — and a status bar is not worth a re-render per keystroke. Monaco
 * already shows the line and column in its own scrollbar-adjacent UI when it
 * matters, and a stale number here would be worse than none.
 *
 * What it shows instead is the part the editor *cannot* infer from the buffer and
 * which is genuinely easy to get wrong: whether the file is CRLF, whether it has
 * a BOM, and whether your edits are on disk.
 */
function StatusBar({ tab }: { tab: import("../stores/editor").EditorTab }) {
  return (
    <div className="flex shrink-0 items-center gap-2 border-t border-[var(--glass-border)] px-3 py-1 text-[10.5px] text-faint">
      <span className="min-w-0 flex-1 truncate font-mono" title={tab.path}>
        {tab.path}
      </span>
      {tab.readOnly && <span className="shrink-0">read-only</span>}
      <span className="shrink-0">{tab.language}</span>
      {/* Both of these are silent-corruption risks — an editor that rewrites a
          file's line endings or drops its BOM has damaged it in a way a diff
          shows as every line changed. Saying which one is in play is the cheapest
          possible warning. */}
      <span className="shrink-0 uppercase">{tab.eol === "crlf" ? "CRLF" : "LF"}</span>
      {tab.bom && <span className="shrink-0">BOM</span>}
      {tab.conflict ? (
        <span className="shrink-0 text-[var(--danger)]">conflict</span>
      ) : tab.dirty ? (
        <span className="shrink-0 text-[var(--accent)]">unsaved</span>
      ) : (
        <span className="shrink-0">saved</span>
      )}
    </div>
  );
}

/** The shared options, from config. */
function buildOptionsFrom(config: import("../types").EditorConfig) {
  return {
    fontSize: config.fontSize,
    tabSize: config.tabSize,
    wordWrap: config.wordWrap as "off" | "on",
    minimap: { enabled: config.minimap },
    lineNumbers: (config.lineNumbers ? "on" : "off") as "on" | "off",
    renderWhitespace: (config.renderWhitespace ? "all" : "none") as "all" | "none",
    fontFamily: '"JetBrains Mono", ui-monospace, Consolas, monospace',
    scrollBeyondLastLine: false,
    smoothScrolling: true,
    cursorBlinking: "smooth" as const,
    padding: { top: 8, bottom: 24 },
  };
}

/** Reads a file's text, or null. Kept here so the effect above stays readable. */
async function ipc_read(path: string): Promise<string | null> {
  const { ipc } = await import("../lib/ipc");
  const { useEditor: store } = await import("../stores/editor");
  const file = await ipc.fileRead(store.getState().workdir, path);
  return file?.text ?? null;
}

/** Re-exported for a caller that only wants the label logic. */
export { tabLabel };
