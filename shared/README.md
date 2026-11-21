# shared

Calx, imported by both [`server/`](../server/) and [`core/`](../core/), so the two can't drift apart:

- The protocol's types: every frame, event and content block in [protocol.md](../protocol/protocol.md), with their JSON.
- The command list: each command's name, arguments, answer, and whether it asks first when an agent uses it. The app's `cmd` frames and the agents' tools in `loom mcp` are both made from it.

- [`protocol.calx`](protocol.calx): the protocol versions, the frame size and the close codes.
- [`obj.calx`](obj.calx): building and reading the JSON objects frames are made of.
- [`noise.calx`](noise.calx): over Noise (protocol.md, "Over Noise"): the handshake's name and prologue, sealing a message in pieces and joining them again, and the frame, request and response messages a phone and the bridge send.
