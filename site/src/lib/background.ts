/**
 * The two colours the application paints into its window before its bundle loads.
 *
 * That is the whole of this module, and the narrowing is the point. An earlier version
 * described a *backdrop*: the app's Porcelain preset, its woven texture, four radial washes, a
 * film-grain overlay and a fifty-two second drift. All of it went, and what survived is the
 * part that has to be exact — because it is the colour that is on screen before anything else
 * is, and being one shade out shows up as a flash.
 *
 * ---------------------------------------------------------------------------
 * Why these two values rather than a colour chosen here
 * ---------------------------------------------------------------------------
 *
 * They are the app's own pre-mount colours. The app writes one of them onto `<html>` before its
 * bundle loads, so the window's first frame is already right; this page does the same thing for
 * the same reason, with the same two values, so moving between the site and the application is
 * not a change of colour.
 *
 * They are also unavoidably a copy: the app states them in `src/styles.css`, this project
 * cannot import the app's TypeScript at build time, and the theme-boot script in `layout.tsx`
 * can read neither. Three copies exist because three contexts need the value at a moment when
 * the others are unavailable. `background.test.ts` asserts the *arrangement* the app uses
 * rather than the value — see the long note there, which records the bug that taught the
 * difference.
 */

/** The app's light pre-mount colour: Porcelain's base. */
export const LIGHT = "#eef1f7";

/** The app's dark pre-mount colour. */
export const DARK = "#070a12";

/** Both, by the theme name the boot script has already applied. */
export const PAGE = { light: LIGHT, dark: DARK } as const;

export type ThemeName = keyof typeof PAGE;
