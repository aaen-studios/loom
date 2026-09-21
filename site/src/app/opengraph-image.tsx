import { ImageResponse } from "next/og";
import { SITE } from "@/lib/site";
import { FIGURES, SECTIONS, TABLES } from "@/lib/document";

/**
 * The Open Graph card: the cover of the manual.
 *
 * ---------------------------------------------------------------------------
 * What this replaced, and why the replacement is the same idea twice
 * ---------------------------------------------------------------------------
 *
 * The previous card drew a seven-row weaving draft in squared cells, which was the
 * previous site's whole conceit rendered as a picture. It was competent and it
 * communicated nothing: a share card showing an unlabelled grid means whatever the
 * reader already thought, which for most readers is "a grid". A card that only makes
 * sense once you have learned a notation has failed at the one job a card has, which
 * is to be legible in a timeline next to nine other links.
 *
 * A book cover is the opposite: it says the title, what the thing is, and how much of
 * it there is — in that order, at a glance, with no prerequisite. That is what a
 * technical publisher has put on the front of a manual for a hundred years and it is
 * the correct answer here, because it is the same answer to the same problem.
 *
 * ---------------------------------------------------------------------------
 * Two Satori constraints shape every line below
 * ---------------------------------------------------------------------------
 *
 *  1. **It cannot see CSS variables.** Satori renders only the inline styles it is
 *     given and never loads the stylesheet, so `var(--accent)` here would silently
 *     render as nothing at all — a bug you discover when someone shares the link.
 *     Every colour below is a literal from the application's palette: the dark
 *     `--accent` (`#8ea2ff`), the dark `--ink` (`#f4f6fc`), the dark `--ink-soft`
 *     (`rgb(244 246 252 / 0.76)` as a hex), and the pre-mount ground (`#070a12`).
 *  2. **An element with more than one child needs an explicit `display`.** This
 *     includes text interpolation: `{n} sections` is two child nodes and Satori
 *     refuses it outright unless the parent is told how to lay them out. Every
 *     string below that mixes a value with a word is a single template literal, and
 *     that is not a stylistic preference — it is the difference between a card that
 *     builds and a card that renders a bare number with the word missing.
 *
 * There is no grid either: rows and columns are nested flex containers with fixed
 * sizes, because Satori implements only part of flexbox and none of grid.
 */
export const alt = "Loom — a desktop workspace for AI chat and agents";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

/**
 * The cover's facts, read from the registers.
 *
 * Deliberately imported rather than hardcoded, unlike the thread count the previous
 * card took from its own drawing. A cover that says "7 sections" over a manual with
 * eight is the kind of small lie that costs more than an import saves — and unlike a
 * decorative grid, these numbers are the card's actual content.
 */
const FACTS = [
  `${SECTIONS.length} sections`,
  `${FIGURES.length} figures`,
  `${TABLES.length} tables`,
  "MIT licensed",
] as const;

export default function OpengraphImage() {
  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          backgroundColor: "#070a12",
          padding: "68px 76px",
          fontFamily: "sans-serif",
        }}
      >
        {/* The running head: the mark and the name, at the size a cover uses. */}
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <svg width="34" height="34" viewBox="0 0 24 24" fill="none">
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
          <div style={{ display: "flex", fontSize: 26, fontWeight: 600, color: "#f4f6fc" }}>
            Loom
          </div>
          <div
            style={{
              display: "flex",
              fontSize: 20,
              color: "#7d8497",
              marginLeft: 10,
            }}
          >
            The manual
          </div>
        </div>

        {/* The title block, pushed to sit low on the cover as a title does. */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            marginTop: "auto",
          }}
        >
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              borderTop: "2px solid #f4f6fc",
              paddingTop: 30,
            }}
          >
            <div
              style={{
                display: "flex",
                fontSize: 66,
                fontWeight: 600,
                color: "#f4f6fc",
                lineHeight: 1.05,
                letterSpacing: "-0.035em",
                maxWidth: 900,
              }}
            >
              An agent you can watch work.
            </div>

            <div
              style={{
                display: "flex",
                fontSize: 23,
                color: "#a9b0c0",
                marginTop: 22,
                maxWidth: 820,
                lineHeight: 1.45,
              }}
            >
              A desktop workspace for AI chat and agents: a real terminal, an editor
              with git, and the model&rsquo;s reasoning kept in the transcript where it
              happened.
            </div>
          </div>

          {/* The four facts, on one line, tabular and separated by rules — the
              register a cover carries. */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 18,
              marginTop: 34,
              fontSize: 19,
              color: "#7d8497",
            }}
          >
            {FACTS.map((fact, index) => (
              <div key={fact} style={{ display: "flex", alignItems: "center", gap: 18 }}>
                {index > 0 && <div style={{ display: "flex", color: "#39404f" }}>|</div>}
                <div style={{ display: "flex" }}>{fact}</div>
              </div>
            ))}
            <div style={{ display: "flex", flexGrow: 1 }} />
            <div style={{ display: "flex", color: "#8ea2ff" }}>
              {SITE.url.replace("https://", "")}
            </div>
          </div>
        </div>
      </div>
    ),
    size,
  );
}
