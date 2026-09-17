import type { DockEdge, DockLayout, DockZone } from "../types";

/**
 * The dock's sizing rules, mirrored from `crates/loom-core/src/dock.rs`.
 *
 * Rust owns the constants and enforces them on every write, because a torn-off
 * window has to agree with the main one about how wide a zone may be. This
 * mirror exists for one reason only: a drag has to clamp *while* the pointer is
 * moving. Round-tripping every pointermove through the backend would make
 * resizing feel like dragging a wet rope, so the UI clamps locally for
 * smoothness and Rust clamps again on commit.
 *
 * The two copies are kept honest by `dockGeometry.test.ts`, which asserts the
 * same accept/reject table the Rust tests do. If they ever disagree the symptom
 * is a splitter that snaps on release, which is exactly the class of bug that is
 * hard to see and easy to write.
 */

/**
 * The band a zone's size is held inside.
 *
 * Kept in step with `dock.rs`, which enforces the same range on every write, and
 * applied on both sides of a drag: locally so the panel follows the pointer
 * without a round trip, and by Rust again on commit so two windows showing the
 * same layout cannot end up disagreeing.
 */
export const MIN_ZONE_PX = 240;
export const MAX_ZONE_PX = 2400;

/**
 * The size each edge's zone opens at, and the size a new zone is created with.
 *
 * A starting point, not a fixed size: a zone can be dragged to anything in the
 * band above. The left column is narrowest because it holds the chats list —
 * short titles in a single column — while the terminal needs real columns for a
 * diff to stay readable.
 */
export const ZONE_SIZE: Record<DockEdge, number> = {
  left: 300,
  right: 460,
  bottom: 260,
};

/**
 * The most of the region's height a bottom zone may take.
 *
 * A cap, not a preference. The chat region above a bottom dock holds the
 * composer, and the composer is how you ask a panel to close — so a drag that
 * squeezed the region away would also remove the means of undoing itself. At
 * 60% the transcript always keeps a usable share of the window.
 */
export const BOTTOM_SHARE = 0.6;

/** How close to a window edge a dragged panel must be to mean "dock here". */
export const EDGE_BAND_PX = 48;

/**
 * How far the pointer must travel before a press becomes a drag.
 *
 * A tab is also a button, so treating every press as the beginning of a drag
 * makes selecting one feel sticky. The same rule the file lists use.
 */
export const DRAG_THRESHOLD_PX = 4;

/** Whether this edge competes for the window's width. */
export function isVerticalEdge(edge: DockEdge): boolean {
  return edge === "left" || edge === "right";
}

/** The size a stored zone is allowed to be, from a config file. */
export function clampStoredSize(desired: number): number {
  return Math.min(Math.max(desired, MIN_ZONE_PX), MAX_ZONE_PX);
}

/** The region a zone is laid out in: the dock's own box, not the window's. */
export interface ZoneRegion {
  width: number;
  height: number;
}

/** The extent a zone on `edge` measures against. */
function extentFor(edge: DockEdge, region: ZoneRegion): number {
  return isVerticalEdge(edge) ? region.width : region.height;
}

/**
 * The largest a zone on `edge` may be, given the region it lives in.
 *
 * A side zone may take the whole width: it is laid *over* the chat rather than
 * taking the chat's space, so a wide column costs legibility but can never take
 * the composer away. A bottom zone is in flow, so it is capped at
 * `BOTTOM_SHARE` for the reason given there.
 */
export function maxZoneSize(edge: DockEdge, region: ZoneRegion): number {
  const extent = extentFor(edge, region);
  const share = isVerticalEdge(edge) ? extent : Math.floor(extent * BOTTOM_SHARE);
  return Math.min(MAX_ZONE_PX, Math.max(MIN_ZONE_PX, share));
}

/** A drag's proposed size, held inside what the region can actually show. */
export function clampZoneSize(
  desired: number,
  edge: DockEdge,
  region: ZoneRegion,
): number {
  return Math.min(
    Math.max(Math.round(desired), MIN_ZONE_PX),
    maxZoneSize(edge, region),
  );
}

/**
 * The size to lay a zone out at.
 *
 * Before the first `ResizeObserver` callback the region is all zeroes, and
 * clamping against that would snap every zone to the 240px floor for a frame —
 * a visible flicker on launch. The stored value is used until there is a real
 * region to clamp against.
 */
export function layoutSize(zone: DockZone, region: ZoneRegion): number {
  if (region.width === 0 && region.height === 0) return clampStoredSize(zone.size);
  return clampZoneSize(zone.size, zone.edge, region);
}

/** Whether any zone is taking part of the window. */
export function hasOpenZone(layout: DockLayout): boolean {
  return layout.zones.some((zone) => zone.open && zone.panels.length > 0);
}

/**
 * Which edge a pointer is close enough to mean "dock here".
 *
 * Coordinates are relative to the dock region, not the window: the region is
 * what a zone is laid out in, and measuring against the window would put the
 * bands 56px off on the vertical axis.
 *
 * Ties go to the earlier check — left, then right, then bottom — so a pointer
 * exactly equidistant from two edges still has one definite answer rather than
 * an arbitrary one.
 *
 * `EDGE_BAND_PX` is generous because a drop target you have to aim at is a drop
 * target you will miss, and everything outside it means "not an edge", which is
 * how the tear-off gesture stays unambiguous.
 */
export function edgeAt(
  x: number,
  y: number,
  viewport: { width: number; height: number },
): DockEdge | null {
  const left = x;
  const right = viewport.width - x;
  const bottom = viewport.height - y;
  const nearest = Math.min(left, right, bottom);
  if (nearest > EDGE_BAND_PX) return null;
  if (nearest === left) return "left";
  if (nearest === right) return "right";
  return "bottom";
}
