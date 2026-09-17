import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
// `CSSProperties` is still needed for `LIQUID_FILL`, which is inline on purpose.
import LiquidGlass from "liquid-glass-react";
import { cn } from "../lib/cn";
import { clampBlur, clampLiquid, clampTint, toBlurAmount } from "../lib/glass";
import { useSettings } from "../stores/settings";
import type { LiquidGlassConfig } from "../types";

/** How the surface's content is laid out. See `layout` below. */
export type SurfaceLayout = "row" | "block";

/**
 * The surfaces the config can switch on and off.
 *
 * A union rather than `keyof LiquidGlassConfig` so this stays the list of
 * *groups*, not of fields: adding a number to the config should not silently
 * make `surface="refraction"` typecheck.
 */
export type SurfaceKey =
  | "pills"
  | "composer"
  | "panels"
  | "popovers"
  | "cards"
  | "overlays";

/**
 * A glass surface, optionally refracted.
 *
 * ## Why this is three layers and not one component
 *
 * `liquid-glass-react` refracts its **backdrop**: it samples what is painted
 * behind it and warps the edge. Loom's own surfaces are already heavy glass —
 * `--panel-bg-strong` is 82% opaque behind a 38px blur — so wrapping one in the
 * other would show a blurred blur and no refraction at all. The shell therefore
 * carries only the tint and the border, and the library's warp layer is the
 * *only* thing doing any frosting.
 *
 * ## Why the content is a sibling of the shell, not a child
 *
 * The library's own box is `overflow: hidden` inline, and every surface this is
 * used on holds absolutely positioned popups: the title-bar pills contain the
 * Panels menu, the workspace chip and the persona menu, and the composer
 * contains the slash menu and the mention menu. A child would be clipped. A
 * sibling cannot be.
 *
 * ## Why `top/left: 50%` and `width/height: 100%` rather than `inset: 0`
 *
 * The library composes its root transform as
 * `translate(calc(-50% + Xpx), calc(-50% + Ypx)) scaleX() scaleY()` and it
 * overwrites whatever `transform` the caller passes. So the only way to land on
 * the shell is to give it the half-size offset it already assumes: fill the
 * shell, sit its top-left on the shell's centre, and let the library translate
 * it back. Fighting the transform with `!important` would also throw away the
 * elasticity and the hover scale — which is the whole "liquid" part of the
 * effect, and the reason for reaching for this library rather than an SVG
 * filter written here. Verified: the root lands within a pixel.
 *
 * ## Why no measurement and no `ResizeObserver`
 *
 * The library sizes its veil and border layers from a `glassSize` it only
 * corrects in a `useEffect`, so the first frame would use its 270×69 guess. But
 * those four nodes are hidden by our own CSS (`styles.css`, the liquid glass
 * section) because Loom has its own border and shadow language. With them gone,
 * a stale `glassSize` only resizes an invisible `<svg>`, so nothing needs
 * measuring and the composer can grow as you type without a re-render.
 */
