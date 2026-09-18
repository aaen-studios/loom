/**
 * The Glass settings.
 *
 * ## There is no preview, and that is the decision rather than an omission
 *
 * Five versions of this file drew a preview: 18px stripes, 44px bands, a 96px
 * checkerboard, 64px squares, and then the user's own background. The first four
 * were a *test card* — a foreign object in a settings page, each one needing a
 * caption to explain which numbers were exaggerated and why. The fifth was honest
 * and invisible, because over a smooth preset the effect it was demonstrating
 * genuinely is invisible.
 *
 * The fifth was closest and still wrong, because it answered the wrong question.
 * A preview exists so someone can judge a change without leaving the screen — but
 * **these controls apply immediately, to the app the user is already looking at.**
 * The drawer these sliders sit in is itself a liquid surface. The title bar is
 * visible above it. The composer is behind it. Moving Tint changes the very panel
 * being read, in the same frame.
 *
 * So the honest interface is the live one, and the section says so in a sentence
 * instead of drawing a diorama. That also removes the failure mode this file kept
 * hitting, which was that a synthetic sample at *any* settings is a claim about
 * how the app looks — and every such claim I made was either exaggerated to
 * flatter the effect or too subtle to see.
 *
 * ## What is worth knowing before touching these
 *
 * The two properties the library uses run in an order that makes them enemies:
 * `backdrop-filter` frosts what is behind, and then `filter: url(#displacement)`
 * bends that frost. So **more frost means less visible bend**, and the defaults
 * deliberately favour legibility over the effect — a surface you cannot read is a
 * worse outcome than one whose rim is subtle. The hints below say this at the two
 * rows where it matters.
 */
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
import { Row, Section, Segmented, Toggle } from "./ui";

/**
 * A slider with the app's number beside it.
 *
 * The layout is the established one — a `w-40` range input plus a tabular value —
 * and the input's own styling comes from the base-layer rules in `styles.css`, so
 * every slider in the app looks the same. The unit sits with the value rather
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
        description="How heavy every surface in the app is — the refracting ones included. These apply as you move them, so the panel you are reading this in is the preview: lower the tint and watch its own background lighten."
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
          hint="How far each surface frosts the artwork behind it. This is what makes glass read as glass rather than as a hole, so it is the control that most affects legibility — and it trades against the effect, because a backdrop that has been blurred has little left for a lens to bend."
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
        description="The title-bar pills, the composer and torn-off panel windows bend the artwork at their edges, the way a thick lens would. Everything else in the app is ordinary frosted glass. The shipped values give each surface exactly the thickness it had before this feature existed, so nothing looks different until you move something."
      >
        <Toggle
          label="Refract"
          hint="Off renders the same surfaces as plain frosted glass, at the same tint and blur."
          checked={liquid.enabled}
          onChange={(enabled) => setLiquid({ enabled })}
        />
        <Toggle
          label="Title-bar pills"
          hint="The easiest place to see it: the pills float over your wallpaper rather than over the app, and the rims catch its detail as you drag the tint slider."
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
        description="What the bending looks like. Every one of these trades against the Glass sliders above rather than adding to them, so the clearest way to see a change here is to lower Tint first."
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
          hint="Backdrop blur, in pixels, and the number that trades hardest against the effect: past about 40px the backdrop is too flat to bend, so the refraction disappears into the blur. The default is the blur each surface already had."
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
