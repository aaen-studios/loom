/**
 * Tailwind v4 runs as a PostCSS plugin. The generated token sheet is pulled in
 * by `src/app/globals.css` with a plain `@import`, which this plugin inlines
 * before it resolves utilities — that is the behaviour `site/src/app/globals.css`
 * relies on, and the reason `scripts/sync-site-tokens.mjs` can ship the app's
 * `@utility` definitions verbatim instead of rewriting them.
 */
export default {
  plugins: {
    "@tailwindcss/postcss": {},
  },
};
