/**
 * The release data the download page and the redirect both read.
 *
 * Fetched from GitHub's public API with a short revalidation window rather
 * than baked in at build time. Two reasons:
 *
 * 1. A build-time snapshot goes stale the moment a release is published, and
 *    the page would confidently offer the previous version until the next
 *    deploy.
 * 2. The repo is public, so this works anonymously. The `GITHUB_TOKEN` in the
 *    Vercel environment is only there to lift the 60-requests-per-hour
 *    anonymous limit, which a shared egress IP can exhaust.
 *
 * Every failure path falls back to the bundled snapshot below. A download page
 * that renders "version unavailable" because GitHub rate-limited a build is
 * worse than one showing a slightly old number, so `release` always returns
 * something usable.
 */
import { REPO } from "./site";

export interface ReleaseAsset {
  name: string;
  /** Bytes, for the file-size line. */
  size: number;
  /** Direct download URL on GitHub's CDN. */
  url: string;
}

/**
 * Where the data came from. Three cases, and they are NOT interchangeable:
 *
 * - `live` — GitHub answered with a release.
 * - `none` — GitHub answered, and there is no release yet. Expected before the
 *   first tag, and the page should say so plainly rather than implying a fault.
 * - `unreachable` — GitHub could not be reached at all. A real fault, worth
 *   telling the visitor about because the version shown may be stale.
 *
 * Collapsing `none` into `unreachable` would tell a visitor something is broken
 * when the project simply has not shipped. On a download page, that is worth
 * the extra state.
 */
export type ReleaseSource = "live" | "none" | "unreachable";

export interface Release {
  /** Without the leading `v`. */
  version: string;
  tag: string;
  /** ISO timestamp. */
  publishedAt: string;
  notesUrl: string;
  /** The `Loom-Setup-vX.Y.Z.exe` asset. */
  installer: ReleaseAsset | null;
  /** The payload zip the updater consumes. */
  payload: ReleaseAsset | null;
  /** SHA-256 of the payload, from the release's `update.json`. */
  payloadSha256: string | null;
  /** Whether the payload carries a minisign signature. */
  signed: boolean;
  source: ReleaseSource;
}

/**
 * Used when GitHub cannot be reached, and as the value the build-time
 * prerender has. Kept deliberately in step with the release workflow's asset
 * names: `Loom-Setup-v<tag>.exe` and `loom-v<tag>-payload.zip`.
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

/** How long a fetched release is cached. Short: releases are rare but not scheduled. */
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

/**
 * Reads the latest release, or the snapshot.
 *
 * `update.json` is fetched separately because the payload's checksum and
 * signature live *inside* it — the manifest is the only place the hash is
 * published, so the download page cannot honestly show one without reading it.
 */
export async function getRelease(): Promise<Release> {
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  // Optional: raises the rate limit from 60/hr. Absent in local development.
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }

  try {
    const response = await fetch(
      `https://api.github.com/repos/${REPO.slug}/releases/latest`,
      { headers, next: { revalidate: REVALIDATE_SECONDS } },
    );

    // A 404 means no release has been tagged yet. That is a normal state for a
    // project before its first tag, not a fault, and the page says so.
    if (response.status === 404) return { ...SNAPSHOT, source: "none" };
    if (!response.ok) return SNAPSHOT;

    const data = (await response.json()) as GithubRelease;
    const tag = data.tag_name;

    const find = (prefix: string, suffix: string) =>
      data.assets?.find(
        (asset) => asset.name.startsWith(prefix) && asset.name.endsWith(suffix),
      ) ?? null;

    const installer = find("Loom-Setup-", ".exe");
    const payload = find("loom-", "-payload.zip");

    const toAsset = (asset: GithubAsset | null): ReleaseAsset | null =>
      asset
        ? { name: asset.name, size: asset.size, url: asset.browser_download_url }
        : null;

    // The manifest carries the hash and signature. A failure here is not fatal:
    // the download still works, the page just cannot show a checksum.
    let payloadSha256: string | null = null;
    let signed = false;
    try {
      const manifest = await fetch(
        `https://github.com/${REPO.slug}/releases/latest/download/update.json`,
        { next: { revalidate: REVALIDATE_SECONDS } },
      );
      if (manifest.ok) {
        const parsed = (await manifest.json()) as {
          sha256?: string;
          signature?: string;
        };
        payloadSha256 = parsed.sha256 ?? null;
        signed = Boolean(parsed.signature);
      }
    } catch {
      /* Leave both null; the page renders without them. */
    }

    return {
      version: tag.replace(/^v/, ""),
      tag,
      publishedAt: data.published_at,
      notesUrl: data.html_url,
      installer: toAsset(installer),
      payload: toAsset(payload),
      payloadSha256,
      signed,
      source: "live",
    };
  } catch {
    // Network failure, DNS, a GitHub outage: the page still renders.
    return SNAPSHOT;
  }
}

/** `11.4 MB` — decimal, which is how Windows reports file sizes. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} kB`;
  return `${bytes} bytes`;
}
