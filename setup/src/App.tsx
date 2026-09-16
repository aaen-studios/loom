import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles.css";

interface SetupInfo {
  payload: string | null;
  payloadBytes: number;
  defaultDir: string;
  currentVersion: string;
  installedVersion: string | null;
  autostartEnabled: boolean;
}

interface SetupProgress {
  phase: string;
  file: string | null;
  done: number;
  total: number;
}

type Step = "welcome" | "installing" | "done" | "error";

function formatMb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

/** The app's mark: two warp threads over a weft. */
function LoomMark({ size = 18, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      className={className}
      aria-hidden="true"
    >
      <path d="M6.5 5.5c0 6.5 5.5 6.5 5.5 13" />
      <path d="M12 5.5c0 6.5 5.5 6.5 5.5 13" />
      <path d="M6.5 18.5h11" opacity="0.55" />
    </svg>
  );
}

function CheckIcon({ size = 18, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d="M5 12.5l4.5 4.5L19 7.5" />
    </svg>
  );
}

/** The app's switch, for the install options. */
function Switch({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        "mt-0.5 flex h-5 w-9 shrink-0 items-center rounded-capsule border p-0.5 transition-colors",
        checked
          ? "border-transparent bg-[var(--accent)]"
          : "border-[var(--border)] bg-[var(--ink-ghost)]",
      )}
    >
      <span
        className={cn(
          "h-3.5 w-3.5 rounded-capsule transition-transform",
          checked ? "translate-x-4 bg-white" : "translate-x-0 bg-[var(--ink-faint)]",
        )}
      />
    </button>
  );
}

function Choice({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4 px-1.5 py-2.5">
      <span className="min-w-0">
        <span className="block text-[13px] text-soft">{label}</span>
        <span className="block text-[11.5px] leading-4 text-faint">{hint}</span>
      </span>
      <Switch checked={checked} onChange={onChange} label={label} />
    </div>
  );
}

