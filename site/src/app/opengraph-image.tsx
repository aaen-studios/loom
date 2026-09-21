import { ImageResponse } from "next/og";
import { SITE } from "@/lib/site";
import { profile } from "@/lib/weave";

/**
 * The Open Graph card: the weave, as a picture.
 *
 * ---------------------------------------------------------------------------
 * What this card is doing
 * ---------------------------------------------------------------------------
 *
 * A share card has one job — be legible next to nine other links in a timeline — and most cards for
 * products like this fail it the same way, by drawing the product's logo large and saying nothing. This
 * one draws the page's own material: a field of threads under tension in the application's dark palette,
 * with the claim beneath them.
 *
 * The thread positions come from `profile()` — the same sum-of-sines the hero figure uses — with a
 * different seed. They are drawn as *bars* rather than as paths for one reason, and it is the renderer
 * rather than the design: Satori's SVG support is unpredictable and its flexbox support is not, so a row
 * of thin divs is a reliable thread field where a `<path>` is a gamble. This is the only place on the
 * site where the artwork is not drawn with a path.
 *
 * ---------------------------------------------------------------------------
 * Three constraints shape every line below
 * ---------------------------------------------------------------------------
 *
 *  1. **It cannot see CSS variables.** Satori renders only the inline styles it is given and never loads
 *     the stylesheet, so `var(--accent)` here would silently render as nothing at all — a bug you
 *     discover when someone shares the link. Every colour below is a literal from the application's own
 *     dark palette: `--accent` at `#8ea2ff`, `--thread-bright` at `#bed0ff`, the dark `--ink` at
 *     `#f4f6fc`, `--ink-soft` composited over the ground to `#a9b0c0`, the glass border at
 *     `rgba(255,255,255,0.12)`, and the pre-mount ground itself at `#070a12`.
 *
 *  2. **An element with more than one child needs an explicit `display`.** This includes text
 *     interpolation: `{n} panels` is two child nodes, and Satori refuses it outright unless the parent
 *     is told how to lay them out. Every string below that mixes a value with a word is a single
 *     template literal.
 *
 *  3. **A style value of `undefined` fails the whole build**, with "Cannot read properties of undefined
 *     (reading 'trim')", which names nothing at all. So every style is stated unconditionally, including
 *     the ones that look inferable.
 */
export const alt = "Loom — a window that holds everything";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

/** How many threads the field draws, and how tall it is. */
const THREADS = 108;
const FIELD = 236;

/** The seed. Different from the hero's, so the card is not a copy of the top of the page. */
const SEED = 6011;

export default function OpengraphImage() {
  const threads = profile(SEED, THREADS);
  const weft = profile(SEED + 91, 4);

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          backgroundColor: "#070a12",
          padding: "54px 62px",
          fontFamily: "sans-serif",
        }}
      >
        {/* The running head. */}
        <div style={{ display: "flex", alignItems: "center", gap: 13 }}>
          <svg width="30" height="30" viewBox="0 0 24 24" fill="none">
            <path
              d="M6.5 5.5c0 6.5 5.5 6.5 5.5 13"
              stroke="#8ea2ff"
              strokeWidth="2"
              strokeLinecap="round"
            />
            <path
              d="M12 5.5c0 6.5 5.5 6.5 5.5 13"
              stroke="#8ea2ff"
              strokeWidth="2"
              strokeLinecap="round"
            />
            <path
              d="M6.5 18.5h11"
              stroke="#8ea2ff"
              strokeWidth="2"
              strokeLinecap="round"
              opacity="0.55"
            />
          </svg>
          <div style={{ display: "flex", fontSize: 25, fontWeight: 600, color: "#f4f6fc" }}>
            Loom
          </div>
          <div style={{ display: "flex", marginLeft: "auto", fontSize: 17, color: "#7d8497" }}>
            {SITE.url.replace("https://", "")}
          </div>
        </div>

        {/* The field. Threads whose horizontal offset comes from the profile, and whose opacity
            follows the same curve — so the field has a *shape* rather than being a flat barcode. */}
        <div
          style={{
            display: "flex",
            position: "relative",
            height: FIELD,
            marginTop: 34,
            alignItems: "flex-start",
            overflow: "hidden",
          }}
        >
          {threads.map((value, index) => (
            <div
              key={index}
              style={{
                display: "flex",
                width: 1,
                height: FIELD,
                marginLeft: index === 0 ? 0 : 9,
                marginTop: Math.round(value * 42),
                backgroundColor:
                  value > 0.62 ? "rgba(190, 208, 255, 0.72)" : "rgba(142, 162, 255, 0.26)",
              }}
            />
          ))}

          {/* The weft: four hairlines crossing the field, at positions from a second profile. Each is
              absolutely placed so it crosses every thread rather than sitting between two of them. */}
          {weft.map((value, index) => (
            <div
              key={`weft-${index}`}
              style={{
                display: "flex",
                position: "absolute",
                left: 0,
                right: 0,
                top: Math.round(24 + value * (FIELD - 48)),
                height: 1,
                backgroundColor: "rgba(142, 162, 255, 0.34)",
              }}
            />
          ))}
        </div>

        {/* The words, under the field rather than over it — a card with text on top of a texture is
            a card nobody reads. */}
        <div style={{ display: "flex", flexDirection: "column", marginTop: "auto" }}>
          <div
            style={{
              display: "flex",
              fontSize: 56,
              fontWeight: 600,
              color: "#f4f6fc",
              lineHeight: 1.02,
              letterSpacing: "-0.04em",
            }}
          >
            {"A window that holds everything."}
          </div>

          <div
            style={{
              display: "flex",
              fontSize: 20,
              color: "#a9b0c0",
              marginTop: 18,
              lineHeight: 1.45,
              maxWidth: 940,
            }}
          >
            {
              "A desktop workspace for AI chat and agents: a real terminal, an editor with git, and the model's reasoning kept in the transcript where it happened."
            }
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 14,
              marginTop: 24,
              fontSize: 18,
              color: "#7d8497",
            }}
          >
            <div style={{ display: "flex" }}>{"Windows"}</div>
            <div style={{ display: "flex", color: "#39404f" }}>|</div>
            <div style={{ display: "flex" }}>{"8 panels · 12 provider presets"}</div>
            <div style={{ display: "flex", color: "#39404f" }}>|</div>
            <div style={{ display: "flex" }}>{"Free · MIT licensed"}</div>
            <div style={{ display: "flex", marginLeft: "auto", color: "#8ea2ff" }}>
              {"12 models, or your own"}
            </div>
          </div>
        </div>
      </div>
    ),
    size,
  );
}
