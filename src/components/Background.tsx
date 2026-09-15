import { useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { assetUrl } from "../lib/tauri";
import { backgroundStyle, presetById } from "../lib/background";
import { useSettings } from "../stores/settings";

const GRAIN =
  "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='180' height='180'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%25' height='100%25' filter='url(%23n)' opacity='0.55'/></svg>\")";

/** Dark theme keeps the veil lifted; a light preset would otherwise look like fog. */
const DARK_DIM_FLOOR = 48;

/** Tracks the theme class on <html>, which App keeps in sync. */
function useThemeIsDark(): boolean {
  const [dark, setDark] = useState(
    () => typeof document !== "undefined" && document.documentElement.classList.contains("dark"),
  );

  useEffect(() => {
    const root = document.documentElement;
    const update = () => setDark(root.classList.contains("dark"));
    update();
    const observer = new MutationObserver(update);
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);

  return dark;
}

/**
 * Full-bleed background layer. Every glass surface blurs it, which is what
 * makes the panels read as frosted glass instead of flat translucency.
 */
export function Background() {
  const config = useSettings((state) => state.config.background);
  const preset = presetById(config.preset);
  const usesMedia = config.kind !== "builtin" && !!config.path;
  const dark = useThemeIsDark();

  const dim = dark ? Math.max(config.dim, DARK_DIM_FLOOR) : config.dim;

  const mediaStyle = {
    filter: config.blur > 0 ? `blur(${config.blur}px)` : undefined,
    transform: config.blur > 0 ? "scale(1.06)" : undefined,
  };

  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 overflow-hidden"
    >
      {usesMedia && config.kind === "image" && (
        <img
          src={assetUrl(config.path as string)}
          alt=""
          className="absolute inset-0 h-full w-full object-cover"
          style={mediaStyle}
        />
      )}

      {usesMedia && config.kind === "video" && (
        <video
          src={assetUrl(config.path as string)}
          autoPlay
          loop
          muted
          playsInline
          className="absolute inset-0 h-full w-full object-cover"
          style={mediaStyle}
        />
      )}

      {!usesMedia && (
        <div
          className={cn("absolute inset-[-6%]", !preset.still && "animate-drift")}
          style={{
            ...backgroundStyle(config),
            filter: config.blur > 0 ? `blur(${config.blur}px)` : undefined,
          }}
        />
      )}

      {/* Dim veil: keeps text legible over bright art. */}
      <div
        className="absolute inset-0"
        style={{ backgroundColor: `rgb(3 6 14 / ${dim / 100})` }}
      />

      <div
        className="absolute inset-0 opacity-[0.04] mix-blend-overlay"
        style={{ backgroundImage: GRAIN, backgroundSize: "180px 180px" }}
      />
    </div>
  );
}
