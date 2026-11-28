# shared

Calx, imported by both [`server/`](../server/) and [`core/`](../core/), so the two can't drift apart:

- The protocol's types: every frame, event and content block in [protocol.md](../protocol/protocol.md), with their JSON.
- The command list: each command's name, arguments, answer, and whether it asks first when an agent uses it. The app's `cmd` frames and the agents' tools in `loom mcp` are both made from it.

- [`protocol.calx`](protocol.calx): the protocol versions, the frame size and the close codes.
- [`client.calx`](client.calx): a small client of an instance, for the command line, a device installing loom on a host, and the tests: setup, login, link codes and `/api/version` over HTTP, and a WebSocket session with `cmd` and events. The core keeps its own connection.
- [`hosts.calx`](hosts.calx): what a host's name or ssh target may be, and the plan for installing loom on one, with the commands it runs there, which the instance shows and the device that reaches the host makes again from what was signed.
- [`obj.calx`](obj.calx): building and reading the JSON objects frames are made of.
- [`badge.calx`](badge.calx): a service's badge on a card: its initial, and one of the thread colours picked from its name, so an MCP server or a service a call reaches looks the same on the instance's cards and the core's questions, in both apps.
- [`reply.calx`](reply.calx): replying to a message (PLAN.md, "Replying to a message"): what a message keeps of the one it replies to, from the part selected or the message's words, which the instance keeps and the core shows while it waits to go, and what the agent is told of it.
- [`noise.calx`](noise.calx): over Noise (protocol.md, "Over Noise"): the handshake's name and prologue, sealing a message in pieces and joining them again, and the frame, request and response messages a phone and the bridge send.
- [`emoji.calx`](emoji.calx): emoji (PLAN.md, "Emoji"): the built-in set from [`emoji/emoji.tsv`](emoji/emoji.tsv), embedded, which `scripts/emoji.calx` makes with its licences beside it; finding one by name for the composer's row and the picker's search, the picker's groups, what's typed after a colon, outside code, and emoticons; the tokens of a person's own, `<:name:id>`, and the text an agent reads (`:name:`) and writes, which the instance turns into the emoji.
- [`shape.calx`](shape.calx): the shapes an agent's thread takes in the sidebar: the eight that come with loom, which `scripts/tokens.calx` writes into the design tokens, and smoothing a drawing the person makes, which the instance keeps on the agent.
- [`sounds.calx`](sounds.calx): loom's sounds ([PLAN.md, "Sounds"](../PLAN.md#sounds)): a device's sound prefs, which cue an event gets, and whether it sounds, from the prefs, the agent's mute, the chat in front, quiet hours and the device's clock. The core asks it for every sound it tells the app of, and the instance for each phone's push.
- [`notices.calx`](notices.calx): notifications ([PLAN.md, "Notifications and push"](../PLAN.md#notifications-and-push)): whether a live event is one on a device, from whether someone is looking at its chat there and how far it's read on any device, and which of those a device showed a read takes back. The core asks it for each live event, and the instance whether someone is looking at a chat, whose reply it then reads at once.
