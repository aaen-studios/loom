/**
 * The one helper the site needs.
 *
 * `clsx` is not a dependency here — eleven lines of joining do not justify a
 * package — but the semantics are copied from the app's `src/lib/cn.ts`, because
 * that is the helper the app's own components rely on and a `cn` that behaved
 * differently would be a trap for anyone moving between the two codebases. Falsy
 * entries disappear rather than rendering as the string "false".
 */
export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
