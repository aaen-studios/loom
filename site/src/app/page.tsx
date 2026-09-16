import { DOWNLOAD } from "@/lib/site";
import { Background } from "@/components/background";
import { Hero } from "@/components/hero/hero";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Faq } from "@/components/sections/faq";
import { Features } from "@/components/sections/features";
import { HowItWorks } from "@/components/sections/how-it-works";
import { Providers } from "@/components/sections/providers";
import { Trust } from "@/components/sections/trust";

export default function Home() {
  return (
    <>
      <Background />
      <SiteHeader />
      <main>
        <Hero />
        <Providers />
        <Features />
        <HowItWorks />
        <Trust />
        <Faq />

        {/* The closing call to action. The page has made its case by here, so
            this is one line and one button rather than another pitch. */}
        <section className="px-4 py-16 sm:px-6 sm:py-20">
          <div className="mx-auto max-w-5xl">
            <div className="panel rounded-sheet flex flex-col items-start gap-5 p-6 sm:flex-row sm:items-center sm:justify-between sm:p-8">
              <div>
                <h2 className="text-[20px] font-medium tracking-tight sm:text-[24px]">
                  Free, and there is nothing to sign up for.
                </h2>
                <p className="text-soft mt-2 text-[14px] leading-6">
                  Install it, add a key, point it at a folder.
                </p>
              </div>
              <a
                href={DOWNLOAD.publicPath}
                className="btn-primary h-11 shrink-0 px-5 text-[14.5px]"
              >
                Download for Windows
              </a>
            </div>
          </div>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
