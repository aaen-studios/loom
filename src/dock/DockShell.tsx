import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { cn } from "../lib/cn";
import {
  BOTTOM_SHARE,
  DRAG_THRESHOLD_PX,
  ZONE_SIZE,
  clampZoneSize,
  edgeAt,
  isVerticalEdge,
  layoutSize,
  type ZoneRegion,
} from "../lib/dockGeometry";
import { ipc } from "../lib/ipc";
import { useChat } from "../stores/chat";
import { useDock } from "../stores/dock";
import { targetKey, targetLabel, useDockDrag, type DropTarget } from "../stores/dockDrag";
import { usePty } from "../stores/pty";
import type { DockEdge, DockZone } from "../types";
import { CloseIcon, PopOutIcon } from "../components/icons";
import { StripRenderedProvider, iconFor, panelDef, titleFor } from "./registry";
import { useTerminalAppearance } from "../components/TerminalPanel";

/**
 * The dock: zones on the window's edges, each a tab stack of panels.
 *
 * The arrangement comes from the store, which projects the backend's copy (see
 * `stores/dock.ts` for why Rust owns it). Nothing here decides *what* a panel is
 * — that is `registry.tsx` — so this component is only geometry.
 *
 * ---
 *
 * **Side zones overlay; the bottom zone takes height.**
 *
 * Panels used to take their width *from* the chat, which meant a splitter drag
 * had to answer "what happens when the chat runs out of room" — and every
 * answer moved it: clamping made the drag stop short of the pointer, collapsing
 * to a spine replaced the transcript with a button, and because the chat panel
 * is centred, squeezing it shrank it from both sides so its left edge slid
 * right while the right edge slid left. Three symptoms, one cause: the chat's
 * *width* was being decided by something else.
 *
 * So no zone touches the chat's width. A zone on the left or right edge is
 * **fixed size and overlaid**, and the transcript underneath is not reflowed by
 * it. The cost is honest and worth paying: a wide column over a narrow window
 * covers part of the transcript, but covered content is still there and still
 * scrolls, whereas text that reflowed is text you have to find again.
 *
 * The bottom edge is the exception, and deliberately so. A bottom dock is for
 * watching something — a terminal, a run — *while* you read, so hiding the
 * lower third of the transcript behind it defeats the purpose. It takes height
 * in flow instead and pushes the chat up, which shortens the transcript without
 * moving a single word sideways. Height is the axis nothing else is competing
 * for, which is exactly why it is safe to reflow there and not across.
 */

interface Viewport {
  width: number;
  height: number;
}

/**
 * The dock region's own size, measured rather than assumed.
 *
 * Needed for pointer hit-testing: a drop target is resolved from where the
 * pointer is inside this region, not inside the window. The region excludes the
 * title bar, so measuring against `window.innerHeight` would put the bottom
 * band ~56px off.
 */
function useRegionSize(ref: React.RefObject<HTMLElement | null>): Viewport {
  const [size, setSize] = useState<Viewport>({ width: 0, height: 0 });
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(() => {
      const rect = node.getBoundingClientRect();
      setSize({ width: rect.width, height: rect.height });
    });
    observer.observe(node);
    const rect = node.getBoundingClientRect();
    setSize({ width: rect.width, height: rect.height });
    return () => observer.disconnect();
  }, [ref]);
  return size;
}

