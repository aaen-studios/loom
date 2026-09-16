import { ImageResponse } from "next/og";
import { SITE } from "@/lib/site";

/**
 * The Open Graph card.
 *
 * Rendered by `next/og`, so it is generated from the same strings as the site
 * rather than being a static PNG that goes stale the first time the tagline
 * changes.
 *
 * Two constraints shape what is below:
 *
 * 1. **`ImageResponse` has no access to CSS variables.** It renders through
 *    Satori, which only understands the styles given to it inline and does not
 *    load the stylesheet — so a `var(--accent)` here would silently render
 *    nothing. The colours are therefore hardcoded, and they are the app's own
 *    values from `src/styles.css` (`--accent`, `--ink`, `--ink-soft`, and the
 *    Porcelain background's base).
 * 2. **Only a subset of flexbox and no grid.** Everything is a flex row or
 *    column with explicit sizes, and the mark is inlined as SVG paths rather
 *    than imported, because a component returning `<svg>` cannot be used here.
 */
export const alt = "Loom — a desktop app for AI chat and agents";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function OpengraphImage() {
  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          // The same layered wash as the site's Porcelain background, flattened
          // into a gradient Satori can render.
          background:
            "linear-gradient(158deg, #f9fbff 0%, #eef1f7 52%, #e7ebf5 100%)",
          padding: "72px 80px",
          fontFamily: "sans-serif",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          {/* The mark, inline. `viewBox` + `path` only: Satori renders SVG but
              will not run the site's CSS animation. */}
          <svg width="44" height="44" viewBox="0 0 24 24" fill="none">
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
          <div style={{ fontSize: 34, fontWeight: 600, color: "#0a0c16" }}>
            Loom
          </div>
        </div>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            marginTop: "auto",
          }}
        >
          <div
            style={{
              fontSize: 62,
              fontWeight: 600,
              color: "#0a0c16",
              lineHeight: 1.1,
              letterSpacing: "-0.02em",
              maxWidth: 900,
            }}
          >
            An AI agent that runs on your machine.
          </div>
          <div
            style={{
              fontSize: 26,
              color: "#3a3f52",
              marginTop: 24,
              maxWidth: 860,
              lineHeight: 1.4,
            }}
          >
            Streaming replies with visible reasoning, real permission modes, MCP
            servers, and a workspace it can search.
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 14,
              marginTop: 40,
              fontSize: 22,
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
