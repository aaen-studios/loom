/**
 * The product and the publisher, in one place.
 *
 * Every string below appears in more than one surface — metadata, the footer,
 * the download page, the OG card, the legal pages — and a product name spelled
 * two ways across a site is the cheapest way to look unfinished. So there is one
 * definition and the components read it.
 */

export const SITE = {
  name: "Loom",
  url: "https://loom.rip",
  /** The meta description, the OG card's subhead and the hero's subline. */
  description:
    "Loom is a desktop app for AI chat and agents on Windows: streaming replies with visible reasoning, tools with real permission modes, MCP servers, skills, and a coding workspace you can point at a folder.",
  publisher: "Aaen Studios",
  publisherUrl: "https://github.com/aaen-studios",
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
 * The download, defined once.
 *
 * `publicPath` is ours rather than GitHub's, and that is the point: it is a
 * route handler in this project that resolves the current release and redirects
 * to it. The repository has already moved once, and the app's updater hardcodes
 * a GitHub URL that had to be repointed by hand — a link on a landing page is
 * copied into issues, chat messages and blog posts, so it has to survive the
 * estate moving underneath it.
 */
export const DOWNLOAD = {
  publicPath: "/download/latest",
  requirements: "Windows 10 or 11, 64-bit",
} as const;
