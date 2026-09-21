import type { Figure, FigureKind } from "@/lib/weave";
import { figure } from "@/lib/weave";

/**
 * The renderer.
 *
 * Thin on purpose. `lib/weave.ts` decides what the geometry *is*; this decides how it is painted, and
 * there is deliberately nothing in between. A figure is drawn in four possible ways — a path with a
 * gradient, a path with a flat stroke, an edge, a node — and every one of them is here.
 *
 * ---------------------------------------------------------------------------
 * Why the colours are the application's own
 * ---------------------------------------------------------------------------
 *
 * Every stroke takes its colour from one of the app's seven `--thread-*` tokens or from `--accent`. The
 * site does not own a palette: the artwork on it is drawn in the same material the product draws its own
 * interface in, which is why the two do not look like a website and an application that happen to be
 * near each other.
 *
 * ---------------------------------------------------------------------------
 * Why the geometry is a prop and not computed here
 * ---------------------------------------------------------------------------
 *
 * The client component that animates the hero needs the geometry on every frame, inside a
 * `requestAnimationFrame` callback, where it cannot await anything and should not re-render. So the maths
 * is a plain function of its arguments and this component takes the result. It also means the *server*
 * can render a complete figure — `Still` does exactly that for the five figures that never move — with no
 * client JavaScript involved at all.
 */

export interface FigureProps {
  kind: FigureKind;
  /** The geometry. Computed by whoever knows the size; see `Still` and `AnimatedWeave`. */
  shape: Figure;
  className?: string;
  /**
   * How the strands are painted. `gradient` fades them at both ends, which is what stops a field of
   * threads reading as a barcode; `flat` is for a figure meant to look like a drawing.
   *
   * Omitted means "decide by kind", and the decision is orientation rather than taste — see the note in
   * the component.
   */
  paint?: "gradient" | "flat";
  /** Opacity of the whole figure, 0–1. */
  opacity?: number;
  /** A label for a figure that carries meaning a reader needs. Omit for pure texture. */
  label?: string;
  /**
   * Unique per figure on the page.
   *
   * Ids are global, and two figures sharing a gradient id would have the second silently take the
   * first's colours — a bug that looks like a theming problem and is not.
   */
  id: string;
}

/** The figure's own coordinate space, read back out of the viewBox its generator declared. */
function boxOf(viewBox: string): { width: number; height: number } {
  const parts = viewBox.split(/\s+/).map(Number);
  return { width: parts[2] ?? 0, height: parts[3] ?? 0 };
}

