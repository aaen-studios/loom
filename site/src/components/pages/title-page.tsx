import Link from "next/link";
import { DOWNLOAD } from "@/lib/site";
import { APPENDICES, FIGURES, SECTIONS, TABLES } from "@/lib/document";
import { Action } from "@/components/doc/section";
import { Spec } from "@/components/doc/spec";

/**
 * The title page.
 *
 * A manual opens by saying what it is about, and that is all this does: the
 * instrument, one paragraph of standfirst, the one action the document contains,
 * and a short block of facts. No headline, no tagline, no badge.
 *
 * ---------------------------------------------------------------------------
 * Why there is no headline
 * ---------------------------------------------------------------------------
 *
 * The previous two versions of this page both opened with a claim — "An AI agent
 * that runs on your machine", then "An agent you can watch work" — and both were
 * arguing before they had said what the thing was. A reader who arrives at a
 * manual wants to know what is inside it; a reader who wants a claim is better
 * served by the three paragraphs in section 1, where there is room to support one.
 *
 * So the biggest type on the page is the product's name, the standfirst says what
 * it is in one sentence, and the substantial argument starts overleaf. That is a
 * decision about confidence rather than about modesty: a document that leads with
 * its contents is making a claim of its own, and it is the harder one to fake.
 *
 * ---------------------------------------------------------------------------
 * The facts block
 * ---------------------------------------------------------------------------
 *
 * Six lines, every one of them checkable — the platform from the installer, the
 * licence from the repository, the stack from the build, the provider count from
 * the settings grid, and the document's own length from `lib/document.ts` so that
 * a section added without updating the front matter fails a test.
 */
export function TitlePage() {
  return (
    <section className="pt-12 pb-2 sm:pt-16">
      <p className="rubric">The manual</p>
      <h1 className="t-title mt-4">Loom</h1>

      <p className="t-standfirst mt-5">
        A desktop workspace for AI chat and agents, for Windows. One window holding
        the conversation, a real terminal, an editor with git, and the files it is
        all working on — with the model&rsquo;s reasoning kept in the transcript,
        where it happened.
      </p>

      <Action>
        <Link href={DOWNLOAD.publicPath} className="btn-primary h-10 px-4 text-sm">
          Download for Windows
        </Link>
        <span className="action-note">
          {DOWNLOAD.requirements} · free · no account
        </span>
      </Action>

      <Spec
        rows={[
          { term: "Platform", def: DOWNLOAD.requirements },
          {
            term: "Licence",
            def: "MIT. Free to use, read, fork and ship. No account, no telemetry, no server of the publisher's own.",
          },
          {
            term: "Built with",
            def: "Tauri v2, React 19 and Tailwind v4 over a Rust engine in its own crate.",
          },
          {
            term: "Providers",
            def: "Twelve named presets, plus any OpenAI- or Anthropic-compatible endpoint — including a model running on the machine in front of you.",
          },
          {
            term: "This document",
            def: `${SECTIONS.length} sections, ${APPENDICES.length} appendices, ${FIGURES.length} figures and ${TABLES.length} tables. Set in Inter, in the application's own two palettes.`,
          },
        ]}
      />
    </section>
  );
}
