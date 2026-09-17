import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { clipTop, useMenu } from "../lib/menu";
import { useDock } from "../stores/dock";
import { usePty } from "../stores/pty";
import { PANEL_LIST, iconFor, titleFor } from "../dock/registry";
import { CheckIcon, ChevronDownIcon, PanelLeftIcon } from "./icons";
import { Kbd } from "./ui";

/**
 * The dock's one control: a menu of every panel.
 *
 * This replaced an icon rail on the window edge and a pair of buttons for the
 * terminal and Runs. The rail was the part that read as clutter — a strip of
 * five glyphs floating over the artwork, permanently, for a dock that is usually
 * closed — and the pair of buttons could only ever cover two of six panels, so
 * Files, Goal and Browser were unreachable.
 *
 * One menu answers both: it is a single control in the chrome that already
 * exists, it lists every panel with its state, and it stays correct when a panel
 * is added. It follows the `ModeChip`/`WorkspaceChip` pattern deliberately —
 * measured drop direction, `panel-strong rounded-sheet`, capsulated trigger —
 * so the three menus in the title bar read as one family.
 */
export function PanelsMenu() {
  const layout = useDock((state) => state.layout);
  const openPanel = useDock((state) => state.openPanel);
  const closePanel = useDock((state) => state.closePanel);
  const collapseAll = useDock((state) => state.collapseAll);
  const profiles = usePty((state) => state.profiles);

  const containerRef = useRef<HTMLDivElement>(null);
  const { open, setOpen } = useMenu("panels", containerRef);
  const [drop, setDrop] = useState({ up: false, maxHeight: 480 });

  /** A panel is showing when its zone is open and its tab is the active one. */
  const showing = (id: string) => {
    const zone = layout.zones.find((entry) => entry.panels.includes(id));
    return Boolean(zone?.open && zone.panels[zone.active] === id);
  };
  /** Docked at all, whether or not it is the visible tab. */
  const docked = (id: string) =>
    layout.zones.some((zone) => zone.open && zone.panels.includes(id));

  const anyOpen = layout.zones.some((zone) => zone.open);

  const toggle = () => {
    if (open) {
      setOpen(false);
      return;
    }
    // Opens away from the title bar, which sits at the top of the window: the
    // room is always below it unless the window is tiny. Measured rather than
    // assumed so a very short window still gets a usable menu.
    const rect = containerRef.current?.getBoundingClientRect();
    const gap = 12;
    const above = (rect?.top ?? 0) - clipTop(containerRef.current) - gap;
    const below = window.innerHeight - (rect?.bottom ?? 0) - gap;
    const up = below < 220 && above > below;
    setDrop({
      up,
      maxHeight: Math.max(200, Math.min(560, (up ? above : below) - 4)),
    });
    setOpen(true);
  };

  // A menu that stays open across a keyboard toggle can end up describing a
  // layout that has since changed underneath it.
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  return (
    <div className="relative shrink-0" ref={containerRef}>
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        aria-haspopup="menu"
        title="Panels — the dock's terminal and side views"
        className={cn(
          "hover-surface flex h-8 items-center gap-1.5 rounded-full px-2.5 text-[12.5px]",
          anyOpen ? "text-soft" : "text-faint",
        )}
      >
        <PanelLeftIcon size={15} />
        <span>Panels</span>
        <ChevronDownIcon size={13} />
      </button>

      {open && (
        <div
          role="menu"
          className={cn(
            "panel-strong animate-fade-up absolute right-0 z-50 w-[264px] overflow-y-auto rounded-sheet p-1.5",
            drop.up ? "bottom-full mb-2" : "top-full mt-2",
          )}
          style={{ maxHeight: drop.maxHeight }}
        >
          <p className="px-2 pt-1 pb-1 text-[11px] font-semibold tracking-[0.08em] text-faint uppercase">
            Panels
          </p>

          {PANEL_LIST.map((def) => {
            const Icon = iconFor(def.id);
            const isShowing = showing(def.id);
            const isDocked = docked(def.id);
            return (
              <button
                key={def.id}
                type="button"
                role="menuitemcheckbox"
                aria-checked={isShowing}
                onClick={() => {
                  if (isShowing) {
                    const zone = layout.zones.find((entry) =>
                      entry.panels.includes(def.id),
                    );
                    if (zone) closePanel(zone.id, def.id);
                    return;
                  }
                  openPanel(def.id, def.edge);
                }}
                className={cn(
                  "hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px]",
                  isShowing ? "text-[var(--ink)]" : "text-soft",
                )}
              >
                <Icon size={15} className="shrink-0 text-faint" />
                <span className="min-w-0 flex-1 truncate">{def.title}</span>
                {/* A shell count, because "Terminal" alone does not say whether
                    losing the panel would lose one shell or four. */}
                {def.id === "terminal" && profiles.length > 0 && (
                  <span className="shrink-0 text-[10.5px] text-faint">
                    {profiles.length} available
                  </span>
                )}
                {isShowing ? (
                  <CheckIcon size={14} className="shrink-0 text-[var(--accent)]" />
                ) : (
                  isDocked && (
                    <span className="shrink-0 text-[10.5px] text-faint">docked</span>
                  )
                )}
              </button>
            );
          })}

          <div className="mt-1 border-t border-[var(--glass-border)] pt-1">
            <button
              type="button"
              disabled={!anyOpen}
              onClick={() => {
                setOpen(false);
                collapseAll();
              }}
              className={cn(
                "hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[13px]",
                anyOpen ? "text-soft" : "cursor-default text-faint",
              )}
            >
              <span className="flex-1">Close every panel</span>
              <Kbd>Ctrl+`</Kbd>
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** Re-exported so a torn-off window can title itself the same way. */
export function panelTitle(id: string): string {
  return titleFor(id);
}
