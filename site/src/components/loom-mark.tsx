/**
 * The Loom mark: two warp threads over a weft.
 *
 * Copied from the app (`src/components/icons.tsx`) rather than redrawn, because
 * the geometry *is* the mark — the exact curve of the warp threads is what makes
 * it read as a loom rather than as a generic "A". The paths, the 1.9 stroke
 * width and the 0.55 opacity on the weft are all the app's.
 *
 * `weaving` draws the threads on as it mounts: the warp first, then the weft,
 * on the same 640ms stagger and 120ms delay the app's opening screen uses. It
 * relies on `pathLength={1}`, which makes one dash the whole thread so the
 * offset can animate without measuring anything.
 *
 * Presentational and server-renderable: no state, no effects. The CSS lives in
 * the generated token sheet, so the animation is the app's own keyframes.
 */
export function LoomMark({
  size = 18,
  weaving = false,
  className,
}: {
  size?: number;
  weaving?: boolean;
  className?: string;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      className={weaving ? `loom-mark-weaving ${className ?? ""}` : className}
      aria-hidden="true"
    >
      <path
        d="M6.5 5.5c0 6.5 5.5 6.5 5.5 13"
        pathLength={weaving ? 1 : undefined}
      />
      <path
        d="M12 5.5c0 6.5 5.5 6.5 5.5 13"
        pathLength={weaving ? 1 : undefined}
      />
      <path d="M6.5 18.5h11" opacity="0.55" pathLength={weaving ? 1 : undefined} />
    </svg>
  );
}
