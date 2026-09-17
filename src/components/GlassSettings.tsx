import { useState } from "react";
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
 * same. The unit is part of the value rather than the label, so a row that
 * reads "6 px" cannot be mistaken for the library's own unitless `blurAmount`,
 * which is a different number by a factor of 32.
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
 * The glass surfaces, and what they refract.
 *
 * Split into two sections rather than one because they are different kinds of
 * decision: "Glass" moves every surface in the app at once, and "Liquid glass"
 * is the refraction sitting on top of three of them. Merging them put a master
 * switch next to a saturation slider, which reads as one feature when it is two.
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
        description="How heavy every surface in the app is. Both apply everywhere — panels, pills, popovers and the composer."
      >
        <Row
          label="Tint opacity"
          hint="Lower is more see-through. The floor protects the dense popovers — the model picker and the persona menu — from becoming unreadable over bright artwork."
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
          hint="How far each surface frosts the artwork behind it. The terminal keeps its own heavier blur, because a shell is read for minutes."
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
        <Row label="Reset">
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

        <Row label="Preset" hint={preset ? undefined : "These values are your own."}>
          <Segmented
            value={preset ?? "custom"}
            options={[
                        { id: "custom" as const, label: "Custom", title: "No preset matches these values." },
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
        description="What the bending looks like. Changes here are visible on the title bar immediately."
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
        <Row label="Mode" hint="Three displacement maps, from gentlest to strongest. 'Prominent' bends furthest, at the cost of a slightly softer centre.">
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

/**
 * The refracting preview.
 *
 * The opaque rectangle is load-bearing rather than decorative. The settings
 * drawer is itself `panel-strong`, so a sample pill drawn straight onto it would
 * refract the drawer's own glass — which is to say, a blurred version of the
 * drawer, and nothing like what the title bar actually shows. So the preview
 * paints a real background preset underneath and puts the samples over that, the
 * way the title bar has one.
 *
 * The background follows the theme and the chosen preset, so switching from a
 * light preset to a dark one changes what the glass is reacting to as well as
 * the swatch it is reacting over.
 */
export function GlassPreview() {
  const config = useSettings((state) => state.config);
  const theme = config.theme;
  const dark = theme === "dark";
  const preset = resolvePreset(config.background.preset, dark);
  const [media, setMedia] = useState<"preset" | "pattern">("pattern");

  return (
    <section className="mb-3 rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)] p-3">
      <div className="mb-2 flex items-center justify-between gap-3 px-1">
        <h3 className="text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
          Preview
        </h3>
        <Segmented
          value={media}
          options={[
            { id: "pattern" as const, label: "Hard edges" },
            { id: "preset" as const, label: "Your background" },
          ]}
          onChange={setMedia}
        />
      </div>

      <div
        className="relative h-[132px] overflow-hidden rounded-row border border-[var(--glass-border)]"
        style={
          media === "pattern"
            ? // A hard-edged grid, deliberately. The refraction warps whatever
              // is behind it, and a smooth gradient gives it nothing to bend —
              // which is exactly why the effect is quiet over Loom's own
              // presets. Showing it over both is more honest than showing it
              // over whichever flatters it.
              {
                backgroundColor: dark ? "#0b1020" : "#eef1f7",
                backgroundImage:
                  "repeating-linear-gradient(45deg, rgb(255 90 90 / 0.55) 0 14px, transparent 14px 28px)," +
                  "repeating-linear-gradient(-45deg, rgb(90 120 255 / 0.55) 0 14px, transparent 14px 28px)",
              }
            : {
                ...backgroundStyle(config.background, dark),
                background: preset.layers,
                backgroundColor: preset.base,
              }
        }
      >
        <div className="absolute top-4 left-4">
          <LiquidSurface
            className="h-10 rounded-capsule"
            contentClassName="gap-0.5 p-1"
            surface="pills"
          >
            <span className="px-2 text-[12.5px] text-soft">Pills</span>
          </LiquidSurface>
        </div>

        <div className="absolute right-4 bottom-4 left-4">
          <LiquidSurface
            surface="composer"
            className="w-full rounded-sheet"
            contentClassName="px-3 py-2.5"
            contentStyle={{ display: "block" }}
            tint="var(--panel-bg-strong)"
            tintStrength={70}
            params={{ refraction: 16 }}
          >
            <span className="text-[12.5px] text-soft">Composer</span>
          </LiquidSurface>
        </div>
      </div>

      <p className="mt-2 px-1 text-[11.5px] leading-4 text-faint">
        {media === "pattern"
          ? "The effect is only as visible as the detail behind it is sharp. This is the best case."
          : `Over ${preset.name}. Loom's own presets are smooth washes by design, so the bending is subtler here — the same is true of the real title bar.`}
      </p>
    </section>
  );
}
