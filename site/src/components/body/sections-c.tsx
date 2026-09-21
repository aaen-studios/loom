import Link from "next/link";
import { DOWNLOAD, REPO } from "@/lib/site";
import { Action, Section, Sub } from "@/components/doc/section";
import { Code, Hanging, Note, P, Shell } from "@/components/doc/text";
import { Glossary } from "@/components/doc/glossary";
import { APPENDICES, SECTIONS } from "@/lib/document";

/**
 * Section 7, section 8, and the two appendices.
 *
 * The last of the argument and then the reference material. Section 7 is the
 * questions a reader has at this point — including the two a product page would
 * rather not be asked — and section 8 is the one part of the document that is
 * instructions rather than description, so it is the one part written as numbered
 * steps.
 */

// ---------------------------------------------------------------------------
// 7 — Heddles
// ---------------------------------------------------------------------------

/**
 * The questions.
 *
 * Chosen for being the ones the repository already answers rather than the ones a
 * marketing page would prefer to be asked. Two of them are unflattering: the
 * installer is not code-signed, and the application runs on Windows only. Answering
 * those at length is worth more than a fourth paragraph about streaming, because a
 * reader who is going to be disappointed by either one is better served now than
 * after a 90 MB download.
 */
const QUESTIONS = [
  {
    q: "Is it really free?",
    a: "Yes, and there is nothing to unlock. Loom has no paid tier, no account and no licence key. You pay your provider directly for the tokens you use, and that bill never passes through this project. A local model through Ollama or LM Studio costs nothing at all.",
  },
  {
    q: "Why does Windows say the publisher is unknown?",
    a: "Because the installer is not code-signed yet. A certificate is a recurring cost, and until one is in place SmartScreen shows the same generic warning for any installer that lacks it. That is separate from the updater: release payloads are signed with minisign and the application verifies that signature before applying anything, so an update cannot be tampered with in transit — but that is not the same as proving who built the installer. The download page publishes the SHA-256 of the file so you can check it yourself, and tells you how.",
  },
  {
    q: "Does it work on macOS or Linux?",
    a: "Not yet, and the Windows-specific parts are not incidental. Loom is built on Tauri, which is cross-platform, but computer use is built on UI Automation and Win32 input, the credential store is Windows Credential Manager, the terminal uses ConPTY, and launch-at-login is a registry entry. This section will say so plainly when another platform is real rather than planned.",
  },
  {
    q: "What does Loom do with my code?",
    a: "It reads the folder you point it at and sends what it reads to the provider of the model you chose, which is the same thing pasting a file into any other client does. Nothing goes anywhere else, and indexing uses your own provider's embedding endpoint. Run a local model and nothing leaves the machine at all.",
  },
  {
    q: "Can it run commands while I am not watching?",
    a: "Yes, deliberately, and it is bounded. A command that outlives its deadline is adopted rather than killed, and at most eight are tracked at once so processes cannot accumulate invisibly. All of them are listed in the Runs panel with a Stop button, output is capped at 5 MB per command, and a command still running after a restart is reported as orphaned rather than assumed dead — because it may well still be alive.",
  },
  {
    q: "What is the difference between Atelier and Build?",
    a: "Build can change your workspace. Atelier can change Loom itself: it additionally exposes tools that edit personas, prompts, skills, MCP servers, providers and settings. Every configuration write takes a snapshot first. It is deliberately per-chat, with no way to make it the global default, and it is the one mode that runs deletes without a confirmation card — see the note in section 3 for exactly what that does and does not protect.",
  },
  {
    q: "What happens to my chats if I uninstall?",
    a: "They stay. Uninstalling removes the application from %LOCALAPPDATA%\\Loom and the shortcuts it created. ~/.loom is left alone, because deleting someone's conversations as a side effect of removing a binary would be hostile. Delete that folder yourself when you want it gone, and that is the entire deletion process.",
  },
] as const;