export function LiquidSurface({
  children,
  className,
  contentClassName,
  /**
   * How the content is laid out.
   *
   * `row` is chrome — a pill holding a few buttons side by side. `block` is
   * everything that contains a *list*: a menu of rows, a card with paragraphs,
   * a drawer of sections. Those are the majority, and passing `row` by accident
   * lays a popup's rows out horizontally, which is a confusing way to discover
   * that the default was wrong for it.
   */
  layout = "row",
  /**
   * Inline styles for the content, for values that cannot be a class.
   *
   * Only the popovers need it, and they need it genuinely: their height comes
   * from a measured `maxHeight` computed per open (`drop.maxHeight`), which is a
   * number rather than one of a fixed set of classes.
   */
  contentStyle,
  /** Which token the surface tints with. */
  tint = "var(--pill-bg)",
  /**
   * How much of that token survives, as a percentage.
   *
   * A token like `--pill-bg` is about 90% alpha in light theme, so 50 here lands
   * near 45% — enough to keep text legible over busy artwork while still letting
   * the refraction read. Above about 80 the warp disappears behind the tint and
   * the surface looks like ordinary frosted glass, which is the trap this whole
   * layering exists to avoid; below about 35 the chrome stops separating from
   * the artwork.
   *
   * The app-wide Tint slider scales this, so this is the ceiling rather than the
   * final value.
   */
  tintStrength = 50,
  /**
   * Which config switch governs this surface.
   *
   * Resolved here rather than passed in as a boolean so every call site obeys
   * the master switch without having to remember to combine them. The failure
   * mode of the other arrangement is one surface that quietly ignores "off".
   */
  surface,
  /** Off renders the same geometry as ordinary frosted glass, no refraction. */
  liquid,
  /** Per-surface overrides, merged over the stored config. */
  params,
}: {
  children: ReactNode;
  className?: string;
  contentClassName?: string;
  layout?: SurfaceLayout;
  contentStyle?: CSSProperties;
  tint?: string;
  tintStrength?: number;
  surface?: SurfaceKey;
  liquid?: boolean;
  params?: Partial<LiquidGlassConfig>;
}) {
  const reduced = useReducedMotion();
  const glass = useSettings((state) => state.config.interface.glass);
  const config = glass.liquid;

  // The master switch, this surface's own switch, and the caller's override —
  // in that order, so a surface cannot opt itself in past "off".
  const enabled = config.enabled && (surface ? config[surface] : true) && (liquid ?? true);

  // Config first, so a caller can override one field for one surface. The
  // composer wants less refraction than a 40px pill: the displacement maps
  // stretch to the box, so the same scale reads much heavier on a wide, short
  // surface than on a small round one.
  const safe = clampLiquid(params ? { ...config, ...params } : config);

  /*
    The two app-wide multipliers reach here, and this is the only place they can.

    `.lg-tint` is an inline `color-mix`, so `--glass-tint` cannot scale it in CSS
    the way the unlayered `.pill` overrides do — and the vector-effect problem is
    the opposite way round for the warp: its `backdrop-filter` is written by the
    library, which knows nothing about our variables. So both are folded in here,
    in JS, where the arithmetic is a plain number and cannot silently produce an
    invalid declaration the way a nested `calc()` inside `color-mix()` can.

    The practical effect is that Tint and Blur in Settings now move the
    refracting surfaces too, not only the plain ones. Before this they moved
    nothing anywhere, because the variables were never written — and even once
    they were, the pills and the preview ignored them.
  */
  const strength = Math.round(tintStrength * (clampTint(glass.tint) / 100));
  // Floored at 1px: the library adds its own 4px base, and a frost of 0 would
  // leave the warp sampling a backdrop with no frost at all, which reads as a
  // hard-edged copy of the artwork rather than as glass.
  const frost = Math.max(1, Math.round(safe.frost * (clampBlur(glass.blur) / 100)));

  /*
    One declaration for both modes, deliberately.

    The static path used to take the *full* token while the refracting path took
    a fraction of it, which meant turning refraction off made the surface more
    opaque — the opposite of what the toggle's own hint promises ("off renders
    the same surfaces as ordinary frosted glass, at the same tint and blur").
    Same fill, different mechanism for the frosting: the warp here, a plain
    `backdrop-filter` in `.lg-static`.
  */
  const fill = `color-mix(in srgb, ${tint} ${strength}%, transparent)`;

  return (
    <div className={cn("lg-stage", className)}>
      {enabled ? (
        <div className="lg-shell">
          <LiquidGlass
            className="lg-glass"
            displacementScale={safe.refraction}
            // `frost` is in px, already scaled by the app-wide Blur slider; the
            // library wants its own unit, which is a different scale by a factor
            // of 32. See `toBlurAmount`.
            blurAmount={toBlurAmount(frost)}
            saturation={safe.saturation}
            aberrationIntensity={safe.chromatics}
            // The pointer-following is a transform rewritten every frame, so the
            // global reduced-motion override — which only shortens transitions
            // and animations — does not cover it. This does.
            elasticity={reduced ? 0 : safe.elasticity}
            // The shell owns the silhouette; a radius here would be a second,
            // smaller one on the inner box.
            cornerRadius={0}
            mode={safe.mode}
            padding="0"
            style={LIQUID_FILL}
          >
            {/*
              Deliberately empty, and the prop is required so it cannot be
              omitted. The library renders children inside its own box, which is
              `overflow: hidden` inline — so anything passed here is clipped. All
              real content is a sibling, in `.lg-content`, where the title-bar
              menus and the composer's slash menu can overflow freely.

              The library only needs *something* so its inline-flex box does not
              collapse to nothing, which would leave the warp painting zero
              pixels. Our CSS flattens that box to fill the shell regardless.
            */}
            {null}
          </LiquidGlass>
          {/* Above the warp, never below it: `backdrop-filter` samples what is
              painted behind the element, so a tint on the shell would become
              part of the warp's own input and get frosted along with everything
              else. */}
          <div className="lg-tint" style={{ backgroundColor: fill }} />
        </div>
      ) : (
        <div className="lg-static" style={{ backgroundColor: fill }} />
      )}
      <div
        className={cn(
          "lg-content",
          layout === "row" ? ROW_CONTENT_CLASS : BLOCK_CONTENT_CLASS,
          contentClassName,
        )}
        style={contentStyle}
      >
        {children}
      </div>
    </div>
  );
}

/**
 * Fill the shell, and sit its top-left on the shell's centre.
 *
 * `transform` is absent on purpose: the library replaces it, and its own
 * `translate(-50%, -50%)` is what does the centring from here.
 */
const LIQUID_FILL: CSSProperties = {
  position: "absolute",
  top: "50%",
  left: "50%",
  width: "100%",
  height: "100%",
};

/**
 * The two content layouts, as Tailwind classes rather than as rules inside the
 * unlayered `.lg-content` block.
 *
 * Unlayered rules beat every utility, so a `display` there could not have been
 * overridden by a call site at all. Keeping it here means `layout` decides and
 * the caller can still append anything else.
 *
 * Both are always applied alongside the caller's `contentClassName`, which is
 * appended rather than substituted. The first version of this treated
 * `contentClassName` as the whole value, so the pills' `"gap-0.5 p-1"` silently
 * replaced `flex` and their buttons stacked vertically — the stage measured 40px
 * while its content measured 72, and every shell-geometry check still passed.
 */
const ROW_CONTENT_CLASS = "flex h-full items-center";
const BLOCK_CONTENT_CLASS = "block h-full";

/** Tracks the OS setting, including while the app is open. */
function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);

  return reduced;
}
