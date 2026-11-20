# server

Calx: the instance, and the `loom` program.

- One process on std.http.server, listening on 127.0.0.1: the HTTP routes and the `/ws` WebSocket from [protocol.md](../protocol/protocol.md).
- SQLite, with every row kept per person, and the event log.
- The ACP client, running each agent program as a process of its own.
- The `loom` command line: `loom serve`, `loom user add`, `loom status`, and later `loom mcp`, `loom install`, `loom receive`, `loom update`.

The same code is compiled into the desktop app's library for an instance with no server ([PLAN.md](../PLAN.md#an-instance-in-the-desktop-app)).

Started in Phase 1 with `main.calx`, the `loom` program, which so far only says its version: `calx server/main.calx version`.
