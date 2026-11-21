# server

Calx: the instance, and the `loom` program.

- [`main.calx`](main.calx): the `loom` command line: `serve`, `user add`, `user list`, `acp add`, `acp list`, `files limit`, `status`, `update`, `mcp` and `version`. Later `loom install` and `loom receive`.
- [`mcp.calx`](mcp.calx): `loom mcp`, loom's own tools as an MCP server on standard input and output. The instance gives it to every agent's session, and it hands each call back to the instance. The tools are in [`shared/tools.calx`](../shared/tools.calx).
- [`instance.calx`](instance.calx): one loop on std.http.server, listening on 127.0.0.1. It serves the HTTP routes and the `/ws` WebSocket from [protocol.md](../protocol/protocol.md), and runs each agent's turns through ACP, turning what the agents say into events and deltas.
- [`store.calx`](store.calx): SQLite. Every row is kept per person, and that includes the event log. What only the instance should read (secrets, ACP programs' settings, webhook secrets) is sealed under `secret.key`, beside the database.
- [`hashing.calx`](hashing.calx): argon2id hashing on a thread of its own, so a login doesn't hold up the loop.
- [`acp.calx`](acp.calx): the ACP client. Each agent program runs as a process of its own, written to and read by a thread of its own.
- [`installer.calx`](installer.calx): ACP agents from the ACP Registry, on a thread of its own: reading the registry, and downloading, checking and unpacking an agent. `serve --registry` points it at a mirror.
- [`routines.calx`](routines.calx): when a routine runs next, from its schedule. The instance looks for routines that are due once a second, and runs them as a prompt to their agent (PLAN.md, "Routines"). `POST /hooks/<id>` runs one from outside, with its secret. `serve --test-minute-ms` shortens the minute, for tests.

Trying it from source, from the repository's root:

```sh
calx server/main.calx acp add claude npx -y @agentclientprotocol/claude-agent-acp
calx server/main.calx serve                       # prints a setup code on the first start
```

Then `POST /api/setup` with the code makes the owner. `echo "a password" | calx server/main.calx user add bob` adds a second account. Everything is kept in `loom-data/` unless `--data` names another folder. [`tests/protocol/phase1.calx`](../tests/protocol/phase1.calx) shows a whole session.

The same code is compiled into the desktop app's library for an instance with no server ([PLAN.md](../PLAN.md#an-instance-in-the-desktop-app)).
