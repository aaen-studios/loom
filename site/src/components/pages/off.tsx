import Link from "next/link";
import { DOWNLOAD } from "@/lib/site";
import { Code, Pick } from "@/components/weave/pass";
import { passById } from "@/lib/weave/passes";

/**
 * Pass seven: off the loom.
 *
 * The last thing that happens to a cloth is that it is cut off the loom and
 * finished. So this is the closing ask — and it is short, because by here the page
 * has made its case, and another pitch would be arguing with someone who already
 * agrees.
 *
 * The cut is drawn: a row of warp threads above the call to action, ending at uneven
 * lengths, the way they do the moment the tension is released.
 */
const CUT = [14, 22, 9, 18, 26, 12, 20, 15, 24, 11, 19, 13];

export function OffTheLoom() {
  const pass = passById("off");

  return (
    <Pick
      pass={pass}
      title="Cut it off the loom."
      lead="Free, MIT licensed, and there is nothing to sign up for. Install it, add a key, point it at a folder."
    >
      <div className="panel rounded-sheet p-6 sm:p-8">
        {/* The cut edge. Uneven on purpose: a warp under tension is a flat line, and
            one that has just been released is not. */}
        <div aria-hidden="true" className="flex items-start gap-2">
          {CUT.map((height, index) => (
            <span
              key={index}
              className="w-px"
              style={{
                height: `${height}px`,
                background:
                  "linear-gradient(to bottom, var(--thread-line-strong), transparent)",
              }}
            />
          ))}
        </div>

        <div className="mt-6 flex flex-col items-start gap-5 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h3 className="text-[20px] font-medium tracking-tight sm:text-[24px]">
              Free, and there is nothing to sign up for.
            </h3>
            <p className="text-soft mt-2 text-[14px] leading-6">
              {DOWNLOAD.requirements} · No account · No telemetry
            </p>
          </div>
          <Link
            href={DOWNLOAD.publicPath}
            className="btn-primary h-11 shrink-0 px-5 text-[14.5px]"
          >
            Download for Windows
          </Link>
        </div>

        {/* The three things a visitor might still not know, one line each. */}
        <div className="mt-8 grid gap-6 border-t border-[var(--glass-border)] pt-6 sm:grid-cols-3">
          <Fact label="Install">
            One portable installer with the app embedded. Add <Code>--silent --dir</Code>{" "}
            and it installs unattended.
          </Fact>
          <Fact label="Updates">
            Payloads are signed with minisign and verified before anything is applied,
            so an update cannot be tampered with in transit.
          </Fact>
          <Fact label="Uninstall">
            Removing the app leaves <Code>~/.loom</Code> alone. Delete it yourself when
            you want it gone.
          </Fact>
        </div>
      </div>
    </Pick>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="text-faint text-[11px] font-medium tracking-[0.12em] uppercase">
        {label}
      </p>
      <p className="text-soft mt-2 text-[13px] leading-[1.6]">{children}</p>
    </div>
  );
}
