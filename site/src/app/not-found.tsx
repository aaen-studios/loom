import Link from "next/link";
import { DOWNLOAD } from "@/lib/site";
import { Drifting } from "@/components/weave/animated";

/**
 * The 404.
 *
 * A dropped thread: the figure behind the words is a field with no strands reaching the bottom, which
 * is the smallest possible visual joke and the only one on the site.
 *
 * Kept in the page's own language — mono label, the accent, the thread field — rather than dressed as a
 * framework error, because a visitor cannot tell "wrong address" from "broken deployment" from where
 * they are standing, and only one of those is worth alarming them about.
 */
export default function NotFound() {
  return (
    <div className="relative isolate overflow-hidden pt-20 pb-24">
      <div className="pointer-events-none absolute inset-0 -z-10 h-full opacity-50">
        <Drifting
          kind="bundle"
          seed={404}
          width={1600}
          height={700}
          detail={0.7}
          className="h-full w-full"
          id="not-found"
        />
      </div>

      <div className="shell">
        <p className="t-label">
          <span className="t-index mr-3" aria-hidden="true">
            404
          </span>
          no such page
        </p>

        <h1 className="t-display mt-6 max-w-[13ch]">That thread goes nowhere.</h1>

        <p className="t-lede mt-6 max-w-[44ch]">
          The link may be old, or the page may have moved. The site is one page, so everything it
          contains is on the front of it.
        </p>

        <div className="mt-9 flex flex-wrap items-center gap-3">
          <Link href="/" className="btn-primary h-11 px-5 text-[14.5px]">
            Back to the page
          </Link>
          <Link href={DOWNLOAD.publicPath} className="btn-ghost h-11 px-5 text-[14.5px]">
            Download
          </Link>
          <Link href="/privacy" className="btn-ghost h-11 px-5 text-[14.5px]">
            Privacy
          </Link>
          <Link href="/terms" className="btn-ghost h-11 px-5 text-[14.5px]">
            Terms
          </Link>
        </div>
      </div>
    </div>
  );
}
