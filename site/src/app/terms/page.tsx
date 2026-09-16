import type { Metadata } from "next";
import { Clause, Legal } from "@/components/pages/legal";
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
 * page would be misleading — you cannot ship MIT and then add "but you may not
 * resell it" in a footer. So this page points at the licence rather than competing
 * with it, and says the two things a download page should not be coy about: no
 * warranty, and no promise that the software is fit for anything in particular.
 */
export default function TermsPage() {
  return (
    <Legal
      title="Terms"
      summary="Loom is free software, licensed under MIT. It is provided as-is, with no warranty. Here is what that means in plain language."
    >
      <Clause title="The licence">
        <p>
          Loom is released under the{" "}
          <a
            href={SITE.licenseUrl}
            target="_blank"
            rel="noreferrer"
            className="text-[var(--accent)] hover:underline"
          >
            MIT licence
          </a>
          , copyright {SITE.publisher}. That licence — not this page — is the
          agreement between us. It grants you the right to use, copy, modify, merge,
          publish, distribute, sublicense and sell copies of the software, on one
          condition: the copyright notice and the licence text keep travelling with
          it.
        </p>
        <p>
          In practice that means you can read the source, build your own version, fork
          it, change it, and distribute it commercially. You do not need to ask, and
          you do not owe anything.
        </p>
      </Clause>

      <Clause title="No warranty">
        <p>
          The software is provided &ldquo;as is&rdquo;, without warranty of any kind,
          express or implied — including any implied warranty of merchantability,
          fitness for a particular purpose, or non-infringement.
        </p>
        <p>
          Stated plainly: Loom is a tool that reads your files, runs shell commands and
          — if you arm it — controls your mouse and keyboard. A model can be wrong. It
          can delete something, run a destructive command, or click the wrong button.
          Use the permission modes and the panic stop, keep backups, and do not point
          an agent at anything you cannot afford to lose.
        </p>
        <p>
          In no event shall the authors or copyright holders be liable for any claim,
          damages or other liability arising from the software or its use.
        </p>
      </Clause>

      <Clause title="Your providers">
        <p>
          Loom is a client for model providers you choose and pay directly. It has no
          subscription, resells nothing, and is not a party to the agreement between
          you and your provider. Their terms govern the requests you make through
          them.
        </p>
      </Clause>

      <Clause title="This website">
        <p>
          The site exists to describe and distribute the software, and it comes with
          the same absence of warranty. It may be unavailable or inaccurate: the
          version number shown is read from GitHub and is only as current as the last
          release.
        </p>
      </Clause>

      <Clause title="Changes">
        <p>
          The licence cannot be revoked for a version you already have. If these terms
          change, they change for future releases — and the commit history, which is
          public, shows what changed and when.
        </p>
      </Clause>
    </Legal>
  );
}
