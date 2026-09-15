import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles.css";

interface SetupInfo {
  payload: string | null;
  payloadBytes: number;
  defaultDir: string;
  currentVersion: string;
  installedVersion: string | null;
}

type Step = "welcome" | "installing" | "done" | "error";

function formatMb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function App() {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [dir, setDir] = useState("");
  const [desktop, setDesktop] = useState(true);
  const [step, setStep] = useState<Step>("welcome");
  const [message, setMessage] = useState("");

  useEffect(() => {
    void invoke<SetupInfo>("setup_info").then((result) => {
      setInfo(result);
      setDir(result.defaultDir);
    });
  }, []);

  const browse = async () => {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === "string") setDir(picked);
  };

  const install = async () => {
    setStep("installing");
    setMessage("Extracting files…");
    try {
      await invoke<string>("install", { dir, desktopShortcut: desktop });
      setStep("done");
    } catch (error) {
      setMessage(String(error));
      setStep("error");
    }
  };

  const launch = async () => {
    await invoke("launch_app", { dir });
    await getCurrentWindow().close();
  };

  return (
    <div className="drag flex h-full w-full flex-col p-3">
      <header className="flex items-center gap-2 px-2 py-1 text-[13px] text-[var(--ink-soft)]">
        <span className="font-semibold tracking-tight">Loom Setup</span>
        <span className="rounded-full border border-[var(--border)] px-1.5 py-0.5 text-[10.5px]">
          {info?.currentVersion ?? "…"}
        </span>
        <button
          type="button"
          onClick={() => void getCurrentWindow().close()}
          className="ml-auto grid h-6 w-7 place-items-center rounded-md text-[var(--ink-faint)] hover:bg-white/10 hover:text-[var(--ink)]"
          aria-label="Close"
        >
          ✕
        </button>
      </header>

      <main className="panel mt-2 flex min-h-0 flex-1 flex-col rounded-2xl p-6">
        {step === "welcome" && (
          <>
            <h1 className="text-[19px] font-semibold tracking-tight">
              {info?.installedVersion
                ? `Update Loom to 0.${info.currentVersion.split(".")[1]}`
                : "Install Loom"}
            </h1>
            <p className="mt-1 text-[13px] text-[var(--ink-soft)]">
              {info?.installedVersion
                ? `An existing install was found. Loom ${info.currentVersion} replaces it in place — chats and settings in ~/.loom are untouched.`
                : "Chat and agent workspace for Windows. Your data lives in ~/.loom."}
            </p>

            <div className="mt-5 space-y-3">
              <label className="block text-[12px] uppercase tracking-[0.08em] text-[var(--ink-faint)]">
                Install folder
              </label>
              <div className="flex gap-2">
                <input
                  value={dir}
                  onChange={(event) => setDir(event.currentTarget.value)}
                  className="min-w-0 flex-1 rounded-xl border border-[var(--border)] bg-white/5 px-3 py-2 font-mono text-[12px]"
                />
                <button
                  type="button"
                  onClick={() => void browse()}
                  className="rounded-xl border border-[var(--border)] px-3 py-2 text-[12.5px] text-[var(--ink-soft)] hover:text-[var(--ink)]"
                >
                  Browse…
                </button>
              </div>

              <label className="flex items-center gap-2 text-[13px] text-[var(--ink-soft)]">
                <input
                  type="checkbox"
                  checked={desktop}
                  onChange={(event) => setDesktop(event.currentTarget.checked)}
                />
                Create a desktop shortcut
              </label>

              {info && (
                <p className="text-[12px] text-[var(--ink-faint)]">
                  Payload: {info.payload ? formatMb(info.payloadBytes) : "missing"}
                </p>
              )}
            </div>

            <div className="mt-auto flex justify-end gap-2 pt-6">
              <button
                type="button"
                onClick={() => void getCurrentWindow().close()}
                className="rounded-xl border border-[var(--border)] px-4 py-2 text-[13px] text-[var(--ink-soft)]"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={!info?.payload}
                onClick={() => void install()}
                className="rounded-xl bg-[var(--control-bg)] px-4 py-2 text-[13px] font-medium text-[var(--control-ink)] disabled:opacity-40"
              >
                {info?.installedVersion ? "Update" : "Install"}
              </button>
            </div>
          </>
        )}

        {step === "installing" && (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <div className="h-1.5 w-56 overflow-hidden rounded-full bg-white/10">
              <div className="h-full w-2/3 animate-pulse rounded-full bg-[var(--accent)]" />
            </div>
            <p className="text-[13px] text-[var(--ink-soft)]">{message}</p>
          </div>
        )}

        {step === "done" && (
          <>
            <h1 className="text-[19px] font-semibold tracking-tight">
              Loom is installed
            </h1>
            <p className="mt-1 text-[13px] text-[var(--ink-soft)]">
              Install folder: <span className="font-mono text-[12px]">{dir}</span>
            </p>
            <div className="mt-auto flex justify-end gap-2 pt-6">
              <button
                type="button"
                onClick={() => void getCurrentWindow().close()}
                className="rounded-xl border border-[var(--border)] px-4 py-2 text-[13px] text-[var(--ink-soft)]"
              >
                Close
              </button>
              <button
                type="button"
                onClick={() => void launch()}
                className="rounded-xl bg-[var(--control-bg)] px-4 py-2 text-[13px] font-medium text-[var(--control-ink)]"
              >
                Launch Loom
              </button>
            </div>
          </>
        )}

        {step === "error" && (
          <>
            <h1 className="text-[19px] font-semibold tracking-tight text-[var(--danger)]">
              Install failed
            </h1>
            <p className="mt-2 whitespace-pre-wrap text-[12.5px] text-[var(--ink-soft)]">
              {message}
            </p>
            <div className="mt-auto flex justify-end pt-6">
              <button
                type="button"
                onClick={() => setStep("welcome")}
                className="rounded-xl border border-[var(--border)] px-4 py-2 text-[13px] text-[var(--ink-soft)]"
              >
                Back
              </button>
            </div>
          </>
        )}
      </main>
    </div>
  );
}
