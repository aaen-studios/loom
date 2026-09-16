"use client";

import Link from "next/link";
import { DOWNLOAD } from "@/lib/site";
import { passById } from "@/lib/weave/passes";
import { Cloth } from "@/components/weave/cloth";
import { Draft } from "@/components/weave/draft";
import { ShuttleComposer } from "@/components/weave/shuttle-composer";
import { Shed } from "@/components/weave/pass";
import { useScene } from "@/components/weave/use-scene";

/**
 * Pass one: the warp.
 *
 * The frame, and the machine running inside it.
 *
 * ---------------------------------------------------------------------------
 * Why the claim and the demonstration sit side by side
 * ---------------------------------------------------------------------------
 *
 * This was a stack: eyebrow, headline, lead, two buttons, a requirements line, and
 * then — a screen and a half down — the thing the page is actually about. That is
 * the conventional shape and it is the wrong one here, because the whole argument is
 * that Loom shows you the work. Putting the work below the fold makes a visitor take
 * the claim on faith first.
 *
 * On `lg` and up the two are columns: the claim on the left, the machine on the
 * right, both above the fold. Below `lg` it stacks back to copy-then-machine, which
 * is the only thing that fits.
 *
 * ---------------------------------------------------------------------------
 * Why the draft sits above the cloth rather than beside it
 * ---------------------------------------------------------------------------
 *
 * An earlier arrangement put the plan in a left-hand rail with the transcript to its
 * right, which read as two competing panels. Stacking them is both calmer and more
 * faithful: in the product the goal and task panel sits directly above the
 * transcript, and the composer below that. So the order here is the order the app
 * uses, and it is the same on a phone as on a desktop.
 *
 * The drawn application window is gone entirely. It was accurate and inert — a
 * picture of a window invites you to look *at* it, while a cloth that grows while you
 * read invites you to watch it happen, which is the only thing about an agent worth
 * showing. What is left is the part that was always the interesting part.
 */
export function Hero() {
  const scene = useScene();
  const pass = passById("warp");

  return (
    <section
      id={pass.id}
      data-pass={pass.pick}
      className="scroll-mt-24 pt-10 pb-16 sm:pt-14 sm:pb-20"
    >
      <div ref={scene.ref} className="warp-grid">
        <Shed from={2} span={10}>
          <div className="lg:grid lg:grid-cols-[minmax(0,23rem)_minmax(0,1fr)] lg:items-start lg:gap-12">
            {/* The claim. */}
            <div>
              <p className="text-faint flex items-center gap-2.5 text-[11.5px] font-medium tracking-[0.16em] uppercase">
                <span className="knot-lit knot" />
                <span>
                  Pass {String(pass.pick).padStart(2, "0")} · {pass.element}
                </span>
              </p>

              <h1 className="mt-5 text-[32px] leading-[1.06] font-medium tracking-tight text-balance sm:text-[40px] lg:text-[42px]">
                An AI agent that runs on
                <span className="text-[var(--thread-bright)]"> your </span>
                machine.
              </h1>

              <p className="text-soft mt-5 max-w-md text-[15px] leading-6">
                Most clients hide the interesting part. Loom keeps it on screen: the
                reasoning behind an answer, every tool it calls, the commands it runs,
                and the task list it keeps.
              </p>

              <div className="mt-7 flex flex-col gap-3 sm:flex-row sm:items-center">
                <Link
                  href={DOWNLOAD.publicPath}
                  className="btn-primary h-11 px-5 text-[14.5px]"
                >
                  Download for Windows
                </Link>
                <Link href="#pick" className="btn-ghost h-11 px-5 text-[14.5px]">
                  Watch a turn
                </Link>
              </div>

              <p className="text-faint mt-5 text-[12.5px] leading-5">
                {DOWNLOAD.requirements}
                <br />
                No account · No telemetry · MIT licensed
              </p>
            </div>

            {/* The machine: the plan, the result, and the thing you send the next
                pick with — in the order the app stacks them. */}
            <div className="mt-10 lg:mt-0">
              <Draft scene={scene} />

              <div className="mt-3">
                <Cloth scene={scene} />
              </div>

              <div className="mt-3">
                <ShuttleComposer
                  value={scene.sent ? "" : scene.prompt}
                  typing={scene.typing}
                  busy={scene.sent && scene.phase !== "settled"}
                  hero
                />
              </div>

              <p className="text-faint mt-4 text-[12px]">
                Strung with the app&rsquo;s own stylesheet and thread colours — not a
                screenshot. Switch the theme and it follows.
              </p>
            </div>
          </div>
        </Shed>
      </div>
    </section>
  );
}
