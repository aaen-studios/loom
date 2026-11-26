// The ACP Registry's agent programs, read once per build for the home page's Brains panel, from where
// loom itself reads them (server/installer.calx): each one's name, version, description and icon.

const REGISTRY = 'https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json';

/** @typedef {{ id: string, name: string, version: string, description: string, icon: string }} Brain */

/** @type {Promise<any[]> | undefined} */
let cached;

/**
 * The registry's entries with these ids, in this order, each with its icon as a data URL. An id the
 * registry no longer has is left out.
 * @param {string[]} ids
 * @returns {Promise<Brain[]>}
 */
export async function brains(ids) {
	cached ??= load();
	const all = await cached;
	const found = ids.flatMap((id) => all.filter((a) => a.id === id));
	return Promise.all(
		found.map(async (a) => ({
			id: a.id,
			name: a.name,
			version: a.version,
			description: a.description ?? '',
			icon: a.icon ? await icon(a.icon) : '',
		})),
	);
}

/**
 * How many agent programs the registry has, for the Brains panel's "to install".
 * @returns {Promise<number>}
 */
export async function count() {
	cached ??= load();
	return (await cached).length;
}

async function load() {
	try {
		const res = await fetch(REGISTRY, { headers: { 'User-Agent': 'loom-site' } });
		if (!res.ok) throw new Error(`the registry answered ${res.status}`);
		return (await res.json()).agents ?? [];
	} catch (e) {
		// A build for the web server must have them, so it fails and the live site stays as it is.
		// A local one carries on without, so the site can be worked on offline.
		if (import.meta.env.PROD) throw new Error(`couldn't read the ACP Registry: ${e}`);
		console.warn(`loom-site: couldn't read the ACP Registry (${e}), so the Brains panel is empty`);
		return [];
	}
}

/**
 * An icon as a data URL. The page uses it only as a mask, drawn in its ink as the app draws them, so
 * nothing in the file ever runs.
 * @param {string} url
 */
async function icon(url) {
	const res = await fetch(url, { headers: { 'User-Agent': 'loom-site' } });
	if (!res.ok) return '';
	return `data:image/svg+xml;base64,${Buffer.from(await res.text()).toString('base64')}`;
}