export function DockHost({ children }: { children: ReactNode }) {
  const rootRef = useRef<HTMLDivElement>(null);
  const workdir = useChat(
    (state) => state.sessions.find((item) => item.id === state.activeId)?.workdir ?? null,
  );
  const layout = useDock((state) => state.layout);
  const load = useDock((state) => state.load);
  const loadProfiles = usePty((state) => state.loadProfiles);
  const size = useRegionSize(rootRef);

  // Restyles every live terminal when the appearance settings change. Called
  // here rather than in the terminal itself, because a terminal whose panel is
  // closed still has to pick up a font or palette change.
  useTerminalAppearance();

  useEffect(() => {
    void load(workdir);
  }, [load, workdir]);

  // Probing for shells spawns `wsl.exe`, so it happens once for the app rather
  // than once per render of a panel.
  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  const open = layout.zones.filter((zone) => zone.open && zone.panels.length > 0);
  const side = open.filter((zone) => isVerticalEdge(zone.edge));
  const bottom = open.filter((zone) => !isVerticalEdge(zone.edge));

  return (
    // A column, so the chat region and any bottom zone can be siblings in flow
    // while the side zones stay laid over the region rather than beside it.
    <div ref={rootRef} className="relative flex min-h-0 min-w-0 flex-1 flex-col">
      {/* The chat region: the base layer, plus the side zones on top of it. The
          chat is never given a width derived from the dock, so opening a column
          cannot move it.

          `isolate` keeps the panels' stacking contexts from escaping into the
          window chrome above. */}
      <div className="relative isolate flex min-h-0 min-w-0 flex-1">
        {children}
        {side.map((zone) => (
          <ZoneView key={zone.id} zone={zone} workdir={workdir} region={size} />
        ))}
      </div>

      {/* In flow, not overlaid — see the note at the top of this file. The chat
          region above is `flex-1`, so it gives up exactly the height this takes
          and the transcript ends up shorter rather than partly hidden. */}
      {bottom.map((zone) => (
        <ZoneView key={zone.id} zone={zone} workdir={workdir} region={size} />
      ))}

      <DragLayer rootRef={rootRef} size={size} />
    </div>
  );
}

/* ---------------------------------------------------------------------------
   A zone
--------------------------------------------------------------------------- */

/**
 * How a zone is laid out: side zones overlaid, the bottom one in flow.
 *
 * The asymmetry is the whole point and is explained at the top of this file —
 * a column is something you consult, so covering part of the transcript costs
 * little, while a bottom dock is for watching something *while* you read, so it
 * has to give the chat less room rather than hide a strip of it.
 */
function placement(
  zone: DockZone,
  region: ZoneRegion,
): { className: string; style: React.CSSProperties } {
  // `layoutSize`, not `ZONE_SIZE`. `ZONE_SIZE` is the *starting* size a zone is
  // created with; the stored size is what a resize drag writes, and it is what
  // has to be laid out. This previously read `ZONE_SIZE[zone.edge] ?? zone.size`,
  // and because `ZONE_SIZE` has an entry for every edge that fallback could
  // never fire — so a drag would have written a value nothing ever read.
  const size = layoutSize(zone, region);
  if (zone.edge === "left") {
    return {
      className: "absolute top-0 bottom-0 left-0",
      style: { width: size },
    };
  }
  if (zone.edge === "right") {
    return {
      className: "absolute top-0 right-0 bottom-0",
      style: { width: size },
    };
  }
  return {
    className: "relative shrink-0",
    // The same ceiling `clampZoneSize` enforces, expressed as CSS as well, so a
    // stale size arriving mid-drag cannot briefly exceed it. See `BOTTOM_SHARE`
    // for why the cap exists: the region above holds the composer, and the
    // composer is how you ask a panel to close.
    style: { height: size, maxHeight: `${BOTTOM_SHARE * 100}%` },
  };
}

