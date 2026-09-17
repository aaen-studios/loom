import { describe, expect, it } from "vitest";
import type { DockLayout, DockZone } from "../types";
import {
  BOTTOM_SHARE,
  EDGE_BAND_PX,
  MAX_ZONE_PX,
  MIN_ZONE_PX,
  ZONE_SIZE,
  clampStoredSize,
  clampZoneSize,
  edgeAt,
  hasOpenZone,
  isVerticalEdge,
  layoutSize,
  maxZoneSize,
} from "./dockGeometry";

/**
 * The values here are the same ones `dock.rs` asserts, deliberately: Rust owns
 * the band and enforces it on every write, and this mirror is what clamps a drag
 * *while* the pointer is moving, so the panel follows without waiting on a round
 * trip. The two copies must agree or the panel snaps the moment you let go.
 */

function zone(patch: Partial<DockZone> & Pick<DockZone, "id" | "edge">): DockZone {
  return { size: 300, open: true, panels: ["terminal"], active: 0, ...patch };
}

function layout(zones: DockZone[]): DockLayout {
  return { zones, shell: null };
}

describe("dock geometry", () => {
  it("matches the Rust constants", () => {
    expect(MIN_ZONE_PX).toBe(240);
    expect(MAX_ZONE_PX).toBe(2400);
  });

  it("only treats left and right as competing for width", () => {
    expect(isVerticalEdge("left")).toBe(true);
    expect(isVerticalEdge("right")).toBe(true);
    // A bottom zone takes height, so it never competes with a side zone for the
    // window's width. This is what lets the terminal and Runs be open together.
    expect(isVerticalEdge("bottom")).toBe(false);
  });

  it("gives every edge a size, and the left one the narrowest", () => {
    for (const edge of ["left", "right", "bottom"] as const) {
      expect(ZONE_SIZE[edge]).toBeGreaterThanOrEqual(MIN_ZONE_PX);
      expect(ZONE_SIZE[edge]).toBeLessThanOrEqual(MAX_ZONE_PX);
    }
    // The chats list is a column of short titles; the terminal needs columns.
    expect(ZONE_SIZE.left).toBeLessThan(ZONE_SIZE.right);
  });

  it("holds a stored size inside the band", () => {
    // Rust clamps on write, so this only has to survive a hand-edited file.
    expect(clampStoredSize(99_999)).toBe(MAX_ZONE_PX);
    expect(clampStoredSize(1)).toBe(MIN_ZONE_PX);
    expect(clampStoredSize(420)).toBe(420);
  });

  it("clamps a drag to what the region can show", () => {
    const region = { width: 1600, height: 900 };
    expect(clampZoneSize(400, "left", region)).toBe(400);
    // Never below the floor, however hard the drag is pushed inward.
    expect(clampZoneSize(10, "left", region)).toBe(MIN_ZONE_PX);
    expect(clampZoneSize(-500, "right", region)).toBe(MIN_ZONE_PX);
    // A side zone stops at the region's own width, and at the constant cap.
    expect(clampZoneSize(99_999, "left", region)).toBe(Math.min(MAX_ZONE_PX, region.width));
    // Fractional pointer positions must not produce a fractional size.
    expect(clampZoneSize(400.6, "left", region)).toBe(401);
  });

  it("keeps a share of the height for the chat when the bottom docks out", () => {
    const region = { width: 1600, height: 1000 };
    expect(maxZoneSize("bottom", region)).toBe(1000 * BOTTOM_SHARE);
    expect(clampZoneSize(900, "bottom", region)).toBe(600);
    // A side zone is not capped the same way: it overlays the chat rather than
    // taking the chat's space, so the composer is never at risk from it.
    expect(maxZoneSize("left", region)).toBe(1600);
  });

  it("uses the stored size until the region has been measured", () => {
    const unmeasured = { width: 0, height: 0 };
    // Clamping against all zeroes would snap every zone to the floor for the
    // first frame, before the ResizeObserver has reported anything.
    expect(layoutSize(zone({ id: "right", edge: "right", size: 460 }), unmeasured)).toBe(460);
    // A hand-edited 99999 is still held inside the band.
    expect(
      layoutSize(zone({ id: "right", edge: "right", size: 99_999 }), unmeasured),
    ).toBe(MAX_ZONE_PX);
    // Once measured, the region is what decides.
    expect(
      layoutSize(zone({ id: "bottom", edge: "bottom", size: 900 }), {
        width: 1600,
        height: 1000,
      }),
    ).toBe(600);
  });

  it("knows whether anything is taking part of the window", () => {
    const closed = layout([zone({ id: "left", edge: "left", open: false })]);
    expect(hasOpenZone(closed)).toBe(false);

    // An open zone with no panels draws no chrome, so it takes nothing.
    const empty = layout([zone({ id: "left", edge: "left", panels: [] })]);
    expect(hasOpenZone(empty)).toBe(false);

    expect(hasOpenZone(layout([zone({ id: "left", edge: "left" })]))).toBe(true);
  });

  it("picks the nearest edge for a drop, and nothing in the middle", () => {
    const viewport = { width: 1200, height: 800 };
    expect(edgeAt(10, 400, viewport)).toBe("left");
    expect(edgeAt(1195, 400, viewport)).toBe("right");
    expect(edgeAt(600, 795, viewport)).toBe("bottom");

    // The middle is not a dock target — which is also what keeps the tear-off
    // gesture unambiguous, since everything outside the band means "out of the
    // window" and not "dock somewhere".
    expect(edgeAt(600, 400, viewport)).toBeNull();
    expect(edgeAt(EDGE_BAND_PX + 1, 400, viewport)).toBeNull();
    expect(edgeAt(600, viewport.height - EDGE_BAND_PX - 1, viewport)).toBeNull();

    // A corner resolves to whichever edge is nearest, and a true tie goes to the
    // earlier check — left, then right, then bottom — so "which way did it
    // dock?" always has one answer.
    expect(edgeAt(4, 300, viewport)).toBe("left");
    expect(edgeAt(1196, 700, viewport)).toBe("right");
    expect(edgeAt(4, 796, viewport)).toBe("left");
  });
});
