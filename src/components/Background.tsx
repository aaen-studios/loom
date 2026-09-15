import { cn } from "../lib/cn";
import { assetUrl } from "../lib/tauri";
import { backgroundStyle } from "../lib/background";
import { useSettings } from "../stores/settings";

const GRAIN =
  "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='180' height='180'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%25' height='100%25' filter='url(%23n)' opacity='0.55'/></svg>\")";

/**
 * The app's own background layer. Sits below every glass surface, which is
 * what makes the backdrop blur read as "glass over something" rather than
 * flat translucency. Built-in presets are pure CSS; image/video kinds render
 * user-picked media from disk (wired in M2).
 */
export function Background() {
  const config = useSettings((state) => state.config.background);
  const usesMedia = config.kind !== "builtin" && !!config.path;

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
          className={cn(
            "absolute inset-[-8%]",
            config.kind === "builtin" && "animate-drift",
          )}
          style={{
            ...backgroundStyle(config),
            filter: config.blur > 0 ? `blur(${config.blur}px)` : undefined,
          }}
        />
      )}

      {/* dim veil keeps glass text legible over bright backgrounds */}
      <div
        className="absolute inset-0"
        style={{ backgroundColor: `rgb(4 8 18 / ${config.dim / 100})` }}
      />

      {/* fine grain, barely visible, adds depth to flat gradients */}
      <div
        className="absolute inset-0 opacity-[0.045] mix-blend-overlay"
        style={{ backgroundImage: GRAIN, backgroundSize: "180px 180px" }}
      />
    </div>
  );
}
