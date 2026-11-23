# site

loom's website, [loom.rip](https://loom.rip), and the user guides.

The guides are in [`docs/`](docs/): running loom, agents and keys, people, phones and Tailscale, and for using loom day to day, settings, the chat, the agent's browser, and permissions. They're read on GitHub, and the site links to them. A guide to building the iPhone app with your own Apple account comes with the app.

## What's on it

| Page | What it is |
|---|---|
| `/` | The wordmark with the agents' threads behind it, the downloads, a live copy of the desktop app, and what loom does |
| `/download/` | The Windows app, and the server for Linux, Docker, or as one program for Linux, macOS and Windows |
| `/changelog/` | Every release's notes, from GitHub's releases |
| `/changelog.xml` | The same as an Atom feed |
| `/llms.txt` | What loom is, for AI assistants and search |
| `/404.html` | A page that isn't there |

The Mac and iPhone apps go on the download page once they ship.

## How it's made

It's [Astro](https://astro.build), with every page drawn by hand, built to plain static files. Its look is the app's:

- **The tokens** come from [`design/generated/tokens.css`](../design/generated/tokens.css), made from `design/tokens.toml`, so a change to the design changes the site with it. Light is the main look; the footer's switch picks dark, or the system's.
- **The fonts** are Geist and Geist Mono, from the `@fontsource-variable` packages, served by the site itself.
- **The threads** are [`src/scripts/thread.ts`](src/scripts/thread.ts), a port of the app's `desktop/src/thread.rs`: the same moods (idle, thinking, writing, a tool, waiting, failed), easing and snap, the shapes agents take in the sidebar (from [`design/generated/shapes.json`](../design/generated/shapes.json)), and the servers' yarn drawn from their names. Hovering one plucks it.
- **The threads behind the wordmark** are [`src/scripts/hero.ts`](src/scripts/hero.ts), drawn as the app's `weave` draws them: they come in from the left as one cord, which comes apart behind the word into its seven colours, and then they drift slowly. They show through the letters' holes.
- **The live copy of the app** is [`src/components/AppDemo.astro`](src/components/AppDemo.astro), played by [`src/scripts/demo.ts`](src/scripts/demo.ts): its sizes, colours and words are the app's (`desktop/src/app.rs`), so when the app's look or wording changes, it changes here too. As in the app, a reply's steps fold under one quiet line that opens, its time shows only where the chat paused, the header has the brain chip (Claude's icon from the ACP Registry), and the sidebar collapses to the rail, as it is anyway in a narrow window. Its approval answers a click.
- **The Brains and Rules panels** are copies of those settings pages (`desktop/src/app.rs`, `desktop/src/rules.rs`), with the rules and What if's answers in the server's own words (`server/rules.calx`, `server/instance.calx`). The brains are the ACP Registry's own, with their names, versions, descriptions and icons, read once per build ([`src/lib/registry.mjs`](src/lib/registry.mjs)).
- **The releases** are read from GitHub's API once per build ([`src/lib/releases.mjs`](src/lib/releases.mjs)). A build for the web server fails without them or the registry, which leaves the live site as it was; a local one carries on without. The download links are the newest release's (`releases/latest/download/…`), so they stay right between builds.
- **Search engines** get each page's title and description, link previews, structured data (Aaen Studios, the app with its newest version and downloads, each page, where it sits, and the home page's questions), the sitemap with when each page was built, and the releases' feed. `public/451ff9cdc5ebc39516500a5b114455f3.txt` is the site's [IndexNow](https://www.indexnow.org) key: sending its pages' addresses with it to `https://api.indexnow.org/indexnow` tells Bing and the other engines that take it that they changed.
- **No scrollbar.** When nobody has scrolled a few seconds after the page opens, an arrow at the bottom says there's more ([`src/components/Nudge.astro`](src/components/Nudge.astro)).
- **Motion** stops off screen and in a hidden tab, and holds still with reduced motion.

## Running it

It needs [Bun](https://bun.sh), as the web server builds it with Bun. From this folder:

```sh
bun install
bun run dev      # http://localhost:4321/, reloading as you edit
bun run build    # the whole site, into dist/
bun run preview  # serves dist/
```

`public/og.png`, the picture link previews show, is made from [`og.html`](og.html); its first lines say how.

## Publishing it

loom.rip is served from Elliot's VPS, the way calxlang.com is, by the setup in `ellipog/vps-hosting`:

- **Deploys.** A push to main that changes `site/` or `design/generated/` builds the site (`bun run build`), and the new `dist/` goes live in one atomic switch. A build that fails leaves the live site as it was. Each deploy shows on GitHub as the commit status `vps/loom`.
- **New releases.** Publishing a release doesn't push to main, so the VPS looks for a new release every hour and builds the site again when there is one, for the download page's version and the changelog.
- **`loom.rip/install.sh`** is sent on to the newest release's `install.sh`, so `curl -fsSL https://loom.rip/install.sh | sudo sh` always installs the newest loom.
- **A GitHub token** is optional: `GITHUB_TOKEN` in the site's `env` file on the VPS lifts GitHub's limit of 60 reads an hour.
