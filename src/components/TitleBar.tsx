import { useEffect, useState, type ReactNode } from "react";
import { cn } from "../lib/cn";
import {
  closeWindow,
  isWindowMaximized,
  minimizeWindow,
  onWindowResized,
  toggleMaximizeWindow,
} from "../lib/window";
import { useSettings } from "../stores/settings";
import {
  CloseIcon,
  MaximizeIcon,
  MinimizeIcon,
  PanelLeftIcon,
  RestoreIcon,
} from "./icons";

function TitleButton({
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
        "glass-hover grid h-8 w-9 place-items-center rounded-lg text-soft",
        "focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]",
        danger && "hover:!bg-[var(--danger)] hover:!text-white",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Floating glass titlebar. Frameless window, so all chrome (drag region,
 * controls) lives here. Windows-style glyphs per the M0 design decision.
 */
export function TitleBar() {
  const toggleSidebar = useSettings((state) => state.toggleSidebar);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    void isWindowMaximized().then(setMaximized);
    return onWindowResized(setMaximized);
  }, []);

  return (
    <header className="chrome pointer-events-none absolute inset-x-0 top-0 z-30 flex px-3 pt-3">
      <div
        className="glass pointer-events-auto flex h-11 w-full items-center gap-1 rounded-2xl px-1.5"
        data-tauri-drag-region
      >
        <button
          type="button"
          title="Toggle sidebar"
          aria-label="Toggle sidebar"
          onClick={toggleSidebar}
          className="glass-hover grid h-8 w-9 place-items-center rounded-lg text-soft focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
        >
          <PanelLeftIcon size={17} />
        </button>

        <div className="h-full flex-1" data-tauri-drag-region />

        <div className="flex items-center gap-0.5">
          <TitleButton label="Minimize" onClick={() => void minimizeWindow()}>
            <MinimizeIcon size={16} />
          </TitleButton>
          <TitleButton
            label={maximized ? "Restore" : "Maximize"}
            onClick={() => void toggleMaximizeWindow()}
          >
            {maximized ? <RestoreIcon size={15} /> : <MaximizeIcon size={14} />}
          </TitleButton>
          <TitleButton label="Close" danger onClick={() => void closeWindow()}>
            <CloseIcon size={16} />
          </TitleButton>
        </div>
      </div>
    </header>
  );
}
