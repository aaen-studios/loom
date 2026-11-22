# design

loom looks simple and near-monochrome, like a plain chat app. The only colour comes from the agents: each one is a thread that moves with what the agent is doing.

- [`tokens.toml`](tokens.toml) holds every design token in one file: type, spacing, corners, layout, how big the controls are, colours for dark and light, the eight thread colours, and how each thread state moves. Change values there and nowhere else.
- [`generated/`](generated/) is made from it by `calx scripts/tokens.calx`: `tokens.rs` for the GPUI app, `Tokens.swift` for the iPhone app, and `tokens.css` for the site, the docs and plugin panels. `calx scripts/tokens.calx --check` fails if they're out of date.
- [`logo/`](logo/) holds the mark (the knot), with PNGs and the Windows icon.
- [`explorations/`](explorations/) holds the settled screens and the thread engine:
  - `round-4.html`: chat in dark and light, the thread board, the header menus, building an agent, settings, and the first start.
  - `threads-playground.html`: every thread state, the pluck, the one-time snap when an agent finishes, the handoff between agents, and a streaming reply.
  - `logo.html`: the logo options. B, the knot, was chosen.
  - `rejected/`: earlier rounds, kept so they aren't tried again.

Open the HTML files in a browser. They need nothing else.
