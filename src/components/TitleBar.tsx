import { useEffect, useState, type ReactNode } from "react";
import { cn } from "../lib/cn";
import { LiquidSurface } from "./LiquidSurface";
import {
  closeWindow,
  isWindowMaximized,
  minimizeWindow,
  onWindowResized,
  toggleMaximizeWindow,
} from "../lib/window";
import { useChat } from "../stores/chat";
import { useDock } from "../stores/dock";
import { useSettings } from "../stores/settings";
import { useTasks } from "../stores/tasks";
import { PanelsMenu } from "./PanelsMenu";
import { useUi } from "../stores/ui";
import { PersonaMenu } from "./PersonaMenu";
import { WorkspaceChip } from "./WorkspaceChip";
import {
  CloseIcon,
  IdeIcon,
  MaximizeIcon,
  MinimizeIcon,
  PanelLeftIcon,
  PlusIcon,
  RestoreIcon,
  SettingsIcon,
} from "./icons";

function PillButton({
  label,
  onClick,
  danger,
  active,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  /** Whether the surface this button controls is currently on screen. */
  active?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "hover-surface grid h-8 w-9 place-items-center rounded-full text-soft",
        "focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]",
        // A panel that is already open should look open. Without this the
        // button's only feedback is a change somewhere else on screen, which is
        // exactly the kind of thing you have to hunt for.
        active && "bg-[var(--hover-bg)] text-[var(--ink)]",
        danger && "hover:!bg-[var(--danger)] hover:!text-white",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Four floating pills — Settings, then navigation on the left; app surfaces and
 * window controls on the right — over an invisible full-width drag strip.
 *
 * Settings is here rather than at the foot of the chats list, which is where it
 * used to be. That row was permanently reserved space at the bottom of a column
 * whose whole job is showing as many chats as possible, and it was only
 * reachable while the chats panel was open — so the one place you go to change
 * something was inside a panel you might not have open. A title-bar button is
 * always visible and costs the list nothing.
 */
export function TitleBar() {
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const settingsOpen = useUi((state) => state.settingsOpen);
  const openPanel = useDock((state) => state.openPanel);
  const toggleZone = useDock((state) => state.toggleZone);
  // The open zone whose visible tab is the chats list, if there is one. It
  // drives both the button's pressed state and what a click does — see below.
  // Selecting the zone object rather than a boolean is deliberate: a click has
  // to know *which* zone to toggle, and `find` returns the same reference for an
  // unchanged layout, so this does not re-render per frame.
  const chatsZone = useDock((state) =>
    state.layout.zones.find(
      (zone) => zone.open && zone.panels[zone.active] === "sessions",
    ),
  );
  const newSession = useChat((state) => state.newSession);
  const busyCount = useChat(
    (state) => Object.keys(state.busy).length,
  );
  const loadTasks = useTasks((state) => state.load);
  const ideMode = useSettings((state) => state.config.interface.ideMode);
  const setInterface = useSettings((state) => state.setInterface);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    void isWindowMaximized().then(setMaximized);
    return onWindowResized(setMaximized);
  }, []);

  // Loads runs once so the Panels menu can show what is available before the
  // dock has ever been opened.
  useEffect(() => {
    void loadTasks();
  }, [loadTasks]);

  return (
    <header className="chrome relative z-20 flex h-14 shrink-0 items-center justify-between px-3">
      <div className="absolute inset-0 -z-10" data-tauri-drag-region />

      <div className="flex items-center gap-2">
        {/* Left of the chats pill, so the left group reads settings-then-
            navigation. Its own capsule rather than a third icon inside the
            navigation pill: that pill is about moving around one conversation,
            and this opens a whole surface over everything. */}
        <LiquidSurface
          className="h-10 rounded-capsule"
          contentClassName="gap-0.5 p-1"
        >
          {/* Settings first, then IDE mode.
              The order is deliberate rather than incidental: Settings is the
              fixed point — it has been the leftmost control since the title bar
              was built, and muscle memory for it is the strongest in the app.
              IDE mode is the new one, so it goes *after* the anchor rather than
              displacing it. */}
          <PillButton
            label="Settings"
            active={settingsOpen}
            onClick={() => setSettingsOpen(true)}
          >
            <SettingsIcon size={17} />
          </PillButton>
          <PillButton
            label={ideMode ? "Leave IDE mode" : "IDE mode — editor, git and chat"}
            active={ideMode}
            onClick={() => setInterface({ ideMode: !ideMode })}
          >
            <IdeIcon size={17} />
          </PillButton>
        </LiquidSurface>
        <LiquidSurface
          className="h-10 rounded-capsule"
          contentClassName="gap-0.5 p-1"
        >
          {/* Opens the popup unless the dock is already holding the list, in
              which case it pops the zone out of the way — one button, and it
              always does the thing that shows you less of what is in the way. */}
          <PillButton
            label="Chats"
            active={Boolean(chatsZone)}
            onClick={() => {
              // One button, two directions, decided from what is actually on
              // screen rather than from a flag that could disagree with it: a
              // visible chats list means the click hides it, and anything else
              // means the click shows it. `openPanel` puts the sessions tab in
              // front when the left zone is already open on another panel, so
              // pressing this always ends with the list in front of you.
              if (chatsZone) toggleZone(chatsZone.id);
              else openPanel("sessions", "left");
            }}
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
        </LiquidSurface>
      </div>

      <div className="flex items-center gap-2">
        <LiquidSurface
          className="h-10 rounded-capsule"
          contentClassName="gap-0.5 p-1"
        >
          <PanelsMenu />
          <WorkspaceChip align="down" />
          <PersonaMenu align="down" />
        </LiquidSurface>
        <LiquidSurface
          className="h-10 rounded-capsule"
          contentClassName="gap-0.5 p-1"
        >
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
        </LiquidSurface>
      </div>
    </header>
  );
}
