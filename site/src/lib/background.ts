/**
 * The two colours the application paints into its window before React mounts.
 *
 * This used to be a whole backdrop — a woven texture over four radial washes, with
 * a film-grain overlay and a 52-second drift. All of it is gone, and the reason is
 * the organising idea of the site: this is a document, and a document is set on
 * paper. A page of long-form prose under a drifting gradient has to compete with
 * its own wallpaper, and putting glass on it reads as a mock-up of the
 * application rather than a description of it.
 *
 * So the page is flat, and what is left here is the part that has to be exact.
 *
 * ---------------------------------------------------------------------------
 * Why these two values, and not a colour of this project's own choosing
 * ---------------------------------------------------------------------------
 *
 * They are the app's own pre-mount backdrop, in both themes. The app writes one of
 * them onto `<html>` before its bundle loads, so the first frame of the window is
 * already the right colour; this page does the same thing for the same reason, and
 * uses the same two values so switching between the documentation and the
 * application is not a change of colour.
 *
 * They are also, unavoidably, a copy: `src/styles.css` states them for the app,
 * this project cannot import the app's TypeScript at build time, and the theme-boot
 * script in `layout.tsx` cannot read either. Three copies exist because three
 * contexts need the value at a moment when none of the others is available.
 * `background.test.ts` reads the app's source and asserts that Porcelain still
 * resolves its base from the same constant, and that this file still agrees with
 * it — so the copy cannot rot silently.
 */

/** The app's light pre-mount colour: Porcelain's base. */
export const LIGHT = "#eef1f7";

/** The app's dark pre-mount colour. */
export const DARK = "#070a12";

/** Both, by the theme name the boot script has already applied. */
export const PAGE = { light: LIGHT, dark: DARK } as const;

export type ThemeName = keyof typeof PAGE;

/**
 * How hard the app veils its own background in dark mode, 0–100.
 *
 * Not used by this site, and deliberately still exported, because it is the value
 * that *used* to make dark mode unreadable here and the name is the cheapest way
 * to explain why it no longer applies: the app's dark palette is near-white ink,
 * which needs the artwork underneath held down. There is no artwork underneath
 * this page, so there is nothing to hold down — the ground is already dark, and
 * `background.test.ts` asserts the contrast rather than the veil.
 */
export const DARK_DIM_FLOOR = 48;