function ZoneView({
  zone,
  workdir,
  region,
}: {
  zone: DockZone;
  workdir: string | null;
  region: ZoneRegion;
}) {
  const active = zone.panels[zone.active] ?? null;
  const { className, style } = placement(zone, region);

  /**
   * Whether this panel's own tab strip is standing in for the zone's.
   *
   * Decided once here and provided to *both* the header and the body, because
   * they are siblings. Getting that wrong is not subtle: the provider used to
   * wrap only the strip inside the header, so the panel below never saw the flag
   * and drew a second, identical row of shell tabs under the first.
   */
  const soloStrip = Boolean(active && panelDef(active)?.tabStrip && zone.panels.length === 1);

  return (
    <section
      data-dock-zone={zone.id}
      style={style}
      // A side zone fades in over the chat; a bottom zone rises into the space
      // it is claiming, so the push reads as the panel arriving rather than as
      // the transcript jumping. Which animation applies is decided by the zone's
      // own laid-out position, not by a prop, so the two can never disagree.
      className={cn(
        "panel z-20 m-1 flex min-h-0 min-w-0 flex-col overflow-hidden rounded-sheet",
        isVerticalEdge(zone.edge) ? "animate-fade-up" : "animate-dock-rise",
        className,
      )}
      aria-label={active ? titleFor(active) : "Dock"}
    >
      <StripRenderedProvider value={soloStrip}>
        <ZoneHeader zone={zone} workdir={workdir} soloStrip={soloStrip} />
        {/* `relative` so the resize handle can span exactly this box — the
            panel's content, below the header — rather than the whole section.
            That keeps the tab row and its buttons fully clickable right up to
            the edge, which is where the handle's hit area starts. */}
        <div className="relative min-h-0 min-w-0 flex-1">
          <PanelBody zone={zone} workdir={workdir} />
          <ResizeHandle zone={zone} region={region} />
        </div>
      </StripRenderedProvider>
    </section>
  );
}

/**
 * The drag strip on a panel's inner edge.
 *
 * Pointer-captured rather than tracked on `window`, so the drag survives the
 * pointer leaving the 8px strip — which it does immediately, and which is why a
 * listener bound to the strip alone would drop the gesture after a few pixels.
 *
 * Clamping happens here, per frame, against the measured `region`. Rust clamps
 * again on commit (see `stores/dock.ts`), so a disagreement between the two
 * shows up as one correction on release rather than as drift during the drag.
 *
 * The store is written on every move and persisted only on release: writing the
 * config per frame would lag behind the pointer, and the drag's *result* is the
 * only thing worth saving.
 */
