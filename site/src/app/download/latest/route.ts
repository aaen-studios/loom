import { NextResponse } from "next/server";
import { getRelease } from "@/lib/release";

/**
 * `loom.rip/download/latest` — a stable pointer at the current installer.
 *
 * A redirect rather than a proxy, deliberately: the bytes come from GitHub's
 * CDN, so a large download does not consume this deployment's bandwidth, and a
 * checksum can be verified against what GitHub actually serves.
 *
 * The indirection buys one specific thing: the URL is ours. If the repository
 * is renamed, moved to a different organisation, or the release asset naming
 * changes, only this handler has to change — every link anyone has already
 * copied, and every bookmark, keeps working. That is not hypothetical: this
 * project's repository was moved once already, and the app's own updater
 * hardcodes a GitHub URL that had to be repointed by hand.
 */
export const revalidate = 300;

export async function GET() {
  const release = await getRelease();

  // Before the first tag there is nothing to send anyone to. A 404 with a
  // sentence is better than a redirect to a URL that would itself 404, because
  // at least this one says why.
  if (!release.installer) {
    return NextResponse.json(
      {
        error: "No Loom release has been published yet.",
        releases: "https://github.com/aaen-studios/loom/releases",
      },
      { status: 404 },
    );
  }

  // `no-store` on the redirect itself: the target URL is only valid for the
  // release it points at, and a cached redirect would pin visitors to an old
  // version until the cache expired. The lookup behind it is still revalidated.
  return NextResponse.redirect(release.installer.url, {
    status: 302,
    headers: { "Cache-Control": "no-store, max-age=0" },
  });
}
