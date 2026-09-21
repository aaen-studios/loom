import type { Metadata } from "next";
import { Standalone } from "@/components/doc/standalone";
import { Sub } from "@/components/doc/section";
import { P } from "@/components/doc/text";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Terms",
  description: "Loom is free, MIT licensed software provided as-is, with no warranty.",
  alternates: { canonical: "/terms" },
};

/**
 * Terms.
 *
 * Short, because the licence does the legal work. The MIT licence *is* the agreement
 * between publisher and user, and stacking extra restrictions on top of it in a web
 * page would be misleading — you cannot ship MIT and then add "but you may not resell
 * it" in a footer. So this page points at the licence rather than competing with it,
 * and then says the two things a download page should not be coy about: there is no
 * warranty, and this is software that will run commands on your machine.
 *
 * The second of those is the substantive one. Loom reads files, runs shell commands,
 * and — if armed — drives the mouse and keyboard. A model can be wrong. Saying so
 * plainly is the whole point of this page existing rather than being a copy of the
 * licence text.
 */
export default function TermsPage() {
  return (
    <Standalone
      title="Terms"
      standfirst="Loom is free software, licensed under MIT and provided as-is. Here is what that means in plain language, and the one risk worth reading about."
    >
      <Sub>The licence</Sub>

      <P>
        Loom is released under the{" "}
        <a
          href={SITE.licenseUrl}
          target="_blank"
          rel="noreferrer"
          className="text-[var(--accent)] hover:underline"
        >
          MIT licence
        </a>
        , copyright {SITE.publisher}. That licence &mdash; not this page &mdash; is the
        agreement between publisher and user. It grants the right to use, copy, modify,
        merge, publish, distribute, sublicense and sell copies, on one condition: the
        copyright notice and the licence text keep travelling with it.
      </P>

      <P>
        In practice: you can read the source, build your own version, fork it, change it
        and distribute it commercially. You do not need to ask and you do not owe
        anything.
      </P>

      <Sub>No warranty, and the specific reason it matters</Sub>

      <P>
        The software is provided &ldquo;as is&rdquo;, without warranty of any kind,
        express or implied, including any implied warranty of merchantability, fitness
        for a particular purpose, or non-infringement.
      </P>

      <P>
        Stated plainly, because the generic sentence above does not convey it: Loom is a
        tool that reads your files, runs shell commands, and &mdash; if you arm the
        computer-use chip &mdash; controls your mouse and keyboard. A language model can
        be wrong. It can delete something, run a destructive command, or click the wrong
        button. Use the read-only modes when you are not watching, keep backups, notice
        that an agent in Atelier mode can remove parts of its own configuration without
        asking, and do not point an agent at anything you cannot afford to lose.
      </P>

      <P>
        In no event shall the authors or copyright holders be liable for any claim,
        damages or other liability arising from the software or its use.
      </P>

      <Sub>Your providers</Sub>

      <P>
        Loom is a client for model providers you choose and pay directly. It has no
        subscription, resells nothing, and is not a party to the agreement between you
        and your provider. Their terms govern the requests you make through them.
      </P>

      <Sub>This website</Sub>

      <P>
        The site exists to describe and distribute the software and comes with the same
        absence of warranty. It may be unavailable or inaccurate: the version number on
        the download page is read from GitHub and is only as current as the last release.
        Where this site and the licence disagree, the licence wins.
      </P>

      <Sub>Changes</Sub>

      <P>
        The licence cannot be revoked for a version you already have. If these terms
        change, they change for future releases &mdash; and the commit history, which is
        public, shows what changed and when.
      </P>
    </Standalone>
  );
}