function ResizeHandle({ zone, region }: { zone: DockZone; region: ZoneRegion }) {
  const resizeZone = useDock((state) => state.resizeZone);
  const persist = useDock((state) => state.persist);
  const drag = useRef<{ pointer: number; size: number } | null>(null);
  const vertical = isVerticalEdge(zone.edge);

  return (
    <div
      role="separator"
      aria-orientation={vertical ? "vertical" : "horizontal"}
      aria-label={`Resize the ${zone.edge} panel`}
      onPointerDown={(event) => {
        // Left button only, and prevent the default so a drag cannot start a
        // text selection in the panel behind it.
        if (event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = {
          pointer: vertical ? event.clientX : event.clientY,
          size: layoutSize(zone, region),
        };
      }}
      onPointerMove={(event) => {
        if (!drag.current) return;
        const now = vertical ? event.clientX : event.clientY;
        // Which way is "bigger" depends on which side the panel is anchored to:
        // a left panel grows as the pointer moves right, a right panel grows as
        // it moves left, and a bottom panel grows as it moves up. Getting this
        // wrong makes a drag feel inverted, which is worse than not working.
        const delta =
          zone.edge === "left" ? now - drag.current.pointer : drag.current.pointer - now;
        resizeZone(
          zone.id,
          clampZoneSize(drag.current.size + delta, zone.edge, region),
        );
      }}
      onPointerUp={(event) => {
        if (!drag.current) return;
        event.currentTarget.releasePointerCapture(event.pointerId);
        drag.current = null;
        persist();
      }}
      // A cancelled drag still saves: the store already holds every frame it
      // saw, so discarding the write would leave the config behind the screen.
      onPointerCancel={() => {
        if (!drag.current) return;
        drag.current = null;
        persist();
      }}
      onDoubleClick={() => {
        // Back to the edge's default. Cheap to offer, and the only way to
        // recover a panel dragged to a sliver without hunting for the handle.
        resizeZone(zone.id, clampZoneSize(ZONE_SIZE[zone.edge], zone.edge, region));
        persist();
      }}
      className={cn(
        "absolute z-30 touch-none",
        vertical
          ? cn(
              "top-0 bottom-0 w-2 cursor-col-resize",
              zone.edge === "left" ? "right-0" : "left-0",
            )
          : "top-0 right-0 left-0 h-2 cursor-row-resize",
      )}
    >
      {/* The visible line is a hairline; the hit area is the full 8px strip. A
          1px target is the reason a divider feels ungrabbable. */}
      <span
        aria-hidden="true"
        className={cn(
          "absolute bg-transparent transition-colors hover:bg-[var(--accent)]",
          vertical
            ? "top-1 bottom-1 w-px left-1/2 -translate-x-1/2"
            : "left-1 right-1 h-px top-1/2 -translate-y-1/2",
        )}
      />
    </div>
  );
}

/**
 * The zone's tab row.
 *
 * When the active panel provides a `tabStrip` and is the only panel in the zone,
 * that strip replaces this one entirely — which is how the terminal's shells
 * become the zone's tabs instead of sitting in a second row below them. With two
 * panels stacked there is no single panel whose tabs could stand in for the
 * zone's, so the normal row returns.
 */
function ZoneHeader({
  zone,
  workdir,
  soloStrip,
}: {
  zone: DockZone;
  workdir: string | null;
  soloStrip: boolean;
}) {
  const toggleZone = useDock((state) => state.toggleZone);
  const active = zone.panels[zone.active] ?? null;
  const def = active ? panelDef(active) : null;

  return (
    <div className="glass-thin flex h-9 shrink-0 items-center gap-0.5 border-b border-[var(--glass-border)] px-1">
      {soloStrip && def?.tabStrip ? (
        // The strip owns the whole row including its own trailing buttons, so
        // its `+` sits exactly where a panel `+` would.
        <div className="flex min-w-0 flex-1 items-center gap-0.5">
          <StripComponent def={def} workdir={workdir} />
        </div>
      ) : (
        <>
          <div
            className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto"
            role="tablist"
            aria-label={`${zone.edge} dock tabs`}
          >
            {zone.panels.map((panel, index) => (
              <Tab
                key={panel}
                zoneId={zone.id}
                panel={panel}
                index={index}
                active={index === zone.active}
              />
            ))}
          </div>
          {active && <PanelMenu zone={zone} panel={active} />}
        </>
      )}

      <button
        type="button"
        aria-label="Close this panel"
        title="Close"
        onClick={() => toggleZone(zone.id)}
        className="grid h-7 w-7 shrink-0 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
      >
        <CloseIcon size={14} />
      </button>
    </div>
  );
}

/** Renders a panel's own tab strip, as a component. */
function StripComponent({
  def,
  workdir,
}: {
  def: NonNullable<ReturnType<typeof panelDef>>;
  workdir: string | null;
}) {
  const Strip = def.tabStrip;
  if (!Strip) return null;
  return <Strip workdir={workdir} />;
}

/**
 * One panel tab, and the drag source for rearranging the dock.
 *
 * The pointerdown does not start a drag immediately. A tab is also a button, and
 * treating every press as the beginning of a drag would make selecting one feel
 * sticky; instead the press is remembered and the drag only begins once the
 * pointer has moved `DRAG_THRESHOLD_PX`. That is the same rule the file lists
 * use, and why a click still selects and a deliberate pull still moves.
 */
function Tab({
  zoneId,
  panel,
  index,
  active,
}: {
  zoneId: string;
  panel: string;
  index: number;
  active: boolean;
}) {
  const setActive = useDock((state) => state.setActive);
  const start = useDockDrag((state) => state.start);
  const dragging = useDockDrag((state) => state.panel === panel);
  const target = useDockDrag((state) => state.key);
  const Icon = iconFor(panel);
  const origin = useRef<{ x: number; y: number } | null>(null);

  // Only highlight when the drop would actually change something: the tab's own
  // slot is where it already is, so marking it reads as "nothing will happen".
  const isDropTarget =
    target === targetKey({ kind: "zone", zoneId, index }) &&
    useDockDrag.getState().panel !== panel;

  return (
    <button
      type="button"
      data-dock-tab={panel}
      role="tab"
      aria-selected={active}
      title={`${titleFor(panel)} — drag to move`}
      onClick={() => setActive(zoneId, index)}
      onPointerDown={(event) => {
        // Left button only: a right-click on a tab is a context menu elsewhere,
        // and a middle-click should not begin a move.
        if (event.button !== 0) return;
        origin.current = { x: event.clientX, y: event.clientY };
        const move = (moveEvent: PointerEvent) => {
          if (!origin.current) return;
          const dx = moveEvent.clientX - origin.current.x;
          const dy = moveEvent.clientY - origin.current.y;
          if (Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;
          origin.current = null;
          cleanup();
          start(panel, zoneId);
        };
        const cleanup = () => {
          window.removeEventListener("pointermove", move);
          window.removeEventListener("pointerup", cleanup);
          window.removeEventListener("pointercancel", cleanup);
        };
        window.addEventListener("pointermove", move);
        window.addEventListener("pointerup", cleanup);
        window.addEventListener("pointercancel", cleanup);
      }}
      className={cn(
        "flex max-w-[170px] shrink-0 items-center gap-1.5 rounded-row px-2 py-1 text-[12px] transition-colors",
        active
          ? "bg-[var(--hover-bg)] text-[var(--ink)]"
          : "text-faint hover:bg-[var(--hover-bg)] hover:text-soft",
        dragging && "opacity-40",
        isDropTarget && "ring-1 ring-[var(--accent)]",
      )}
    >
      <Icon size={13} className="shrink-0" />
      <span className="truncate">{titleFor(panel)}</span>
    </button>
  );
}

/** The per-tab menu: tear off, move to another edge, close. */
function PanelMenu({ zone, panel }: { zone: DockZone; panel: string }) {
  const [open, setOpen] = useState(false);
  const movePanel = useDock((state) => state.movePanel);
  const closePanel = useDock((state) => state.closePanel);
  const dropOnEdge = useDock((state) => state.dropOnEdge);
  const zones = useDock((state) => state.layout.zones);

  const popOut = () => {
    const def = panelDef(panel);
    void ipc.openPanelWindow(panel, def?.windowTitle ?? "Loom");
    // The panel moves to the window rather than being duplicated: two live views
    // of one shell would be two terminals fighting over one pty.
    closePanel(zone.id, panel);
  };

  // Every edge, always. The ones with a zone become a move; the ones without
  // create one, which is the way to build a multi-edge layout without dragging.
  const edges: DockEdge[] = ["left", "right", "bottom"];

  return (
    <div className="relative shrink-0">
      <button
        type="button"
        aria-label={`More actions for ${titleFor(panel)}`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="grid h-7 w-7 place-items-center rounded-control text-faint hover:bg-[var(--hover-bg)] hover:text-[var(--ink)]"
      >
        <span aria-hidden="true" className="text-[13px] leading-none">
          ⋯
        </span>
      </button>

      {open && (
        <>
          <button
            type="button"
            aria-label="Close the tab menu"
            onClick={() => setOpen(false)}
            className="fixed inset-0 z-40 cursor-default"
          />
          <div className="panel-strong absolute right-0 z-50 mt-1 w-[200px] overflow-hidden rounded-sheet p-1">
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                popOut();
              }}
              className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[12.5px] text-soft"
            >
              <PopOutIcon size={14} />
              <span className="flex-1">Open in a window</span>
            </button>

            <p className="px-2 pt-1.5 pb-0.5 text-[11px] font-semibold tracking-[0.08em] text-faint uppercase">
              Move to
            </p>
            {edges.map((edge) => {
              const existing = zones.find((entry) => entry.edge === edge);
              const current = zone.edge === edge;
              return (
                <button
                  key={edge}
                  type="button"
                  disabled={current}
                  onClick={() => {
                    setOpen(false);
                    if (existing) movePanel(panel, existing.id, existing.panels.length);
                    else dropOnEdge(panel, edge, 0);
                  }}
                  className={cn(
                    "hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[12.5px]",
                    current ? "cursor-default text-faint" : "text-soft",
                  )}
                >
                  <span className="flex-1 capitalize">{edge}</span>
                  {current && <span className="text-[10.5px]">current</span>}
                  {!current && !existing && (
                    <span className="text-[10.5px] text-faint">new</span>
                  )}
                </button>
              );
            })}

            <button
              type="button"
              onClick={() => {
                setOpen(false);
                closePanel(zone.id, panel);
              }}
              className="hover-surface flex w-full items-center gap-2 rounded-row px-2 py-1.5 text-left text-[12.5px] text-soft"
            >
              <CloseIcon size={14} />
              <span className="flex-1">Close</span>
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/** The active panel. Only the active one is mounted; see `lib/terminals.ts`. */
function PanelBody({ zone, workdir }: { zone: DockZone; workdir: string | null }) {
  const id = zone.panels[zone.active];
  const def = id ? panelDef(id) : null;

  if (!def) {
    // An id this build does not know. `dock.rs` keeps it, so a downgrade does
    // not destroy the layout; saying so is better than rendering nothing.
    return (
      <div className="grid h-full place-items-center p-4">
        <p className="max-w-[220px] text-center text-[12.5px] leading-5 text-faint">
          This build does not have a “{id}” panel. Its place is remembered.
        </p>
      </div>
    );
  }
  const Body = def.render;
  return <Body workdir={workdir} />;
}

/* ---------------------------------------------------------------------------
   Dragging a panel
--------------------------------------------------------------------------- */

/**
 * The drag ghost, the hit-testing, and the drop.
 *
 * Listeners are attached to `window` for the duration of a drag rather than to
 * the tab, because the pointer leaves the tab within a few pixels and a listener
 * on the source would stop receiving moves the moment it did. That is the whole
 * reason the original drag never worked.
 *
 * The ghost is positioned by writing a transform straight onto the element, so a
 * drag across the window renders React once per *change of drop target* rather
 * than once per frame. `useDockDrag` holds only the target; the pointer position
 * never enters React state.
 */
function DragLayer({
  rootRef,
  size,
}: {
  rootRef: React.RefObject<HTMLDivElement | null>;
  size: Viewport;
}) {
  const panel = useDockDrag((state) => state.panel);
  const target = useDockDrag((state) => state.target);
  const ghostRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!panel) return;

    const hitTest = (x: number, y: number): DropTarget | null => {
      const root = rootRef.current?.getBoundingClientRect();
      if (!root) return null;
      // Outside the region, or above it in the title bar: the deliberate
      // "make it a window" target. A band you cannot hit while aiming at a zone.
      if (x < root.left || x > root.right || y < root.top || y > root.bottom) {
        return { kind: "tearoff" };
      }

      // `pointer-events: none` on the ghost keeps this from returning the ghost.
      const element = document.elementFromPoint(x, y);
      const zoneEl = element?.closest?.("[data-dock-zone]") as HTMLElement | null;
      if (zoneEl?.dataset.dockZone) {
        const tabs = Array.from(zoneEl.querySelectorAll<HTMLElement>("[data-dock-tab]"));
        // Where in the strip the drop would land, from the tab midpoints, so the
        // insert position follows what the pointer is actually over.
        let index = tabs.length;
        for (let i = 0; i < tabs.length; i += 1) {
          const rect = tabs[i].getBoundingClientRect();
          if (x < rect.left + rect.width / 2) {
            index = i;
            break;
          }
        }
        return { kind: "zone", zoneId: zoneEl.dataset.dockZone, index };
      }

      // Not over a zone: the nearest edge means "dock here".
      const edge = edgeAt(x - root.left, y - root.top, size);
      if (edge) return { kind: "edge", edge };
      return null;
    };

    let frame = 0;
    let last = { x: 0, y: 0 };

    const paint = () => {
      frame = 0;
      const ghost = ghostRef.current;
      if (ghost) {
        // +14/+12 keeps the ghost off the cursor so the label stays readable and
        // the pointer keeps hitting what is under it.
        ghost.style.transform = `translate3d(${last.x + 14}px, ${last.y + 12}px, 0)`;
      }
      const next = hitTest(last.x, last.y);
      const changed = useDockDrag.getState().retarget(next);
      if (changed && labelRef.current) {
        labelRef.current.textContent = targetLabel(next) ?? "";
        labelRef.current.style.opacity = next ? "1" : "0";
      }
    };

    const onMove = (event: PointerEvent) => {
      last = { x: event.clientX, y: event.clientY };
      // Coalesce to one hit-test per frame: `elementFromPoint` forces layout, so
      // running it per pointermove is what made dragging feel heavy.
      if (!frame) frame = requestAnimationFrame(paint);
    };

    const finish = (commit: boolean) => {
      if (frame) cancelAnimationFrame(frame);
      const final = useDockDrag.getState().target;
      const source = useDockDrag.getState().fromZone;
      const moving = useDockDrag.getState().panel;
      cleanup();
      useDockDrag.getState().end();
      if (!commit || !moving || !final) return;

      const store = useDock.getState();
      if (final.kind === "tearoff") {
        const def = panelDef(moving);
        void ipc.openPanelWindow(moving, def?.windowTitle ?? "Loom");
        // The panel moves to the window rather than being copied: two live views
        // of one shell would be two terminals fighting over one pty.
        if (source) store.closePanel(source, moving);
        return;
      }
      if (final.kind === "edge") {
        store.dropOnEdge(moving, final.edge, 0);
        return;
      }
      store.movePanel(moving, final.zoneId, final.index);
    };

    const onUp = () => finish(true);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") finish(false);
    };

    // A drag must not select the page's text as it passes over it.
    const previousSelect = document.body.style.userSelect;
    const previousCursor = document.body.style.cursor;
    document.body.style.userSelect = "none";
    document.body.style.cursor = "grabbing";

    function cleanup() {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      window.removeEventListener("keydown", onKey);
      document.body.style.userSelect = previousSelect;
      document.body.style.cursor = previousCursor;
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("keydown", onKey);

    return cleanup;
  }, [panel, rootRef, size]);

  if (!panel) return null;
  const Icon = iconFor(panel);

  return (
    <>
      {/* The band shows where an edge drop will land, at the size the zone will
          actually open — so "dock to the left" is visible before the pointer is
          released rather than only described. */}
      {target?.kind === "edge" && <EdgeBand edge={target.edge} />}

      <div
        ref={ghostRef}
        aria-hidden="true"
        className="pointer-events-none fixed top-0 left-0 z-[60]"
        style={{ willChange: "transform" }}
      >
        <div className="panel-strong flex max-w-[260px] items-center gap-2 rounded-sheet px-2.5 py-1.5 shadow-lg">
          <Icon size={14} className="shrink-0 text-faint" />
          <span className="truncate text-[12.5px] text-[var(--ink)]">
            {titleFor(panel)}
          </span>
          <span
            ref={labelRef}
            className="shrink-0 text-[11.5px] text-[var(--accent)] opacity-0 transition-opacity"
          />
        </div>
      </div>
    </>
  );
}

/** The accent band that previews an edge dock, sized as the zone will be. */
function EdgeBand({ edge }: { edge: DockEdge }) {
  const vertical = isVerticalEdge(edge);
  const size = ZONE_SIZE[edge];
  return (
    <div
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute z-30 rounded-sheet border border-[var(--accent)] bg-[var(--accent-soft)]",
        edge === "right" && "top-0 right-0 bottom-0",
        edge === "left" && "top-0 bottom-0 left-0",
        edge === "bottom" && "right-0 bottom-0 left-0",
      )}
      style={vertical ? { width: size } : { height: size }}
    />
  );
}
