import { useEffect, useMemo, useState } from "react";
import {
  BASE_ACCENT,
  BASE_PALETTE,
  PALETTE_TOKENS,
  type Swatch,
  deriveAdaptive,
  enforceContrast,
  fromPaletteHex,
  swatchSignature,
  swatchesFromGradient,
  tokensFor,
  type Palette,
} from "./palette";
import { resolvePreset } from "./background";
import { sampleBackground } from "./sampleImage";
import { assetUrl } from "./tauri";
import type { AppConfig, PaletteConfig } from "../types";

/**
 * Turning a palette choice into the custom properties the app paints with.
 *
 * The tokens are written to `<html>` as inline custom properties, which beat the
 * stylesheet's own declarations by specificity. That is deliberate: the
 * stylesheet keeps shipping a complete, correct default theme, and a custom or
 * adaptive palette *overlays* it rather than replacing it. Switching back to
 * Default is then a matter of removing the properties, with nothing to restore.
 *
 * `--ansi-*` and `--terminal-*` are never touched. A terminal has its palette on
 * purpose, and it reads `--ink` for its foreground, so it inherits a custom
 * theme's text colour without having its ANSI colours scrambled by a photograph.
 */

/**
 * The palette a config resolves to, once everything legible is enforced.
 *
 * `swatches` is passed in rather than sampled here so this stays a pure
 * function: the sampling is async and browser-only, and mixing the two would
 * make the contrast guarantees untestable.
 */
export function resolvePalette(
  palette: PaletteConfig,
  isDark: boolean,
  swatches: Swatch[],
): Palette | null {
  const base = BASE_PALETTE[isDark ? "dark" : "light"];

  switch (palette.mode) {
    case "default":
      // Null means "leave the stylesheet alone" — the properties are removed.
      return null;
    case "custom":
      return enforceContrast(fromPaletteHex(palette, base));
    case "adaptive":
      return deriveAdaptive(swatches, isDark, BASE_ACCENT[isDark ? "dark" : "light"]);
  }
}

/**
 * The palette `App` resolved, shared with the Settings panel.
 *
 * A React context rather than a second `usePalette` call, because the hook
 * *installs* the tokens: calling it twice would write every custom property
 * twice per change and give two components independent ideas of what is
 * applied. The panel only needs to read what is already there.
 */
let applied: Palette | null = null;

/**
 * One shared empty list, so "this theme has no swatches" is a stable value.
 *
 * Returning a fresh `[]` from a hook each render is harmless here — the memo
 * key is a string — but this keeps the intent obvious.
 */
const NO_SWATCHES: Swatch[] = [];

/** Readers to wake when the applied palette changes. */
const listeners = new Set<() => void>();

export function setAppliedPalette(palette: Palette | null): void {
  // Identity, not deep equality: `usePalette` memoises on a signature string,
  // so it hands back the same object whenever the derived colours are
  // unchanged. Equal identity therefore means "nothing to tell anyone".
  if (palette === applied) return;
  applied = palette;
  for (const listener of listeners) listener();
}

/**
 * Notified when the applied palette changes.
 *
 * Settings renders the palette `App` installed rather than deriving a second
 * one, so it needs to know when that value moves. Without this the panel only
 * ever saw whatever `getAppliedPalette()` happened to return on its last
 * render, which is why picking a new picture left the old swatches on screen.
 */
