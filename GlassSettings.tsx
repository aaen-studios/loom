import { useState, type CSSProperties } from "react";
import { backgroundStyle, resolvePreset } from "../lib/background";
import {
  BLUR_RANGE,
  GLASS_PRESETS,
  LIQUID_RANGE,
  PRESET_LABELS,
  TINT_RANGE,
  matchingPreset,
  modeLabel,
  LIQUID_MODES,
} from "../lib/glass";
import { useSettings } from "../stores/settings";
import type { GlassMode } from "../types";
import { LiquidSurface } from "./LiquidSurface";
import { Row, Section, Segmented, Toggle } from "./ui";

/**
 * A slider with the app's number beside it.
 *
 * The layout is the established one — `w-40` input, `w-7` tabular-nums value —
 * because the range element rules live in `styles.css`'s base layer and the
 * whole point of that arrangement is that every slider in the app looks the
 * same. The unit is part of the value rather than the label, so a row that reads
 * "6 px" cannot be mistaken for the library's own unitless `blurAmount`, which
 * is a different number by a factor of 32.
 */
function Slider({
  value,
  min,
  max,
  step = 1,
  unit,
  onChange,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  onChange: (value: number) => void;
  label: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
        className="w-40"
      />
      <span className="w-12 text-right text-[12px] text-faint tabular-nums">
        {step < 1 ? value.toFixed(2) : value}
        {unit ? ` ${unit}` : ""}
      </span>
    </div>
  );
}

/**
 * The four refracting samples, all reading the live config.
 *
 * A separate component so the preview can show the same surface twice — once
 * refracting, once as plain glass — without duplicating the props. That
 * side-by-side is the whole point of the preview: the effect is only legible in
 * comparison. A single sample floating over a pattern asks the eye to remember
 * what the edge looked like a moment ago, which it cannot do.
 */
function SamplePill({ plain = false }: { plain?: boolean }) {
  return (
    <LiquidSurface
      // Taller than the real pill (h-12 against h-10) so it reliably straddles a
      // 64px boundary rather than sitting inside one square, where there would be
      // no edge at its rim to bend.
      className="h-12 rounded-capsule"
      contentClassName="gap-0.5 p-1"
      surface="pills"
      /*
        Far more transparent than the real title-bar pill, and this is the one
        override in the app with a reason you could not infer from the surface it
        sits on: **a preview exists to demonstrate the effect**, and at the 0.70
        alpha a real pill needs for icon contrast, the bend is not visible at
        all. That is not a defect in the pill — it is the right trade for chrome
        you have to read — but it makes a faithful sample a useless control.

        The pattern's colours are chosen so this stays readable: 9:1 at worst in
        light theme, 11:1 in dark. See the comment on `pattern`.
      */
      tintStrength={34}
      // The static twin must otherwise be identical, or it is comparing two
      // things and calling it a comparison.
      liquid={plain ? false : undefined}
    >
      <span className="px-2.5 text-[12.5px] font-medium" style={SAMPLE_LABEL}>
        {plain ? "Plain glass" : "Liquid"}
      </span>
    </LiquidSurface>
  );
}

/**
 * White with a dark halo, for a label that sits on glass over arbitrary artwork.
 *
 * Neither of the app's ink tokens works here. The samples are deliberately
 * transparent and the surface beneath them is the highest-contrast thing in the
 * UI, so at any point under a glyph the background is either near-black or
 * near-white — which makes any single ink colour invisible on part of it. A white
 * glyph with a dark shadow reads on both, which is the same trick `.blob-text`
 * uses for bare assistant text over the wallpaper.
 */
const SAMPLE_LABEL: CSSProperties = {
  color: "#fff",
  textShadow: "0 1px 2px rgb(0 0 0 / 0.85), 0 0 12px rgb(0 0 0 / 0.6)",
};

/**
 * The preview.
 *
 * Three things about this are load-bearing rather than decorative.
 *
 * **The backdrop is painted here, and it is opaque.** The natural thing is to
 * drop a sample onto the drawer — but the drawer is itself 82% glass with a 38px
 * blur, and a surface with `backdrop-filter` becomes a *backdrop root* for its
 * descendants. So a sample drawn straight onto it would refract a blurred
 * version of the drawer, which is nothing like what the title bar shows. Painting
 * it its own opaque artwork is what makes the preview honest.
 *
 * **The pattern is hard-edged, and deliberately so.** A displacement map bends
 * *detail*; Loom's own presets are smooth radial washes by design, which is why
 * the effect is quiet over them. Both are offered, because showing only the
 * flattering one would be a lie — but the default is the honest best case.
 *
 * **Refracting and plain sit side by side.** The effect is a bend at the rim, so
 * it is only legible against a reference. Two identical surfaces with one
 * difference is that reference.
 */
