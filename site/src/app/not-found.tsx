import Link from "next/link";
import { Background } from "@/components/background";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";

/**
 * The 404.
 *
 * Styled like the rest of the site — the background layer and header are the
 * same components — because a bare Next.js 404 on a styled site reads as a
 * broken deployment rather than a wrong URL.
 */
export default function NotFound() {
  return (
    <>
      <Background />
      <SiteHeader />
      <main className="px-4 pt-20 pb-4 sm:px-6 sm:pt-28">
        <div className="mx-auto max-w-2xl text-center">
          <p className="text-faint font-mono text-[12px] tracking-[0.14em] uppercase">
            404
          </p>
          <h1 className="mt-3 text-[30px] leading-tight font-medium tracking-tight sm:text-[36px]">
            That page does not exist.
          </h1>
          <p className="text-soft mt-4 text-[15px] leading-6">
            The link may be old, or the page may have moved. Everything the site
            has is below.
          </p>
          <div className="mt-7 flex flex-wrap items-center justify-center gap-3">
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
      </main>
      <SiteFooter />
    </>
  );
}
