"use client";

import Link from "next/link";
import { DOWNLOAD } from "@/lib/site";
import { AppWindow } from "./app-window";
import { HeroComposer } from "./composer";
import { GoalPanel } from "./goal-panel";
import { useScene } from "./use-scene";

/**
 * The landing hero.
 *
 * The window is the product, rendered rather than pictured: same tokens, same
 * surfaces, same animation keyframes as the desktop app, because both read the
 * generated token sheet. It stays sharp at any device pixel ratio, repaints
 * correctly in both palettes, and cannot go stale the way a screenshot can.
 *
 * The scripted turn exists because the interesting part of an agent is its
 * *sequence* — reasoning arriving before the answer, a tool running, the reply
 * streaming in, a task list checking itself off. A still of the finished reply
 * would show the least revealing moment of the whole interaction.
 *
 * ---------------------------------------------------------------------------
 * Why the composer is outside the frame
 *
 * In the app it sits *inside* the window, and it moves: centred under the
 * greeting in an empty chat, then docked at the bottom edge once there are
 * messages. An earlier version of this hero reproduced both layouts and
 * crossfaded between them, which was more faithful and worse as a hero — two
 * nested glass surfaces in a window that also changed height read as busy, and
 * the movement pulled attention away from the reply.
 *
 * Here the window holds the transcript and the composer sits below it as its own
 * card. The frame stays a fixed size, and what you are meant to watch — the
 * reasoning, the tool call, the streaming answer — is the thing on screen.
 * ---------------------------------------------------------------------------
 */
export function Hero() {
  const scene = useScene();

  return (
    <section className="px-4 pt-10 pb-4 sm:px-6 sm:pt-16 lg:pt-20">
      <div className="mx-auto max-w-5xl">
        <div className="mx-auto max-w-2xl text-center">
          <p className="text-faint text-[12.5px] font-medium tracking-[0.14em] uppercase">
            Windows · Free · Open source
          </p>
          <h1 className="mt-4 text-[32px] leading-[1.08] font-medium tracking-tight text-balance sm:text-[46px] lg:text-[54px]">
            An AI agent that runs on
            <span className="text-[var(--accent)]"> your </span>
            machine.
          </h1>
          <p className="text-soft mx-auto mt-5 max-w-xl text-[15px] leading-6 sm:text-[16.5px] sm:leading-7">
            Loom is a desktop app for chat and agents. Bring your own provider,
            point it at a folder, and watch it plan, read, run commands and check
            off its own task list — with every step visible while it happens.
          </p>

          <div className="mt-7 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <Link
              href={DOWNLOAD.publicPath}
              className="btn-primary h-11 w-full px-5 text-[14.5px] sm:w-auto"
            >
              Download for Windows
            </Link>
            <Link
              href="#how"
              className="btn-ghost h-11 w-full px-5 text-[14.5px] sm:w-auto"
            >
              See how it works
            </Link>
          </div>

          <p className="text-faint mt-4 text-[12.5px]">
            {DOWNLOAD.requirements} · No account · No telemetry
          </p>
        </div>

        {/* `ref` is on this wrapper so the scene's visibility gate covers the
            whole demonstration rather than only the window — the animation
            should pause when the hero is off screen, not when its inner scroll
            area is. */}
        <div ref={scene.ref} className="mt-10 sm:mt-14">
          {/* A single, moderately-blurred surface. The app gets away with
              34—38px blurs because it never scrolls its document and has few
              panels on screen; on a scrolling page one heavy blur is affordable
              and several are not.

              The shadow is a `dark:` variant rather than a token, because a
              shadow has no token: a slate shadow reads as depth on a pale page
              and vanishes on a dark one, where it has to be black and stronger
              or the window looks pasted on. */}
          <div className="mx-auto max-w-[1040px] drop-shadow-[0_40px_70px_-40px_rgb(15_23_42_/_0.45)] dark:drop-shadow-[0_40px_70px_-40px_rgb(0_0_0_/_0.85)]">
            <AppWindow scene={scene} />
          </div>

          {/* The composer and the task list, stacked under the frame at the same
              measure as the window so the three read as one column. */}
          <div className="mx-auto mt-3 max-w-[1040px]">
            <GoalPanel scene={scene} />
            <HeroComposer
              value={scene.sent ? "" : scene.prompt}
              typing={scene.typing}
              busy={scene.sent && scene.phase !== "settled"}
              hero
            />
          </div>

          <p className="text-faint mx-auto mt-5 max-w-[1040px] text-center text-[12px]">
            {/* An honest note, and a useful one: the animation is built from the
                product's own signals rather than decoration invented for a
                landing page. */}
            Rendered live in your browser from the app&rsquo;s own stylesheet —
            not a screenshot.
          </p>
        </div>
      </div>
    </section>
  );
}
