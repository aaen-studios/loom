/**
 * Tailwind v4 as a PostCSS plugin.
 *
 * The app's design tokens reach this project as a generated stylesheet which
 * `src/app/globals.css` pulls in with a plain CSS `@import`. Tailwind has to
 * inline that sheet *before* it resolves its own directives, or the `@utility`
 * definitions the whole site is built from would simply not exist by the time
 * the utilities are generated. `GATE-TEST.md` records the check that proved it
 * does, against the emitted CSS rather than the build log.
 */
export default {
  plugins: {
    "@tailwindcss/postcss": {},
  },
};