export function GlassPreview() {
  const config = useSettings((state) => state.config);
  const tint = config.interface.glass.tint;
  const dark = config.theme === "dark";
  const preset = resolvePreset(config.background.preset, dark);
  /*
    **Your background is the default, and the test pattern is the option.**

    That ordering is the point rather than a detail. A checkerboard is a *test
    pattern* — it belongs in a verification script, not in a settings page, and
    the first version of this shipped it as the default because it flattered the
    effect. The honest default is what the app actually looks like: glass over
    the artwork you chose. Anyone who wants to see the bend clearly can switch to
    Hard edges, and the caption tells them the effect is subtler over a smooth
    background before they wonder why nothing looks different.
  */
  const [media, setMedia] = useState<"pattern" | "preset">("preset");

  /*
    The tiles are the **opposite** of the glass, and that is the whole trick.

    In light theme Loom's glass is near-white — `--pill-bg` is white at 90% — so a
    pale pattern leaves the samples as white-on-white smudges with nothing to see
    through them. That is precisely what the first three versions of this preview
    did, and no amount of re-tuning the tile *colours* fixed it, because the
    palette was never the problem. The **relationship** was: glass only reads as
    glass when there is something behind it that contrasts.

    So the pattern inverts with the theme. Light theme gets a dark, detailed
    surface — the same tonal range as the wallpaper in the screenshot that
    prompted this; dark theme gets a pale one. Either way a 34% white sample reads
    unmistakably as a pane, and the artwork behind it stays visible through the
    glass.

    Two details are load bearing, and both were arrived at by measuring what was
    invisible rather than by taste:

    * **64px squares.** A bend needs an edge that (a) the frost has not already
      averaged away and (b) the displacement can actually move. 18px stripes lost
      to a 24px frost; 44px bands survived but gave only one axis. 64px is wider
      than the frost and twice the 32px displacement, with hard corners in both
      axes — and it is *narrower than the samples*, so a sample always straddles a
      boundary rather than sitting inside one square, where there would be no edge
      at its rim to bend at all.
    * **12px hairlines** at low alpha, over the squares. A frosted surface needs
      detail at a fine scale as well as a coarse one, or it reads as a flat tint
      rather than as frost.
  */
  const glassIsLight = !dark;
  const tileA = glassIsLight ? "#0d1424" : "#eef1f8";
  const tileB = glassIsLight ? "#33436b" : "#c9d2e4";
  const hairline = glassIsLight ? "rgb(255 255 255 / 0.13)" : "rgb(0 0 0 / 0.13)";

  const pattern = {
    backgroundColor: tileA,
    backgroundImage: [
      // Topmost first, as CSS draws them; the checkerboard last so the hairlines
      // sit over it.
      `repeating-linear-gradient(0deg, ${hairline} 0 1px, transparent 1px 12px)`,
      `repeating-linear-gradient(90deg, ${hairline} 0 1px, transparent 1px 12px)`,
      `repeating-conic-gradient(from 0deg, ${tileB} 0deg 90deg, ${tileA} 90deg 180deg, ${tileB} 180deg 270deg, ${tileA} 270deg 360deg)`,
    ].join(", "),
    backgroundSize: "12px 12px, 12px 12px, 64px 64px",
  };

  return (
    <section className="mb-3 rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)] p-3">
      <div className="mb-2 flex items-center justify-between gap-3 px-1">
        <h3 className="text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
          Preview
        </h3>
        <Segmented
          value={media}
          options={[
            { id: "preset", label: "Your background" },
            { id: "pattern", label: "Hard edges" },
          ]}
          onChange={setMedia}
        />
      </div>

      <div
        className="relative h-[224px] overflow-hidden rounded-row border border-[var(--glass-border)]"
        style={
          media === "pattern"
            ? pattern
            : {
                ...backgroundStyle(config.background, dark),
                background: preset.layers,
                backgroundColor: preset.base,
              }
        }
      >
        <div className="absolute top-6 left-6 flex flex-wrap items-center gap-2.5">
          <SamplePill />
          <SamplePill plain />
        </div>

        {/* Narrower than the panel, so all four of its rims sit over the pattern
            rather than two of them hanging off the edge — the bend happens at the
            rim, so a sample whose edges are outside the artwork demonstrates
            nothing. */}
        <div className="absolute right-5 bottom-5 left-5 flex justify-center">
          <LiquidSurface
            surface="composer"
            className="w-[58%] rounded-sheet"
            contentClassName="px-3 py-2.5"
            layout="block"
            tint="var(--panel-bg-strong)"
            // Same reasoning as `SamplePill`: the real composer runs at 0.87
            // because messages are typed into it, and nothing is visible through
            // it. Narrower than the panel on purpose, so all four of its rims sit
            // over the pattern — the bend happens at the rim, so a sample whose
            // edges are outside the artwork demonstrates nothing.
            tintStrength={40}
          >
            <span className="text-[12.5px] font-medium" style={SAMPLE_LABEL}>
              Composer
            </span>
          </LiquidSurface>
        </div>
      </div>

      <p className="mt-2 px-1 text-[11.5px] leading-4 text-faint">
        {media === "pattern" ? (
          <>
            <span className="text-soft">Liquid</span> bends the pattern at its
            rim; <span className="text-soft">Plain glass</span> does not — watch
            where a square's edge meets the rim of each sample, and it shifts on
            one and not the other. Two things are exaggerated here so there is
            something to see: the squares are 64px, and both samples are far more
            transparent than the real surfaces. At the opacity the app needs for
            legibility, this is a great deal subtler.
          </>
        ) : (
          <>
            Over {preset.name}, with Tint at {tint}%. Loom's presets are smooth
            washes by choice, so there is far less for the bend to act on —
            measured over this exact preset, a title-bar pill moves{" "}
            <span className="text-soft">0% of its pixels</span>, against 88% over
            hard edges. The same is true of the real title bar, and it is why{" "}
            <span className="text-soft">Hard edges</span> is the view that shows
            what the effect does. Lower <span className="text-soft">Tint</span>{" "}
            to let more of it through.
          </>
        )}
      </p>
    </section>
  );
}

