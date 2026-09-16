/**
 * The one helper the hero needs, matching the app's `src/lib/cn.ts`.
 *
 * `clsx` is not a dependency of the site — adding a package for eleven lines of
 * joining would be silly — but the semantics have to match, because components
 * here are transliterated from the app and rely on falsy values being dropped.
 */
export function cn(
  ...parts: Array<string | false | null | undefined>
): string {
  return parts.filter(Boolean).join(" ");
}
