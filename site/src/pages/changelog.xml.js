// loom's releases as an Atom feed, for feed readers and search engines: the changelog page's notes,
// newest first.
import { releases } from '../lib/releases.mjs';
import { ORIGIN } from '../lib/site.mjs';

/** @param {string} s */
const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

export async function GET() {
	const all = await releases();
	const updated = all[0]?.date ?? new Date(0).toISOString();
	const entries = all
		.map(
			(r) => `	<entry>
		<id>${ORIGIN}/changelog/#${esc(r.tag)}</id>
		<title>${esc(r.title)}</title>
		<link href="${ORIGIN}/changelog/#${esc(r.tag)}"/>
		<updated>${r.date}</updated>
		<content type="html">${esc(r.html)}</content>
	</entry>`,
		)
		.join('\n');
	const xml = `<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
	<id>${ORIGIN}/changelog/</id>
	<title>loom releases</title>
	<subtitle>What changed in each release of loom, the self-hostable agent app.</subtitle>
	<link href="${ORIGIN}/changelog.xml" rel="self"/>
	<link href="${ORIGIN}/changelog/"/>
	<updated>${updated}</updated>
	<author><name>Aaen Studios</name></author>
	<icon>${ORIGIN}/favicon-32.png</icon>
${entries}
</feed>
`;
	return new Response(xml, { headers: { 'Content-Type': 'application/atom+xml; charset=utf-8' } });
}