export function FigureSvg({
  kind,
  shape,
  className,
  paint,
  opacity = 1,
  label,
  id,
}: FigureProps) {
  const gradient = `weave-${id}`;
  const box = boxOf(shape.viewBox);

  /*
   * How this figure is painted, defaulting by kind.
   *
   * The distinction is orientation. `field` and `bundle` are made of strands that run *down* the figure,
   * so fading them at top and bottom lets them dissolve into the page instead of stopping at a line —
   * which is what stops a thread field reading as a barcode. `lattice` and `rings` are radial and
   * non-directional: a lattice that faded top-to-bottom would look cropped, and rings that faded would
   * look like they were falling into something.
   */
  const painting = paint ?? (kind === "field" || kind === "bundle" ? "gradient" : "flat");

  return (
    <svg
      className={className}
      viewBox={shape.viewBox}
      /*
       * `preserveAspectRatio="none"` stretches the figure to whatever box it is given, which for a
       * texture is right — a texture that letterboxes has visible margins. It is only used when the figure
       * is *not* labelled, because stretching a figure that carries meaning distorts it.
       */
      preserveAspectRatio={label ? "xMidYMid meet" : "none"}
      role={label ? "img" : "presentation"}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      data-figure={kind}
    >
      {painting === "gradient" && (
        <defs>
          {/*
            * Fading at both ends, so a strand dissolves instead of stopping at the edge of its box. A
            * thread at full strength where it meets the boundary draws a line where the boundary is, and
            * the boundary is not meant to be part of the picture.
            *
            * The fades are short — six per cent each — and the middle is bright and long. An earlier
            * version spent more than a fifth of the figure fading in and another fifth fading out, which
            * left the top and bottom of the hero at roughly a quarter strength and made the field look
            * like it had been cropped by the page rather than woven into it.
            *
            * ---------------------------------------------------------------------------
            * `userSpaceOnUse`, and this is the subtle one
            * ---------------------------------------------------------------------------
            *
            * The default for a `linearGradient` is `objectBoundingBox`: its coordinates are fractions of
            * *the element's own bounding box*. For a vertical thread that box is the whole figure, so a
            * fade from 0 to 1 fades over the whole figure — and it happens to work, which is exactly why
            * the bug below survived the first version of this page.
            *
            * For a near-horizontal strand it does not work at all. The weft's own bounding box is a few
            * dozen units tall, so a gradient *meant* to fade over nine hundred units collapses into that
            * sliver: the line paints at whatever opacity corresponds to its own slight wander, which is
            * near zero for most of its length. The weft was in the DOM, at the right weight, at full
            * opacity — and invisible. It took looking at the whole hero to notice that the figure read as
            * a comb rather than as a weave.
            *
            * The deeper problem is that `objectBoundingBox` means two threads *in the same figure* fade
            * over different spans, so the field has no shared light. In user space every strand crosses
            * the same gradient at the same height, which is what makes the figure look *lit* rather than
            * merely faded at its edges.
            */}
          <linearGradient
            id={gradient}
            gradientUnits="userSpaceOnUse"
            x1={0}
            y1={0}
            x2={0}
            y2={box.height}
          >
            <stop offset="0" stopColor="var(--thread)" stopOpacity={0} />
            <stop offset="0.06" stopColor="var(--thread-bright)" stopOpacity={0.8} />
            <stop offset="0.5" stopColor="var(--thread-bright)" stopOpacity={1} />
            <stop offset="0.8" stopColor="var(--thread)" stopOpacity={0.85} />
            <stop offset="1" stopColor="var(--thread)" stopOpacity={0} />
          </linearGradient>
        </defs>
      )}

      <g opacity={opacity}>
        {/* The lattice's connections, under its joints. */}
        {shape.edges.map(([a, b], index) => (
          <line
            key={`e${index}`}
            x1={a.x}
            y1={a.y}
            x2={b.x}
            y2={b.y}
            stroke="var(--thread-line)"
            strokeWidth={1}
            vectorEffect="non-scaling-stroke"
          />
        ))}

        {/*
          * The paths.
          *
          * `non-scaling-stroke` keeps a hairline a hairline however the figure is scaled, which matters
          * because these are drawn into boxes of wildly different sizes: the hero, a decorative strip in a
          * movement, and a 1200×630 share card.
          */}
        {shape.paths.map((d, index) => (
          <path
            key={`p${index}`}
            d={d}
            fill="none"
            stroke={painting === "gradient" ? `url(#${gradient})` : "var(--thread-line-strong)"}
            strokeWidth={shape.weights?.[index] ?? 1}
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            /*
             * The figure's own shading, per strand.
             *
             * `strokeOpacity` rather than `opacity`, deliberately. Element opacity in SVG makes the
             * browser composite that element into its own group, which is a separate render pass per
             * path; `stroke-opacity` is a paint property and costs nothing. On a figure with thirty-odd
             * paths that difference is real, and the hero redraws every frame.
             */
            strokeOpacity={shape.shades?.[index] ?? 1}
          />
        ))}

        {/*
          * The nodes — lattice joints and ring sources. The accent, because a node is a *thing* where a
          * thread is only a line.
          */}
        {shape.nodes.map((point, index) => (
          <circle
            key={`n${index}`}
            cx={point.x}
            cy={point.y}
            r={2}
            fill="var(--accent)"
            vectorEffect="non-scaling-stroke"
          />
        ))}
      </g>
    </svg>
  );
}

/**
 * A still figure, at a fixed size.
 *
 * Used for every figure on the page except the hero's. Computed on the server, rendered into the static
 * HTML, and never re-rendered — which is why five of the six figures here cost a visitor nothing at all.
 *
 * The seed is a required prop rather than a default, because two stills with the same seed would be the
 * same drawing twice, and that is the one thing a page of generative figures must not do.
 */
export function Still({
  kind,
  seed,
  width = 1200,
  height = 620,
  detail = 1,
  className,
  opacity,
  label,
  id,
}: {
  kind: FigureKind;
  seed: number;
  width?: number;
  height?: number;
  detail?: number;
  className?: string;
  opacity?: number;
  label?: string;
  id: string;
}) {
  const shape = figure(kind, { width, height, seed, detail });
  return (
    <FigureSvg
      kind={kind}
      shape={shape}
      className={className}
      opacity={opacity}
      label={label}
      id={id}
    />
  );
}
