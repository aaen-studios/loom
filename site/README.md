# site

loom's website, [loom.rip](https://loom.rip), and the user guides.

The guides are in [`docs/`](docs/): running loom, agents and keys, people, phones and Tailscale, and for using loom day to day, settings, the chat, the agent's browser, and permissions. They're read on GitHub, and the site links to them. A guide to building the iPhone app with your own Apple account comes with the app.

## What's on it

| Page | What it is |
|---|---|
| `/` | The wordmark with the agents' threads behind it, the downloads, a live copy of the desktop app, and what loom does |
| `/download/` | The Windows and Mac apps, and the server for Linux, Docker, or as one program for Linux, macOS and Windows |
| `/changelog/` | Every release's notes, from GitHub's releases |
| `/changelog.xml` | The same as an Atom feed |
| `/llms.txt` | What loom is, for AI assistants and search |
| `/404.html` | A page that isn't there |

The iPhone app goes on the download page once it ships. The Mac app shows once the newest release has `loom-mac.dmg`, and comes first for a visitor on a Mac. The home page shows a visitor on Windows only the Windows app and one on a Mac only the Mac app, each beside the server; anyone else gets the server first with both apps beside it.

## How it's made

It's [Astro](https://astro.build), with every page drawn by hand, built to plain static files. Its look is the app's:

- **The tokens** come from [`design/generated/tokens.css`](../design/generated/tokens.css), made from `design/tokens.toml`, so a change to the design changes the site with it. Light is the main look; the footer's switch picks dark, or the system's.
- **The fonts** are Geist and Geist Mono, from the `@fontsource-variable` packages, served by the site itself.
- **The threads** are [`src/scripts/thread.ts`](src/scripts/thread.ts), a port of the app's `desktop/src/thread.rs`: the same moods (idle, thinking, writing, a tool, waiting, failed), easing and snap, the shapes agents take in the sidebar, drawn thicker on a pinned agent's tile (from [`design/generated/shapes.json`](../design/generated/shapes.json)), and the servers' yarn drawn from their names. Hovering one plucks it.
- **The threads behind the wordmark** are [`src/scripts/hero.ts`](src/scripts/hero.ts), drawn as the app's `weave` draws them: they come in from the left as one cord, which comes apart behind the word into its seven colours, and then they drift slowly. They show through the letters' holes.
- **The live copy of the app** is [`src/components/AppDemo.astro`](src/components/AppDemo.astro), played by [`src/scripts/demo.ts`](src/scripts/demo.ts): its sizes, colours and words are the app's (`desktop/src/app.rs`), so when the app's look or wording changes, it changes here too. As in the app, a reply's steps fold under one quiet line that opens, a subagent's call says how many steps it took and opens to show them, its time shows only where the chat paused, the header has the brain chip (Claude's icon from the ACP Registry), the pinned agents sit at the top of the sidebar as tiles, three across, each with its thread drawn large in its shape and its name under it, every other agent is one line, what it's on sitting quietly after its name while it works, and the sidebar collapses to the rail, as it is anyway in a narrow window, with the control that opens it always at its top, the pinned agents first with a short line under them, and on a Mac the title bar crossing the window over it, the control after the traffic lights. Your messages take a breath of the agent's colour, as the app's default draws them. It replies to the earlier answer, from the Reply by its corner on hover, which puts "Replying to Juniper" and its first line over the composer's words, and the sent message has that quote over its bubble, along a thin line in Juniper's colour, which goes back to the answer and lights it; a message's Reply, or words selected in one, with Copy, Quote and Ask about this in the bar over them, does the same by hand. The header's pin, with how many there are, opens the chat's pins in a popover under it, over the chat: each drawn as the chat draws its message, under Juniper's thread and name or "You" and when, a line in their colour down its left, long words cut with Show more, and Jump, Copy and Unpin under the pointer, Unpin with Undo at the popover's foot; Esc, a click outside or the pin again closes it, and Ctrl+Shift+P opens it. The message is typed with a link pasted and a file attached among its words, each a piece in them with its card or pill over them, and the sent message shows them the same way. A Home folder holds Otto, Juniper goes under Waiting on you while it asks, and Biscuit's thread with Juniper is the "From Biscuit" row under it, which opens the thread in the chat's place under a strip with the way back. Its approval is in the frame every card in the chat has (its icon in the agent's colour, its title with the agent's thread tugging beside it, "Runs a command" in an amber pill, Allow with Always allow under the arrow beside it, and Deny), its edge in the agent's colour while it waits, answers a click, and says how in one quiet line before it goes.
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
