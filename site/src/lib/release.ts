/**
 * The release data behind the download page and the `/download/latest`
 * redirect.
 *
 * Read from GitHub's public API on a short revalidation window rather than baked
 * in at build time. A build-time snapshot would go stale the moment a release is
 * published, and the page would confidently offer the previous version until the
 * next deploy — on a download page that is the one number that has to be right.
 *
 * The repository is public, so this works with no credentials. `GITHUB_TOKEN` is
 * optional and only raises the rate limit from 60 requests an hour, which CI and
 * a shared egress IP can exhaust between them.
 *
 * Every failure path returns a usable object. A page that renders "version
 * unavailable" because GitHub happened to rate-limit a build is worse than one
 * showing a slightly old number, so `getRelease` never throws.
 */
import { REPO } from "./site";

export interface ReleaseAsset {
  name: string;
  /** Bytes, for the file-size line. */
  size: number;
  /** Direct download URL, on GitHub's CDN. */
  url: string;
}

/**
 * Where the numbers came from. The three cases are not interchangeable:
 *
 * - `live` — GitHub answered with a release.
 * - `none` — GitHub answered, and nothing has been tagged yet. Expected before
 *   the first release, and the page should say exactly that rather than hinting
 *   at a fault.
 * - `unreachable` — GitHub could not be reached. A genuine fault, and worth
 *   admitting because the version shown may be stale.
 *
 * Collapsing `none` into `unreachable` would tell visitors something is broken
 * when the truth is that the project has not shipped yet, which is a different
 * sentence and a worse one.
 */
export type ReleaseSource = "live" | "none" | "unreachable";

export interface Release {
  /** Without the leading `v`. */
  version: string;
  tag: string;
  publishedAt: string;
  notesUrl: string;
  /** The `Loom-Setup-*.exe` asset. */
  installer: ReleaseAsset | null;
  /** The payload zip the app's updater consumes. */
  payload: ReleaseAsset | null;
  /** SHA-256 of the payload, read out of the release's `update.json`. */
  payloadSha256: string | null;
  /** Whether that payload carries a minisign signature. */
  signed: boolean;
  source: ReleaseSource;
}

/**
 * The fallback, used before the first release and whenever GitHub is
 * unreachable. Kept in step with the release workflow's asset naming:
 * `Loom-Setup-v<tag>.exe` and `loom-v<tag>-payload.zip`.
 */
const SNAPSHOT: Release = {
  version: "0.1.0",
  tag: "v0.1.0",
  publishedAt: "",
  notesUrl: REPO.releasesUrl,
  installer: null,
  payload: null,
  payloadSha256: null,
  signed: false,
  source: "unreachable",
};

/** Short, because releases are rare but not scheduled. */
const REVALIDATE_SECONDS = 300;

interface GithubAsset {
  name: string;
  size: number;
  browser_download_url: string;
}

interface GithubRelease {
  tag_name: string;
  published_at: string;
  html_url: string;
  assets: GithubAsset[];
}

function headers(): Record<string, string> {
  const map: Record<string, string> = {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  // Optional. Absent in local development, present in CI and production.
  if (process.env.GITHUB_TOKEN) {
    map.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }
  return map;
}

export async function getRelease(): Promise<Release> {
  try {
    const response = await fetch(
      `https://api.github.com/repos/${REPO.slug}/releases/latest`,
      { headers: headers(), next: { revalidate: REVALIDATE_SECONDS } },
    );

    // A 404 means nothing has been tagged. That is a normal state for a project
    // before its first release, not a failure.
    if (response.status === 404) return { ...SNAPSHOT, source: "none" };
    if (!response.ok) return SNAPSHOT;

    const data = (await response.json()) as GithubRelease;
    const tag = data.tag_name;

    const find = (prefix: string, suffix: string): GithubAsset | null =>
      data.assets?.find(
        (asset) => asset.name.startsWith(prefix) && asset.name.endsWith(suffix),
      ) ?? null;

    const toAsset = (asset: GithubAsset | null): ReleaseAsset | null =>
      asset ? { name: asset.name, size: asset.size, url: asset.browser_download_url } : null;

    // The checksum and the signature live *inside* `update.json` — that manifest
    // is the only place they are published, so the page cannot honestly show a
    // hash without reading it. A failure here is not fatal: the download still
    // works, the page just cannot render a checksum.
    let payloadSha256: string | null = null;
    let signed = false;
    try {
      const manifest = await fetch(
        `https://github.com/${REPO.slug}/releases/latest/download/update.json`,
        { next: { revalidate: REVALIDATE_SECONDS } },
      );
      if (manifest.ok) {
        const parsed = (await manifest.json()) as { sha256?: string; signature?: string };
        payloadSha256 = parsed.sha256 ?? null;
        signed = Boolean(parsed.signature);
      }
    } catch {
      /* Leave both unset; the panel renders without them. */
    }

    return {
      version: tag.replace(/^v/, ""),
      tag,
      publishedAt: data.published_at,
      notesUrl: data.html_url,
      installer: toAsset(find("Loom-Setup-", ".exe")),
      payload: toAsset(find("loom-", "-payload.zip")),
      payloadSha256,
      signed,
      source: "live",
    };
  } catch {
    // DNS, a network drop, a GitHub outage: the page still renders.
    return SNAPSHOT;
  }
}

/**
 * `11.4 MB` — decimal, because that is how Windows reports a file size and this
 * number is meant to be compared against Explorer's.
 */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} kB`;
  return `${bytes} bytes`;
}
