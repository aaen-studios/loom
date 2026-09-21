import { Section, Sub } from "@/components/doc/section";
import { Code, Note, P } from "@/components/doc/text";
import { Spec } from "@/components/doc/spec";
import { Table } from "@/components/doc/table";
import { SECTIONS } from "@/lib/document";

/**
 * Sections 4 to 6.
 *
 * The reference half of the document: what the application is made of, what it can
 * be connected to, and where your data ends up. Sections 4 and 5 are inventories and
 * read as such; section 6 is the one that has to be believed rather than read, so it
 * is written as a list of paths and destinations rather than as a promise.
 */

// ---------------------------------------------------------------------------
// 4 — Ends
// ---------------------------------------------------------------------------

/** The capabilities that earn a name and a paragraph. */
const ENDS = [
  {
    title: "Reasoning, kept where it happened",
    body: (
      <>
        A thinking model&rsquo;s notes arrive in the transcript at the point they were
        produced, not hoisted into a summary above the answer. One panel per spell,
        each collapsible on its own, so you can watch the model change its mind rather
        than only seeing what it settled on.
      </>
    ),
  },
  {
    title: "A real terminal",
    body: (
      <>
        Sessions are driven by a pty &mdash; ConPTY on Windows &mdash; so colours,
        interactive prompts, arrow keys, resizing and full-screen programs all work.
        All sixteen ANSI colours are Loom&rsquo;s own rather than xterm&rsquo;s stock
        palette, which is pure-hue and clashes badly with the rest of the window.
        Closing a tab kills the process tree; quitting kills them all.
      </>
    ),
  },
  {
    title: "An editor, and diffs that are diffs",
    body: (
      <>
        Monaco, with find and replace, folding, multi-cursor and a command palette, in
        a theme derived from the application&rsquo;s own stylesheet. A changed file
        opens as an exact diff in a real diff editor &mdash; not as patch text, which
        is a description of a change rather than the change.
      </>
    ),
  },
  {
    title: "Commands that outlive the turn",
    body: (
      <>
        A command that runs past its deadline is <em>adopted</em>, not killed. It gets
        an id, keeps streaming to a log on disk, and appears in the Runs panel with a
        Stop button. At most eight are tracked at once, so processes cannot pile up
        invisibly; stop ends the process tree, so an <Code>npm test</Code>&rsquo;s
        workers go with it.
      </>
    ),
  },
  {
    title: "A workspace it can search",
    body: (
      <>
        Point a chat at a folder and index it. Files are chunked and embedded through
        your own provider, and the model can call <Code>search_workspace</Code> for a
        semantic lookup instead of grepping blindly. Indexing is opt-in per chat.
      </>
    ),
  },
  {
    title: "MCP servers, skills and saved prompts",
    body: (
      <>
        Connect MCP servers for extra tools; they arrive in the same permission system
        as the built-in ones, so a new capability is not a new trust model. Skills are
        markdown files in <Code>~/.loom/skills</Code>, listed in the composer&rsquo;s{" "}
        <Code>/</Code> menu beside saved prompts and the built-in commands.
      </>
    ),
  },
  {
    title: "Replies that render live UI",
    body: (
      <>
        A <Code>loom-ui</Code> fenced block becomes a real, themed widget inside the
        reply &mdash; a comparison, a table, a set of options to choose from &mdash;
        sanitised against an allowlist with no scripts and no event handlers, mounted
        in a shadow root styled from the application&rsquo;s design tokens so it tracks
        the theme on its own. The model gets to hand you something usable instead of
        describing it.
      </>
    ),
  },
  {
    title: "Computer use, with a brake",
    body: (
      <>
        With the chip armed, the model can see the screen and drive the mouse and
        keyboard, run UI Automation and launch applications. Real input from you pauses
        the turn; a control pill on the monitor always shows what it is doing;{" "}
        <Code>Ctrl+Alt+Esc</Code> stops it from anywhere. A read-only agent mode
        narrows the chip to screenshots and listings and says so, rather than
        advertising a mouse it will refuse to use.
      </>
    ),
  },
  {
    title: "Usage you can see",
    body: (
      <>
        Live vendor limits wherever the provider exposes them &mdash; subscription
        windows, credit balances, plan quotas &mdash; plus a local table of tokens and
        list-price cost per provider, built from reply metadata in your own database.
        The composer carries the active model&rsquo;s tightest window as a small badge.
        None of it is uploaded anywhere.
      </>
    ),
  },
  {
    title: "Condensing instead of truncating",
    body: (
      <>
        A chat that outgrows the model&rsquo;s window is condensed, never truncated
        with a note: the last user turn and everything after it stay verbatim, and older
        turns fold into one summary at the head of the request. A deterministic digest
        goes out immediately and a model-written summary replaces it from the next turn
        on. Every reply answered from a folded view says so in one faint line, which
        expands to the exact text the model was given.
      </>
    ),
  },
] as const;

