import Link from "next/link";

/**
 * The 404.
 *
 * A dropped pick: on a loom, a pick that is thrown but does not go through is a
 * *float* — the shuttle has travelled but the weft is lying on the surface instead of
 * being beaten into the cloth, and the row is visibly wrong. Which is what a broken
 * link is: the journey happened, the connection did not.
 *
 * It is drawn as the same figure the manual uses, inverted: a row of warp threads
 * where one never got woven in. Same vocabulary as Figure 3's lanes, so a reader who
 * has seen a plate recognises the shape without being told what it means.
 *
 * Short, and it offers the three things a reader arriving here actually wants: the
 * document, the download, and the two pages that can be reached directly.
 */
export default function NotFound() {
  const threads = [16, 22, 12, 0, 19, 14, 24, 11, 18];

  return (
    <div className="paper pt-16 sm:pt-24">
      <p className="rubric">Float &middot; the pick did not go through</p>

      <h1 className="t-section mt-4">That page does not exist.</h1>

      <p className="t-standfirst mt-4">
        The link may be old, or the page may have moved. The manual is one page, so
        everything the site contains is on it.
      </p>

      {/* A row of threads with a visible break in it. The only drawing on the page,
          and it is the same mark the figures use for a gap in the weave. */}
      <div aria-hidden="true" className="mt-8 flex items-end gap-2">
        {threads.map((height, index) => (
          <span
            key={index}
            className="w-[3px] rounded-[1px]"
            style={{
              height: `${height}px`,
              background:
                height === 0 ? "transparent" : "var(--thread-line-strong)",
              opacity: 0.7,
            }}
          />
        ))}
      </div>

      <div className="mt-8 flex flex-wrap items-center gap-3">
        <Link href="/" className="btn-primary h-10 px-4 text-sm">
          The manual
        </Link>
        <Link href="/download" className="btn-ghost h-10 px-4 text-sm">
          Download
        </Link>
        <Link href="/privacy" className="btn-ghost h-10 px-4 text-sm">
          Privacy
        </Link>
        <Link href="/terms" className="btn-ghost h-10 px-4 text-sm">
          Terms
        </Link>
      </div>
    </div>
  );
}
