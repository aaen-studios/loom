import { cn } from "../lib/cn";
import { useChat } from "../stores/chat";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import { LoomMark, PlusIcon, SettingsIcon } from "./icons";

/**
 * Floating glass sidebar. M0: collapse behaviour, New chat (resets the local
 * stub conversation), and the Settings entry point. History/search land in M1
 * with real sessions.
 */
export function Sidebar() {
  const collapsed = useSettings((state) => state.config.sidebarCollapsed);
  const reset = useChat((state) => state.reset);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);

  return (
    <aside
      className={cn(
        "relative z-10 m-3 mr-0 w-[264px] shrink-0 overflow-hidden transition-[width,opacity,transform] duration-200 ease-out",
        collapsed && "pointer-events-none w-0 -translate-x-4 opacity-0",
      )}
      aria-hidden={collapsed}
    >
      <div className="glass flex h-full w-[264px] flex-col rounded-2xl p-2.5">
        <div className="flex items-center gap-2 px-1.5 py-1.5 text-soft">
          <LoomMark size={17} />
          <span className="text-[13.5px] font-semibold tracking-[0.01em]">
            Loom
          </span>
          <span className="ml-auto rounded-full border border-[var(--glass-border)] px-1.5 py-0.5 text-[10.5px] text-faint">
            M0
          </span>
        </div>

        <button
          type="button"
          onClick={reset}
          className="glass-hover mt-2 flex items-center gap-2 rounded-xl border border-[var(--glass-border)] px-3 py-2 text-left text-[13.5px] font-medium text-soft"
        >
          <PlusIcon size={16} />
          New chat
        </button>

        <div className="mt-4 min-h-0 flex-1 overflow-y-auto px-1.5">
          <p className="text-[13px] text-soft">No chats yet</p>
          <p className="mt-1 text-[12px] leading-5 text-faint">
            History, search, and real sessions arrive with the engine in M1.
          </p>
        </div>

        <div className="mt-2 border-t border-[var(--glass-border)] pt-2">
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            className="glass-hover flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-[13.5px] text-soft"
          >
            <SettingsIcon size={16} />
            Settings
          </button>
        </div>
      </div>
    </aside>
  );
}
