import { useEffect, useState, type ReactNode } from "react";
import { cn } from "../lib/cn";
import {
  closeWindow,
  isWindowMaximized,
  minimizeWindow,
  onWindowResized,
  toggleMaximizeWindow,
} from "../lib/window";
import { useChat } from "../stores/chat";
import { useUi } from "../stores/ui";
import {
  CloseIcon,
  MaximizeIcon,
  MinimizeIcon,
  PanelLeftIcon,
  PlusIcon,
  RestoreIcon,
} from "./icons";

function PillButton({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className={cn(
        "hover-surface grid h-8 w-9 place-items-center rounded-xl text-soft",
        "focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]",
        danger && "hover:!bg-[var(--danger)] hover:!text-white",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Two floating pills — navigation on the left, window controls on the right —
 * over an invisible full-width drag strip.
 */
export function TitleBar() {
  const setSidebarOpen = useUi((state) => state.setSidebarOpen);
  const newSession = useChat((state) => state.newSession);
  const busyCount = useChat(
    (state) => Object.keys(state.busy).length,
  );
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    void isWindowMaximized().then(setMaximized);
    return onWindowResized(setMaximized);
  }, []);

  return (
    <header className="chrome relative z-20 flex h-14 shrink-0 items-center justify-between px-3">
      <div className="absolute inset-0 -z-10" data-tauri-drag-region />

      <div className="flex items-center gap-2">
        <div className="pill flex h-10 items-center gap-0.5 rounded-2xl p-1">
          <PillButton
            label="Chats"
            onClick={() => setSidebarOpen(true)}
          >
            <span className="relative">
              <PanelLeftIcon size={17} />
              {busyCount > 0 && (
                <span className="absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full bg-[var(--accent)]" />
              )}
            </span>
          </PillButton>
          <PillButton label="New chat" onClick={() => void newSession()}>
            <PlusIcon size={17} />
          </PillButton>
        </div>
      </div>

      <div className="pill flex h-10 items-center gap-0.5 rounded-2xl p-1">
        <PillButton label="Minimize" onClick={() => void minimizeWindow()}>
          <MinimizeIcon size={16} />
        </PillButton>
        <PillButton
          label={maximized ? "Restore" : "Maximize"}
          onClick={() => void toggleMaximizeWindow()}
        >
          {maximized ? <RestoreIcon size={15} /> : <MaximizeIcon size={14} />}
        </PillButton>
        <PillButton label="Close" danger onClick={() => void closeWindow()}>
          <CloseIcon size={16} />
        </PillButton>
      </div>
    </header>
  );
}
