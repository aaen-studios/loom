# site

loom's website, [loom.rip](https://loom.rip), and the user guides.

The guides are in [`docs/`](docs/): running loom, agents and keys, people, phones and Tailscale, and for using loom day to day, settings, the chat, the agent's browser, and permissions. They're read on GitHub, and the site links to them. A guide to building the iPhone app with your own Apple account comes with the app.

## What's on it

| Page | What it is |
|---|---|
| `/` | The wordmark with the agents' threads woven through it, the downloads, a live copy of the desktop app, and what loom does |
| `/download/` | The Windows app, and the server for Linux, Docker, or as one program for Linux, macOS and Windows |
| `/changelog/` | Every release's notes, from GitHub's releases |
| `/404.html` | A page that isn't there |

The Mac and iPhone apps go on the download page once they ship.

## How it's made

It's [Astro](https://astro.build), with every page drawn by hand, built to plain static files. Its look is the app's:

- **The tokens** come from [`design/generated/tokens.css`](../design/generated/tokens.css), made from `design/tokens.toml`, so a change to the design changes the site with it. Light is the main look; the footer's switch picks dark, or the system's.
- **The fonts** are Geist and Geist Mono, from the `@fontsource-variable` packages, served by the site itself.
- **The threads** are [`src/scripts/thread.ts`](src/scripts/thread.ts), a port of the app's `desktop/src/thread.rs`: the same moods (idle, thinking, writing, a tool, waiting, failed), easing and snap, and the servers' yarn drawn from their names. Hovering one plucks it.
- **The weave** on the first screen is [`src/scripts/weave.ts`](src/scripts/weave.ts), a port of the app's `weave` and `weave_over`: the threads come in from the left, pass over and under the letters of "loom", and gather into one as the page scrolls on.
- **The live copy of the app** is [`src/components/AppDemo.astro`](src/components/AppDemo.astro), played by [`src/scripts/demo.ts`](src/scripts/demo.ts): its sizes, colours and words are the app's (`desktop/src/app.rs`), so when the app's look or wording changes, it changes here too. Its approval answers a click.
- **The releases** are read from GitHub's API once per build ([`src/lib/releases.mjs`](src/lib/releases.mjs)). A build for the web server fails without them, which leaves the live site as it was; a local one carries on with none. The download links are the newest release's (`releases/latest/download/…`), so they stay right between builds.
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
