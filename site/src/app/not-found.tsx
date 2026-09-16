import Link from "next/link";

/**
 * The 404.
 *
 * A dropped pick. In weaving, a pick that is thrown but does not go through is a
 * *float* — the shuttle has travelled but the weft is lying on the surface instead
 * of being beaten into the cloth, and the row is visibly wrong. Which is exactly
 * what a broken link is: the journey happened, the connection did not.
 *
 * Dressed in the site's own furniture rather than as a bare framework error, because
 * a visitor cannot tell the difference between "wrong address" and "broken
 * deployment" from where they are standing, and one of those is worth alarming them
 * about and the other is not.
 */
export default function NotFound() {
  return (
    <section className="px-4 pt-16 pb-4 sm:px-6 sm:pt-24">
      <div className="warp-grid">
        <div
          className="max-w-2xl"
          style={{ gridColumn: "2 / span 10" }}
        >
          <p className="text-faint flex items-center gap-2.5 text-[11.5px] font-medium tracking-[0.16em] uppercase">
            <span className="knot" />
            <span>Float · the pick did not go through</span>
          </p>

          <h1 className="mt-4 text-[30px] leading-tight font-medium tracking-tight sm:text-[38px]">
            That page does not exist.
          </h1>

          <p className="text-soft mt-4 text-[15px] leading-6">
            The link may be old, or the page may have moved. Everything the cloth has
            is below.
          </p>

          {/* A row of threads with a visible break in it — the dropped pick, drawn
              as what it is rather than described as an error code. */}
          <div aria-hidden="true" className="mt-8 flex items-end gap-2">
            {[16, 22, 12, 0, 19, 14, 24, 11, 18].map((height, index) => (
              <span
                key={index}
                className="w-px"
                style={{
                  height: `${height}px`,
                  background:
                    height === 0
                      ? "transparent"
                      : "linear-gradient(to bottom, var(--thread-line-strong), transparent)",
                }}
              />
            ))}
          </div>

          <div className="mt-8 flex flex-wrap items-center gap-3">
            <Link href="/" className="btn-primary h-10 px-4 text-[14px]">
              Home
            </Link>
            <Link href="/download" className="btn-ghost h-10 px-4 text-[14px]">
              Download
            </Link>
            <Link href="/privacy" className="btn-ghost h-10 px-4 text-[14px]">
              Privacy
            </Link>
            <Link href="/terms" className="btn-ghost h-10 px-4 text-[14px]">
              Terms
            </Link>
          </div>
        </div>
      </div>
    </section>
  );
}
