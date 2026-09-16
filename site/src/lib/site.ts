/**
 * Facts about the product and the publisher.
 *
 * Everything here is a single source of truth for metadata, the footer, the
 * download page and the legal pages, so the product name or the repository
 * cannot end up spelled two ways.
 */

export const SITE = {
  name: "Loom",
  url: "https://loom.rip",
  /** Used in <meta name="description">, the OG card, and the hero's subline. */
  description:
    "Loom is a desktop app for AI chat and agents on Windows: streaming replies with visible reasoning, tools with real permission modes, MCP servers, skills, and a coding workspace you can point at a folder.",
  publisher: "Aaen Studios",
  publisherUrl: "https://github.com/aaen-studios",
  /** Shown in the footer and on /terms. */
  license: "MIT",
  licenseUrl: "https://github.com/aaen-studios/loom/blob/main/LICENSE",
} as const;

export const REPO = {
  owner: "aaen-studios",
  name: "loom",
  get slug() {
    return `${this.owner}/${this.name}`;
  },
  get url() {
    return `https://github.com/${this.owner}/${this.name}`;
  },
  get releasesUrl() {
    return `${this.url}/releases`;
  },
  get issuesUrl() {
    return `${this.url}/issues`;
  },
} as const;

/**
 * One definition of the download, used by the header button, the hero, the
 * `/download` page and the `/download/latest` redirect. Changing where releases
 * live means changing this, not five components.
 */
export const DOWNLOAD = {
  /** Stable across a repository rename: the redirect resolves, not this URL. */
  publicPath: "/download/latest",
  requirements: "Windows 10 or 11, 64-bit",
} as const;
