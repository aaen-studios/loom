import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import LiquidGlass from "liquid-glass-react";
import { cn } from "../lib/cn";
import { DEFAULT_LIQUID, clampLiquid, toBlurAmount, type LiquidParams } from "../lib/glass";

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
 * contains the slash menu. A child would be clipped. A sibling cannot be.
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
 * filter written here.
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
  /** Which token the surface tints with. */
  tint = "var(--pill-bg)",
  /**
   * How much of that token survives, as a percentage.
   *
   * A token like `--pill-bg` is about 74% alpha, so 60 here lands near 44% —
   * enough to keep text legible over busy artwork while still letting the
   * refraction read. Above about 80 the warp disappears behind the tint and the
   * surface looks like ordinary frosted glass, which is the trap this whole
   * layering exists to avoid.
   */
  tintStrength = 60,
  /** Off renders the same geometry as ordinary frosted glass, no refraction. */
  liquid = true,
  params = DEFAULT_LIQUID,
}: {
  children: ReactNode;
  className?: string;
  contentClassName?: string;
  tint?: string;
  tintStrength?: number;
  liquid?: boolean;
  params?: LiquidParams;
}) {
  const reduced = useReducedMotion();
  const safe = clampLiquid(params);

  // The fill is the one place the two modes differ, and it is the same
  // declaration either way: the full token when there is no warp to show
  // through it, and a fraction of it when there is.
  const fill = `color-mix(in srgb, ${tint} ${liquid ? tintStrength : 100}%, transparent)`;

  return (
    <div className={cn("lg-stage", className)}>
      {liquid ? (
        <div className="lg-shell">
          <LiquidGlass
            className="lg-glass"
            displacementScale={safe.refraction}
            // `safe.frost` is in px; the library wants its own unit.
            blurAmount={toBlurAmount(safe.frost)}
            saturation={safe.saturation}
            aberrationIntensity={safe.chromatics}
            // The pointer-following is a transform rewritten every frame, so the
            // global reduced-motion override (which only shortens transitions
            // and animations) does not cover it. This does.
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
              menus and the composer's slash menu can overflow freely. The
              library only needs *something* so its inline-flex box does not
              collapse; our CSS flattens that box to fill the shell regardless.
            */}
            {null}
          </LiquidGlass>
          <div className="lg-tint" style={{ backgroundColor: fill }} />
        </div>
      ) : (
        <div className="lg-static" style={{ backgroundColor: fill }} />
      )}
      <div className={cn("lg-content", contentClassName)}>{children}</div>
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
