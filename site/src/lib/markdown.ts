/**
 * A minimal markdown block parser, for streamed assistant text.
 *
 * This exists because a *streaming* reply is not the same problem as a finished
 * one. Mid-stream the text routinely contains an unterminated code fence, and
 * the app handles that state explicitly: a fence with no end is marked
 * `data-incomplete` and its copy button drops to 35% opacity, because half a
 * block is not worth copying. A parser that only understood complete input would
 * either throw or silently render the fence markers as prose.
 *
 * Deliberately not a markdown library. It handles the three things the app's
 * real renderer produces for the scripted reply — paragraphs, fenced code, and
 * inline spans — and nothing else. Anything more would be a dependency and a
 * lie about what this does.
 *
 * Lives here rather than in the component so it can be tested without a DOM:
 * odd fences, an empty language, a trailing newline and a fence that has opened
 * but sent nothing are all states that occur while text arrives.
 */

export interface ProseBlock {
  kind: "prose";
  text: string;
}

export interface CodeBlock {
  kind: "code";
  /** The fence's info string, e.g. `ts`. Empty when the fence has none. */
  language: string;
  /** The code itself, without a trailing newline. */
  code: string;
  /**
   * True while the closing fence has not arrived. Drives the dimmed copy button,
   * and distinguishes "still streaming" from "this is the whole block".
   */
  incomplete: boolean;
}

export type Block = ProseBlock | CodeBlock;

/**
 * Splits streamed markdown into prose and fenced code blocks.
 *
 * An odd number of fences is normal mid-stream, not an error: the last block is
 * simply still arriving.
 */
export function parseBlocks(text: string): Block[] {
  const parts = text.split("```");
  const blocks: Block[] = [];

  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index];

    // Odd indices are inside a fence, by construction of the split.
    if (index % 2 === 1) {
      // The first line is the language, and it may not have arrived yet — a
      // stream can pause immediately after the opening fence.
      const newline = part.indexOf("\n");
      const language = newline === -1 ? part.trim() : part.slice(0, newline).trim();
      const body = newline === -1 ? "" : part.slice(newline + 1);

      blocks.push({
        kind: "code",
        language,
        // One trailing newline would render as an extra numbered blank line.
        code: body.replace(/\n$/, ""),
        // Only the final part can be unclosed, and only if it is a fence — which
        // oddness already tells us.
        incomplete: index === parts.length - 1,
      });
      continue;
    }

    // Prose. Split on blank lines, because a block that contains a bare
    // newline renders as one run-on paragraph — HTML collapses it to a space.
    // A single `\n` is a soft wrap and stays inside its paragraph.
    const trimmed = part.replace(/^\n+/, "").replace(/\n+$/, "");
    for (const paragraph of trimmed.split(/\n{2,}/)) {
      const text = paragraph.trim();
      if (text) blocks.push({ kind: "prose", text });
    }
  }

  return blocks;
}

/** Splits a line of prose on backticks, so `code spans` can be styled as they arrive. */
export function parseInline(text: string): { code: boolean; text: string }[] {
  return text.split("`").map((part, index) => ({
    code: index % 2 === 1,
    text: part,
  }));
}

/**
 * Whether a code block is long enough to be worth the surrounding chrome.
 *
 * A one-line fence still gets its header in the app — the actions need a home —
 * so this is exposed for callers that want to know, not used to hide anything.
 */
export function isSubstantial(block: CodeBlock): boolean {
  return block.code.trim().length > 0;
}