export function subscribeAppliedPalette(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getAppliedPalette(): Palette | null {
  return applied;
}

/**
 * Applies a palette to the document, or clears the overrides.
 *
 * Clearing matters as much as setting: every key in `PALETTE_TOKENS` is removed
 * before a new palette is written, so a token that the new palette does not
 * define cannot be left behind from the previous one. Without this, switching
 * from Custom to Default would leave custom colours on whatever the last
 * palette happened not to set.
 */
export function applyPalette(
  palette: Palette | null,
  root: HTMLElement = document.documentElement,
): void {
  // Cleared first, always. Every key is removed before a new palette is written
  // so a token the new one does not define cannot survive from the previous one
  // — without this, switching Custom → Default would leave custom colours on
  // whatever the last palette happened not to set.
  for (const token of PALETTE_TOKENS) root.style.removeProperty(token);
  if (!palette) return;

  const tokens = tokensFor(palette);
  for (const [token, value] of Object.entries(tokens)) {
    root.style.setProperty(token, value);
  }

  // The window backdrop follows the palette, so a custom surface does not sit
  // on the previous theme's colour while the first frame paints.
  const backdrop = tokens["--app-backdrop"];
  if (backdrop) root.style.background = backdrop;
}

/**
 * The swatches an adaptive palette derives from.
 *
 * Two sources, because Loom has two kinds of background. A custom image or
 * video is decoded once and cached. A built-in preset has no file to read, but
 * every preset already carries a `swatch` gradient of its literal colours, and
 * deriving from that is what lets Adaptive do something on the woven Porcelain
 * the app opens with — previously it returned nothing at all for a preset, so
 * the mode silently did nothing on the backgrounds most people use.
 */
function useBackgroundSwatches(config: AppConfig, isDark: boolean): Swatch[] {
  const { kind, path, preset } = config.background;
  const [sampled, setSampled] = useState<Swatch[]>(NO_SWATCHES);
  const adaptive = config.palette.mode === "adaptive";

  // Resolved, so that `auto` follows the theme: switching to dark must re-derive
  // rather than keep the light preset's colours.
  const presetSwatches = useMemo(
    () => swatchesFromGradient(resolvePreset(preset, isDark).swatch),
    [preset, isDark],
  );

  useEffect(() => {
    // Only adaptive needs the picture. Sampling for a mode that will not use it
    // would decode a wallpaper on every launch for nothing.
    if (!adaptive || kind === "builtin" || !path) {
      setSampled(NO_SWATCHES);
      return;
    }
    let live = true;
    // `assetUrl`, not `path`. This sampler decodes through an `Image` element,
    // and a raw Windows path (`C:\…`) is not something the webview can fetch:
    // the element fired `error`, the sampler returned no swatches by design,
    // and adaptive fell back to the default accent for *every* custom
    // background. `Background.tsx` has always rendered the same file through
    // `assetUrl`; this addresses it the same way, so the two agree on what the
    // picture is.
    void sampleBackground(kind, assetUrl(path)).then((found) => {
      // The markup may have moved on while the image decoded — a new file
      // picked, or the mode changed — and applying a stale sample would tint
      // the app with the colours of a background it is no longer showing.
      if (live) setSampled(found);
    });
    return () => {
      live = false;
    };
    // `path` is the dependency that matters for "the user picked something
    // else": Loom stores every picked background under a fresh uuid-prefixed
    // name, so a new picture is always a new path. Re-picking the *same* file
    // also lands on a new path, which is what makes that case work too.
  }, [adaptive, kind, path]);

  if (!adaptive) return NO_SWATCHES;
  return kind === "builtin" ? presetSwatches : sampled;
}

/**
 * Installs the palette, and re-derives it whenever anything it depends on
 * changes: the mode, the three custom colours, the base theme, or the background
 * the sample comes from.
 */
export function usePalette(config: AppConfig): Palette | null {
  const isDark = config.theme === "dark";
  // Sampled once per background, and only while the mode needs it.
  const swatches = useBackgroundSwatches(config, isDark);

  /**
   * The inputs that actually change the tokens, flattened to a string.
   *
   * `resolvePalette` rebuilds its result on every render and `swatches` is a
   * fresh array per sample, so depending on object identity would re-apply every
   * custom property continuously. These three hex values plus the base are
   * precisely what `tokensFor` reads, so a change in them is exactly when the
   * work is worth doing.
   */
  const signature = [
    config.palette.mode,
    isDark ? "dark" : "light",
    config.palette.accent,
    config.palette.ink,
    config.palette.surface,
    // The swatches only matter to `adaptive`, and every colour and weight in
    // them is part of the key. A count plus the dominant red channel was not
    // enough to tell two pictures apart, so a new background could keep the
    // previous one's colours — which is precisely what "adaptive does not
    // update" looked like from outside.
    config.palette.mode === "adaptive" ? swatchSignature(swatches) : "-",
  ].join("|");

  const palette = useMemo(
    () => resolvePalette(config.palette, isDark, swatches),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [signature],
  );

  useEffect(() => {
    applyPalette(palette);
    // Mirrored for the Settings panel, which shows what Adaptive derived. The
    // panel subscribes to this rather than deriving a second palette, so its
    // strip always reflects the colours actually applied.
    setAppliedPalette(palette);
  }, [palette]);

  return palette;
}

/** Whitelisted token names, re-exported so the panel can list what is themed. */
export { PALETTE_TOKENS };
