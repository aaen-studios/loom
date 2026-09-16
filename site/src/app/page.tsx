import { Hero } from "@/components/pages/hero";
import { Ends } from "@/components/pages/ends";
import { PickTurn } from "@/components/pages/pick";
import { Count } from "@/components/pages/count";
import { SelvedgeBand } from "@/components/pages/selvedge-band";
import { Heddles } from "@/components/pages/heddles";
import { OffTheLoom } from "@/components/pages/off";

/**
 * The front page, as a draft.
 *
 * Seven passes, and the order is the argument rather than a convention:
 *
 *   01 Warp      the frame, and the machine running inside it
 *   02 Ends      the threads the cloth is made of
 *   03 Pick      one pass of the shuttle, start to finish
 *   04 Count     how many ends, and who supplies them
 *   05 Selvedge  the edge that stops it fraying
 *   06 Heddles   the questions, and what lifts to answer them
 *   07 Off       take it
 *
 * The components are not arranged here so much as *read* from
 * `lib/weave/passes.ts`: each one looks up its own pass, so its anchor, its number
 * and its heading prefix cannot drift out of step with the draft. That is also
 * what the shuttle measures when it decides where the weft is, and what
 * `verify-weave.mjs` checks — one source, four readers.
 *
 * The two bands that are deliberately *not* passes are the two that belong to the
 * frame rather than to the cloth: the draft strip above and the selvedge below.
 * Both live in `layout.tsx`, so they are identical on every page.
 */
export default function Home() {
  return (
    <>
      <Hero />
      <Ends />
      <PickTurn />
      <Count />
      <SelvedgeBand />
      <Heddles />
      <OffTheLoom />
    </>
  );
}