function Ends() {
  const entry = SECTIONS[3];
  return (
    <Section entry={entry}>
      <P>
        Ten things, taken from the repository rather than from an idea of the product,
        because an inaccurate feature list is the fastest way to lose a reader on a page
        whose only job is to hand them a download. They are ordered by how hard they are
        to find elsewhere rather than by how easy they are to describe.
      </P>

      <Spec rows={ENDS.map((item) => ({ term: item.title, def: item.body }))} />

      <Sub>And the rest, stated once</Sub>

      <P>
        A quick-ask overlay on <Code>Ctrl+Shift+Space</Code> that summons a new chat
        from anywhere; voice mode on <Code>Ctrl+Shift+V</Code>; a tray icon and launch
        at login; image generation; subagents; attachments including images, text and
        code files and text-layer PDFs; a message queue you can reorder by dragging;
        two-pass chat titles; <Code>@</Code> to complete a file in the workspace and{" "}
        <Code>#</Code> to reference another chat; and an updater that verifies a
        signature before it applies anything.
      </P>

      <Note>
        Two of those are deliberately not obvious from the interface, which is worth
        knowing before you go looking. Voice mode has no button &mdash; it is{" "}
        <Code>Ctrl+Shift+V</Code> and a row in the shortcut sheet, which makes the
        sheet the only place its keys are discoverable. And the terminal is the
        user&rsquo;s alone: <Code>pty_write</Code> is not an agent tool, is not
        advertised to the model, and no engine path can call it, because a live shell
        has no permission card in front of it and the only safe arrangement is that the
        agent cannot type into one.
      </Note>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// 5 — Count
// ---------------------------------------------------------------------------

const PROVIDERS = [
  "OpenAI",
  "Anthropic",
  "OpenRouter",
  "DeepSeek",
  "Z.ai",
  "Groq",
  "xAI",
  "Google",
  "OpenCode Go",
  "OpenCode Zen",
  "Ollama",
  "LM Studio",
] as const;

function Count() {
  const entry = SECTIONS[4];
  return (
    <Section entry={entry}>
      <P>
        Loom is a client. It has no subscription, resells nothing, and never proxies
        your traffic: you add a key and the requests go from your machine to the
        provider you chose. Twelve providers are set up for you, and any OpenAI- or
        Anthropic-compatible endpoint can be added by hand.
      </P>

      <Spec
        rows={[
          {
            term: "Named presets",
            def: PROVIDERS.join(" · "),
          },
          {
            term: "Local models",
            def: "Ollama and LM Studio are first-class rather than an afterthought. Nothing leaves the machine in that configuration.",
          },
          {
            term: "Custom endpoints",
            def: "Any OpenAI-compatible or Anthropic-compatible base URL, with your own headers. A preset can be added more than once, so two plans for the same vendor each keep their own key — the duplicate copies the endpoint, the headers and the model catalogue but never the key.",
          },
        ]}
      />

      <Sub>Keys, and where they are not</Sub>

      <P>
        API keys go into Windows Credential Manager, one entry per provider. They are
        never written to a configuration file, never included in a log, and never
        uploaded. The rest of your data lives in <Code>~/.loom</Code> on your own disk,
        which{" "}
        <a href="#selvedge" className="text-[var(--accent)] hover:underline">
          section 6
        </a>{" "}
        lists path by path.
      </P>

      <Sub>Model lists</Sub>

      <P>
        Loom asks the provider which models exist &mdash; <Code>GET /models</Code>{" "}
        &mdash; and merges the answer over a catalogue bundled with the application.
        Anything it gets wrong can be corrected by hand: a context window, an output
        cap, a reasoning variant. Your correction is stamped as yours and a later
        refresh will not overwrite it.
      </P>

      <P>
        The choice of which models appear in pickers is stored as a{" "}
        <em>denylist</em>, which means an untouched configuration and every model a
        provider adds later are both selected by default. Unselecting a model hides it
        from the pickers without breaking a chat already using it, and any row still
        referenced by a default or by recent history carries an &ldquo;in use&rdquo;
        chip.
      </P>

      <Note>
        Everything above is per chat. One conversation can be running a local model for
        a quick question while another is working through a repository with a frontier
        model, and neither has to be reconfigured to do it.
      </Note>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// 6 — Selvedge
// ---------------------------------------------------------------------------

/**
 * The permission modes, as a table.
 *
 * Written out rather than described because this is the setting a reader actually
 * has to choose between, and because the honest way to present a permission system
 * is to state exactly what each level permits rather than to characterise it as
 * "safer" or "more capable".
 */
const PERMISSIONS = [
  {
    mode: "Ask",
    allows: "Reads within the workspace. Every write, every command and every path change stops and asks.",
    refuses: "Nothing outright — it asks instead.",
  },
  {
    mode: "Auto read-only",
    allows: "Reads freely, without a card, including the semantic index and the git tools.",
    refuses: "Writes and commands, which still ask.",
  },
  {
    mode: "Auto run all",
    allows: "Reads, writes and commands, all without stopping to ask.",
    refuses: "Deletes that the risk check flags, and anything Atelier's exemption covers — this level deliberately does not inherit it.",
  },
] as const;

function Selvedge() {
  const entry = SECTIONS[5];
  return (
    <Section entry={entry}>
      <P>
        Nothing leaves the machine except to the provider you configured. No analytics,
        no crash reporting, no account, no installation identifier. This section says
        specifically what that means, so it can be checked rather than believed &mdash;
        and the check is short, because there is not much of it.
      </P>

      <Sub>What is stored, and where</Sub>

      <Table n={3} head={["Path", "What it holds"]}>
        <tr>
          <td>
            <Code>~/.loom/config.json</Code>
          </td>
          <td>
            Theme, background, providers, personas, MCP servers, saved prompts,
            workspaces, dock layout, and per-chat defaults.
          </td>
        </tr>
        <tr>
          <td>
            <Code>~/.loom/loom.db</Code>
          </td>
          <td>
            Sessions, messages, and the per-chat workspace index. SQLite, twelve
            migrations deep.
          </td>
        </tr>
        <tr>
          <td>
            <Code>~/.loom/attachments/</Code>
          </td>
          <td>Files sent in chats, grouped per session.</td>
        </tr>
        <tr>
          <td>
            <Code>~/.loom/logs/</Code>
          </td>
          <td>
            One <Code>cmd-&lt;id&gt;.log</Code> per tracked shell command, capped at
            5 MB each.
          </td>
        </tr>
        <tr>
          <td>
            <Code>~/.loom/backups/</Code>
          </td>
          <td>
            Configuration snapshots taken before every change the model makes to
            Loom&rsquo;s own configuration, and copies of overwritten skill files. The
            ten newest snapshots are kept.
          </td>
        </tr>
        <tr>
          <td>
            <Code>~/.loom/generated/</Code>
          </td>
          <td>Images produced by the image tool.</td>
        </tr>
        <tr>
          <td>
            Credential Manager
          </td>
          <td>
            Provider keys, one entry each. Not in any of the files above, and not in
            this list&rsquo;s reach.
          </td>
        </tr>
      </Table>

      <P>
        Set <Code>LOOM_HOME</Code> to move all of it. There is no sync and no export
        that uploads anything.
      </P>

      <Note>
        Command output is logged whether or not you are watching the command, which is
        worth knowing before you run something that might print a secret. The cap is
        per command, not in total, so eight long-running commands can hold 40 MB
        between them — in <Code>~/.loom/logs</Code>, in the clear, on your disk.
      </Note>

      <Sub>What leaves</Sub>

      <P>
        <b>Your conversation, to your provider.</b> Prompts, the files you attach, and
        whatever a tool asks to read &mdash; sent to whichever provider and model you
        selected, under their terms. Loom is a client: it does not sit in the middle
        and cannot see that traffic.
      </P>

      <P>
        <b>Embeddings, when you index a workspace.</b> File chunks go to your
        provider&rsquo;s embedding endpoint so they can be searched semantically.
        Opt-in per chat, and skip it entirely and the rest of the application works
        unchanged.
      </P>

      <P>
        <b>Update checks, to GitHub.</b> Loom fetches a release manifest and verifies
        its signature before applying anything. No identifiers, no usage data and no
        machine fingerprint are sent with it.
      </P>

      <P>
        That is the whole list. Web search and page fetching go to whichever service is
        configured &mdash; your own Jina key if you have stored one, DuckDuckGo
        otherwise. Computer use captures screenshots, which are written to{" "}
        <Code>attachments/</Code> and inlined into the request only for the newest
        capture.
      </P>

      <Note>
        To send nothing at all: configure a local model through Ollama or LM Studio and
        leave the workspace index off. In that configuration Loom makes no outbound
        request except the update check, and you can turn that off too.
      </Note>

      <Sub>Telemetry, or the absence of it</Sub>

      <P>
        There is none, and it is not a setting. No crash reporting, no usage
        statistics, no feature flags, no remote configuration, and no identifier
        generated to recognise an installation. There is no code path that reports to
        the publisher. The source is public and MIT licensed, so this is a claim you
        can check in a way that a privacy policy cannot be checked &mdash; which is why
        the licence matters to this section and not only to the legal one.
      </P>

      <Sub>Permissions</Sub>

      <P>
        Three levels, set globally with a per-chat override, alongside the agent mode in
        the composer. They are separate settings because they answer different
        questions: the mode decides whether the model <em>may</em> write at all, and the
        permission level decides whether it has to ask first.
      </P>

      <Table n={2} head={["Level", "Runs without asking", "Still asks"]}>
        {PERMISSIONS.map((row) => (
          <tr key={row.mode}>
            <td>
              <b>{row.mode}</b>
            </td>
            <td>{row.allows}</td>
            <td>{row.refuses}</td>
          </tr>
        ))}
      </Table>

      <Note>
        There is no level that runs a delete without a card, and no way to turn the
        card off globally &mdash; the exemption is a property of one agent mode, chosen
        for one chat, and it is documented in{" "}
        <a href="#pick" className="text-[var(--accent)] hover:underline">
          section 3
        </a>
        . Uninstalling the application leaves <Code>~/.loom</Code> untouched, because
        deleting someone&rsquo;s conversations as a side effect of removing a binary
        would be hostile. Removing the folder is the whole deletion process.
      </Note>
    </Section>
  );
}

export function SectionsB() {
  return (
    <>
      <Ends />
      <Count />
      <Selvedge />
    </>
  );
}
