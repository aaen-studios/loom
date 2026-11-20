# server

Calx: the instance, and the `loom` program.

- [`main.calx`](main.calx): the `loom` command line: `serve`, `user add`, `user list`, `acp add`, `acp list`, `status` and `version`. Later `loom mcp`, `loom install`, `loom receive` and `loom update`.
- [`instance.calx`](instance.calx): one loop on std.http.server, listening on 127.0.0.1. It serves the HTTP routes and the `/ws` WebSocket from [protocol.md](../protocol/protocol.md), and runs each agent's turns through ACP, turning what the agents say into events and deltas.
- [`store.calx`](store.calx): SQLite. Every row is kept per person, and that includes the event log.
- [`acp.calx`](acp.calx): the ACP client. Each agent program runs as a process of its own, written to and read by a thread of its own.

Trying it from source, from the repository's root:

```sh
calx server/main.calx acp add claude npx -y @zed-industries/claude-agent-acp
calx server/main.calx serve                       # prints a setup code on the first start
```

Then `POST /api/setup` with the code makes the owner. `echo "a password" | calx server/main.calx user add bob` adds a second account. Everything is kept in `loom-data/` unless `--data` names another folder. [`tests/protocol/phase1.calx`](../tests/protocol/phase1.calx) shows a whole session.

Not in yet:
- pings to the apps every 20 seconds ([CLX-394](https://acta.aaenz.no/aaen-studios/clx#CLX-394));
- rate limits on logging in;
- pruning old events;
- picking a session up again after a restart with `session/load`;
- the `auto` permission mode (`ask` and `allow` work).

The same code is compiled into the desktop app's library for an instance with no server ([PLAN.md](../PLAN.md#an-instance-in-the-desktop-app)).
