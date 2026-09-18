/**
 * The Glass settings: a preview of what the app actually looks like, and the
 * controls for how thick its glass is.
 *
 * ## What the preview is, and the four versions it took to get here
 *
 * It paints the user's **own background** — the artwork the title bar and the
 * composer genuinely sit on — and puts two real surfaces over it: one refracting,
 * one identical but plain.
 *
 * Four earlier versions drew a test pattern instead: 18px stripes, then 44px
 * bands, then a 96px checkerboard, then 64px squares. Every one of them was the
 * wrong answer for the same reason, and it is the same mistake that made every
 * panel in the app 48% opaque for a commit: **choosing what shows the effect over
 * what the app looks like.** A checkerboard is a diagnostic. It belongs in
 * `scripts/probe-glass.mjs`, where it is a measurement. In a settings page it is
 * a foreign object, and the caption it needed — three sentences explaining what
 * was exaggerated and why — was the giveaway that it had stopped being a preview.
 *
 * ## What is honest about this
 *
 * The samples are at their **real** tint and frost, taken from the per-group
 * tables in `lib/glass.ts`. Nothing is exaggerated to flatter the effect.
 *
 * The consequence is that over a smooth preset the bend is nearly invisible, and
 * the caption says so rather than implying otherwise. That is not a limitation of
 * the preview — it is the measured behaviour of the feature. Refraction over
 * Loom's own backgrounds moves 0% of pixels; over a detailed photograph it moves
 * 88%. A preview that hid that would be the fifth version of the same lie.
 *
 * ## The one comparison that is worth keeping
 *
 * Refracting and plain sit **side by side**, identical in every other respect.
 * The effect is a bend at the rim, and a bend is only legible against a
 * reference — one sample alone asks the eye to remember what the edge looked like
 * a moment ago, which it cannot.
 */
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
 * The layout is the established one — a range input plus a tabular value — and
 * the input's own styling comes from the base-layer rules in `styles.css`, so
 * every slider in the app looks the same. The unit is part of the value rather
 * than the label, so a row reading "38 px" cannot be mistaken for the library's
 * own unitless `blurAmount`, which is a different number by a factor of 32.
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
 * A sample surface, at the settings it would really have.
 *
 * Deliberately **not** given a `tintStrength` or a frost override, which is the
 * opposite of what three earlier versions did. Lowering them made the bend
 * visible and made the preview a demonstration of a material the app never
 * shows — the same overpromise, one layer down.
 */
function SamplePill({ plain = false }: { plain?: boolean }) {
  return (
    <LiquidSurface
      className="h-10 rounded-capsule"
      contentClassName="gap-0.5 p-1"
      surface="pills"
      // The static twin must otherwise be identical, or it is comparing two
      // things and calling it a comparison.
      liquid={plain ? false : undefined}
    >
      <span className="px-2.5 text-[12.5px] text-soft">
        {plain ? "Plain glass" : "Liquid"}
      </span>
    </LiquidSurface>
  );
}

export function GlassPreview() {
  const config = useSettings((state) => state.config);
  const dark = config.theme === "dark";
  const preset = resolvePreset(config.background.preset, dark);

  return (
    <section className="mb-3 rounded-control border border-[var(--glass-border)] bg-[var(--card-bg)] p-3">
      <h3 className="mb-2 px-1 text-[11px] font-semibold tracking-[0.09em] text-faint uppercase">
        Over your background
      </h3>

      <div
        className="relative h-[184px] overflow-hidden rounded-row border border-[var(--glass-border)]"
        style={{
          ...backgroundStyle(config.background, dark),
          background: preset.layers,
          backgroundColor: preset.base,
        }}
      >
        <div className="absolute top-6 left-6 flex flex-wrap items-center gap-2.5">
          <SamplePill />
          <SamplePill plain />
        </div>

        {/* Narrower than the panel, so all four of its rims sit over the artwork
            rather than two of them hanging off the edge — the bend happens at the
            rim, so a sample whose edges are outside the picture demonstrates
            nothing at all. */}
        <div className="absolute right-5 bottom-5 left-5 flex justify-center">
          <LiquidSurface
            surface="composer"
            className="w-[58%] rounded-sheet"
            contentClassName="px-3 py-2.5"
            layout="block"
            tint="var(--panel-bg-strong)"
          >
            <span className="text-[12.5px] text-soft">Composer</span>
          </LiquidSurface>
        </div>
      </div>

      <p className="mt-2 px-1 text-[11.5px] leading-4 text-faint">
        <span className="text-soft">Liquid</span> and{" "}
        <span className="text-soft">Plain glass</span> are the same tint; the
        first bends what is behind it at the rim and the second does not. Over{" "}
        {preset.name} that bend is <span className="text-soft">very subtle</span>{" "}
        — these backgrounds are smooth washes by design, so there is little in
        them for a displacement to catch. It reads clearly over your own
        photograph or video, which is why the title bar looks busier there than
        it does now.
      </p>
    </section>
  );
}

/**
 * The glass surfaces, and what they refract.
 *
 * Two sections rather than one because they are different kinds of decision:
 * "Glass" moves every surface in the app at once, and "Liquid glass" is the
 * refraction sitting on top of some of them. Merging them put a master switch
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
        description="How heavy every surface in the app is — the refracting ones included. The shipped values give each surface the same thickness it has always had; these are here for when you want it lighter or heavier."
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
          hint="How far each surface frosts the artwork behind it. This is what makes glass read as glass rather than as a hole, so it is the one control that most affects legibility. The terminal keeps its own heavier blur, because a shell is read for minutes."
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
        description="The title-bar pills, the composer and torn-off panel windows bend the artwork at their edges, the way a thick lens would. Everything else in the app is ordinary frosted glass."
      >
        <Toggle
          label="Refract"
          hint="Off renders the same surfaces as plain frosted glass, at the same tint and blur."
          checked={liquid.enabled}
          onChange={(enabled) => setLiquid({ enabled })}
        />
        <Toggle
          label="Title-bar pills"
          hint="The cheapest place for it, and the only place it is easy to see: the pills sit over your wallpaper rather than over the app."
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
          hint="Only the windows detached from the main one. A panel docked inside the app sits over the transcript rather than over artwork, so there is nothing there to refract."
          checked={liquid.panels}
          onChange={(panels) => setLiquid({ panels })}
          disabled={!liquid.enabled}
        />

        <Row label="Preset" hint={preset ? undefined : "These values are your own."}>
          <Segmented
            value={preset ?? "custom"}
            options={[
              // The Custom chip only appears when nothing matches, so the control
              // never claims a preset that is not set.
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
        description="What the bending looks like. These trade against the Glass sliders above rather than adding to them: more frost means less visible bend."
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
          hint="Backdrop blur, in pixels. This is the number that trades against the effect: past about 40px the backdrop is too flat to bend, so the refraction disappears into the blur. The default is the blur each surface already had."
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
        <Row
          label="Saturation"
          hint="How much colour the glass pulls out of what is behind it."
        >
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
