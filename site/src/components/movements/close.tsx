import Link from "next/link";
import { DOWNLOAD, REPO, SITE } from "@/lib/site";
import { PANELS } from "@/lib/panels";
import { MOVEMENTS } from "@/lib/movements";
import { Drifting } from "@/components/weave/animated";
import { Movement, Code, Note, P, Spec, Statement, Table } from "@/components/ui/primitives";

/**
 * Movements five and six: ground truth, and install.
 *
 * The last two, and the only ones where a reader has to be given *facts* rather than a feeling. The
 * figures stop being the argument here and become the setting: a figure behind a privacy inventory
 * would be competing with the only part of the page that has to be believed literally, so both
 * movements use the most restrained figure on the site and let it sit low in the frame.
 */

/* ===========================================================================
   Five — ground truth
=========================================================================== */

export function Ground() {
  return (
    <Movement
      id="ground"
      heading="Nothing leaves the machine except to your provider."
      lead="No analytics, no crash reporting, no account, no installation identifier. The source is public, so this is a claim you can check rather than one you have to take on faith."
    >
      {/* The quietest figure on the page: a wide, shallow field, faded almost to nothing. It is
          here to keep the ground from changing character between movements, not to be looked at. */}
      <div className="figure-plate figure-plate-low" aria-hidden="true">
        <Drifting
          kind="field"
          seed={7011}
          width={1800}
          height={340}
          detail={1.6}
          className="figure-art"
          opacity={0.45}
          id="ground"
        />
      </div>

      <Spec
        rows={[
          {
            term: "Keys",
            def: "Windows Credential Manager, one entry per provider. Never in a config file, never in a log, never uploaded.",
          },
          {
            term: "Conversations",
            def: (
              <>
                <Code>~/.loom/loom.db</Code> — SQLite, on your own disk, twelve migrations deep.
              </>
            ),
          },
          {
            term: "Command output",
            def: (
              <>
                <Code>~/.loom/logs</Code>, one file per tracked command, each capped at 5 MB.
              </>
            ),
          },
          {
            term: "Config snapshots",
            def: (
              <>
                <Code>~/.loom/backups</Code>, taken before every change the model makes to
                Loom&rsquo;s own configuration. The ten newest are kept.
              </>
            ),
          },
          {
            term: "Leaves the machine",
            def: "Your prompts, the files you attach, and whatever a tool asks to read — to the provider you selected, under their terms. Then update checks to GitHub, with the manifest's signature verified before anything is applied.",
          },
          {
            term: "Nothing else",
            def: "No telemetry, no feature flags, no remote configuration, and no identifier generated to recognise an installation. There is no code path that reports to us, because no such path was built.",
          },
        ]}
      />

      <Statement>
        Run a local model with the index off, and Loom makes no outbound request at all — except the
        update check, which can also be turned off.
      </Statement>

      <P>
        Set <Code>LOOM_HOME</Code> to move all of it. There is no sync and no export that uploads
        anything.
      </P>

      <Table
        caption="Table 2 — where the models come from"
        head={["Route", "Notes"]}
      >
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Twelve presets</b>
          </td>
          <td>
            OpenAI, Anthropic, OpenRouter, DeepSeek, Z.ai, Groq, xAI, Google, OpenCode Go, OpenCode
            Zen, Ollama, LM Studio.
          </td>
        </tr>
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Any endpoint</b>
          </td>
          <td>
            Any OpenAI-compatible or Anthropic-compatible base URL, with your own headers. A preset
            can be added twice, so two plans for one vendor each keep their own key.
          </td>
        </tr>
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Per chat</b>
          </td>
          <td>
            One conversation can run a local model for a quick question while another works through
            a repository with a frontier model. Neither has to be reconfigured.
          </td>
        </tr>
        <tr>
          <td>
            <b className="font-medium text-[var(--ink)]">Usage</b>
          </td>
          <td>
            Live vendor limits wherever the provider exposes them, plus a local table of tokens and
            list-price cost built from your own database.
          </td>
        </tr>
      </Table>

      <Note>
        Command output is logged whether or not you are watching the command, which is worth knowing
        before running something that might print a secret. The cap is per command rather than in
        total, so eight long-running commands can hold 40&nbsp;MB between them — in the clear, on
        your disk. Uninstalling leaves <Code>~/.loom</Code> in place, because deleting your
        conversations as a side effect of removing a binary would be hostile; deleting that folder is
        the whole deletion process, and it is yours to run.
      </Note>
    </Movement>
  );
}

/* ===========================================================================
   Six — install
=========================================================================== */

/**
 * The questions, and the download.
 *
 * Two things that do not belong in a movement of their own and do not belong in the one before it,
 * so they share the last movement: the questions a reader has at this point — including the two a
 * page like this would rather not be asked — and the one action the page contains.
 *
 * Static prose rather than a disclosure widget, and that is load-bearing. A collapsible answer is
 * invisible to in-page search and to anyone reading without JavaScript, which for a question that
 * decides whether someone installs the software is exactly the wrong trade. Six questions are short
 * enough to read.
 */
