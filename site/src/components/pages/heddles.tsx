import { Pick } from "@/components/weave/pass";
import { passById } from "@/lib/weave/passes";

/**
 * Pass six: the heddles.
 *
 * Heddles are the wires that lift selected warp threads to open a shed — the part of
 * a loom that decides what the pattern will be. Which is what a question is: the
 * thing that determines which of the threads you were shown actually matter to you.
 *
 * So this is the FAQ, and the questions are the ones the repository already answers
 * rather than the ones a marketing page would prefer to be asked. Two of them are
 * unflattering: the installer is not code-signed, and computer use is Windows-only
 * and needs a vision model. Answering those plainly is worth more than a fourth
 * paragraph about streaming.
 *
 * Native `<details>`, so this needs no JavaScript at all, works with the keyboard,
 * and works with in-page search — browsers open a closed `<details>` when the match
 * is inside it, which a scripted accordion does not do.
 */
const QUESTIONS = [
  {
    q: "Is it really free?",
    a: "Yes, and there is nothing to unlock. Loom has no paid tier, no account and no licence key. You pay your provider directly for the tokens you use, and that bill never passes through this project. Running a local model through Ollama or LM Studio costs nothing at all.",
  },
  {
    q: "Why does Windows say the publisher is unknown?",
    a: "Because the installer is not code-signed yet. A certificate is a recurring cost, and until one is in place SmartScreen shows the generic warning for any installer that lacks it. That is separate from the updater: release payloads are signed with minisign and the app verifies that signature before applying anything, so an update cannot be tampered with in transit — but that is not the same as proving who built the installer. The download page publishes the SHA-256 of the file you are about to run so you can check it yourself.",
  },
  {
    q: "Does it work on macOS or Linux?",
    a: "Not yet. Loom is built on Tauri, which is cross-platform, but the parts that are Windows-specific are not incidental: computer use is built on UI Automation and Win32 input, the credential store is Windows Credential Manager, and launch-at-login is a registry entry. This page will say so plainly when another platform is real rather than planned.",
  },
  {
    q: "What does Loom do with my code?",
    a: "It reads the folder you point it at and sends what it reads to the provider of the model you chose — the same as pasting a file into any other client. Nothing goes anywhere else, and indexing uses your own provider's embedding endpoint. If you need code to stay on the machine, run a local model and nothing leaves at all.",
  },
  {
    q: "What is the difference between Atelier and the other modes?",
    a: "Plan, Review and Build differ in what they will do to your workspace. Atelier is a different axis: it additionally exposes tools that edit Loom's own configuration — personas, prompts, skills, MCP servers, providers and settings. Every write takes a snapshot first, the destructive operations still ask, and it is deliberately per-chat with no way to make it the global default.",
  },
  {
    q: "Can it run commands while I am not watching?",
    a: "Yes, deliberately, and it is bounded. A command that outlives the turn is adopted rather than killed, and at most eight can be tracked at once so processes cannot pile up invisibly. All of them are listed in the Runs panel with a Stop button, output is capped at 5 MB per command, and a command still running after a restart is reported as orphaned rather than assumed dead.",
  },
  {
    q: "What happens to my chats if I uninstall?",
    a: "They stay. Uninstalling removes the application from %LOCALAPPDATA%\\Loom and the shortcuts it created; ~/.loom is left alone, because deleting someone's conversations as a side effect of removing a binary would be hostile. Delete that folder yourself when you want it gone.",
  },
] as const;

export function Heddles() {
  const pass = passById("heddles");

  return (
    <Pick
      pass={pass}
      title="The things worth asking before you install."
      lead="Seven questions, and the two most useful ones are the ones a landing page would usually rather not be asked."
    >
      <div className="panel rounded-sheet divide-y divide-[var(--glass-border)] overflow-hidden">
        {QUESTIONS.map((item, index) => (
          <details key={item.q} className="group">
            <summary className="hover-surface flex cursor-pointer items-center gap-4 px-4 py-4 [&::-webkit-details-marker]:hidden">
              {/* The heddle's number, outside the question so it never travels with
                  a selection made inside the answer. */}
              <span
                aria-hidden="true"
                className="text-faint font-mono text-[10.5px] tabular-nums opacity-70"
              >
                {String(index + 1).padStart(2, "0")}
              </span>
              <span className="flex-1 text-[14px] font-medium">{item.q}</span>
              <span className="text-faint shrink-0 transition group-open:rotate-180">
                <ChevronIcon />
              </span>
            </summary>
            <div className="text-soft pr-4 pb-4 pl-[3.25rem] text-[13.5px] leading-[1.65]">
              {item.a}
            </div>
          </details>
        ))}
      </div>
    </Pick>
  );
}

function ChevronIcon() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}
