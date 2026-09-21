import { Hero, Parts, Meeting, Narrowing } from "@/components/movements/open";
import { Ground, Install } from "@/components/movements/close";

/**
 * The page.
 *
 * ---------------------------------------------------------------------------
 * Six movements, and why they are movements rather than sections
 * ---------------------------------------------------------------------------
 *
 * The page does not walk through the product feature by feature. It states six things, each with a
 * generative figure drawn in the application's own thread colours, and the figure is doing the
 * persuading rather than illustrating a claim that has already been made in a heading.
 *
 * The order runs from the largest abstraction to the smallest, and then lands:
 *
 *   1. the weave     a field of threads under tension — what the page is made of
 *   2. the parts     a lattice of joints holding each other — eight panels, one arrangement
 *   3. where they meet  two systems deflecting each other — your judgement and the model's
 *   4. narrowing     many threads converging to one point — a turn, and the mode that can edit Loom
 *   5. ground truth  the quietest figure on the page, behind the facts that have to be believed
 *   6. install       the ending, and the one action
 *
 * ---------------------------------------------------------------------------
 * What this replaced
 * ---------------------------------------------------------------------------
 *
 * Four earlier attempts, and the pattern in them is worth naming because it is the reason this one is
 * built the way it is.
 *
 * The first ran a scripted chat in the hero — a prompt typing itself, a fabricated token count — which
 * is the most recognisable landing-page pattern in software and therefore the one a visitor has
 * learned to skip.
 *
 * The second kept a weaving vocabulary and drew it as *texture*: hairlines behind the text, a line
 * that followed the scroll, a knot per section. It was a reskin, and it read as decoration because the
 * metaphor was doing nothing the content needed.
 *
 * The third stripped that out for a printed manual — contents list, numbered sections, drawn plates, a
 * colophon — and answered the wrong brief: it was beautiful and it was not a product page. It had no
 * window, nothing interactive, and nothing to look at.
 *
 * The fourth put a working dock on the page, which was the right instinct and the wrong altitude: a
 * live four-zone layout with eight draggable panels is a *tool*, and a hero that asks a reader to
 * rearrange tabs before they know what the product is has put the demonstration ahead of the
 * argument.
 *
 * This one keeps the ambition and drops the literalism. The figures are abstract, they are generated
 * from seeds rather than drawn or screenshotted, and they are drawn in the product's own colours — so
 * the page looks like nothing else in this category without ever showing a picture of a window.
 */
export default function Page() {
  return (
    <>
      <Hero />
      <Parts />
      <Meeting />
      <Narrowing />
      <Ground />
      <Install />
    </>
  );
}