function Heddles() {
  const entry = SECTIONS[6];
  return (
    <Section entry={entry}>
      <P>
        Seven questions. The first two are the ones that actually decide whether you
        install this, and the last two are the ones a page like this usually declines to
        answer.
      </P>

      <div className="mt-6">
        {QUESTIONS.map((item, index) => (
          <Hanging key={item.q} n={index + 1} term={item.q}>
            {item.a}
          </Hanging>
        ))}
      </div>

      <Note>
        A section of this document answering its own questions, rather than a
        disclosure widget, is not an oversight. A collapsible answer is hidden from
        in-page search and from anyone reading the page without JavaScript, which for a
        question that matters is exactly the wrong trade — and the questions here are
        short enough to read.
      </Note>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// 8 — Off the loom
// ---------------------------------------------------------------------------

function Off() {
  const entry = SECTIONS[7];
  return (
    <Section entry={entry}>
      <P>
        One portable installer with the application embedded. No runtime to install
        first, no framework, nothing to configure beyond adding a provider key.
      </P>

      <Specish />

      <Action>
        <Link href={DOWNLOAD.publicPath} className="btn-primary h-10 px-4 text-sm">
          Download for Windows
        </Link>
        <span className="action-note">
          {DOWNLOAD.requirements} · the current release
        </span>
      </Action>

      <Sub>Installing</Sub>

      <ol className="steps">
        <li>
          Run the installer. Windows will warn that the publisher is unknown &mdash; see
          question 2 above, which is the honest answer rather than a reassurance.
        </li>
        <li>
          Choose a folder, or accept the default. If Loom is already installed somewhere
          else, the installer finds it through its uninstall entry and updates it in
          place rather than installing a second copy.
        </li>
        <li>
          Open Loom, add a provider key in Settings &rarr; Providers, and pick a model
          in the composer. If you would rather not add a key at all, point it at Ollama
          or LM Studio and it will find the local models.
        </li>
      </ol>

      <P>
        For a scripted install, <Code>--silent --dir &lt;path&gt;</Code> does the whole
        thing unattended.
      </P>

      <Sub>Verifying what you downloaded</Sub>

      <P>
        Because the installer carries no Authenticode signature, the honest substitute
        is a hash you can check against a number published somewhere other than the file
        itself. In PowerShell:
      </P>

      <Shell>
        {`# in the folder you saved the installer
Get-FileHash .\\Loom-Setup-<tag>.exe -Algorithm SHA256

# compare with the value in the release notes
${REPO.releasesUrl}`}
      </Shell>

      <Note>
        Updates are a separate matter and a stronger one. The payload the updater
        installs is signed with minisign and verified before anything is applied, so an
        update cannot be tampered with in transit even though the installer that
        delivered it cannot prove who built it. A release carrying no signature is
        refused by the application rather than installed — that is the intended
        behaviour, not a fault.
      </Note>

      <Sub>Uninstalling</Sub>

      <P>
        Either add/remove programs, or run{" "}
        <Code>%LOCALAPPDATA%\Loom\uninstall.cmd</Code>, which lives outside the install
        folder so that it can delete the install folder. Your chats, keys and settings
        are deliberately left in place;{" "}
        <Link href="/privacy" className="text-[var(--accent)] hover:underline">
          the privacy page
        </Link>{" "}
        lists exactly what remains and where.
      </P>
    </Section>
  );
}

/** The four facts about the download, as a spec block. */
function Specish() {
  return (
    <dl className="spec">
      {[
        {
          term: "Format",
          def: "One self-contained executable. The application is embedded in the installer at build time, so there is no payload to fetch afterward.",
        },
        {
          term: "Signing",
          def: "The installer is unsigned; the update payload is minisign-signed and verified before it is applied.",
        },
        {
          term: "Footprint",
          def: "Installs to a folder you choose, writes the uninstaller to %LOCALAPPDATA%\\Loom, and registers one HKCU uninstall entry and the shortcuts you asked for.",
        },
        {
          term: "What it leaves behind",
          def: "~/.loom, in full. Nothing in the install folder is needed to read your data, and nothing in your data is needed for the application to run.",
        },
      ].map((row) => (
        <div className="spec-row" key={row.term}>
          <dt className="spec-term">{row.term}</dt>
          <dd className="spec-def">{row.def}</dd>
        </div>
      ))}
    </dl>
  );
}

// ---------------------------------------------------------------------------
// Appendix A — shortcuts
// ---------------------------------------------------------------------------

/**
 * The shortcut sheet.
 *
 * A table rather than the application's own `?` sheet, because a reference table is
 * what a manual is for — and because this is the only place outside the application
 * that the keys are written down at all, which is worth noting: voice mode has no
 * button, so this table is not a convenience, it is the documentation.
 */
const SHORTCUTS = [
  { keys: "Ctrl+`", what: "The dock, on or off. Works from inside the terminal." },
  { keys: "Ctrl+,", what: "Settings." },
  { keys: "Ctrl+N", what: "A new chat." },
  { keys: "Ctrl+K", what: "The chats list. Focuses its search if it is already open." },
  { keys: "Ctrl+Shift+Space", what: "The quick-ask overlay: a new chat from anywhere in Windows." },
  { keys: "Ctrl+Shift+V", what: "Voice mode — speak and listen. There is no button for this." },
  { keys: "Ctrl+Alt+Esc", what: "Stop the turn that is driving the machine with computer use." },
  { keys: "?", what: "Every shortcut, in the application's own sheet." },
  { keys: "Escape", what: "Cancel a panel drag in flight." },
] as const;

function Shortcuts() {
  const entry = APPENDICES[0];
  return (
    <Section entry={entry}>
      <P>
        Nine keys, which is close to all of them. Loom deliberately has no shortcut for
        switching to IDE mode, and the reason is worth a line: Monaco claims most of{" "}
        <Code>Ctrl+&lt;letter&gt;</Code>, so a global binding would work everywhere
        except the editor it was added for.
      </P>

      <table className="booktabs wide mt-7">
        <caption>
          <b>Table 4</b> — Keyboard shortcuts
        </caption>
        <thead>
          <tr>
            <th scope="col">Keys</th>
            <th scope="col">What it does</th>
          </tr>
        </thead>
        <tbody>
          {SHORTCUTS.map((row) => (
            <tr key={row.keys}>
              <td>
                <kbd className="kbd">{row.keys}</kbd>
              </td>
              <td>{row.what}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// Appendix B — glossary
// ---------------------------------------------------------------------------

function GlossaryEntry() {
  const entry = APPENDICES[1];
  return (
    <Section entry={entry}>
      <P>
        Seven words, borrowed from weaving and used as the part names of this document
        and of several panels in the application. They are all the vocabulary there is;
        nothing on this page depends on knowing them, and every one is defined here
        because that is the condition on which a document is allowed to use one.
      </P>

      <Glossary />

      <Note>
        The parallels are close for six of the seven and loose for one. <b>Heddles</b>{" "}
        is the loose one, and its entry says so rather than dressing it up &mdash; which
        is more honest than the previous version of this site managed, where all seven
        were presented as though the mapping were exact and none of them was defined.
      </Note>
    </Section>
  );
}

export function SectionsC() {
  return (
    <>
      <Heddles />
      <Off />
      <Shortcuts />
      <GlossaryEntry />
    </>
  );
}
