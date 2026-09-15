import { cn } from "../lib/cn";
import { useUi } from "../stores/ui";
import { ArrowUpIcon, CloseIcon } from "./icons";

/**
 * Bottom-center notice for a launch-time update check. It only points at
 * Settings → Updates; downloading stays an explicit action.
 */
export function UpdateToast() {
  const update = useUi((state) => state.availableUpdate);
  const setUpdate = useUi((state) => state.setAvailableUpdate);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);

  if (!update) return null;

  return (
    <div
      className={cn(
        "panel-strong animate-fade-up absolute bottom-4 left-1/2 z-40 flex -translate-x-1/2 items-center gap-3 rounded-2xl px-3.5 py-2.5",
      )}
    >
      <span className="grid h-7 w-7 place-items-center rounded-full bg-[var(--accent-soft)] text-[var(--accent)]">
        <ArrowUpIcon size={15} />
      </span>
      <span className="text-[13px]">
        Loom <span className="font-semibold">{update.version}</span> is available
      </span>
      <button
        type="button"
        onClick={() => {
          setSettingsOpen(true);
          setUpdate(null);
        }}
        className="rounded-full bg-[var(--control-bg)] px-3 py-1 text-[12.5px] font-medium text-[var(--control-ink)]"
      >
        Update
      </button>
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => setUpdate(null)}
        className="text-faint hover:text-[var(--ink)]"
      >
        <CloseIcon size={14} />
      </button>
    </div>
  );
}
