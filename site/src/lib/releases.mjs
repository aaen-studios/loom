// loom's releases, read from GitHub once per build: the download page's version and sizes, and the
// changelog. The web server builds the site again when a new release is out (site/README.md).
import { marked } from 'marked';
import { GITHUB } from './site.mjs';

const API = 'https://api.github.com/repos/aaen-studios/loom/releases?per_page=100';

/**
 * @typedef {{ name: string, size: number, url: string }} Asset
 * @typedef {{ tag: string, version: string, title: string, date: string, url: string, html: string, assets: Asset[] }} Release
 */

/** @type {Promise<Release[]> | undefined} */
let cached;

/** Every published release, newest first. */
export function releases() {
	cached ??= load();
	return cached;
}

/** The newest release that isn't a pre-release. */
export async function latest() {
	const all = await releases();
	return all.find((r) => !r.tag.includes('-')) ?? all[0];
}

async function load() {
	/** @type {Record<string, string>} */
	const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'loom-site' };
	// A token is optional: without one, GitHub allows 60 reads an hour from an address.
	const token = import.meta.env.GITHUB_TOKEN ?? process.env.GITHUB_TOKEN;
	if (token) headers.Authorization = `Bearer ${token}`;
	let list;
	try {
		const res = await fetch(API, { headers });
		if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
		list = await res.json();
	} catch (e) {
		// A build for the web server must have them, so it fails and the live site stays as it is.
		// A local one carries on without, so the site can be worked on offline.
		if (import.meta.env.PROD) throw new Error(`couldn't read loom's releases: ${e}`);
		console.warn(`loom-site: couldn't read loom's releases (${e}), so the pages have none`);
		return [];
	}
	return list
		.filter((/** @type {any} */ r) => !r.draft)
		.map((/** @type {any} */ r) => ({
			tag: r.tag_name,
			version: r.tag_name.replace(/^v/, ''),
			title: r.name || r.tag_name,
			date: r.published_at,
			url: r.html_url ?? `${GITHUB}/releases/tag/${r.tag_name}`,
			html: notes(r.body ?? '', r.name || r.tag_name),
			assets: (r.assets ?? []).map((/** @type {any} */ a) => ({ name: a.name, size: a.size, url: a.browser_download_url })),
		}));
}

/**
 * A release's notes as HTML. The notes open with the release's title, which the page already
 * shows, so that line goes, and so does the link to its commits that GitHub adds.
 * @param {string} body
 * @param {string} title
 */
function notes(body, title) {
	const lines = body
		.replace(/\r\n/g, '\n')
		.trim()
		.split('\n')
		.filter((l) => !/^\*\*Full Changelog\*\*/.test(l.trim()));
	if (lines[0]?.trim() === title.trim()) lines.shift();
	return /** @type {string} */ (marked.parse(lines.join('\n').trim(), { async: false }));
}

/** @param {number} bytes */
export function megabytes(bytes) {
	const mb = bytes / 1e6;
	return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
}

/** @param {string} iso */
export function day(iso) {
	return new Date(iso).toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' });
}

/**
 * The size of the newest release's file of this name, if it has one.
 * @param {Release | undefined} release
 * @param {string} name
 */
export function sizeOf(release, name) {
	const a = release?.assets.find((x) => x.name === name);
	return a ? megabytes(a.size) : undefined;
}

/**
 * Whether the newest release has a file of this name, so a link to it works: the Mac app's
 * download shows once a release carries it. A build without releases, offline, shows it.
 * @param {Release | undefined} release
 * @param {string} name
 */
export function carries(release, name) {
	return !release || release.assets.some((x) => x.name === name);
}
