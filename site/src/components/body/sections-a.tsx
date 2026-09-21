import { Section, Sub } from "@/components/doc/section";
import { Code, Note, P } from "@/components/doc/text";
import { Table } from "@/components/doc/table";
import { FigureWindow } from "@/components/figures/window";
import { FigureDropTargets } from "@/components/figures/drop-targets";
import { FigureTurn } from "@/components/figures/turn";
import { SECTIONS } from "@/lib/document";

/**
 * Sections 1 to 3.
 *
 * The document's argument, in order: what the thing is, the window it runs in, and
 * one turn of the agent end to end. Everything after this is reference material.
 *
 * The sections are in three files rather than one so that a reader opening any of
 * them sees a tractable amount of JSX, and so that the diffusion of responsibility
 * — nobody reads a two-thousand-line component — does not set in on prose this
 * dense. They are read in order and they compose into one document; there is no
 * other structure here, which is what "a manual" means.
 */

// ---------------------------------------------------------------------------
// 1 — Instrument
// ---------------------------------------------------------------------------

function Instrument() {
  const entry = SECTIONS[0];
  return (
    <Section entry={entry}>
      <P>
        Loom is a Windows application for talking to language models and letting them
        work on your machine. It is not a chat wrapper with a file tree bolted on: the
        terminal, the editor, the git client and the file browser are all in the same
        window as the conversation, docked to its edges, and the model can use the same
        tools you can see.
      </P>

      <P>
        You point it at a folder. It reads files, searches them, edits them, runs
        commands. Every step of that is on screen as it happens &mdash; the reasoning
        before the answer, each tool call between the two, the command output as it
        streams &mdash; because a model you cannot audit is a model you have to trust,
        and that is a much worse arrangement than one you can watch.
      </P>

      <Sub>Three things worth knowing before the detail</Sub>

      <P>
        <b>It is a client, not a service.</b> There is no subscription and no server
        of the publisher&rsquo;s own. You add a key for a provider you have chosen, and
        requests go from your machine straight to them. Point it at a model running
        locally and nothing leaves at all. This is covered properly in{" "}
        <a href="#selvedge" className="text-[var(--accent)] hover:underline">
          section 6
        </a>
        .
      </P>

      <P>
        <b>It is not a wrapper around a command line.</b> The terminal is a real pty,
        so colours, arrow keys, resizing and full-screen programs work. The editor is
        Monaco, with diffs rendered as diffs rather than as patch text. These are the
        two things most clients of this kind get wrong, and they are wrong in ways that
        make the feature decorative.
      </P>

      <P>
        <b>It is one application, not a suite.</b> The dock, the agent and the editor
        share one shell deliberately &mdash; a second application window would have
        reimplemented the docking, the shortcuts and the theme, and then drifted from
        it.
      </P>

      <Note>
        Loom is at version 0.1 and feature-complete for it. No release has been tagged
        at the time of writing, which the{" "}
        <a href="/download" className="text-[var(--accent)] hover:underline">
          download page
        </a>{" "}
        states plainly rather than filling in a version number that would be a lie.
      </Note>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// 2 — Warp
// ---------------------------------------------------------------------------

function Warp() {
  const entry = SECTIONS[1];
  return (
    <Section entry={entry}>
      <P>
        The window is a docking layout. Any edge can hold a <b>zone</b> &mdash; a
        resizable strip containing a stack of panels as tabs &mdash; and there are
        eight panels to put in them: Terminal, Runs, Chats, Files, Goal, Browser, Git
        and Editor.
      </P>

      <P>
        Every zone starts <b>shut</b>, which is the decision that shapes everything
        else. Loom opens on the conversation. Nothing is taking width from the chat
        until you ask for it, and{" "}
        <Code>Ctrl+`</Code>{" "}
        brings the whole dock back from anywhere &mdash; including from inside the
        terminal, which is the case that catches naive implementations.
      </P>

      <FigureWindow />

      <Sub>Two arrangements that were tried and removed</Sub>

      <P>
        A rail of icons down the window edge, and a hover band that revealed a panel
        when the pointer reached the edge. Both are gone, and the reasons are worth
        recording because both look like improvements on paper. The rail was permanent
        chrome over the artwork, held open for a surface that is usually shut. The
        hover band fought the native window-resize handle, so the two things you could
        do at the edge of the window &mdash; resize it, and open a panel &mdash;
        competed for the same eight pixels.
      </P>

      <P>
        What replaced both is one <b>Panels</b> menu in the title bar. It lists every
        panel with what is open and what is docked, and it stays correct when a ninth
        panel is added, which a fixed pair of buttons could not do.
      </P>

      <Sub>Rearranging it</Sub>

      <P>
        Panels are moved by dragging their tab. A ghost follows the cursor and the
        drop target is whatever is underneath it, so the three possible destinations
        are settled by where you let go rather than by a menu you had to find first.
        <Code>Escape</Code> cancels a drag in flight, and the tab&rsquo;s own{" "}
        <Code>⋯</Code> menu carries the same three moves for anyone who would rather
        not drag.
      </P>

      <FigureDropTargets />

      <Note>
        A layout is stored per workspace folder, in the application&rsquo;s Rust
        configuration rather than in the web view, because a panel dragged out of the
        window becomes a second web view &mdash; and two web views cannot share one
        JavaScript store. The backend enforces only structural rules: a panel appears
        in exactly one zone, the active tab index is in range, sizes are sane. It
        never invents, drops or reorders a panel id, so opening your configuration in
        an older build is not destructive.
      </Note>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// 3 — Pick
// ---------------------------------------------------------------------------

function Pick() {
  const entry = SECTIONS[2];
  return (
    <Section entry={entry}>
      <P>
        A turn is what happens between your message and the reply. This section is one
        of them, in the order its parts arrived, because the order is the claim: the
        reasoning is kept where it happened rather than collected into a summary, and
        the tool calls sit between the thinking that produced them and the answer that
        followed.
      </P>

      <FigureTurn />

      <Sub>The shape of it</Sub>

      <P>
        You send a message. The model reasons, and that reasoning arrives as its own
        collapsible panel in the transcript &mdash; one panel per spell of thinking, so
        a turn with three of them gets three, each independently openable. Then come
        the tool calls: a file read, a search, an edit, a command. Each one is its own
        entry with its own result, and each one lands where it happened.
      </P>

      <P>
        When it is ready the reply streams in under them, and when the turn stops a
        usage line appears beneath the reply with the token counts. Not before: a
        running turn has no final count to report, so showing one would be inventing a
        number.
      </P>

      <Sub>Four modes, and what they change</Sub>

      <P>
        The mode is set per chat and per turn, from a chip in the composer. It decides
        what the model is willing to do, not merely what it prefers to &mdash; the two
        read-only modes refuse the write and command tools outright rather than asking
        for permission and being refused.
      </P>

      <Table n={1} head={["Mode", "What it does", "What it will not do"]}>
        <tr>
          <td>
            <b>Plan</b>
          </td>
          <td>Researches, asks questions, proposes a plan.</td>
          <td>Touches nothing at all. No writes, no commands.</td>
        </tr>
        <tr>
          <td>
            <b>Review</b>
          </td>
          <td>
            Reports findings ranked by severity, each with a file and a line, and
            proposes fixes.
          </td>
          <td>Changes nothing. The findings are the deliverable.</td>
        </tr>
        <tr>
          <td>
            <b>Build</b>
          </td>
          <td>Does the work: edits files, runs commands, iterates on failures.</td>
          <td>Nothing, within the permission mode you have set.</td>
        </tr>
        <tr>
          <td>
            <b>Atelier</b>
          </td>
          <td>
            Everything Build does, and additionally exposes the tools that edit
            Loom&rsquo;s own configuration: personas, prompts, skills, MCP servers,
            providers and settings.
          </td>
          <td>
            Carded deletes are still carded for everything else. See the note below,
            which is the one place in Loom a model can destroy work unprompted.
          </td>
        </tr>
      </Table>

      <Note>
        <b>Atelier is the one exception to how Loom treats removals.</b> Every other
        mode cards a delete. In Atelier the five harness removals run without asking,
        because the mode is a deliberate handover you chose for one chat and a card is
        a question it has already been answered. What that costs is worth stating
        precisely: the harness removals are recoverable from{" "}
        <Code>~/.loom/backups</Code>, but a deleted workspace path and a deleted
        scheduled job are not backed up by anything. Atelier is never accepted as the
        global default, for this reason. Scheduling a job is still carded even in
        Atelier &mdash; not a removal, but a standing commitment to act while nobody
        is watching.
      </Note>
    </Section>
  );
}

/**
 * The first three sections, in order.
 *
 * Exported as one component because the document is one document: splitting the
 * wrapper as well as the contents would mean three elements that have to agree about
 * their order, and there is nothing for them to gain by it.
 */
export function SectionsA() {
  return (
    <>
      <Instrument />
      <Warp />
      <Pick />
    </>
  );
}
