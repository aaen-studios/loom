# Loom

Acta board: aaen-studios/loom

- [PLAN.md](PLAN.md) is the plan, and [protocol/protocol.md](protocol/protocol.md) the wire protocol. Read them before changing anything they cover, and keep them true when the code changes.
- Calx is pinned in `.calx-version`, and CI, the release and the Docker image install exactly that version. A change that needs a newer Calx pins it in the same pull request with `calx scripts/calx_pin.calx bump`, which copies it to `aaen-studios/loom-calx`.
- Loom waits for Calx. When Calx is missing something, it gets a card on `aaen-studios/clx` and the loom work that needs it waits. No workarounds in loom, and no other language for the server, the core or the scripts.
- loom.rip shows the app as it is. A change to how the desktop app looks or what it says changes the site's copy in the same pull request: the live app on the home page (`site/src/components/AppDemo.astro`, `site/src/scripts/demo.ts`), the threads (`site/src/scripts/thread.ts`) and the settings panels ([site/README.md](site/README.md), "How it's made").
- Designs come before code. If a feature, a fix or anything else changes what a person sees at all, in the desktop app, the iPhone app or on loom.rip, show renders of it first and build nothing until one is approved. Show several, different enough to choose between, each a picture of the screen as it would look (not a description), with what differs between them said in a line. Wait for the person to pick one or ask for changes, show it again if they ask for changes, and then build that design. If the design has to change while it's built, show the new one before going on.
- The apps hold no logic. GPUI (`desktop/`) and Swift (`ios/`) draw screens and reach the system. Everything else lives in the Calx core and server.
- v0.1 (Rust engine, React UI, Tauri) is at the `v0.1` tag, for reference only. Nothing from it is carried over unless the plan says so.