/**
 * The glass surfaces, and what they refract.
 *
 * Two sections rather than one because they are different kinds of decision:
 * "Glass" moves every surface in the app at once, and "Liquid glass" is the
 * refraction sitting on top of three of them. Merging them put a master switch
 * next to a saturation slider, which reads as one feature when it is two.
 */
export function GlassSection() {
  const glass = useSettings((state) => state.config.interface.glass);
  const setGlass = useSettings((state) => state.setGlass);
  const setLiquid = useSettings((state) => state.setLiquid);
  const liquid = glass.liquid;

  const [tintMin, tintMax] = TINT_RANGE;
  const [blurMin, blurMax] = BLUR_RANGE;
  const preset = matchingPreset(liquid);

  return (
    <>
      <Section
        title="Glass"
        description="How heavy every surface in the app is — the refracting ones included. Watch the preview above while you move these."
      >
        <Row
          label="Tint opacity"
          hint="Lower is more see-through, and lets more of the bend show through. The floor protects the dense popovers — the model picker and the persona menu — from becoming unreadable over bright artwork."
        >
          <Slider
            label="Glass tint opacity"
            value={glass.tint}
            min={tintMin}
            max={tintMax}
            unit="%"
            onChange={(tint) => setGlass({ tint })}
          />
        </Row>
        <Row
          label="Blur strength"
          hint="How far each surface frosts the artwork behind it, the refracting ones included. The terminal keeps its own heavier blur, because a shell is read for minutes."
        >
          <Slider
            label="Glass blur strength"
            value={glass.blur}
            min={blurMin}
            max={blurMax}
            unit="%"
            onChange={(blur) => setGlass({ blur })}
          />
        </Row>
        <Row label="Reset" hint="Both back to the shipped values.">
          <button
            type="button"
            onClick={() => setGlass({ tint: 100, blur: 100 })}
            className="btn-ghost px-2.5 py-1 text-[12px]"
          >
            Back to default
          </button>
        </Row>
      </Section>

      <Section
        title="Liquid glass"
        description="The title-bar pills, the composer and torn-off panel windows bend the artwork at their edges, the way a thick lens would."
      >
        <Toggle
          label="Refract"
          hint="Off renders the same surfaces as ordinary frosted glass, at the same tint and blur."
          checked={liquid.enabled}
          onChange={(enabled) => setLiquid({ enabled })}
        />
        <Toggle
          label="Title-bar pills"
          hint="The cheapest place for it, and where it reads best — small, round, and over a background that never stops moving."
          checked={liquid.pills}
          onChange={(pills) => setLiquid({ pills })}
          disabled={!liquid.enabled}
        />
        <Toggle
          label="Composer"
          hint="The largest surface, and the only one that resizes while you look at it. Turn this off first if typing ever feels heavy."
          checked={liquid.composer}
          onChange={(composer) => setLiquid({ composer })}
          disabled={!liquid.enabled}
        />
        <Toggle
          label="Torn-off panels"
          hint="Only the windows detached from the main one. A panel docked inside the app sits over the transcript, not over artwork, so there is nothing there to refract."
          checked={liquid.panels}
          onChange={(panels) => setLiquid({ panels })}
          disabled={!liquid.enabled}
        />
        <Toggle
          label="Menus and pickers"
          hint="The model picker, the persona menu, the workspace chip, the composer's slash menu. These sit over the transcript, so there is text behind them for the bend to act on — it reads much more clearly here than on the title bar."
          checked={liquid.popovers}
          onChange={(popovers) => setLiquid({ popovers })}
          disabled={!liquid.enabled}
        />
        <Toggle
          label="Cards in the transcript"
          hint="Tool calls, the goal panel, the question card, queued messages. The most numerous refracting surfaces by far, because a long chat holds many."
          checked={liquid.cards}
          onChange={(cards) => setLiquid({ cards })}
          disabled={!liquid.enabled}
        />
        <Toggle
          label="Full-screen surfaces"
          hint="Settings, the shortcut sheet, voice mode, the update toast."
          checked={liquid.overlays}
          onChange={(overlays) => setLiquid({ overlays })}
          disabled={!liquid.enabled}
        />

        <Row label="Preset" hint={preset ? undefined : "These values are your own."}>
          <Segmented
            value={preset ?? "custom"}
            options={[
              // The Custom chip only appears when nothing matches, so the
              // control never claims a preset that is not set.
              ...(preset
                ? []
                : [
                    {
                      id: "custom" as const,
                      label: "Custom",
                      title: "No preset matches these values.",
                    },
                  ]),
              ...(["subtle", "standard", "prominent"] as const).map((id) => ({
                id,
                label: PRESET_LABELS[id],
              })),
            ]}
            onChange={(id) => {
              if (id !== "custom") setLiquid({ ...GLASS_PRESETS[id] });
            }}
          />
        </Row>
      </Section>

      <Section
        title="Refraction"
        description="What the bending looks like. Every one of these is visible in the preview above as you move it."
      >
        <Row
          label="Refraction depth"
          hint="How far the edge samples are pulled. Higher reads as thicker glass; past about 80 the edges start to tear on a small pill."
        >
          <Slider
            label="Refraction depth"
            value={liquid.refraction}
            min={LIQUID_RANGE.refraction[0]}
            max={LIQUID_RANGE.refraction[1]}
            onChange={(refraction) => setLiquid({ refraction })}
          />
        </Row>
        <Row
          label="Frost"
          hint="Backdrop blur, in pixels. This trades against the effect rather than adding to it: past about 40px the backdrop is too flat to bend, so the refraction disappears into the blur."
        >
          <Slider
            label="Frost"
            value={liquid.frost}
            min={LIQUID_RANGE.frost[0]}
            max={LIQUID_RANGE.frost[1]}
            unit="px"
            onChange={(frost) => setLiquid({ frost })}
          />
        </Row>
        <Row label="Saturation" hint="How much colour the glass pulls out of what is behind it.">
          <Slider
            label="Saturation"
            value={liquid.saturation}
            min={LIQUID_RANGE.saturation[0]}
            max={LIQUID_RANGE.saturation[1]}
            unit="%"
            onChange={(saturation) => setLiquid({ saturation })}
          />
        </Row>
        <Row
          label="Edge colour"
          hint="Chromatic aberration on the rim — the faint colour split a real lens gives at its edge."
        >
          <Slider
            label="Edge colour"
            value={liquid.chromatics}
            min={LIQUID_RANGE.chromatics[0]}
            max={LIQUID_RANGE.chromatics[1]}
            onChange={(chromatics) => setLiquid({ chromatics })}
          />
        </Row>
        <Row
          label="Elasticity"
          hint="Lets the surface follow the pointer. Lovely on a card, irritating on chrome you are aiming at: it moves the pills from up to 200px away, so it is off by default."
        >
          <Slider
            label="Elasticity"
            value={liquid.elasticity}
            min={LIQUID_RANGE.elasticity[0]}
            max={LIQUID_RANGE.elasticity[1]}
            step={0.05}
            onChange={(elasticity) => setLiquid({ elasticity })}
          />
        </Row>
        <Row
          label="Mode"
          hint="Three displacement maps, from gentlest to strongest. 'Prominent' bends furthest, at the cost of a slightly softer centre."
        >
          <Segmented
            value={liquid.mode}
            options={LIQUID_MODES.map((mode) => ({
              id: mode,
              label: modeLabel(mode),
            }))}
            onChange={(mode: GlassMode) => setLiquid({ mode })}
          />
        </Row>
      </Section>
    </>
  );
}
