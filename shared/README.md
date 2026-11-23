# shared

Calx, imported by both [`server/`](../server/) and [`core/`](../core/), so the two can't drift apart:

- The protocol's types: every frame, event and content block in [protocol.md](../protocol/protocol.md), with their JSON.
- The command list: each command's name, arguments, answer, and whether it asks first when an agent uses it. The app's `cmd` frames and the agents' tools in `loom mcp` are both made from it.

- [`protocol.calx`](protocol.calx): the protocol versions, the frame size and the close codes.
- [`client.calx`](client.calx): a small client of an instance, for the command line, a device installing loom on a host, and the tests: setup, login, link codes and `/api/version` over HTTP, and a WebSocket session with `cmd` and events. The core keeps its own connection.
- [`hosts.calx`](hosts.calx): what a host's name or ssh target may be, and the plan for installing loom on one, with the commands it runs there, which the instance shows and the device that reaches the host makes again from what was signed.
- [`obj.calx`](obj.calx): building and reading the JSON objects frames are made of.
- [`noise.calx`](noise.calx): over Noise (protocol.md, "Over Noise"): the handshake's name and prologue, sealing a message in pieces and joining them again, and the frame, request and response messages a phone and the bridge send.
- [`shape.calx`](shape.calx): the shapes an agent's thread takes in the sidebar: the eight that come with loom, which `scripts/tokens.calx` writes into the design tokens, and smoothing a drawing the person makes, which the instance keeps on the agent.