const QUESTIONS = [
  {
    q: "Is it really free?",
    a: "Yes, and there is nothing to unlock. No paid tier, no account, no licence key. You pay your provider directly for the tokens you use, and that bill never passes through this project. A local model through Ollama or LM Studio costs nothing at all.",
  },
  {
    q: "Why does Windows say the publisher is unknown?",
    a: "Because the installer is not code-signed yet. A certificate is a recurring cost, and until one is in place SmartScreen shows the same generic warning for any installer that lacks it. That is separate from the updater: release payloads are signed with minisign and the application verifies that signature before applying anything, so an update cannot be tampered with in transit — but that is not the same as proving who built the installer. The download page publishes the SHA-256 of the file and tells you how to check it.",
  },
  {
    q: "Does it work on macOS or Linux?",
    a: "Not yet, and the Windows-specific parts are not incidental. Loom is built on Tauri, which is cross-platform, but computer use is built on UI Automation and Win32 input, the credential store is Windows Credential Manager, the terminal uses ConPTY, and launch-at-login is a registry entry. This page will say so plainly when another platform is real rather than planned.",
  },
  {
    q: "What does it do with my code?",
    a: "It reads the folder you point it at and sends what it reads to the provider of the model you chose, which is what pasting a file into any other client does. Nothing goes anywhere else, and indexing uses your own provider's embedding endpoint. Run a local model and nothing leaves the machine at all.",
  },
  {
    q: "Can it run commands while I am not watching?",
    a: "Yes, deliberately, and it is bounded. A command that outlives its deadline is adopted rather than killed, and at most eight are tracked at once so processes cannot accumulate invisibly. All of them are listed in the Runs panel with a Stop button, output is capped at 5 MB per command, and a command still running after a restart is reported as orphaned rather than assumed dead — because it may well still be alive.",
  },
  {
    q: "What happens to my chats if I uninstall?",
    a: "They stay. Uninstalling removes the application from %LOCALAPPDATA%\\Loom and the shortcuts it created. ~/.loom is left alone, because deleting your conversations as a side effect of removing a binary would be hostile. Delete that folder yourself when you want it gone, and that is the entire deletion process.",
  },
] as const;

export function Install() {
  return (
    <Movement
      id="install"
      heading="Then it is yours."
      lead="One portable installer with the application embedded. No runtime to install first, no framework, and nothing to configure beyond adding a provider key."
    >
      <div className="figure-plate figure-plate-low" aria-hidden="true">
        <Drifting
          kind="bundle"
          seed={991}
          width={1800}
          height={260}
          detail={2}
          className="figure-art"
          opacity={0.4}
          id="install"
        />
      </div>

      <div className="mt-10 flex flex-wrap items-center gap-4">
        <Link href={DOWNLOAD.publicPath} className="btn-primary h-12 px-6 text-[15px]">
          Download for Windows
        </Link>
        <span className="t-small">{DOWNLOAD.requirements} · the current release</span>
      </div>

      <div className="mt-16">
        <p className="t-label">Before you install</p>
        <div className="mt-5 max-w-[var(--measure)]">
          {QUESTIONS.map((item, index) => (
            <div
              key={item.q}
              className="grid grid-cols-[2.25rem_1fr] gap-x-4 border-b border-[var(--glass-border)] py-5 first:border-t first:border-[var(--glass-border)]"
            >
              <span className="t-index pt-[0.15rem]" aria-hidden="true">
                {String(index + 1).padStart(2, "0")}
              </span>
              <div>
                <h3 className="t-h3">{item.q}</h3>
                <p className="mt-2 text-[14px] leading-[1.62] text-soft">{item.a}</p>
              </div>
            </div>
          ))}
        </div>
      </div>

      <Spec
        rows={[
          {
            term: "Format",
            def: "A single self-contained executable. The application is embedded into the installer at build time, so there is no payload to fetch afterwards.",
          },
          {
            term: "Silent install",
            def: (
              <>
                <Code>--silent --dir &lt;path&gt;</Code> does the whole thing without a window.
              </>
            ),
          },
          {
            term: "Updates",
            def: "The payload is minisign-signed and verified before anything is applied. A release carrying no signature is refused rather than installed, which is the intended behaviour and not a fault.",
          },
          {
            term: "Leaves behind",
            def: "~/.loom, in full. Nothing in the install folder is needed to read your data, and nothing in your data is needed for the application to run.",
          },
        ]}
      />

      <P>
        Releases are built and published from the{" "}
        <a href={REPO.url} target="_blank" rel="noreferrer" className="link">
          public repository
        </a>
        , which is {SITE.license} licensed and published by {SITE.publisher}. The download button
        resolves through this site rather than pointing at GitHub directly, so a link you have
        already copied keeps working if the repository is ever renamed or moved.
      </P>

      <Note>
        {MOVEMENTS.length} movements, {PANELS.length} panels, one window. If any of the claims above
        turn out to be wrong, the fix is a pull request rather than a support ticket — that is the
        point of shipping the source beside the installer.
      </Note>
    </Movement>
  );
}
