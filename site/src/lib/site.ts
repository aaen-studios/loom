/**
 * The product and the publisher, in one place.
 *
 * Every string below appears in more than one surface — metadata, the colophon, the
 * download block, the OG card, the legal pages — and a product name spelled two ways
 * across a site is the cheapest way to look unfinished. So there is one definition and
 * the components read it.
 */

export const SITE = {
  name: "Loom",
  url: "https://loom.rip",
  /**
   * The meta description, the OG card's subhead, and the sentence a search result is
   * judged on.
   *
   * Rewritten twice now, and both times for the same reason. The first version led on
   * reasoning and MCP servers, which described the product two releases earlier. The
   * second led on a claim — "an agent you can watch work" — which is a good opening
   * line for a document and a poor meta description, because a search result has to say
   * what the thing *is* before it says what it is like.
   *
   * So this leads with the noun: a desktop workspace, for Windows, holding a terminal
   * and an editor beside the chat. The claim gets its own line in the interlude, where
   * there is room for it.
   */
  description:
    "Loom is a desktop workspace for AI chat and agents on Windows: the conversation, a real terminal, an editor with git and the files they are working on, in one window. Free, MIT licensed, and it runs against any provider — including a model on your own machine.",
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
 * `publicPath` is ours rather than GitHub's, and that is the point: it is a route handler
 * in this project that resolves the current release and redirects to it. The repository
 * has already moved once, and the app's updater hardcodes a GitHub URL that had to be
 * repointed by hand — a link on a page like this is copied into issues, chat messages and
 * blog posts, so it has to survive the estate moving underneath it.
 */
export const DOWNLOAD = {
  publicPath: "/download/latest",
  requirements: "Windows 10 or 11, 64-bit",
} as const;
