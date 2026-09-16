/**
 * The Loom mark: two warp threads over a weft.
 *
 * Traced from the app's `src/components/icons.tsx` rather than redrawn. The
 * geometry *is* the mark — the particular curve of those two threads is what
 * makes it read as a loom rather than as an abstract "A" — so the paths, the
 * 1.9 stroke and the 0.55 opacity on the hem are the app's, values and all.
 *
 * `weaving` draws the threads on at mount: warp first, then weft, on the same
 * 640ms throw and 120ms stagger the app's opening screen uses. It works because
 * `pathLength={1}` makes a single dash span the whole path, so the offset can
 * animate from hidden to drawn without measuring anything at runtime.
 *
 * Server-renderable — no state, no effects — and the animation itself comes from
 * the shared token sheet, so these are the app's own keyframes rather than a
 * second copy of them.
 *
 * `src/lib/iconConsistency.test.ts` in the app asserts this file keeps the three
 * path definitions, because a hand-maintained copy of the mark cannot be reached
 * by the icon regeneration script and is therefore the one that silently rots.
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
