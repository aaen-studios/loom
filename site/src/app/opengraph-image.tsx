import { ImageResponse } from "next/og";
import { SITE } from "@/lib/site";

/**
 * The Open Graph card, drawn as a weaving draft.
 *
 * A draft is the notation a weaver draws before starting: a grid of squared cells,
 * some filled in, showing which threads lift on which pick. It is the most
 * recognisable image in the craft and it is what this card is — twelve columns, a
 * few lifted threads, and the mark.
 *
 * ---------------------------------------------------------------------------
 * Two constraints shape everything below
 * ---------------------------------------------------------------------------
 *
 *  1. **`ImageResponse` cannot see CSS variables.** It renders through Satori,
 *     which understands only the inline styles it is given and never loads the
 *     stylesheet — so `var(--accent)` here would silently render as nothing at all,
 *     which is a bug you find out about when someone shares the link. Every colour
 *     below is a literal, taken from the app's own palette in `src/styles.css`: the
 *     light `--accent` (`#4f5bd5`), the light `--thread-bright` (`#6070e2`), the
 *     light `--ink` (`#0a0c16`), and Porcelain's base (`#eef1f7`).
 *  2. **No grid, and only part of flexbox.** Everything is a flex row or column with
 *     explicit sizes, and the mark is inlined as SVG paths rather than imported — a
 *     component returning `<svg>` cannot be used here.
 */
export const alt = "Loom — a desktop app for AI chat and agents";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

/**
 * One pick of the draft: which of the twelve threads are lifted.
 *
 * Written as data rather than as nested loops so the pattern is editable by eye —
 * this is meant to look like a draft, and a draft is read as rows. The first row is
 * fully open because that is where the cloth starts.
 */
const DRAFT: readonly boolean[][] = [
  [true, true, true, true, true, true, true, true, true, true, true, true],
  [true, false, true, false, false, true, false, false, true, false, false, true],
  [true, false, true, false, false, true, false, false, true, false, false, true],
  [false, false, true, true, false, false, true, true, false, false, true, false],
  [false, false, true, true, false, false, true, true, false, false, true, false],
  [false, true, false, false, true, true, false, false, true, true, false, false],
  [false, true, false, false, true, true, false, false, true, true, false, false],
];

/**
 * The draft's width, read from the draft itself.
 *
 * Not imported from `components/chrome/warp.tsx`: that would pull a React component
 * into this route's bundle to obtain one integer, and the two would then have to
 * stay in step for a reason neither of them can state. The rows above are the
 * pattern; their length is the thread count.
 */
const THREADS = DRAFT[0].length;

export default function OpengraphImage() {
  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          background: "linear-gradient(158deg, #f9fbff 0%, #eef1f7 52%, #e7ebf5 100%)",
          padding: "64px 72px",
          fontFamily: "sans-serif",
        }}
      >
        {/* The mark and the name, as on every page. */}
        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          <svg width="40" height="40" viewBox="0 0 24 24" fill="none">
            <path
              d="M6.5 5.5c0 6.5 5.5 6.5 5.5 13"
              stroke="#4f5bd5"
              strokeWidth="1.9"
              strokeLinecap="round"
            />
            <path
              d="M12 5.5c0 6.5 5.5 6.5 5.5 13"
              stroke="#4f5bd5"
              strokeWidth="1.9"
              strokeLinecap="round"
            />
            <path
              d="M6.5 18.5h11"
              stroke="#4f5bd5"
              strokeWidth="1.9"
              strokeLinecap="round"
              opacity="0.55"
            />
          </svg>
          <div style={{ fontSize: 32, fontWeight: 600, color: "#0a0c16" }}>Loom</div>
        </div>

        <div style={{ display: "flex", flexDirection: "column", marginTop: "auto" }}>
          <div
            style={{
              fontSize: 60,
              fontWeight: 600,
              color: "#0a0c16",
              lineHeight: 1.08,
              letterSpacing: "-0.02em",
              maxWidth: 840,
            }}
          >
            An AI agent that runs on your machine.
          </div>

          <div
            style={{
              fontSize: 24,
              color: "#3a3f52",
              marginTop: 20,
              maxWidth: 760,
              lineHeight: 1.4,
            }}
          >
            Streaming replies with visible reasoning, real permission modes, MCP
            servers, and a workspace it can search.
          </div>

          {/* The draft itself: twelve threads across, seven picks down, the lifted
              ones filled. Drawn with nested flex rows because Satori has no grid,
              and sized so it reads as notation rather than as noise. */}
          <div style={{ display: "flex", flexDirection: "column", gap: 5, marginTop: 36 }}>
            {DRAFT.map((pick, rowIndex) => (
              <div key={rowIndex} style={{ display: "flex", gap: 5 }}>
                {Array.from({ length: THREADS }, (_, column) => (
                  <div
                    key={column}
                    style={{
                      width: 22,
                      height: 12,
                      borderRadius: 2,
                      background: pick[column]
                        ? "#6070e2"
                        : "rgba(10, 12, 22, 0.07)",
                    }}
                  />
                ))}
              </div>
            ))}
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 14,
              marginTop: 34,
              fontSize: 20,
              color: "#5a6076",
            }}
          >
            <div>{SITE.url.replace("https://", "")}</div>
            <div style={{ color: "#b9c0d0" }}>·</div>
            <div>Windows</div>
            <div style={{ color: "#b9c0d0" }}>·</div>
            <div>Free</div>
            <div style={{ color: "#b9c0d0" }}>·</div>
            <div>MIT licensed</div>
          </div>
        </div>
      </div>
    ),
    size,
  );
}
