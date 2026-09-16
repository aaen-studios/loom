import type { CSSProperties } from "react";

/**
 * The warp: the threads everything else is tied to.
 *
 * Fixed, full height, behind the content. Twelve hairlines at the leading edge of
 * twelve columns, drawn in the app's own thread colours because those seven
 * `--thread-*` properties are in the shared `tokens` region — the app uses them for
 * the quick-ask overlay's woven column, so the loom here is strung with the
 * product's real material rather than with a palette invented for a landing page.
 *
 * The count lives here *and* in `globals.css` (`--warp-columns`), because the two
 * cannot read each other: this is a fixed background element and the content grid is
 * a Tailwind-managed layout, and neither can be generated from the other without a
 * build step larger than the problem. So the number is stated twice and
 * `verify-weave.mjs` fails if they disagree — the same arrangement the token sheet
 * uses, and honest about the duplication instead of hiding it.
 */
export const WARP_THREADS = 12;

export function Warp() {
  return (
    // Decorative in the strict sense: it carries no information a reader needs, so
    // it is hidden from assistive technology rather than described to it.
    <div aria-hidden="true" className="warp-field">
      <div className="warp-grid">
        {Array.from({ length: WARP_THREADS }, (_, index) => (
          <span
            key={index}
            className="warp-line"
            // Read by the `loom-warp-draw` stagger in `globals.css`, so the threads
            // come up left to right the way a loom is actually warped. Cast because
            // `CSSProperties` does not admit custom properties, which is a gap in
            // the type rather than a mistake here.
            style={{ "--i": String(index) } as CSSProperties}
          />
        ))}
      </div>
    </div>
  );
}
