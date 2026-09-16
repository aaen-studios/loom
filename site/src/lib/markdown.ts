/**
 * Markdown blocks for *streamed* text.
 *
 * A streamed reply is not a finished document, and the difference is the whole
 * reason this file exists. Mid-stream the text routinely contains an
 * unterminated fence, and the app treats that as a real state rather than an
 * error: a fence with no closing marker is marked incomplete, and its copy
 * action dims, because half a code block is not worth copying. A parser that
 * only understood complete input would either throw or render the backticks as
 * prose.
 *
 * This is deliberately not a markdown library. It handles the three things the
 * app's renderer produces for the reply on the landing page — paragraphs, fenced
 * code and inline spans — and nothing else. Anything more would be a dependency
 * and a claim about the product that is not true.
 *
 * It lives here rather than inside the component so the awkward inputs (an empty
 * language, a fence that has opened but sent nothing, a trailing newline) can be
 * tested without mounting anything.
 */

export interface ProseBlock {
  kind: "prose";
  text: string;
}

export interface CodeBlock {
  kind: "code";
  /** The fence's info string, e.g. `ts`. Empty when the fence has none. */
  language: string;
  /** The code, without a trailing newline. */
  code: string;
  /**
   * True while the closing fence has not arrived. Drives the dimmed copy action,
   * and is the only thing distinguishing "still streaming" from "this is all of
   * it".
   */
  incomplete: boolean;
}

export type Block = ProseBlock | CodeBlock;

/**
 * Splits streamed markdown into prose and fenced code.
 *
 * An odd number of fences is normal mid-stream rather than malformed, so the
 * final block is simply still arriving.
 */
export function parseBlocks(text: string): Block[] {
  const parts = text.split("```");
  const blocks: Block[] = [];

  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index];

    // Odd indices are inside a fence — that is what the split guarantees.
    if (index % 2 === 1) {
      // The first line is the language, and it may not have arrived yet: a
      // stream can pause immediately after the opening fence.
      const newline = part.indexOf("\n");
      const language = newline === -1 ? part.trim() : part.slice(0, newline).trim();
      const body = newline === -1 ? "" : part.slice(newline + 1);

      blocks.push({
        kind: "code",
        language,
        // One trailing newline would render as an extra numbered blank line.
        code: body.replace(/\n$/, ""),
        // Only the last part can be unclosed, and oddness already told us it is
        // a fence.
        incomplete: index === parts.length - 1,
      });
      continue;
    }

    // Prose. Split on blank lines, because HTML collapses a lone newline to a
    // space — so a paragraph containing one would render as a single run-on.
    // A single `\n` is a soft wrap and stays where it is.
    const trimmed = part.replace(/^\n+/, "").replace(/\n+$/, "");
    for (const paragraph of trimmed.split(/\n{2,}/)) {
      const content = paragraph.trim();
      if (content) blocks.push({ kind: "prose", text: content });
    }
  }

  return blocks;
}

/**
 * Splits a line on backticks so `code spans` can be styled as they arrive.
 *
 * An unterminated span counts as code, which is what stops a half-typed span
 * from snapping into place when the closing backtick shows up.
 */
export function parseInline(text: string): { code: boolean; text: string }[] {
  return text.split("`").map((part, index) => ({ code: index % 2 === 1, text: part }));
}
