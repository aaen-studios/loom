/**
 * The product and the publisher, in one place.
 *
 * Every string below appears in more than one surface — metadata, the footer, the release panel, the OG
 * card, the legal pages — and a product name spelled two ways across a site is the cheapest way to look
 * unfinished. So there is one definition and the components read it.
 */

export const SITE = {
  name: "Loom",
  url: "https://loom.rip",
  /**
   * The meta description and the OG card's subhead.
   *
   * Leads with the noun rather than with a claim. A search result has to say what the thing *is* before
   * it says what it is like, and the claim this product actually makes — one window holding every tool
   * — needs the six movements behind it rather than one sentence wedged under a title tag.
   */
  description:
    "Loom is a desktop workspace for AI chat and agents on Windows: the conversation, a real terminal, an editor with git and the files they are working on, in one window — with the model's reasoning kept in the transcript where it happened. Free, MIT licensed, and it runs against any provider, including a model on your own machine.",
  publisher: "Aaen Studios",
  /**
   * The studio's own site, and the one link on this page that goes somewhere other than the repository.
   *
   * It points at the studio rather than at its GitHub organisation, because a legal notice, a colophon
   * and a page's `author` metadata are all asking the same question — *who made this* — and an
   * organisation page answers it with a list of code. Also the one place the site links to something it
   * does not otherwise talk about, so it is a deliberate exception rather than an oversight.
   */
  publisherUrl: "https://aaenz.no",
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
 * `publicPath` is ours rather than GitHub's, and that is the point: it is a route handler in this
 * project that resolves the current release and redirects to it. The repository has already moved once,
 * and the app's updater hardcodes a GitHub URL that had to be repointed by hand — a link on a page like
 * this is copied into issues, chat messages and blog posts, so it has to survive the estate moving
 * underneath it.
 */
export const DOWNLOAD = {
  publicPath: "/download/latest",
  requirements: "Windows 10 or 11, 64-bit",
} as const;
