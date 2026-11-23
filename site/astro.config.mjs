// @ts-check
// loom.rip: loom's website. Every page is an Astro page in src/pages, drawn in loom's own look from
// the design tokens (design/generated/tokens.css). site/README.md says how it fits together.
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import { fileURLToPath } from 'node:url';
import { ORIGIN } from './src/lib/site.mjs';

export default defineConfig({
	// The site's one address, for canonical links, link previews and the sitemap. Every build names
	// it, local ones too, so no page ever points anywhere else.
	site: ORIGIN,
	// /download/ rather than /download, as the web server sends every page.
	trailingSlash: 'always',
	devToolbar: { enabled: false },
	// The CSS in each page, not a stylesheet it waits for: a round trip less before the first paint
	// on a phone (about 210 ms on slow 4G).
	build: { inlineStylesheets: 'always' },
	// Every page, with when it was last built: the site is built again whenever what's on it changes.
	integrations: [
		sitemap({
			filter: (page) => !page.includes('/404'),
			serialize: (item) => ({ ...item, lastmod: new Date().toISOString() }),
		}),
	],
	vite: {
		// The tokens and the logo live in design/, outside this folder.
		server: { fs: { allow: [fileURLToPath(new URL('..', import.meta.url))] } },
	},
});
