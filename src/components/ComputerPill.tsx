import { useEffect, useState } from "react";
import { subscribeTauri } from "../lib/listen";
import { ipc } from "../lib/ipc";

type PillState = "active" | "paused";

/**
 * The on-screen control indicator: while Loom drives the computer this pill
 * floats at the top of the primary monitor with Stop, and shows Resume while
 * a takeover pause is in effect.
 *
 * Its own clicks are ignored by the takeover detector and it never takes
 * focus, so pressing Resume or Stop neither counts as taking over nor pulls
 * focus away from the window Loom is driving.
 */
export function ComputerPill() {
  const [state, setState] = useState<PillState>("active");
  const [idleSeconds, setIdleSeconds] = useState(0);
  const [resumeInSeconds, setResumeInSeconds] = useState(30);
  const [takeoverError, setTakeoverError] = useState<string | null>(null);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.add("dark");
    root.style.background = "transparent";
    document.body.style.background = "transparent";

    let dispose: (() => void) | undefined;
    // Subscribed through the race-safe helper: `listen` returns a promise, and
    // StrictMode's cleanup runs before it resolves — so the plain shape attached
    // a second `loom://computer-state` listener.
    dispose = subscribeTauri<string>("loom://computer-state", (payload) => {
      setState(payload === "paused" ? "paused" : "active");
      setIdleSeconds(0);
    });

    const timer = window.setInterval(() => {
      void ipc.computerStatus().then((status) => {
        if (!status) return;
        // The status call reports the hooks as well as the pause, so it runs
        // even when the pill is hidden: "takeover detection is off" must be
        // visible from the first computer tool on, not only after a pause.
        setTakeoverError(status.takeoverActive ? null : status.takeoverError ?? "unavailable");
        if (typeof status.resumeInSeconds === "number") {
          setResumeInSeconds(status.resumeInSeconds);
        }
        if (status.state === "hidden") return;
        setState(status.state === "paused" ? "paused" : "active");
        setIdleSeconds(status.idleSeconds);
      });
    }, 1000);

    return () => {
      dispose?.();
      window.clearInterval(timer);
    };
  }, []);

  const resumeIn = Math.max(0, resumeInSeconds - idleSeconds);

  return (
    <div className="flex h-full w-full items-center justify-center p-1">
      <div className="flex w-full items-center gap-2.5 rounded-full border border-[var(--glass-border)] bg-[color-mix(in_srgb,var(--panel)_88%,transparent)] px-3.5 py-2 shadow-lg backdrop-blur">
        <span
          className={
            state === "paused"
              ? "h-2.5 w-2.5 shrink-0 rounded-full bg-[var(--warning,#f59e0b)]"
              : "h-2.5 w-2.5 shrink-0 animate-pulse rounded-full bg-[var(--danger)]"
          }
        />
        <span className="min-w-0 flex-1 truncate text-[12.5px] text-soft">
          {takeoverError
            ? "Loom can't tell when you take over — use Stop"
            : state === "paused"
              ? `Paused — you're in control · resumes in ${resumeIn}s`
              : "Loom is controlling your computer"}
        </span>
        {state === "paused" ? (
          <button
            type="button"
            onClick={() => void ipc.resumeComputer()}
            className="shrink-0 rounded-full bg-[var(--control-bg)] px-3 py-1 text-[12px] font-medium text-[var(--control-ink)]"
          >
            Resume
          </button>
        ) : (
          <span className="shrink-0 text-[11px] text-faint">Ctrl+Alt+Esc</span>
        )}
        <button
          type="button"
          onClick={() => void ipc.stopComputer()}
          className="shrink-0 rounded-full border border-[var(--glass-border)] px-3 py-1 text-[12px] text-soft hover:text-[var(--danger)]"
        >
          Stop
        </button>
      </div>
    </div>
  );
}
