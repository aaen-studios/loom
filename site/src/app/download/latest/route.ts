import { NextResponse } from "next/server";
import { getRelease } from "@/lib/release";
import { REPO } from "@/lib/site";

/**
 * `/download/latest` — a stable pointer at the current installer.
 *
 * A redirect rather than a proxy, and that is the point: the bytes come from
 * GitHub's CDN, so a large download does not consume this deployment's bandwidth,
 * and a checksum can be verified against what GitHub actually serves.
 *
 * The indirection earns its keep for one specific reason: the URL belongs to this
 * project. If the repository is renamed, moved to another organisation, or the
 * release asset naming changes, only this handler has to change and every link
 * anyone already copied keeps working. That is not hypothetical — this repository
 * has already moved once, and the app's own updater hardcodes a GitHub URL that
 * had to be repointed by hand.
 */

/**
 * Force the handler to run per request, while leaving the lookup behind it
 * cached.
 *
 * Without this, Next evaluates a `GET` handler with no dynamic API calls at build
 * time and serves that result — so a build made *before* the first release would
 * bake in a 404 and keep handing it to visitors until the revalidation window
 * came round. The decision is cheap and should be made fresh; the `fetch` calls
 * inside `getRelease` carry their own `revalidate`, so GitHub is still asked once
 * every five minutes rather than once per visitor.
 */
export const dynamic = "force-dynamic";

export async function GET() {
  const release = await getRelease();

  // Before the first tag there is nothing to send anyone to. A 404 carrying a
  // sentence beats a redirect to a URL that would itself 404, because at least
  // this one explains itself.
  if (!release.installer) {
    return NextResponse.json(
      {
        error: "No Loom release has been published yet.",
        releases: REPO.releasesUrl,
      },
      { status: 404 },
    );
  }

  // `no-store` on the redirect itself: the target is only valid for the release
  // it points at, and a cached redirect would pin visitors to an old version
  // until the cache expired — which is precisely the failure the indirection
  // exists to avoid.
  return NextResponse.redirect(release.installer.url, {
    status: 302,
    headers: { "Cache-Control": "no-store, max-age=0" },
  });
}