export default function App() {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [dir, setDir] = useState("");
  const [desktop, setDesktop] = useState(true);
  const [startAtLogin, setStartAtLogin] = useState(true);
  const [progress, setProgress] = useState(0);
  const [phase, setPhase] = useState("");
  const [file, setFile] = useState<string | null>(null);
  const [step, setStep] = useState<Step>("welcome");
  const [message, setMessage] = useState("");
  const [launchError, setLaunchError] = useState("");

  useEffect(() => {
    void invoke<SetupInfo>("setup_info").then((result) => {
      setInfo(result);
      setDir(result.defaultDir);
      // Fresh installs default to on; updates keep whatever is registered.
      setStartAtLogin(
        result.installedVersion ? result.autostartEnabled : true,
      );
    });
  }, []);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen<SetupProgress>("setup://progress", (event) => {
      const { phase, file, done, total } = event.payload;
      setPhase(phase);
      setFile(file);
      if (phase === "extracting" && total > 0) {
        setProgress(Math.min(1, done / total));
      }
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose?.();
  }, []);

  const browse = async () => {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === "string") setDir(picked);
  };

  const installing = step === "installing";

  const install = useCallback(async () => {
    setStep("installing");
    setProgress(0);
    setPhase("");
    setFile(null);
    setLaunchError("");
    try {
      const installed = await invoke<string>("install", {
        dir,
        desktopShortcut: desktop,
        startAtLogin,
      });
      setDir(installed);
      setStep("done");
    } catch (error) {
      setMessage(String(error));
      setStep("error");
    }
  }, [dir, desktop, startAtLogin]);

  const launch = useCallback(async () => {
    setLaunchError("");
    try {
      await invoke("launch_app", { dir });
    } catch (error) {
      setLaunchError(String(error));
      return;
    }
    await getCurrentWindow().close();
  }, [dir]);

  // A half-finished extract is not something to discover on next launch, so
  // the window refuses to close while it runs.
  useEffect(() => {
    if (!installing) return;
    let dispose: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => event.preventDefault())
      .then((unlisten) => {
        dispose = unlisten;
      });
    return () => dispose?.();
  }, [installing]);

  // Enter takes the primary action, Esc closes — the way a Windows dialog
  // behaves. Enter is left alone when a button has focus: its own click would
  // fire too, and the action would run twice.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (installing) return;
      if (event.key === "Escape") {
        void getCurrentWindow().close();
        return;
      }
      if (event.key !== "Enter") return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("button")) return;
      if (step === "welcome" && info?.payload) void install();
      else if (step === "error") void install();
      else if (step === "done") void launch();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [installing, step, info, install, launch]);

  const percent = Math.round(progress * 100);
  // Phases with no measurable bytes: the bar sweeps instead of pretending.
  const busy =
    phase === "" ||
    phase === "closing" ||
    phase === "shortcuts" ||
    phase === "registering";
  const status =
    phase === "extracting"
      ? `Extracting${file ? ` ${file}` : ""} · ${percent}%`
      : phase === "closing"
        ? "Closing the running Loom…"
        : phase === "shortcuts"
          ? "Creating shortcuts…"
          : phase === "registering"
            ? "Registering with Windows…"
            : "Installing…";

  const installedVersion = info?.installedVersion ?? null;
  const reinstall =
    installedVersion !== null && installedVersion === info?.currentVersion;

  return (
    <div className="flex h-full w-full flex-col bg-[var(--surface)]">
      <header className="drag flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-4 py-3">
        <LoomMark size={15} className="text-[var(--accent)]" />
        <span className="text-[13px] font-semibold tracking-tight">Loom Setup</span>
        <span className="rounded-capsule border border-[var(--border)] px-2 py-0.5 text-[10.5px] text-faint">
          {info?.currentVersion ?? "…"}
        </span>
        <button
          type="button"
          disabled={installing}
          onClick={() => void getCurrentWindow().close()}
          className="no-drag ml-auto grid h-6 w-7 place-items-center rounded-control text-faint transition-colors hover:bg-[var(--hover)] hover:text-[var(--ink)] disabled:pointer-events-none disabled:opacity-30"
          aria-label="Close"
        >
          ✕
        </button>
      </header>

      <main className="flex min-h-0 flex-1 flex-col p-4">
        <div className="panel flex min-h-0 flex-1 flex-col overflow-hidden rounded-sheet p-5">
          {step === "welcome" && (
            <div className="animate-fade-up flex min-h-0 flex-1 flex-col">
              <div className="flex items-start gap-3">
                <span className="grid h-9 w-9 shrink-0 place-items-center rounded-control bg-[var(--accent-soft)] text-[var(--accent)]">
                  <LoomMark size={18} />
                </span>
                <span className="min-w-0">
                  <h1 className="text-[17px] font-semibold tracking-tight">
                    {reinstall
                      ? `Reinstall Loom ${info?.currentVersion}`
                      : installedVersion
                        ? `Update Loom to ${info?.currentVersion}`
                        : "Install Loom"}
                  </h1>
                  <p className="mt-0.5 text-[12.5px] leading-4 text-soft">
                    {installedVersion
                      ? "The existing copy is replaced in place — chats and settings in ~/.loom are untouched."
                      : "Chat and agent workspace for Windows. Your data lives in ~/.loom."}
                  </p>
                </span>
              </div>

              <div className="mt-5">
                <label className="block px-1 text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
                  Install folder
                </label>
                <div className="mt-1.5 flex gap-2">
                  <input
                    value={dir}
                    spellCheck={false}
                    onChange={(event) => setDir(event.currentTarget.value)}
                    className="field min-w-0 flex-1 font-mono text-[12px]"
                  />
                  <button
                    type="button"
                    onClick={() => void browse()}
                    className="btn-ghost shrink-0 px-3 py-2 text-[12.5px]"
                  >
                    Browse…
                  </button>
                </div>
              </div>

              <div className="mt-3.5 overflow-hidden rounded-sheet border border-[var(--border)] px-1.5">
                <Choice
                  label="Create a desktop shortcut"
                  hint="Puts a Loom shortcut on your desktop."
                  checked={desktop}
                  onChange={setDesktop}
                />
                <div className="mx-1.5 h-px bg-[var(--border)]" />
                <Choice
                  label="Start Loom when you sign in"
                  hint="Windows launches Loom automatically at login."
                  checked={startAtLogin}
                  onChange={setStartAtLogin}
                />
              </div>

              <div className="mt-auto flex items-center justify-end gap-2 pt-5">
                <span className="mr-auto text-[11.5px] text-faint">
                  {info
                    ? info.payload
                      ? `Payload ${formatMb(info.payloadBytes)}`
                      : "Payload missing"
                    : ""}
                </span>
                <button
                  type="button"
                  onClick={() => void getCurrentWindow().close()}
                  className="btn-ghost px-4 py-2 text-[13px]"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={!info?.payload}
                  onClick={() => void install()}
                  className="btn-primary px-4 py-2 text-[13px]"
                >
                  {reinstall ? "Reinstall" : installedVersion ? "Update" : "Install"}
                </button>
              </div>
            </div>
          )}

          {step === "installing" && (
            <div className="animate-fade-up flex flex-1 flex-col items-center justify-center gap-3">
              <div className="relative h-1.5 w-56 overflow-hidden rounded-full bg-[var(--ink-ghost)]">
                <div
                  className="h-full rounded-full bg-[var(--accent)] transition-[width] duration-150"
                  style={{ width: `${percent}%` }}
                />
                {busy && (
                  <span className="bar-sweep absolute inset-y-0 w-1/3 rounded-full bg-white/50" />
                )}
              </div>
              <p className="text-[13px] text-soft">{status}</p>
              <p className="text-[11.5px] text-faint">
                A running Loom is closed so its files can be replaced.
              </p>
            </div>
          )}

          {step === "done" && (
            <div className="animate-fade-up flex min-h-0 flex-1 flex-col">
              <div className="flex flex-1 items-center">
                <div className="flex items-start gap-3">
                  <span className="grid h-9 w-9 shrink-0 place-items-center rounded-control bg-[var(--accent-soft)] text-[var(--accent)]">
                    <CheckIcon size={18} />
                  </span>
                  <span className="min-w-0">
                    <h1 className="text-[17px] font-semibold tracking-tight">
                      Loom {info?.currentVersion} is installed
                    </h1>
                    <p className="mt-0.5 text-[12.5px] leading-5 text-soft">
                      Install folder:{" "}
                      <span className="font-mono text-[12px]">{dir}</span>
                    </p>
                  </span>
                </div>
              </div>
              <div className="flex items-center justify-end gap-3">
                {launchError && (
                  <p className="mr-auto text-[11.5px] leading-4 text-[var(--danger)]">
                    {launchError}
                  </p>
                )}
                <button
                  type="button"
                  onClick={() => void getCurrentWindow().close()}
                  className="btn-ghost px-4 py-2 text-[13px]"
                >
                  Close
                </button>
                <button
                  type="button"
                  onClick={() => void launch()}
                  className="btn-primary px-4 py-2 text-[13px]"
                >
                  Launch Loom
                </button>
              </div>
            </div>
          )}

          {step === "error" && (
            <div className="animate-fade-up flex min-h-0 flex-1 flex-col">
              <div className="flex flex-1 flex-col justify-center">
                <h1 className="text-[17px] font-semibold tracking-tight text-[var(--danger)]">
                  Install failed
                </h1>
                <p className="mt-2 max-h-56 overflow-y-auto text-[12.5px] leading-5 break-words whitespace-pre-wrap text-soft">
                  {message}
                </p>
              </div>
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setStep("welcome")}
                  className="btn-ghost px-4 py-2 text-[13px]"
                >
                  Back
                </button>
                <button
                  type="button"
                  onClick={() => void install()}
                  className="btn-primary px-4 py-2 text-[13px]"
                >
                  Try again
                </button>
              </div>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
