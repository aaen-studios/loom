/**
 * The one helper the product window needs.
 *
 * `clsx` is not a dependency here — eleven lines of joining do not justify a
 * package — but the semantics are copied from the app's `src/lib/cn.ts` because
 * the components in `components/product/` are transliterated from the app and
 * rely on falsy entries disappearing rather than rendering as "false".
 */
export function cn(
  ...parts: Array<string | false | null | undefined>
): string {
  return parts.filter(Boolean).join(" ");
}
