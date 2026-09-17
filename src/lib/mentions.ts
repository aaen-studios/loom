import type { Session } from "../types";

/**
 * `#` for other chats, `@` for files in this chat's workspace.
 *
 * The input is a plain `<textarea>`, so none of this is contenteditable
 * cleverness: it is text in, text out. The two functions that matter are
 * [`findMention`], which decides whether the caret is inside a mention, and
 * [`resolveChatMentions`], which turns the readable title you typed into
 * something the model can act on without making the composer unreadable.
 */

export type MentionTrigger = "#" | "@";

export interface MentionSpan {
  trigger: MentionTrigger;
  /** What has been typed after the trigger, so far. */
  query: string;
  /** Index of the trigger character itself. */
  start: number;
  /** Caret index: the end of the span, exclusive. */
  end: number;
}

/** The character before a trigger has to be one of these, or nothing. */
function startsWord(value: string, index: number): boolean {
  if (index === 0) return true;
  return /\s/.test(value[index - 1]);
}

/**
 * The mention the caret is sitting in, or `null`.
 *
 * Deliberately strict about where a trigger may start. "#42 is broken" and
 * "email me at foo@bar" are prose, and a popup appearing in the middle of
 * either would be worse than not having the feature: the trigger must begin a
 * word, and everything between it and the caret must be free of whitespace and
 * of the other trigger.
 */
export function findMention(value: string, caret: number): MentionSpan | null {
  if (caret < 0 || caret > value.length) return null;

  for (let index = caret - 1; index >= 0; index -= 1) {
    const char = value[index];
    if (char === "#" || char === "@") {
      if (!startsWord(value, index)) return null;
      return {
        trigger: char,
        query: value.slice(index + 1, caret),
        start: index,
        end: caret,
      };
    }
    // Whitespace ends the search, which is also what stops a mention spanning
    // a line break — there is no separate newline case to get wrong.
    if (/\s/.test(char)) return null;
  }
  return null;
}

/**
 * Replaces a mention span with the chosen text and says where the caret lands.
 *
 * The trailing space matters: after picking, the next thing typed is almost
 * always more prose, and without it the cursor sits flush against the mention
 * and the popup reopens on the next keystroke.
 */
export function insertSuggestion(
  value: string,
  span: MentionSpan,
  replacement: string,
): { value: string; caret: number } {
  const before = value.slice(0, span.start);
  const after = value.slice(span.end);
  const inserted = `${span.trigger}${replacement} `;
  return { value: `${before}${inserted}${after}`, caret: before.length + inserted.length };
}

/** Chats whose title matches, newest first, capped for the popup. */
export function matchChats(
  query: string,
  sessions: Session[],
  limit = 8,
): Session[] {
  const needle = query.trim().toLowerCase();
  const matches = sessions.filter((session) => {
    const title = session.title || "New chat";
    return !needle || title.toLowerCase().includes(needle);
  });
  return [...matches]
    .sort((left, right) => right.updatedAt - left.updatedAt)
    .slice(0, limit);
}

/**
 * Files whose path matches. A path that *ends* with the query wins over one
 * that merely contains it, because "@main" almost always means `src/main.rs`
 * rather than `src/maintenance/notes.md`.
 */
export function matchFiles(query: string, files: string[], limit = 8): string[] {
  const needle = query.trim().toLowerCase();
  const matches = files.filter((file) =>
    !needle ? true : file.toLowerCase().includes(needle),
  );
  return [...matches]
    .sort((left, right) => {
      const a = left.toLowerCase();
      const b = right.toLowerCase();
      const aEnds = needle ? a.endsWith(needle) : false;
      const bEnds = needle ? b.endsWith(needle) : false;
      if (aEnds !== bEnds) return aEnds ? -1 : 1;
      // Shallow files first: a top-level file is more likely to be wanted than
      // one six directories down.
      const depth = a.split("/").length - b.split("/").length;
      if (depth !== 0) return depth;
      if (a.length !== b.length) return a.length - b.length;
      return a.localeCompare(b);
    })
    .slice(0, limit);
}

/** The short id form a resolved mention carries. */
export function shortId(id: string): string {
  return id.replace(/-/g, "").slice(0, 8);
}

/** The marker a resolved mention already carries, measured from the title's end. */
const RESOLVED = /^\s*\(id:\s*[0-9a-f]+\)/i;

/**
 * Rewrites `#Some chat` into `#Some chat (id: 1a2b3c4d)` at send time.
 *
 * This is what lets the model call `read_chat` on a chat you mentioned: the
 * title is what you can read, the id is what a tool needs, and the transcript
 * stores both so the reference still resolves after a reload. Resolution
 * happens on send, not on pick, so the composer stays readable while you write.
 *
 * One left-to-right pass, and at each `#` the *longest* title that matches
 * there wins. Resolving a title at a time instead cannot work: once
 * `#Fix the updater` has become `#Fix the updater (id: …)`, the title `Fix`
 * still matches at that same position, and a second pass would resolve it too.
 * Choosing per position makes the question "which chat did you mean" answerable
 * once, from the text as typed.
 */
export function resolveChatMentions(text: string, sessions: Session[]): string {
  const titles = sessions
    .map((session) => ({ title: session.title.trim(), id: session.id }))
    .filter((entry) => entry.title.length > 0)
    .sort((left, right) => right.title.length - left.title.length);

  let out = "";
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    // A mention starts a word, or "#42 is broken" would be one.
    const opens = char === "#" && (index === 0 || /\s/.test(text[index - 1]));
    if (!opens) {
      out += char;
      index += 1;
      continue;
    }

    const rest = text.slice(index + 1);
    const hit = titles.find((entry) => rest.startsWith(entry.title));
    const afterTitle = hit ? index + 1 + hit.title.length : index + 1;
    // Already resolved: leave it exactly as it is, so sending the same text
    // twice cannot grow a second id.
    if (!hit || RESOLVED.test(text.slice(afterTitle))) {
      out += char;
      index += 1;
      continue;
    }

    out += `#${hit.title} (id: ${shortId(hit.id)})`;
    index = afterTitle;
  }
  return out;
}
