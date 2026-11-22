# The loom protocol

Version 1. This is what an app (always through its core) and an instance say to each other. It's the contract between `core/` and `server/`, and its types live once, in `shared/`, which both import.

[PLAN.md](../PLAN.md) says why things are the way they are. This file says exactly what goes over the wire.

## Contents

- [Transport](#transport)
- [Conventions](#conventions)
- [HTTP routes](#http-routes)
- [The WebSocket session](#the-websocket-session)
- [Frames from the app](#frames-from-the-app)
- [Frames from the instance](#frames-from-the-instance)
- [Devices as tools](#devices-as-tools)
- [Agent defaults](#agent-defaults)
- [The queue](#the-queue)
- [Tasks](#tasks)
- [Agents run commands](#agents-run-commands)
- [The agent's browser](#the-agents-browser)
- [The event log](#the-event-log)
- [Messages and streaming](#messages-and-streaming)
- [Events](#events)
- [Commands](#commands)
- [Plugins](#plugins)
- [Errors and close codes](#errors-and-close-codes)
- [Over Noise](#over-noise)
- [Push](#push)
- [Versions](#versions)
- [How ACP maps onto loom](#how-acp-maps-onto-loom)
- [One message's whole trip](#one-messages-whole-trip)
- [Later versions](#later-versions)

## Transport

- **HTTP/1.1** for what isn't a live session: setup, login, invites, files and webhooks.
- **One WebSocket** (RFC 6455) per device for everything live, at `/ws`.
- **TLS** comes from whatever is in front of the instance (Caddy, a Cloudflare tunnel). The instance itself listens on 127.0.0.1. A desktop instance's own app connects over loopback without TLS. Phones reaching a desktop instance use a Noise handshake, over one WebSocket that carries their HTTP requests too ([Over Noise](#over-noise)).
- **One JSON object per WebSocket text frame**, UTF-8. Binary frames aren't used in version 1; files go over HTTP.
- **Frame size:** at most 1 MiB. A bigger frame closes the socket with 1009.

## Conventions

- **`t`** names a frame's type. Every frame has it.
- **Ids** are strings with a prefix that says what they are, so a log reads on its own: `u_` user, `d_` device, `a_` agent, `c_` conversation, `msg_` message, `run_` run, `tc_` tool call, `f_` file, `q_` command (chosen by the app), `m_` client id of a message (chosen by the app). The rest of an id is opaque. Apps never parse it.
- **Times** are integers, milliseconds since 1970 in UTC, in fields named `at` or ending in `_at`.
- **Unknown fields are ignored**, both ways. That's how fields are added without a new version.
- **An unknown frame type** gets an [`error`](#error) with code `unknown_frame`, and the socket stays open.
- **Optional fields** are left out, not sent as `null`, unless `null` means something (it says so where it does).

## HTTP routes

Every request and response body is JSON. Routes that need a login take the device token as `Authorization: Bearer <token>`. An error is a status code with a body:

```json
{ "error": { "code": "bad_password", "message": "That username and password don't match." } }
```

`message` is for people. `code` is for programs, and is one of those listed with each route, or `bad_request`, `unauthorized`, `rate_limited` (with a `Retry-After` header), `too_large` or `internal`.

### `GET /api/info`

No login. What an app needs before it logs in.

```json
{
  "instance": "i_8sk2",
  "key": "<the instance's public key, base64>",
  "protocol": [1],
  "setup": false,
  "moved": null
}
```

`setup` is `true` until the owner account exists. `key` is checked on every later connection ([PLAN.md: Connecting devices](../PLAN.md#connecting-devices)). `moved` is the [`moved`](#moved) frame once the instance has moved, and null until then.

Once the instance has moved, every other route answers 410 with `{"error": {"code": "moved", …}, "moved": <the moved frame>}`. While it's moving, every route but a `GET` answers 503 with `moving`.

### `POST /api/setup`

No login. Makes the owner account, once, with the code from the one-time setup link the server printed on its first start.

```json
→ { "code": "<setup code>", "username": "elliot", "password": "…", "device": { "name": "Desk PC", "kind": "windows", "tz": "Europe/Oslo" } }
← { "user": "u_1", "device": "d_1", "token": "<device token>" }
```

Errors: `bad_code`, `already_set_up`, `weak_password`, `bad_username`.

### `POST /api/login`

```json
→ { "username": "elliot", "password": "…", "device": { "name": "Desk PC", "kind": "windows", "tz": "Europe/Oslo" } }
← { "user": "u_1", "device": "d_3", "token": "<device token>" }
```

Each login makes a new device and a new token. The password isn't used again until the next login. `kind` is one of `windows`, `macos`, `ios`, `cli`. `tz` is an IANA time zone name.

Errors: `bad_password` (also for a username that doesn't exist, so the two can't be told apart), `rate_limited`.

### `POST /api/link`

A device joins with the one-time code in the QR code another device of the same account shows ([`link.create`](#commands), [PLAN.md: Connecting devices](../PLAN.md#connecting-devices)). No login.

```json
→ { "code": "<code from the QR code>", "device": { "name": "iPhone", "kind": "ios", "tz": "Europe/Oslo" } }
← { "user": "u_1", "device": "d_4", "token": "<device token>" }
```

A code works once, for five minutes. The device that showed it hears `device.added`. Errors: `bad_code` (also for one that's been used, and it counts toward `rate_limited` as a wrong password does), `expired`.

### `POST /api/join/<code>`

No login. Makes an account from an invite ([`invite.create`](#commands)), with the username and password the person picked, and its first device. The same body and answer as `/api/setup`, without `code`. The account gets the invite's role. Errors: `bad_code` (it counts toward `rate_limited` as a wrong password does), `expired`, `used_up`, `username_taken`, `bad_username`, `weak_password`.

### `POST /api/reset/<code>`

No login. A new password, from the link an owner or admin made ([`password.reset`](#commands)), and a new device signed in with it, as a login gives:

```json
→ { "password": "…", "device": { "name": "iPhone", "kind": "ios", "tz": "Europe/Oslo" } }
← { "user": "u_2", "device": "d_9", "token": "<device token>" }
```

A link works once, for a day. Every other device of the person's is signed out (`device.revoked`, and 4001 on its socket). Errors: `bad_code` (also for a link that was used, and it counts toward `rate_limited` as a wrong password does), `expired`, `weak_password`. The owner's own reset is `loom user password` on the server.

### `POST /api/logout`

Login needed. Revokes this device's token. No body, and `204` back.

### `GET /ws`

The WebSocket upgrade. It takes no token in the URL or headers: the token goes in the first frame, [`hello`](#hello), so it never ends up in a proxy's log.

### `POST /files?name=<name>`

Login needed, with the device token in `Authorization: Bearer <token>`. The body is the file, as it is, with its type in `Content-Type`, and its name, percent-encoded, in `name`. Bodies go up to 64 MB.

```json
{ "file": "9f86d0…", "name": "shot.png", "mime": "image/png", "size": 48213 }
```

- `file` is the file's SHA-256 in lowercase hex. The instance keeps each file once, by it, so sending one again costs nothing and gives the same answer.
- `name` is the name made safe for any system: no folders, and nothing Windows refuses.
- `mime` is what the bytes say for PNG, JPEG, GIF, WebP and PDF, else the `Content-Type` sent, else what the name's extension says.
- `413` with `storage_full` when the file would take the person over the space the owner gives each person (`loom files limit`).

A file goes in a message by its `file`, in [`send`](#send)'s `files`.

### `GET /files/<sha256>`

Login needed, as above. The file, with its type, for whoever uploaded it or got it from an agent, and `404` for anyone else. With `?thumb`, a thumbnail of a PNG or JPEG image instead, at most 512 pixels a side, as a JPEG, or a PNG when it has transparency. It's made just after the upload; until it's there, and for files that have none, `?thumb` gives the file itself. Both are cached for good (`Cache-Control: private, immutable`), since a file's name is its content.

### `GET /api/mcp/tools` and `POST /api/mcp/call`

For `loom mcp` alone, which the instance starts for each agent (below, [How ACP maps onto loom](#how-acp-maps-onto-loom)). They take that agent's tool token for that conversation's session, so a call finds the run it came from, `Authorization: Bearer <token>`, not a device's, and an app never calls them. `tools` gives `{"tools": [...]}`, MCP's tool definitions from `shared/tools.calx`. `call` takes `{"name", "arguments"}` and gives an MCP tool result, `{"content": [{"type": "text", "text"}], "isError"?}`. A `message_agent` call is answered when the other agent's reply is done.

In Code Mode ([PLAN.md](../PLAN.md#code-mode)), `loom mcp` also uses these, with the same token:

- `GET /api/mcp/servers`: `{"servers": [...]}`, the agent's MCP servers, as `session/new` would have had them (`{name, command, args, env}` or `{type: "http", name, url, headers}`, with each `env` and `headers` a list of `{name, value}` and its secrets filled in), and, unless the agent runs in a sandbox, the person's plugins' tool servers it allowed, each named `plugin-<id>`.
- `GET /api/mcp/deno`: where Deno is, `{"path"}`, or `{"error"}` when it can't be had here. The first time, the instance fetches it on its installer's thread, and the request waits until it's there.
- `POST /api/mcp/allow` with `{"server", "tool", "read_only", "arguments"}`: whether a script's call to that server's tool may go. It's put to the agent's permission mode as kind `read` (`read_only`) or `other`; one the mode allows is answered at once, and otherwise it's one of loom's own tools that asks ([How ACP maps onto loom](#how-acp-maps-onto-loom)), titled `Use TOOL on the MCP server SERVER` (or `on the plugin ID`), with the call's `server`, `tool`, `arguments` and `act` as `detail`, and answered once the person does. The answer is an MCP tool result: `isError` with why for a no, or else with `act`, the activity log's entry for the call (0 for one that only reads).
- `POST /api/mcp/done` with `{"act", "ok", "why"}`: how that call went, which settles its entry as `done` or `failed`. Only for an `act` that `allow` gave this token.

### `/api/scripts/*`

The person's own scripts ([PLAN.md](../PLAN.md#scripts)), with the script token `scripts.on` gave, as `Authorization: Bearer <token>`. Only from the instance's computer, straight: a request from anywhere but a loopback address, or with `X-Forwarded-For`, `Forwarded`, `X-Real-IP`, `Via` or `X-Loom-Bridge` (which the phones' bridge adds to everything it forwards, and takes off what a phone sent), gets `403` with `not_here`. A missing or wrong token gets `401`.

- `POST /api/scripts/ask` with `{"agent", "text", "conversation"?}`: the agent (by name or id) is asked, in its chat or the conversation named, as a person's message with `from: "script"`, up to 20,000 bytes. The answer comes once its run ends: `{"reply", "conversation", "message"}`, or `502` with `agent_failed`. `409` with `busy` while it's answering in that conversation, `404` for an agent or conversation that isn't the person's.
- `POST /api/scripts/routine` with `{"routine"}`, a routine's name or id: `routine.run`'s answer.
- `GET /api/scripts/approvals`: `{"approvals"}`, what waits for the person's yes, each `call`, `agent`, `agent_name`, `title` and `on_a_device`.
- `POST /api/scripts/approve` with `{"call", "allow"}`: answers one, as `script` (`approval.answered`'s `by`). One `on_a_device` is refused with `403`: a device signs its yes.
- `POST /api/scripts/import` with `{"from", "path"?}`: `import.run`'s answer, as `script`, or `400` with its error. `loom import openbot|grokbot [PATH]` sends it.

### `POST /hooks/<id>`

A routine's webhook, for something outside to run it, such as GitHub Actions when a build fails. `<id>` is the routine's `hook`, and its `secret` comes one of two ways: as `Authorization: Bearer <secret>`, or as GitHub's webhooks sign, with `X-Hub-Signature-256: sha256=<hex>`, the HMAC-SHA256 of the body under the secret. The body can be anything. As UTF-8, up to 20,000 bytes of it, it goes to the agent after the routine's prompt, in a code block, with `X-GitHub-Event` named when it comes.

- `202` with `{"run"}`: it's running.
- `401` with `unauthorized`: the secret or the signature is wrong.
- `404`: no routine has that hook.
- `405`: a method other than POST.
- `409` with `paused`, or `busy` while its last run is still going.

## The WebSocket session

```
app                                               instance
 │── open /ws ────────────────────────────────────▶│
 │── hello (token, versions, after) ──────────────▶│
 │◀───────────────────────── welcome (version, seq) │
 │◀──────────── event, event, … (everything after)  │   catch-up
 │◀──────────────────── delta (each run so far)     │   replies in progress
 │◀─────────────────────────────── synced (seq)     │
 │◀═════════════ events and deltas, live ══════════▶│   and the app's frames
```

1. The app opens `/ws` and sends `hello` straight away. Anything else first closes the socket with 4000.
2. The instance checks the token and the versions, and answers `welcome`, or closes with [4001 or 4002](#errors-and-close-codes).
3. It sends every event after the app's cursor, in order ([The event log](#the-event-log)).
4. For each of the person's runs still going, it sends what the reply has so far, as deltas from position 0 ([Messages and streaming](#messages-and-streaming)).
5. It sends `synced`. From then on, events and deltas come as they happen.

The app may send frames as soon as it has `welcome`. It doesn't wait for `synced`.

**Keeping it alive.** The instance sends a WebSocket ping every 20 seconds. A connection that hasn't answered for 60 seconds is closed. The app treats 45 seconds with nothing from the instance as a dead connection, and reconnects.

**Reconnecting.** The app reconnects after 0.5 s, then doubles the wait each time, up to 30 s, with up to 20% random jitter. It tries again at once when the system wakes from sleep or the network changes. Each time it sends `hello` with the last `seq` it stored, and nothing else is needed to pick up where it was.

## Frames from the app

### `hello`

```json
{ "t": "hello", "token": "<device token>", "versions": [1], "after": 1041, "device": { "tz": "Europe/Oslo", "active": true } }
```

- `versions`: the protocol versions the app speaks.
- `after`: the last event `seq` the app has stored. `0` for a device that has nothing yet.
- `device`: what changed about the device since it logged in, all optional: `tz`, `name`, and `active` (whether someone is using it right now).

### `send`

A message from the person into a conversation.

```json
{ "t": "send", "cid": "m_4f2", "conversation": "c_9", "text": "Did the build pass?", "files": [] }
```

- `cid`: a client id the app makes when the message is written, before it's sent. The instance remembers each device's client ids. A `send` with a `cid` it has seen gets the same `ack` again and does nothing else, so an outbox can send again after a reconnect without making a second message.
- `files`: the `file`s of files uploaded first with [`POST /files`](#post-filesnamename). Each becomes a `file` block after the text. A send needs `text`, `files` or both.

Answered by [`ack`](#ack), or [`error`](#error) with `ref` set to the `cid`.

### `approve`

The person's answer to an [`approval.needed`](#approvals) event.

```json
{ "t": "approve", "call": "tc_3", "allow": true, "always": false }
```

`always: true` also allows this kind of request from this agent from now on, where the agent offers that. An approval that was already answered, by this device or another, gets an `error` with code `already_answered`.

For a call to one of the person's devices (`approval.needed` with a `request`), a yes carries `approval`: `{ "key": "<the approving device's Ed25519 public key, hex>", "signature": "<hex>" }`, the device's signature of `loom device call\n` followed by the `request`'s exact text. The core adds it. The instance passes it on, and the device doing the work acts only on a signature it trusts ([Devices as tools](#devices-as-tools)).

### `cancel`

Stops a run. The reply so far is kept, and the run ends with `status: "cancelled"`.

```json
{ "t": "cancel", "run": "run_77" }
```

### `cmd`

Every other action is a command ([Commands](#commands)).

```json
{ "t": "cmd", "id": "q_19", "name": "conversation.create", "args": { "agent": "a_2", "title": "Build checks" } }
```

Answered by [`result`](#result) with the same `id`.

### `device`

The device's state changed: the same optional fields as `hello`'s `device`. Sent when someone starts or stops using the device, or its time zone changes.

```json
{ "t": "device", "active": false }
```

A device that offers tools to agents also sends, after `welcome` and whenever the person changes them on it, its `key` (its Ed25519 public key, hex) and its `tools`, each `off`, `ask` or `allow`:

```json
{ "t": "device", "key": "3d4017c3…", "tools": { "run_command": "ask", "read_file": "allow", "list_files": "off", "open": "off" } }
```

### `device.result`

The answer to a [`device.call`](#devicecall): `ok`, and `output`, the command's output (with its exit status) or the file, or why it wasn't done.

```json
{ "t": "device.result", "call": "tc_9", "ok": true, "output": "hi from loom\nExit status 0." }
```

## Frames from the instance

### `welcome`

```json
{ "t": "welcome", "version": 1, "user": "u_1", "device": "d_3", "seq": 1046, "reset": false, "key": "<the instance's public key, base64>", "relay": "<the push relay's key, base64>" }
```

- `version`: the one version this connection uses.
- `relay`: only while the instance pushes through the [push relay](#through-the-push-relay). It's the relay's X25519 public key, which the core seals the phone's APNs token with before `push.register`.
- `key`: the instance's Ed25519 public key, as `GET /api/info` gives it. The app keeps the first one it sees, checks a [`moved`](#moved) frame's signature with it, and expects the same key at the new place.
- `seq`: the newest event `seq` the person has right now. Catch-up runs from `after` up to this.
- `reset`: `true` when the app's cursor is too old (its events were pruned, which includes a new device's `0` once any have been) or ahead of the instance's (the instance was restored from an older backup). The app clears its cache, and a [`snapshot`](#snapshot) follows instead of the missed events.

### `snapshot`

Only after a `welcome` with `reset: true`. Everything the app needs to start again from `seq`: the conversations, the agents and the newest messages of each conversation. Older messages are fetched with the `messages.list` command.

```json
{ "t": "snapshot", "seq": 1046, "agents": [ … ], "conversations": [ … ], "messages": [ … ] }
```

Events after `seq` follow as usual.

### `event`

One entry of the [event log](#the-event-log).

```json
{ "t": "event", "seq": 1042, "kind": "message.created", "at": 1795187131000, "conversation": "c_9", "message": { … } }
```

`kind` says what happened, and the rest depends on it ([Events](#events)).

### `delta`

Live text for a reply that's still being written. No `seq`; it isn't in the log.

```json
{ "t": "delta", "run": "run_77", "message": "msg_311", "block": 0, "kind": "text", "pos": 0, "text": "Checking" }
```

See [Messages and streaming](#messages-and-streaming).

### `synced`

Catch-up is over. Everything after this is live.

```json
{ "t": "synced", "seq": 1046 }
```

### `ack`

A `send` was stored.

```json
{ "t": "ack", "cid": "m_4f2", "message": "msg_310", "run": "run_77" }
```

`run` is the run it started. While the conversation's agent is still answering, the message waits in the conversation's queue instead ([The queue](#the-queue)): the `ack` has `queued: true` and no `run`, and `message` is the id it gets once it goes. Where the agent's `busy`, or the person's default, is `steer`, it goes first and steers at once, as `queue.steer` does, and the `ack` has `steered: true` too.

### `result`

The answer to a `cmd`.

```json
{ "t": "result", "id": "q_19", "ok": true, "value": { "conversation": "c_12" } }
{ "t": "result", "id": "q_20", "ok": false, "error": { "code": "not_found", "message": "There's no agent a_9." } }
```

A command that changes something also makes events, so every other device sees the change. The device that sent the command gets those events too, and applies them like any other.

### `device.call`

A call to one of this device's tools, from an agent's `use_device`. `request` is the call as text: `call`, `device` (this device's id), `tool`, `args`, `agent` and `at` (Unix ms). It's what an approval signs, and what the device runs. `approval` is there when the person said yes to a tool that asks.

```json
{ "t": "device.call", "call": "tc_9", "request": "{\"call\":\"tc_9\",\"device\":\"d_1\",\"tool\":\"run_command\",\"args\":{\"command\":\"echo hi\"},\"agent\":\"a_1\",\"at\":1795262400000}", "approval": { "key": "3d4017c3…", "signature": "9a0f…" } }
```

The device answers with [`device.result`](#deviceresult).

### `browser.state`

An agent's browser opened or closed, was taken over or handed back, or is asking for the person: `agent`, `open`, `person` (whether the person has it), `asked` (what `browser_ask_person` said, while it waits; otherwise empty) and `secret` (what `browser_ask_secret` asks for, while it waits). Every live session of the person's gets it, not only those watching. It isn't an event, so it isn't kept: an app that connects asks with `browser.watch`.

### `browser.frame`

A picture of the page, to each session watching that agent's browser: `agent`, `data` (a JPEG, base64), `width` and `height` (the page's pixels, which `browser.input`'s `x` and `y` are in).

### `error`

Something the app sent couldn't be done. The socket stays open.

```json
{ "t": "error", "code": "unknown_conversation", "message": "There's no conversation c_44.", "ref": "m_4f2" }
```

`ref` is the `cid`, `call` or `run` of the frame it answers, when there is one.

### `moved`

The instance has moved to `address` ([Moving an instance](../PLAN.md#moving-an-instance)). Every connected device gets it the moment the move is done, and a device that connects later gets it instead of `welcome`. The socket closes with 4004 after it.

```json
{ "t": "moved", "instance": "i_8sk2", "address": "https://loom.example.com", "at": 1795262400000, "signature": "<hex>" }
```

`signature` is the instance's Ed25519 signature of `loom moved <instance> to <address> at <at>` (`shared/protocol.calx`, `moved_message`). The app checks it with the key `welcome` gave it, moves its token to `address`, and connects there with the same `after`. The instance there has the same key, the same tokens and the same events, so it carries on where it was. An app that finds another key there doesn't send its token.

It isn't an event: the move takes the event log as it was, and the events go on at the new place from the same number.

## Devices as tools

The person's devices are tools for their agents while their apps run ([PLAN.md](../PLAN.md#your-devices)). The device decides, not the instance:

- **Each tool is off until the person turns it on, on that device**: `off`, `ask` each time, or `allow`. The device tells the instance (the `device` frame), so agents see it in `list_devices`, and checks it again itself before it acts on a `device.call`.
- **A tool that asks** shows in the agent's chat as `approval.needed` with `device` (the device's id) and `request`, and `can_always: false`. A yes is signed by the device it's given on (`approve`'s `approval`). The device doing the work acts only on its own key's signature of the exact request. A yes from another device, or one the instance makes up, is refused, and the agent hears so. Approving on one device for another comes with linking devices to each other, which exchanges their keys without the instance in between.
- **A request is done once.** The device keeps the calls it did, and refuses a request more than 15 minutes from its clock.
- **The tools:** `run_command` (`command`, in PowerShell on Windows and zsh on a Mac, up to 2 minutes), `read_file` (`path`), `list_files` (`path`, folders with a `/`), `open` (`target`, a file or a link, with its usual program), `screenshot` (the main screen, as a JPEG up to 1568 pixels on its longest side), `read_clipboard` (its text) and `write_clipboard` (`text`). Output past 20,000 bytes is cut.
- **A screenshot** comes back in `device.result` as `image` (base64) and `mime`. The agent's tool call gets it as an MCP `image`, and the chat keeps it as a file block in `tool.finished`'s `output`.
- Each call goes in the activity log as kind `device`, by the device that did it.

## Agent defaults

The person's defaults are what every agent of theirs follows, live, unless it sets that field itself ([PLAN.md](../PLAN.md#agent-defaults)). They're kept on the instance per person, so every device has them: an app asks with `defaults.get` once it's connected, and `defaults.updated` keeps it up to date.

- `brain`: the program `agent.create` uses when it names none. An agent that's made keeps its program.
- `config`: per program, the options its agents' sessions are set to, by id. An agent's own `config` wins, option by option. A default its program doesn't offer is passed over, so the session stays on what the program starts on.
- `permission`: the mode of an agent with no `permission` of its own. Loom's own risky tools ask whatever it is.
- `instructions`: told to every agent, beside the person's profile ([How ACP maps onto loom](#how-acp-maps-onto-loom)).
- `busy`: what a `send` to an agent that's answering does, `queue` (it waits, [The queue](#the-queue)) or `steer`. An agent's own `busy` wins.
- An agent's field goes back to following the default with `agent.update` and `null` (or `""`): `permission`, `busy`, or an option in `config`.
- An agent made before there were defaults keeps the `permission` it was made with, as its own, until the person sets it to follow the default; its model and effort already follow it wherever it hasn't picked its own. One made with `agent.create` and no `permission` follows the default from the start, but one an agent makes (`create_agent`, or `run_command` with `agent.create`) or an import brings has `ask` of its own.
- Agents read them at once, and `defaults.put` asks the person first ([Agents run commands](#agents-run-commands)), since it changes every agent that follows them.

## The queue

A conversation has one turn at a time. A `send` while its agent is still answering waits in the conversation's queue ([PLAN.md](../PLAN.md#work-that-waits-its-turn)), kept in the instance's database, and goes once the turn ends: as the person's message, with the id the `ack` gave, and a run of its own. One goes at a time, first first. A turn that ends `done` or `failed` lets the next go. One the person stops (`cancel`) or that runs out (`limit`) leaves the queue waiting, until they send again or steer. A turn the instance's restart cut short finished `failed`, so once it's running again the next goes.

- Every change to a queue is a `queue.changed` event: `conversation`, and `queue`, the whole queue, first first, each item its `id`, `text`, `files`, `from` (the device that sent it), `at`, and `steered` once it's been steered.
- `queue.list` (`conversation`) gives it as `queue`. `queue.edit` (`id`, `text`) changes an item's text, in its place; it can't leave one with nothing. `queue.order` (`conversation`, `ids`) puts the items in that order, the ones it names first and the rest after, as they were. `queue.drop` (`id`) drops one. `not_found` is an item that's gone, sent or dropped.
- `queue.steer` (`id`) sends one now: it goes first, and the running turn is stopped (`session/cancel`, so its run finishes `cancelled`). Then it goes, in the same ACP session, so the agent has what it did so far, and its prompt starts with a `[loom: …]` block that says it was stopped for this message. With no turn running, it goes at once.

## Tasks

A task is a long job an agent works on in a conversation of its own, so its chat with the person stays free ([PLAN.md](../PLAN.md#work-that-waits-its-turn)). The conversation has `task` (the task's id) in `conversation.created` and the `conversations` list, and it's never the one an app shows as the agent's chat, or the one a `send` that names only `agent` goes to. Everything else about it is a conversation's: the person can read it, and a `send` naming it, or its queue, goes into the task. A branch (`branch_of`) and the side questions' conversation (`side`) are never the agent's chat either.

- A task is `id`, `agent`, `conversation`, `title`, `status`, `run` (its run now, or its last), `by` (the device or agent that started it), `attempts`, `at` and `updated` (Unix ms), `result` once it's `done` (the start of the agent's last answer, up to 2000 bytes), and `error` while it's `failed`, or `paused` by a limit.
- `status` is `running`, `paused`, `done`, `failed` or `cancelled`. Any run in the task's conversation is the task's: it's `running` while one goes, and the run's end ends it. `done` is done, `failed` is failed, and a run stopped any other way (the person's `cancel`, a steer, `limit`) leaves it `paused`, unless `task.cancel` stopped it, which makes it `cancelled`.
- Every change is a `task.updated` event, with the whole `task`.
- `task.start` (`agent`, `prompt`, `title`?: the prompt's first line if left out) makes the conversation, puts the prompt in it as the person's message, and starts a run. `task.list` gives `tasks`, newest first, but not those of deleted agents.
- `task.pause` (`id`) stops a running one, which stays `paused`. `task.resume` (`id`) carries a paused one on, in the same ACP session, with a run whose prompt is only a `[loom: …]` block that says so. `task.cancel` (`id`) stops a running or paused one for good. `task.retry` (`id`) tries a `failed` or `cancelled` one again the same way, as its next `attempts`. A task in the wrong state for the command is refused `not_now`, and one whose agent is working on it already `busy`.
- A task that was `running` when the instance stopped carries on once it starts again: its run was `failed` by the restart, and a new one starts with a `[loom: …]` block that says the instance restarted.
- An agent starts one with `start_task` (`title`, `prompt`), with itself as `by`. It may have three running that it started itself.

## Agents run commands

An agent runs the commands below as its person's app does, with their rights ([PLAN.md](../PLAN.md#agents-can-do-anything-the-app-can)), through two of loom's own tools:

- `list_commands` gives each command an agent can run: `name`, `args` (in words), `does`, and `how`: `runs at once` or `asks your person first`.
- `run_command` (`name`, `args`, an object) runs one. Its result is the command's `value` as JSON, or its error's message. Which run at once and which ask is in `server/commands.calx`.
- **At once**: `conversation.create`, `rename` and `branch`, `messages.list`, `search` and `pinned`, `message.pin`, `agent.create` (always with `permission: "ask"`), `agent.update`, `agent.revisions`, `agent.rollback`, `activity.list`, `memory.list` and `put`, `table.list`, `create`, `write`, `rows` and `delete_rows`, `profile.get` and `put`, `defaults.get`, `routine.list`, `put` and `run`, `task.*`, `queue.list`, `edit`, `order` and `drop`, `skill.list`, `get` and `put`, `plugin.list`, `get`, `put` and `revisions`, `mcp.list`, `secret.list`, `acp.list` and `acp.registry`. What changes something is in the activity log as "Ran" and the command's name. A task started by another agent is told so, and an agent can start three running tasks for each agent.
- **Asks first**, every time, as one of loom's own tools that asks ([How ACP maps onto loom](#how-acp-maps-onto-loom)), titled with what the command does, with its `args` as `detail`: every `.delete` and `.remove`, and `device.revoke` (tool `delete`); `instance.move` (tool `move`); and everything else (tool `other`), among them `mcp.put`, `acp.install`, `acp.settings`, `acp.share` and `acp.members`, `invite.*`, `user.list` and `user.role`, `device.rename`, `link.create`, `instance.phones`, `instance.relay` and `server.offer`, any command not listed, `defaults.put` (one with a `permission` looser than the agent's own is refused before it asks), and `agent.create` with a `folder` or `agent.update` with a `permission` (`null` included, which follows the default) or a `folder`. A yes runs it as the device that gave it, so commands only a device may send work. `acp.install` answers once it's installed.
- **Never**: `account.delete`, `password.reset`, `secret.put` (a secret's value is never an agent's), `plugin.allow`, `plugin.cmd`, `plugin.data.*`, `plugin.message` and `chat.ask` (agents have `message_agent`), `push.*`, `browser.*` (agents have their `browser_` tools), `queue.steer` and `scripts.*`. An agent can't delete itself or loosen its own permission mode, or give another a looser one than its own.

## The agent's browser

Each agent has a browser of its own ([PLAN.md](../PLAN.md#the-agents-browser)): a headless Chrome, Edge or Chromium on the instance's computer, which the instance drives over the DevTools protocol, one per agent, on a thread of its own. It starts on the agent's first browser call and stops after 10 minutes with nothing to do, and the next call starts it again. `LOOM_BROWSER` names the one to run; otherwise it's the first Chrome, Edge or Chromium installed in the usual places or on PATH.

- **The profile** is `browsers/<user>/<agent>` in the instance's data, not the agent's folder, so cookies and logins last between calls but the agent can't read them. Deleting the agent, or removing the person, stops its browser and removes the profile.
- **The tools:** `browser_open` (`url`), `browser_snapshot`, `browser_click` (`ref`), `browser_type` (`ref`, `text`, `submit`?, `clear`?, true unless false), `browser_press` (`key`: `Enter`, `Tab`, `Escape`, `Backspace`, `Delete`, the arrows, `Home`, `End`, `PageUp`, `PageDown`, `Space`, or one character), `browser_scroll` (`direction`, `down` or `up`), `browser_back` and `browser_screenshot`.
- **The outline** that `browser_snapshot` gives, and `browser_open`, `browser_click`, `browser_press`, `browser_back` and a submitting `browser_type` give after they act, is the page's accessibility tree: a line for each part that means something, indented under what it's in, as `- role "name": value [states] [ref=e12]`, after a first line `Page: <title> (<address>)`. Parts that only hold others are left out unless they're named. A ref lasts until the next outline. It's cut at 40,000 bytes.
- **A screenshot** is an MCP `image`, a JPEG of the 1280 × 800 page, with the page's line.
- **What it opens:** `http`, `https`, `file` and `about` pages. A sandboxed person's agent opens only `http` and `https`, and its browser loads nothing from the instance's computer (`localhost`, `127.*`, `[::1]` and every IPv6 address written as one) or a private network (`10.*`, `172.16–31.*`, `192.168.*`, `169.254.*`, `0.*`). Downloads are refused.
- **Watching it.** `browser.watch` (`agent`, `on`? true unless false) has this session watch that agent's browser, or stop; a session watches one at a time, and stops when it closes. It answers with the browser's state, as `browser.state` has it. While anyone watches, the instance sends each watching session `browser.frame`s as the page changes, a JPEG of it each, under 1 MiB. It doesn't open a browser that isn't.
- **Taking over.** `browser.take` (`agent`) gives the person the browser, if it's open: the agent's browser calls wait meanwhile, in order. `browser.input` (`agent`, `kind`, and for each kind: `click` and `move` with `x` and `y`, the page's own pixels as the frames are; `wheel` with `x`, `y` and `dy`; `key` with `key`, as `browser_press` takes it; `text` with `text`) is a click, a scroll, a key or text typed, refused with `not_yours` unless they've taken over. `browser.give` (`agent`) hands it back: the calls that waited go on, and each `browser_ask_person` gets its answer. `not_open` is a browser that isn't open, and `not_found` an agent that isn't theirs.
- **`browser_ask_person`** (`why`) asks the person to take over, for a login, a code or a CAPTCHA, with a `notify` (so a phone gets it too) and `browser.state`'s `asked`. Its result comes once they hand the browser back: how many clicks and keys they gave, and the page's outline. It needs the browser open.
- **Secrets go in unseen.** `browser_fill_secret` (`ref`, `secret`, `submit`?) types one of the person's secrets (`secret.put`) into a field. The first time a secret goes to a site (its host and port, or a file on the instance's computer), it asks the person in the agent's chat, as the tools that delete do (`approval.needed`, titled "Fill your secret NAME on HOST"), and a yes keeps that site for that secret. `browser_ask_secret` (`ref`, `why`, `submit`?) asks the person to type something in once: a `notify`, and `browser.state`'s `secret`. `browser.secret` (`agent`, `value`) types it into the field, or with `value` empty says no. Either way the agent's result says it's in, never what it was, and every outline after names a filled secret `[secret NAME]` (or `[secret what your person typed]`) wherever the page shows it. A screenshot shows the page as it is.
- Each `browser_open`, each ask and each hand back goes in the activity log as kind `browser`.
- **`html_preview`** (`html`, the whole page, up to 2 MiB; `width`? 1280 and `height`? 800, the window in pixels; `full_page`? false) draws the page in a new background tab of the agent's browser (`Target.createTarget`, then `Page.setDocumentContent`), with the same blocked addresses for a sandboxed person's, waits for it to load and 300 ms more, and closes the tab. Its result is an MCP `image`, a JPEG of the window, or of the whole page up to 16,000 pixels tall, and a line saying the size and what the console said: logs, errors thrown and the browser's own entries, as `level: text`, up to 50 lines of 400 bytes.
- **`html_render`** (`html`, up to 2 MiB; `title`, up to 80 bytes) keeps the page as the person's file, `<title>.html` of type `text/html`, and adds it to the reply of the turn that made the call as a `file` block ([PLAN.md](../PLAN.md#pages-in-the-chat)). It's refused when the agent isn't answering in a chat, or the person's files are at their limit. It goes in the activity log as kind `page`. An app shows a `text/html` file block as a page, opened beside the chat in a web view walled off as a [panel](#plugins) is, with nothing through the door.

## The event log

- **Every change** the instance makes for a person is an event with a `seq`, counted per person from 1. Each `seq` is one more than the last, with no gaps.
- **An event is stored before it's sent.** An app that has `seq` N has everything up to N.
- **The app stores the `seq`** of the last event it applied, in the same transaction as the change it made to its cache. That's its cursor, and it sends it as `after` on the next `hello`.
- **Every device of a person** sees the same events in the same order, including events about what that device did itself.
- **Old events are pruned** after 30 days, keeping at least each person's newest 10,000. The instance looks once an hour. A device further behind than that gets `reset: true` and a snapshot.
- **What isn't an event:** deltas, `ack`, `result`, `error`, `welcome` and `synced`. None of them change state that a cache keeps.

## Messages and streaming

A message is a list of **content blocks**:

```json
{
  "id": "msg_311",
  "conversation": "c_9",
  "role": "agent",
  "agent": "a_2",
  "run": "run_77",
  "at": 1795187133000,
  "blocks": [
    { "kind": "thought", "text": "The user means the CI run on main." },
    { "kind": "tool", "call": "tc_3" },
    { "kind": "text", "text": "Yes. The last build on main passed at 14:02." }
  ]
}
```

A reply that names files in its text, in `code` like `notes/hours.md`, also has `mentions`: each of them (ten at most) that is a file in its agent's folder or the shared folder, with `path` as the reply wrote it, `name` and `size`. The apps show them as chips, and `agent.file` opens one.

- `role` is `person` or `agent`. A person's message has `from` (a device id) instead of `agent` and `run`. A message one agent sent another with `message_agent` is a `person` message too, in the other agent's conversation, with the sending agent's id as `from`. One a plugin's panel sent with `plugin.message` has `plugin:<id>` as `from`, and one brought over with `import.run` has `import:openbot` or `import:grokbot`, as its pin's `by` does. An imported agent's reply has `agent` and no `run`.
- Block kinds in version 1: `text` (Markdown), `thought` (the agent's reasoning, shown folded), `tool` (a tool call, whose details are in its `tool.*` events), `plan` (the agent's plan, as `entries` with `content` and `status`) and `file` (`file`, `name`, `mime`, `size`).

**While a reply is being written**, its message exists but isn't finished. `run.started` gives its id, and deltas fill its blocks:

```json
{ "t": "delta", "run": "run_77", "message": "msg_311", "block": 2, "kind": "text", "pos": 4, "text": " The last build" }
```

- `block`: which block, counted from 0. A delta for a block the app doesn't have yet starts that block, with `kind`.
- `pos`: how many UTF-8 bytes of that block's text came before this piece. The app adds the piece when `pos` equals what it has, skips it when `pos + len(text)` is no more than what it has (it's already there), and treats a gap as a lost connection (it reconnects).
- A tool call doesn't stream. It comes as `tool.started` and `tool.finished` events, and its block points at it.

**When the reply is finished**, `message.created` carries the whole message, and it replaces whatever the deltas built. Then `run.finished` follows. An app that missed every delta still ends up with the right message.

**Joining in the middle of a reply.** After catch-up, before `synced`, the instance sends each run still going as deltas from `pos` 0, one per block, with everything so far. Then the rest comes live.

## Events

Version 1's kinds. Each event also has `seq`, `kind` and `at`.

### Conversations and messages

| Kind | Fields | When |
|---|---|---|
| `conversation.created` | `conversation` (the whole conversation: `id`, `agent`, `title`, `at`, `task` for a task's, `side: true` for the one where an agent answers side questions,, `branch_of` and `branch_at` for a branch: the conversation and the message it goes on from, and `routine` for a routine's thread of its own: the routine) | A conversation is made |
| `conversation.updated` | `conversation` (`id` and what changed) | Renamed, or its agent changed |
| `conversation.deleted` | `conversation` (the id) | Deleted |
| `message.created` | `conversation`, `message` (the whole message) | A person's message is stored, or an agent's reply is finished |
| `message.updated` | `conversation`, `message` (the whole message, as it is now) | A stored message changed: it was pinned (it has `pinned`: `at` and `by`) or unpinned (it hasn't) |
| `message.deleted` | `conversation`, `message` (its id) | A stored message is gone: a routine's message, taken back when its run ended quiet (`routine_quiet`) |

### Runs

A run is one turn of an agent, answering one message.

| Kind | Fields | When |
|---|---|---|
| `run.started` | `run`, `conversation`, `agent`, `revision`, `message` (the reply's id), `answering` (the person's message id) | The agent was given the message |
| `run.finished` | `run`, `status` (`done`, `cancelled`, `failed` or `limit`), `error` (for `failed`), `usage` (`input`, `output` and `total` tokens, where the agent program reports them with ACP's `usage`) | The turn is over, after `message.created` for its reply. Only the owner's agents run as the instance's own user. Anyone else's run in their sandbox ([Sandboxes](#sandboxes)), and on an instance without sandboxes their runs are `failed`, saying so |
| `queue.changed` | `conversation`, `queue` (the whole queue, first first) | A message waits for its agent, or one that waited was edited, reordered, dropped, steered or sent ([The queue](#the-queue)) |
| `task.updated` | `task` (the whole task) | A task started, or its status changed ([Tasks](#tasks)) |
| `table.changed` | `table`, `by` (the agent or device), `dropped`? (`true` when it's gone) | A shared table was made, written to, had rows deleted, or was dropped ([PLAN.md](../PLAN.md#shared-tables)) |
| `server.offered` | `address`, `by` (the device that said yes) | An agent's `add_server` was allowed: that device's app adds the server at `address`, for the person to sign in to ([PLAN.md](../PLAN.md#several-servers-in-one-app)) |

### Tool calls

| Kind | Fields | When |
|---|---|---|
| `tool.started` | `run`, `call`, `tool` (a kind: `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, `other`), `title`, `input` (each string in it cut at 65,536 bytes, with "…", here and in `approval.needed`'s `detail`, so a big one can't make a frame past 1 MiB), `locations` (files and lines it touches) | The agent started a tool |
| `tool.updated` | `call`, and what changed: `title`, `output`, `locations` | The agent reported progress |
| `tool.finished` | `call`, `status` (`done`, `failed`, `denied`), `output` (blocks, kept short: long output is cut, with `cut: true`) | The tool ended |

### Approvals

| Kind | Fields | When |
|---|---|---|
| `approval.needed` | `run`, `call`, `tool`, `title`, `detail` (what exactly would happen: the command, the diff, the file), `can_always`, and for a call to a device, `device` and `request` ([Devices as tools](#devices-as-tools)) | The agent asks first, and its permission mode sends the question to the person |
| `approval.answered` | `call`, `allow`, `always`, `by` (the device that answered, or `auto` when the permission mode did) | Someone answered, on any device. Every other device stops showing the question |

An approval nobody answers stays open while the run does. Cancelling the run answers it with `allow: false`, `by: "cancel"`.

### Agents and devices

| Kind | Fields | When |
|---|---|---|
| `agent.created`, `agent.updated` | `agent` (the whole agent, with its `revision`), and for `agent.updated`, `by` (the device that changed it, or the agent's id when an agent did, with a tool) | Made or changed, by a person or by an agent |
| `defaults.updated` | `defaults` (the whole defaults, as `defaults.get` gives them), `by` (the device that changed them; a yes to an agent's `defaults.put` is done as the device that gave it) | The person's agent defaults changed. Every agent that follows a field follows the new one from its next prompt ([Agent defaults](#agent-defaults)) |
| `notify` | `agent`, `title`, `body`, and for a routine's reply, `conversation` | An agent sent its person a notification with its `notify` tool, or a routine with notify on finished a run (its name as `title`, the start of the reply as `body`). The apps show it, even while someone's looking |
| `agent.deleted` | `agent` (the id) | Deleted |
| `routine.updated` | `routine` (the whole routine, as `routine.list` gives it) | Made, changed, or run: `last` says how its newest run is going |
| `routine.deleted` | `routine` (the id) | Deleted |
| `flow.updated` | `flow`, as `flow.list` gives each | A routine's flow started, or one of its steps started or ended. Once `finished`, it's no longer listed |
| `plugin.updated` | `plugin` (as `plugin.list` gives it) | A plugin was put, or its person allowed what it asks |
| `plugin.deleted` | `id` | Deleted |
| `plugin.data` | `plugin`, `key` | A panel's data under `key` changed. A panel showing it on another device reads it again |
| `device.added` | `device` (`id`, `name`, `kind`, `at`) | A login, on any of the person's devices |
| `device.updated` | `device` (`id` and what changed) | Renamed |
| `device.revoked` | `device` (the id) | Signed out. The revoked device's own socket is closed with 4001 |
| `user.joined` | `user`, `username`, `role` | Someone joined with an invite. To every owner and admin |
| `people.logged` | `entry`: its `kind`, `text`, `by` and `at`, as `people.log` gives each | Something went in the people log. To every owner and admin |

## Commands

Commands are defined once in `shared/` as a list: each one's name, its arguments, what it answers, and whether it asks first when an agent uses it. The app's `cmd` frames and the agents' tools in `loom mcp` both come from that list ([PLAN.md](../PLAN.md#agents-can-do-anything-the-app-can)).

Version 1's commands:

| Name | Arguments | Answers |
|---|---|---|
| `conversation.create` | `agent`, `title`? | `conversation` |
| `conversation.rename` | `conversation`, `title` | — |
| `conversation.delete` | `conversation` | — |
| `conversation.branch` | `message`, `title`? (left out, "Branch: " and the message's first words) | `conversation`, a new one with the same agent, and `conversation.created` with `branch_of` and `branch_at` ([PLAN.md](../PLAN.md#branching-a-chat)). The agent's first turn in it starts with the chat up to that message, its 40 newest, in a `[loom: …]` block. `bad_request` for a message in the side questions' conversation |
| `messages.list` | `conversation`, `before`? (a message id), `limit`? (up to 100) | `messages`, newest last, and `more` |
| `chat.ask` | `conversation` (a chat), `question` (up to 4,000 bytes) | `reply`, the agent's answer, once it's done ([PLAN.md](../PLAN.md#side-questions)). It's asked in the agent's conversation for side questions (made the first time, with `side: true`), with the chat's 40 newest messages in a `[loom: …]` block, and a tool that asks is refused there. `busy` while the last is being answered. Not for agents |
| `message.pin` | `message`, `pinned`? (`false` takes the pin off) | `message`, as it is now, and `message.updated` to every device ([PLAN.md](../PLAN.md#pinned-messages)) |
| `messages.pinned` | `conversation` | `messages`: the pinned ones, the newest pin first |
| `agent.create` | `name`, `acp_agent`? (left out or empty, the person's default brain; `bad_request` with none), `instructions`, `folder`?, `mcp`? (the names of the person's MCP servers it gets), `permission`? (the permission mode: `ask` asks about everything; `auto` allows the tools that only look, of kind `read`, `search`, `think` and `fetch`, and asks about the rest; `allow` allows everything; or a list of kinds to allow, like `"read,search,edit"`. Left out, the agent has none of its own and follows the person's default, `ask` until they change it ([Agent defaults](#agent-defaults))), `busy`? (`queue` or `steer`; left out, the default's), `color`? (one of the eight thread colours in `design/tokens.toml`; left out, they go round in turn), `code_mode`? (`false` gives the agent every tool plainly, and its MCP servers, in place of [Code Mode](../PLAN.md#code-mode)'s `search_tools` and `run_code`; `true` if left out, as for an agent without it) | `agent`, with `permission` and `busy` only when they're its own |
| `agent.update` | `agent`, and what changes, `code_mode` among them, which its next session follows. `config` sets the agent's options by id, like `{"model": "opus", "effort": "high"}`: a value its program doesn't offer is refused, and `null` (or `""`) takes the option back to following the person's default for its program, or what the program starts on without one. Each session is set to it, over the default, before its next prompt. `permission` and `busy` set its own, and `null` (or `""`) takes either back to following the default ([Agent defaults](#agent-defaults)) | `agent`, with its new `revision`. An agent has `options` once its program has started a session: what it offers as ACP session config options, each `id`, `name`, `category` (`model`, `thought_level`, `mode` or another), `value` (what a new session starts on) and `options` (each `value`, `name` and maybe `description`). When they change, `agent.updated` comes with the agent's id as `by` |
| `memory.list` | `agent`? (that agent's notes and the shared ones; every note without it), `query`? (words, any of them, best match first; newest first without), `limit`? (up to 500, 100 if left out) | `memories`: each `id`, `agent` (empty for a shared note), `shared`, `text`, `by` (the agent or device that kept it) and `at` |
| `memory.put` | `text`, `agent`? (empty or left out: shared by all the person's agents) | `id` |
| `memory.delete` | `id` | — |
| `scripts.get` | — | `on`, `since` (when they were turned on, Unix ms, or null) and `port`, where the instance listens on its computer ([PLAN.md](../PLAN.md#scripts)). The person's alone, from a device |
| `scripts.on` | — | `token`, a new script token, shown this once, and `port`. Only its SHA-256 is kept, and it ends the last one |
| `scripts.off` | — | — . The token stops working |
| `table.list` | — | `tables`: each `name`, `description`, `owner` (the agent that made it, or empty for the person), `at` and `rows`, how many ([PLAN.md](../PLAN.md#shared-tables)) |
| `table.create` | `table` (1 to 64 letters, digits, hyphens and underscores), `description`? (up to 500 bytes) | `table`, as `table.list` gives it, owned by the caller. `exists` for a name that's taken, `limit` past 100 tables |
| `table.write` | `table`, `rows` (1 to 500, each an object of its fields, up to 16 KiB as JSON; with `id`, it changes that row) | `ids`, each row's. Changing a row takes its writer, the table's owner or the person, or it's `forbidden`. `limit` past 100,000 rows in all. Nothing's written unless every row can be |
| `table.rows` | `table`, `where`? (fields and the strings, numbers, `true`, `false` or `null` they equal), `order_by`? (a field, or `id`, the default: the order they were written in), `desc`?, `limit`? (50 if left out, up to 200), `offset`? | `owner`, `rows` (each `id`, `by`, the agent that last wrote it or empty for the person, `at`, `updated` and `data`, its fields; and `untrusted: true` when `by` is an agent that isn't the owner) and `more` |
| `table.delete_rows` | `table`, `ids` | `deleted`, how many. Each row as `table.write` changes it, or none is deleted |
| `table.drop` | `table` | — . The person's alone: an agent's `drop_table` asks them |
| `link.create` | — | `code` (one-time, for [`POST /api/link`](#post-apilink)), `expires` (Unix ms, five minutes on), `instance` and `key` (the instance's id and public key, as `/api/info` gives them), and while phones are on, `noise`: `{"on":true,"port":47420,"key":"<its Noise key, base64>"}` ([Over Noise](#over-noise)). The core adds `qr`, the QR code's text. While phones are on, its address is a `noise://` place ([Over Noise](#over-noise)) with the instance's Noise key and this computer's IPv4 addresses on its port, in the order to try them: the local network's (10/8, 172.16/12, 192.168/16), then Tailscale's (100.64/10). Interfaces that are down, loopback, or networks inside the machine (Hyper-V's and WSL's switches, Docker's and Podman's bridges) are left out. Only a device can ask, never an agent |
| `push.register` | `token` (the device's APNs token, hex), `env` (`production`, or `development` for a debug build). Or, [through the push relay](#through-the-push-relay): `sealed` (the token and env sealed for the relay, base64) and `box` (the phone's X25519 public key, base64). The core sends these in place of `token` whenever `welcome` had a `relay` | Nothing back. Only a device, for itself. A device signed out loses it. A sealed token while the relay is off is refused `not_here` |
| `push.unregister` | — | Nothing back. This device stops taking push |
| `profile.get` | — | `profile`: `text`, `revision`, `by` (the device or agent that wrote it) and `at`, or `null` when there's none yet |
| `profile.put` | `text` (the whole profile, up to 4000 bytes; empty clears it) | `profile`, as `profile.get` gives it, with its new `revision` |
| `defaults.get` | — | `defaults`: `brain`, `config`, `permission`, `instructions`, `busy`, `revision`, `by` and `at` ([Agent defaults](#agent-defaults)) |
| `defaults.put` | What changes, the rest as it was: `brain` (an installed program the person may use, or `null` for none; else `not_found` or `not_allowed`), `config` (`{"claude": {"model": "opus", "effort": "high"}}`: per program, its options by id; an option `null` or `""` goes, and a program `null` loses all of its), `permission` (`ask`, `auto`, `allow` or a list of kinds; `null` is `ask`), `instructions` (up to 8000 bytes; empty clears them), `busy` (`queue` or `steer`) | `defaults`, as `defaults.get` gives them, with the next `revision`, and `defaults.updated` to every device. In the activity log, kind `agent` |
| `routine.list` | — | `routines`, oldest first: each `id`, `name`, `agent`, `prompt`, `schedule` (null for a routine that only runs by webhook), `hook` and `secret` (with a webhook: [`POST /hooks/<id>`](#post-hooksid)), `when` (in words, such as `every 30 minutes, and by webhook`), `tz` (an IANA name), `on`, `notify`, `catch_up`, `next` (when it runs next, Unix ms, or 0 while it's off), `conversation` (where it last posted), `last` (null before its first run, then `at`, `status`: `running`, `done`, `quiet` (its agent had nothing to say: no message, no notification), `failed`, `cancelled` or `skipped`, `run`, and `error`), `failures` (failed runs in a row since its last change; at 5 it turns itself off, with a notification), `flow` (null, or its steps: each `name`, `agent` (the id), `prompt` and `after`), `by` and `at` |
| `routine.put` | To make one: `name` (up to 100 bytes), `agent` (by id or name), `prompt` (up to 20,000 bytes), `schedule` (`{"every": minutes}`, 5 to 10,080; `{"daily": "HH:MM"}`; or `{"weekdays": "HH:MM"}`, Monday to Friday; a time of day on a 24-hour clock in `tz`, summer time included), `webhook` (true for a webhook, which gets a new `hook` and `secret`; false takes it away), at least one of the two, and optionally `tz` (an IANA name, such as `Europe/Oslo`; the device's time zone if left out, or UTC when loom doesn't know the device's; one loom doesn't know is refused), `notify` (false), `on` (true), `catch_up` (true: a run missed while the instance was off happens once when it starts; false: it waits for its next time), `post` (`chat`, its agent's chat, the default; or `thread`, a thread of its own: a conversation marked with the routine, made at its first run and kept for the next, which is never the agent's chat; anything else is `bad_request`) and `flow` (what happens to its answer, [Routine flows](../PLAN.md#routine-flows): up to 20 steps, each `name` (up to 40 bytes, not `routine`), `agent` (by id or name), `prompt` (up to 20,000 bytes) and `after`, the steps it waits for: `routine` or steps before it, the step before it if left out; an empty list takes it off; one that loops back is `bad_request`, and an agent that isn't there `not_found`). To change one: its `id` and what changes | `routine`. Its next run stays unless its schedule or time zone changed |
| `routine.run` | `id` | `run`. It runs now, whether it's on or not, unless its last run is still going (`busy`) |
| `routine.stop` | `id` | —. The routine's running flow stops: the step that's running is stopped and the rest are skipped. `not_running` when it has none |
| `flow.list` | — | `flows`, the routine flows running: each `id`, `routine`, `name` (the routine's), `stopped`, `finished` and `steps`, each `name`, `agent` (the id; the routine's own is first, named `routine`) and `status`: `waiting`, `running`, `done`, `failed` or `skipped` |
| `routine.delete` | `id` | — |
| `task.list` | — | `tasks`, newest first ([Tasks](#tasks)) |
| `task.start` | `agent`, `prompt`, `title`? | `task` |
| `task.pause`, `task.resume`, `task.cancel`, `task.retry` | `id` | Nothing back for `pause` and `cancel`; `run` for `resume` and `retry`. `not_now` when the task isn't in a state for it |
| `skill.list` | `bodies`? (`true` for each skill's text too) | `skills`, by name: each `name`, `description` (when it's for) and `at`, and `body` with `bodies` |
| `skill.get` | `name` | `skill`: `name`, `description`, `body`, `at` |
| `skill.put` | `name` (lowercase letters, digits and hyphens, up to 64), `description`, `body` (Markdown) | `skill`. It reaches every one of the person's agents: through `list_skills` and `read_skill`, and as `.claude/skills/<name>/SKILL.md` in each agent's folder, which Claude agents load by themselves |
| `skill.delete` | `name` | — |
| `plugin.list` | — | `plugins`, by id: each `id`, `name`, `version`, `description`, `panel` (`entry`) or null, `tools` (`command`, `args`) or null, `asks`, `allowed`, `waiting` (what's asked and not allowed), `revision`, `by`, `at` ([Plugins](#plugins)) |
| `plugin.get` | `id` | `plugin`, and `files`: `{path: text}` |
| `plugin.put` | `id` (lowercase letters, digits and hyphens, up to 64, starting with a letter), `files` (`{path: text}`, `plugin.toml` among them; 200 files and 2 MB at most) | `plugin`, a new revision. From a device, what it asks is allowed. From an agent, only what was allowed before and is still asked stays allowed, and the rest waits. `bad_plugin` says what's wrong with the manifest or a path |
| `plugin.allow` | `id` | `plugin`, with everything it asks allowed. Only a device: an agent can't |
| `plugin.revisions` | `id` | `revisions`, newest first, each as `plugin.list` gives it |
| `plugin.delete` | `id` | —. Its panel's data goes too |
| `plugin.data.get` | `plugin`, `key` (1 to 200 characters) | `value`, or null |
| `plugin.data.put` | `plugin`, `key`, `value` (any JSON; null forgets it) | —. A panel keeps 1 MB at most (`too_large`) |
| `plugin.cmd` | `plugin`, `name`, `args`? | The command's `value`, when the person allowed the plugin `cmd:<name>`, or `not_allowed`. It's how a panel's door sends commands |
| `plugin.message` | `plugin`, `agent` (a name or id), `text` (up to 20,000 bytes) | `reply`: the agent's answer, once its run ends, when the person allowed the plugin `agents`, or `not_allowed`. The plugin and the agent keep one conversation, `From <plugin's name>`, where the message is the person's, with `plugin:<id>` as `from`. `busy` while the agent is still answering the last one, and `agent_failed` when its run fails |
| `activity.list` | `agent`?, `before`? (an entry's `id`), `limit`? (up to 200, 50 if left out) | `activity`, newest first: each `id`, `agent`, `kind` (`tool`, `approval`, `agent`, `message`, `notify`, `memory`, `routine`, `device`, `browser` or `import`), `text`, `by` (a device, an agent, `auto` for the permission mode, or empty for the agent itself) and `at`. An action recorded before it happened, kind `tool`, also has `status` (`started`, then `done`, `failed` or `refused`), `input` (what it was given, as JSON text) and `why` (for `failed` and `refused`; a refusal's starts with its rule: `never`, `own-mode`, `no-more-than-its-own`, `not-itself`, `chain-loop`, `chain-depth`, `person-said-no`, `device-tool-off` or `needs-a-chat`), and `ended` once it has. Every call to one of loom's tools that acts is recorded, as its name (`run_command NAME` for a command), and so is each tool call of the agent's own, as its title; a call that only reads (`list_`, `read_`, `search_messages`, `recall`, `query_rows`, `browser_snapshot` and the like, or a command ending `.list`, `.get`, `.search` or `.revisions`) isn't. The person's secrets, in any entry, show only as `[secret NAME: N characters]`, and a member of `input` named like a password or a token as `[N characters]`. One still `started` when the instance stops is marked `failed` when it starts again. It's kept for good, unlike events |
| `activity.export` | `agent`? | `csv`, the activity log as CSV, oldest first, with a header row and the columns in this order: `id`, `at`, `agent` (by name), `kind`, `what`, `by`, `status`, `why`, `input` and `ended`, the times in UTC as RFC 3339; and `entries`, how many. The newest 100,000 at most. An agent's asks the person |
| `server.offer` | `address` (`http://` or `https://`) | `{}`, and `server.offered` to the person's devices. Only from a device: an agent's `add_server` asks the person, and their yes sends it |
| `instance.move` | `to` (the new place's `http://` or `https://` address), `code` (the move code `loom receive` printed there) | `address`, once the instance is there. Only the owner, and not while an agent is answering. Until then, sends and commands get `moving`, and routines wait. The instance goes as `loom export` writes it, in chunks of 4 MB, each sealed with XChaCha20-Poly1305 under a key from the code, which also gives the bearer token for the new place's `POST /api/move/chunk?n=<n>` and `POST /api/move/done`. Then every device gets [`moved`](#moved). `move_failed` says why one didn't happen. An agent's `move_instance` asks the person first |
| `import.scan` | `openbot`?, `grokbot`? (a folder to look in for each, in place of where the app keeps its data on the instance's computer) | `sources`, one for each app: `from` (`openbot` or `grokbot`), `name`, `found`, and once found, `where` (its folder), `counts` (`agents`, `conversations`, `messages`, `files`, `memories`, `routines`, `skills`, `mcp`, `tables` and `rows`) and `text`, the counts in words, such as `2 agents, 3 messages`; or, when not found, `why` ([PLAN.md](../PLAN.md#importing-from-openbot-and-grok-bot)). Only the owner, since it reads the instance's computer (`not_allowed`) |
| `import.run` | `from` (`openbot` or `grokbot`), `path`? (a copy of that app's data folder, such as on a server; left out, where it keeps its data on the instance's computer) | `came` (how many of each kind came now: `agents`, `conversations`, `messages`, `files`, `pins`, `memories`, `routines`, `skills`, `mcp`, `tables` and `rows`), `already` (how many were here from an earlier import, and stayed as they are), `not` (what couldn't come or changed on the way, each in a sentence), `text` (what came, in words), `from` and `where`. The other app's files are only read: its SQLite opened read-only, and its secrets never at all. Each thing is kept with the other app's id, so importing again brings only what's new. Agents come on the agent program named for their provider, or the first one installed, with the permission mode `ask`, `config.model` (a Claude model by its family: `opus`, `sonnet`, `haiku`), the nearest thread colour and their pin; conversations, messages, memories and pins with their own times, and as their events: `agent.created`, `conversation.created` (a thread beside an agent's chat as a branch of it, or as a routine's own), and `message.created` for each conversation's 50 newest, older ones coming with `messages.list`; routines on loom's schedules, with what changed in `not`; skills, MCP servers without their environment and headers, and shared tables and their rows, none of them over one of the person's own of the same name. Recorded in the activity log, kind `import`. `not_found` when the app's data isn't there. Only the owner |
| `instance.phones` | `on`? (true or false) | `on`, and while they are, `port` and `key` ([Over Noise](#over-noise)). With `on`, turns phones on or off: they reach the instance on the network over Noise, and stay on across a restart. Without it, says how things are. Only the owner, from a device, and only on an instance in the desktop app (or `loom serve --lan`): elsewhere `not_here` |
| `instance.relay` | `url`? (the push relay's address, such as `https://push.loom.example`; empty turns it off) | `on`, and while it is, `url` and `key` (the relay's X25519 public key, base64). Anyone sees it. With `url`, the owner, from a device, turns the [push relay](#through-the-push-relay) on or off. The instance asks the address for its key at once, and refuses it `unreachable` when it doesn't answer as a relay. Phones that connect after that seal their tokens for that key |
| `invite.create` | `role`? (`member`, the default, or `admin`), `uses`? (1 to 100, default 1), `days`? (1 to 30, default 7) | `invite`, `code` (for [`POST /api/join/<code>`](#post-apijoincode), shown once), `role`, `uses`, `expires`. Owners and admins, from a device; only the owner makes admins. On an instance in the desktop app, `not_here`: it's its owner's alone |
| `invite.list` | — | `invites`: each open one's `invite`, `role`, `uses` left, `expires`, `by` and `at`, newest first. Owners and admins |
| `invite.delete` | `invite` | — |
| `user.list` | — | `users`: each person's `user`, `username`, `role` and `at`, oldest first. Owners and admins |
| `user.role` | `user`, `role` (`member` or `admin`) | —. The owner only, and never for the owner |
| `user.remove` | `user` | —. Removes the person and everything of theirs, and their sessions close with 4001 once this is answered. The owner removes anyone but the owner, an admin only members, and not while one of their agents is answering (`busy`) |
| `account.delete` | — | —. The person removes their own account, as `user.remove` does. Not the owner |
| `password.reset` | `user` | `code` (for [`POST /api/reset/<code>`](#post-apiresetcode), shown once), `expires` (a day on). The owner resets anyone but the owner, an admin only members. Each new link replaces the person's last |
| `people.log` | `before`? (an entry's `id`), `limit`? (50; up to 200) | `entries`, newest first: each `id`, `kind` (`joined`, `invited`, `role`, `removed`, `left`, `reset`, `password` or `programs`), `text` (what happened, in words, with usernames), `by` (the username of who did it, or empty when it came from the server's command line or the person is gone) and `at`. Owners and admins. It keeps who joined and how, invites made and taken back, role changes, removals, accounts deleted, reset links made and used, accounts and passwords from the command line, and who the owner gives each agent program and its shared settings to |
| `agent.revisions` | `agent` | `revisions`, newest first: each `revision`, `by` (the device or agent that made it), `at`, and `agent` as it was then |
| `agent.file` | `agent`, `path` (as a reply's `mentions` has it) | `file`: the file, kept as the person's, as a `file` block, to fetch with `file.get`. Only from the agent's folder or the shared folder, with links followed and `..` resolved, and 25 MiB at most |
| `agent.summary` | `agent` | What its settings panel counts: `memories` (its own notes), `skills` (the person's, which every agent has), `tables` (the shared tables it owns), `files` and `bytes` (in its folder, but for the skills loom writes there; 20,000 files at most), `routines`, and `tokens_today` and `tokens_month` (what its runs used, from the start of the day and month in UTC) |
| `agent.rollback` | `agent`, `revision` | `agent`: a new revision with that one's name, ACP agent, instructions, folder and colour. The permission mode stays as it is |
| `agent.pin` | `agent`, `pinned`? (`false` takes the pin off) | `agent`, with `pinned`: when it was pinned. Pinned agents go at the top of the list, in that order. Not a revision; `agent.updated` tells every device |
| `agent.delete` | `agent` | — |
| `device.rename` | `device`, `name` | — |
| `device.revoke` | `device` | — |
| `acp.list` | — | `programs`: the agent programs installed on the instance, each with `name`, `command`, `source` (the ACP Registry id it came from, or empty for one added with `loom acp add`), `version`, and `settings`: the names of its settings, never their values, and `share` when the owner shares them ([`acp.share`](#commands)): `tokens`, `per`, and for someone it applies to, the tokens `left` of theirs this day or month. To the owner, a program kept to some people has `members`, their ids, and so does a share kept to some. Anyone else gets only the programs they have ([`acp.members`](#commands)). And `owner`: whether they're the owner |
| `acp.registry` | `again`? (`true` to wait for it to be read again now) | `agents`: the [ACP Registry](https://agentclientprotocol.com/get-started/registry)'s, each with `id`, `name`, `version`, `description`, `website`?, `icon`? (its address), `icon_svg`? (the icon itself, an SVG in currentColor, which the instance fetched with the registry; one that couldn't be fetched or isn't an SVG is left out), `how` (how it would run on the instance: `binary`, `npx`, `uvx`, or null when it can't), and `installed` (the version installed) if it is; `programs`, which registry agent each program on the instance is, by its name (`{"claude": "claude-acp"}`): its own id, or the agent named for it; and `platform`, such as `windows-x86_64`. The instance answers at once with what it last read, which it keeps in `acp/registry.json` across restarts, and once that is 10 minutes old, reads it again behind the answer, for the next one. The first time, or with `again`, the answer comes once it has been read. It fetches every icon at once |
| `acp.install` | `id` (a registry id), `env`? (settings to give it, `{NAME: value}`) | `program`, as `acp.list` gives it, once it's installed. A binary is downloaded into the instance's `acp/<id>/`, checked against the registry's SHA-256 and unpacked; an npx or uvx agent is run with `npx -y <package>` or `uvx <package>`. Installing one that's installed updates it, keeping its settings. Only the owner installs. `install_failed` says why it didn't |
| `acp.settings` | `name`, `env` (`{NAME: value}` sets one, `{NAME: null}` removes it) | `program`. Settings are the environment variables the program starts with: an API key, a model. They're kept sealed under `secret.key`, as secrets are, and only their names come back. Agents that aren't answering start again with them at their next message. Only the owner |
| `acp.share` | `name`, `tokens` (each person's budget, or 0 to stop sharing), `per`? (`day`, the default, or `month`, in UTC), `members`? (people's ids, or null for everyone; without it, who it goes to stays as it was) | `program`. Shares the program's settings with everyone but the owner, or only the `members`, whose agents otherwise get only their own secrets ([Sandboxes](#sandboxes)). A person with their own secret for every one of the settings uses theirs, with no budget. Otherwise each turn on the shared settings counts the tokens its program reports (ACP's `usage`), and once a person's add up to `tokens` this day or month, their runs on the program finish `limit` until it begins again. The turn that crosses it finishes. Agents that aren't answering start again at their next message. Each change goes in the people log. Only the owner |
| `acp.members` | `name`, `members` (people's ids, or null for everyone) | `program`. Gives the program to everyone, the default, or only the `members`. Anyone else doesn't see it in `acp.list` and can't make an agent on it or switch one to it (`not_allowed`), and a run of an agent of theirs already on it finishes `failed`, saying the owner hasn't given it to them. The owner always has every program. Agents that aren't answering start again at their next message. Each change goes in the people log. Only the owner |
| `acp.remove` | `name` | —. Removes the program and, for one installed from the registry, its folder. Only the owner |
| `mcp.list` | — | `servers`: the person's MCP servers, each with `name`, and `command`, `args` and `env`, or `url` and `headers`. A value in `env` or `headers` is text, or `{"secret": NAME}` |
| `mcp.put` | `name` (lowercase letters, digits and hyphens; not `loom`), and `command` with `args`? and `env`?, or `url` (http or https) with `headers`? | `server`. Each agent whose `mcp` names it gets it in its next new session, with its secrets filled in, beside loom's own. One over `url` goes only to agent programs that offer `mcpCapabilities.http` |
| `mcp.delete` | `name` | — |
| `secret.list` | — | `secrets`: the person's secrets, each `name` and `at`. Never their values |
| `secret.put` | `name` (letters, digits, `_`, `-` and `.`), `value` | `name`. It's sealed with XChaCha20-Poly1305 under the instance's `secret.key`, bound to the person and the name, and opened only to start an agent's MCP server, or to fill it into an agent's browser (`browser_fill_secret`) |
| `secret.delete` | `name` | — |
| `messages.search` | `query`, `agent`?, `after`? and `before`? (Unix ms), `limit`? (up to 100, 30 if left out) | `results`, best first, each with its `message` and a `snippet`: the words around the match, with each matched word between U+0002 and U+0003. Every word of `query` must be in the message, the last as a prefix, so it works while typing. It searches text blocks and file names, in conversations that aren't deleted. A person's message counts as its conversation's agent's |

Arguments marked `?` are optional. "—" means `value` is `{}`.

## Plugins

A plugin is text files, one of them `plugin.toml`, its manifest (PLAN.md, "Plugins"):

```toml
name = "Build board"
version = "0.1.0"
description = "Shows how the builds are doing."

[panel]
entry = "index.html"          # a small web app, in a web view of its own

[tools]
command = "calx"              # an MCP server the instance runs for every agent
args = ["tools.calx"]         # an arg naming one of its files is that file

[permissions]
commands = ["routine.list"]   # commands the panel may send
agents = true                 # the panel may message the person's agents
```

What it asks for is its `asks`: `tools` when it has a tool server, `agents`, and `cmd:<name>` for each command. A panel never gets the commands that would let it allow itself more, borrow another plugin's permissions, run programs on the instance, or reach the person's devices, secrets and sign-in: `plugin.put`, `plugin.allow`, `plugin.delete`, `plugin.cmd`, `plugin.message`, `instance.move`, `instance.phones`, `instance.relay`, `invite.create`, `invite.delete`, `user.role`, `user.remove`, `account.delete`, `password.reset`, `link.create`, `push.register`, `push.unregister`, `device.revoke`, `device.rename`, `secret.list`, `secret.put`, `secret.delete`, `mcp.put`, `acp.install`, `acp.remove`, `acp.settings`, `acp.share`, `acp.members`, `import.scan` and `import.run`. Until its person allows something, it waits, and the plugin goes without it.

- **A tool server** the person allowed is in every one of their agents' new sessions, as the MCP server `plugin-<id>`. It runs from a folder holding the revision's files, with `LOOM_PLUGIN_DIR` set to that folder.
- **A panel** talks to the app through one door, `await loom.send(op, args)` (desktop/src/panel.rs). The app sends these on, and refuses everything else, and the instance checks what's allowed:

| `op` | `args` | Allowed |
|---|---|---|
| `data.get` | `key` | Always: `plugin.data.get` for this plugin |
| `data.put` | `key`, `value` | Always: `plugin.data.put` for this plugin |
| `cmd` | `name`, `args` | When `cmd:<name>` is allowed: `plugin.cmd`. The command's `value` comes back |
| `message_agent` | `agent`, `text` | When `agents` is allowed: `plugin.message`. The agent's reply comes back as `reply` |

## Errors and close codes

`error` frames and failed `result`s share their codes. The common ones:

| Code | Means |
|---|---|
| `bad_frame` | Not JSON, or a field has the wrong type |
| `unknown_frame` | A `t` this version doesn't have |
| `unknown_command` | A `cmd` whose `name` this version doesn't have |
| `not_found` | An id that doesn't exist, or isn't this person's (the two look the same) |
| `already_answered` | An approval someone already answered |
| `not_allowed` | This person can't do that |
| `busy` | `acp.install` for an agent that's being installed already, or a `task.resume` or `task.retry` for a task its agent is working on already |
| `not_now` | A `task.pause`, `task.resume`, `task.cancel` or `task.retry` for a task that isn't in a state for it |
| `unsupported` | `acp.install`: the agent has no build for the instance's computer, and npx or uvx, which it could run with otherwise, isn't there |
| `install_failed` | `acp.install`: the download, its check or unpacking failed. The message says which |
| `registry_unavailable` | `acp.registry`: the ACP Registry couldn't be read |
| `unknown_file` | A `send` names a file this person hasn't uploaded |
| `storage_full` | A file would take the person over the space the owner gives each person |
| `moving` | The instance is moving: nothing changes until it's at the new place |
| `moved` | `instance.move` on an instance that has moved already |
| `move_failed` | `instance.move`: the export, sending it, or the new place failed. The message says which, and the instance goes on where it is |
| `not_here` | `instance.phones` on an instance on a server, which phones reach at its own address, or `push.register` with a sealed token while the push relay is off |
| `unreachable` | `instance.relay`: the address didn't answer, or isn't a push relay |
| `internal` | A bug in the instance. It's logged there |

WebSocket close codes:

| Code | Means | The app |
|---|---|---|
| 1000 | Closed on purpose (the app quit, or logged out) | — |
| 1001 | The instance is restarting or stopping | Reconnects |
| 1009 | A frame over 1 MiB | Reconnects, and reports a bug |
| 4000 | The first frame wasn't `hello`, or the session broke the protocol | Reconnects, and reports a bug |
| 4001 | The token is wrong or was revoked | Logs out on this device, and asks for a login |
| 4002 | No protocol version in common | Says the app needs an update, or the instance does |
| 4003 | Too slow: the app didn't read fast enough and the instance's queue for it filled up | Reconnects |
| 4004 | The instance moved. A [`moved`](#moved) frame came just before | Checks it, and connects at the new address |
| 4005 | Over Noise: the handshake failed, or a message didn't open | Checks the Noise key it has, and reconnects |

## Over Noise

A phone reaches an instance in the desktop app ([PLAN.md: Reaching an instance in a desktop app](../PLAN.md#connecting-devices)) without TLS, so the connection is encrypted end to end with a [Noise](https://noiseprotocol.org/noise.html) handshake against the instance's Noise key. The instance itself listens only on 127.0.0.1. Once the owner turns phones on (`instance.phones`), a bridge listens on every address too, on port 47420 if it's free and on the same port after that, and takes only one route there:

- **`GET /ws/noise`**, a WebSocket of binary messages. Any other request on that port gets 404 with `noise_only`, `/ws` and `/api/info` included, and a text message closes the socket with 4000.
- **The handshake** is `Noise_NK_25519_ChaChaPoly_SHA256` with the prologue `loom noise 1`: the phone knows the instance's static key from the QR code, and has none of its own, since its device token says who it is. The phone sends the first message and the instance answers with the second, each one binary WebSocket message with an empty payload. If it fails, the socket closes with 4005.
- **After it**, each binary message is one Noise transport message, sealed with the handshake's keys in order: a piece of a message. A piece is a flag byte, `1` if more pieces of the same message follow and `0` on its last, then up to 65,518 bytes of it. A message is one kind byte, then:

| Kind | Holds | Means |
|---|---|---|
| `f` | A frame's JSON, as `/ws` carries it | A frame of the WebSocket session, both ways. The first one the phone sends opens its session, so `hello` goes first, as on `/ws` |
| `q` | `{"id":1,"method":"POST","path":"/api/link","headers":{"Content-Type":"application/json"}}`, a newline, the body | An HTTP request from the phone to one of the instance's routes, `GET` or `POST` but not `/ws`, with the device token in `Authorization` as over HTTP. `id` is the phone's own |
| `r` | `{"id":1,"status":200,"headers":{"Content-Type":"…"}}`, a newline, the body | The instance's answer to request `id`, with its `Content-Type` and any `Content-Disposition`. Requests are answered as they finish, not in order |

So a phone joins with `POST /api/link`, uploads and fetches files, and runs its session, all on the one socket. A message goes up to 64 MB and a bit more, for a file and its head. When the session ends, as with a 4001 or 4004, the socket closes with the same code and reason. When phones are turned off, or the instance stops, it closes with 1001.

The instance's Noise key is X25519, made from the same seed as its Ed25519 key, so it moves with the instance. `link.create` gives its public half, with the port, while phones are on.

**Found by mDNS.** While phones are on, the instance also says where it is on the local network with mDNS and DNS-SD: the service `_loom._tcp`, named `loom` (or `loom (2)` and so on), on its Noise port, with its Noise public key, hex, as the TXT property `key`. A phone whose addresses from the QR code no longer answer looks for the service whose `key` is the one it has, at most every 30 seconds, and tries its addresses: 192.168/16 and 10/8 first, then 172.16/12, then Tailscale's 100.64/10. Anyone on the network can see the key, which is public, and the handshake still needs the instance to hold its private half.

## Push

A phone that registered with `push.register` gets push through APNs while it isn't connected, once the owner has set up the APNs key with `loom push key`. Without a key, nothing is sent. The instance sends each one as an HTTP/2 `POST /3/device/<token>` to `api.push.apple.com`, or `api.sandbox.push.apple.com` for a token whose `env` is `development`, with `apns-push-type: alert` and the bundle id as `apns-topic`. Its provider token is a JWT signed ES256 with the key, naming the key's id (`kid`) and the team (`iss`), made again every 50 minutes. A token APNs answers 410, `BadDeviceToken` or `DeviceTokenNotForTopic` for is forgotten.

| Pushed for | When | `aps.alert` | `aps.category` | `loom` |
|---|---|---|---|---|
| `notify` | To each phone that isn't connected, even while the person is on another device | `title`, the agent's name as `subtitle`, `body` | — | `kind: "notify"`, `agent`, `conversation` (for a routine's) |
| `approval.needed` | The same | "<agent> asks first" as `title`, the tool call's title as `body` | `approval`, which has Allow and Deny | `kind: "approval"`, `run`, `call`, `conversation`, `can_always` |
| An agent's finished reply (`message.created`) | When the person is on none of their devices, for a run they started (not a routine's, nor another agent's) that ended `done` or `failed` | The agent's name as `title`, the start of the reply as `body` | — | `kind: "reply"`, `agent`, `conversation`, `message` |

Each one's `aps.thread-id` is its conversation, so the lock screen groups them, and its `aps.sound` is `default`. An approval's `apns-collapse-id` is its `call`. The app answers Allow and Deny with an `approve` frame, the same as on screen, once it has connected.

### Through the push relay

An instance without an APNs key of its own can push through the relay ([PLAN.md](../PLAN.md#the-push-relay-opt-in), [push/](../push/README.md)), once its owner turns it on with `instance.relay`. Neither the instance nor the relay can read what the other has.

1. **The phone's token is sealed for the relay.** `welcome` gives the relay's key as `relay`. When the app sends `push.register` with its `token`, the core seals `{"token": "<hex>", "env": "production"}` for that key. It sends that as `sealed`, with its own `box` key beside it. The instance keeps only these, so it never sees the token. A device that can't keep a box key sends nothing, and the app hears an `error` with the code `keyring`.
2. **Each notification is sealed for the phone.** For a phone with a `box`, the instance seals the notification it would have sent APNs (the table above) for that key. It hands the relay `{"aps": {"alert": {"title": "loom", "body": "Something new"}, "mutable-content": 1, "sound": "default", "thread-id"?, "category"?}, "sealed": "<base64>"}`. APNs and the relay see only that something is new, and in which conversation. The app's notification service extension opens `sealed` with `loom_push_open` and shows what's in it.
3. **The instance signs each request.** `POST <relay>/v1/push` with the body `{"token": "<sealed>", "payload": <the notification>, "collapse": "<apns-collapse-id or empty>"}`, and three headers:
   - `X-Loom-Instance`: its Ed25519 public key, base64.
   - `X-Loom-Time`: Unix ms.
   - `X-Loom-Signature`: base64, its Ed25519 signature over `loom push 1\n<time>\n<SHA-256 of the body, hex>`.

   The relay refuses a request more than five minutes from its clock or not signed by that key with 401, and one past its limit an hour for that key with 429. It opens the token, sends the notification to APNs with its own key, and passes APNs's answer back: 200, or 410 when the token is gone, which the instance forgets as it does a direct one.

A sealed box is the sender's one-time X25519 public key (32 bytes), a 24-byte nonce, and the message sealed with XChaCha20-Poly1305. Its key is HMAC-SHA256, keyed with the X25519 shared secret, over the sender's public key and then the recipient's, and the associated data is `loom seal 1` ([shared/seal.calx](../shared/seal.calx)). The phone's box key stays in the system's credential store, under the core's keyring service, as `push box`.

## Versions

- **The app offers, the instance picks.** `hello` lists every version the app speaks, and `welcome` names the one the connection uses: the newest both have.
- **An instance speaks its newest version and the one before**, so an app one release behind still works.
- **Fields are added without a new version**, since unknown fields are ignored. A new version is only for a change an older app would get wrong.
- `GET /api/info`'s `protocol` lists the versions too, so an app can say "this instance needs an update" before it logs in.

## How ACP maps onto loom

Inside the instance, each agent runs as an ACP agent program ([PLAN.md](../PLAN.md#agents-run-on-acp)). The apps never see ACP. What the agent says becomes loom's own frames:

| From the agent (ACP) | In loom |
|---|---|
| The instance sends the person's message to the session with `session/prompt` | `run.started` |
| Every session gets `mcpServers` with one server, `loom`: the `loom` program (or the desktop app's own `loom.exe`) with the argument `mcp`, and `LOOM_URL` and that agent's `LOOM_MCP_TOKEN` in its environment. In Code Mode, the default ([PLAN.md](../PLAN.md#code-mode)), `LOOM_CODE_MODE=1` is there too, and it's the only server: it offers only `search_tools` (`query`?, `limit`? up to 30: the TypeScript signatures of the tools that match, or every tool's name without a query) and `run_code` (`code`, `timeout`? in seconds, up to 300: runs the script in Deno with no permissions, and gives what it logged and returned, or `isError` with why it failed), through which the agent reaches the tools below, its MCP servers' and its person's plugins'. A call by one of the tools' own names still works. With `code_mode: false` on the agent, the session also gets the agent's MCP servers and the plugins' tool servers, and `loom mcp` lists the tools below itself. It gives the agent loom's own tools: `message_agent`, `list_agents`, `create_agent`, `update_agent`, `edit_instructions`, `list_acp_agents`, `search_messages`, `list_skills`, `read_skill`, `save_skill`, `notify`, `remember`, `recall`, `forget`, `update_profile` (writes the person's whole profile anew), `list_devices`, `use_device` ([Devices as tools](#devices-as-tools)), `browser_open`, `browser_snapshot`, `browser_click`, `browser_type`, `browser_press`, `browser_scroll`, `browser_back`, `browser_screenshot`, `browser_ask_person`, `browser_fill_secret` and `browser_ask_secret` ([The agent's browser](#the-agents-browser)), `schedule` and `list_routines` (`routine.put` and `routine.list`; a routine is for the calling agent unless it names another), `start_task` ([Tasks](#tasks)), `list_plugins`, `read_plugin` and `save_plugin` ([Plugins](#plugins)), `list_commands` and `run_command` ([Agents run commands](#agents-run-commands)), `install_server` (the command that puts loom on a new server), `delete_agent`, `set_permission`, `delete_skill`, `delete_routine`, `delete_plugin`, `move_instance` and `add_server` (`server.offer`). They act with the person's rights. The last seven ask the person every time, whatever the permission mode: the call shows as a tool (`delete`, `move` or `other`) in the run that made it, with `approval.needed` (`can_always: false`), and is answered once the person does. With no run going, they refuse; an agent can't delete itself or loosen its own mode, or give another a looser mode than its own (ask, then auto, then allow). `message_agent` takes `agent` and `text`, `task`, `bounds` and `good_answer`, the last three each under its heading (`The task: …`, `Its bounds: …`, `A good answer: …`), and `files`: up to 20 paths, of 25 MiB each at most, in the agent's own folder or the shared folder, with links followed and `..` resolved. Each is kept as the person's file and goes with the message as a `file` block, as the person's attachments do; a path anywhere else is refused. A chain of asks, an asked agent asking another, goes at most four deep and never back to an agent already in it; the tool refuses with the chain's names | what the tools do: `message.created`, `agent.updated`, `notify` and the rest, as if the person had done it. A yes is done as the device that gave it, so `by` names it |
| A conversation's first message to an agent program: `session/new`, or after a restart, `session/resume` (where the agent offers `sessionCapabilities.resume`) or `session/load` (where it offers `loadSession`) with the session the conversation had. What `session/load` plays back is dropped, and if the agent can't pick the session up, a new one starts | nothing: the apps see the same conversation |
| Where the person is writing from, in that prompt | a first text block, `[loom: This message comes from … It's 2026-11-21 13:05 UTC.]`, with the device's name, what it is and its time zone, the agent that sent it with `message_agent`, or the routine that sent it (`the person's routine "Morning brief", run every 30 minutes`), whose message has the routine's id as `from`. It comes when the session hasn't been told yet, or when it changes, never in the instructions |
| The shared folder, in a session's first prompt | a text block, `[loom: Your person's agents share the folder PATH. …]`, once a session: `shared/USER` in the data folder, or `shared` in a sandboxed person's own folder, made when it's first given. Every agent of the person reads and writes it, and `message_agent` hands over files from it ([PLAN.md](../PLAN.md#sharing-and-learning)) |
| A fresh session for a full one | when the agent's last `usage_update` (`used` of `size` tokens) says its session is 80% full or more, the conversation's next run starts a new session, whose prompt begins with a text block, `[loom: Your chat with your person goes on from an earlier session, which grew too long, so this one starts fresh. … The newest of it, newest last: … For anything older, search_messages finds it.]`, with the chat before the run's message. The activity log has it, as `session` ([PLAN.md](../PLAN.md#agents-run-on-acp)) |
| The instructions every agent follows, in that prompt | a text block, `[loom: Instructions your person gives every agent of theirs. Follow them, as you do your own.` and the defaults' `instructions` `]`, after the shared folder's. It comes when the session hasn't had them as they are now: in the first prompt, and after they change. When they're cleared, a block says they were taken back |
| The person's profile, in that prompt | a text block, `[loom: About the person, as they and their agents keep it. update_profile changes it.` and the profile's text `]`, after the one above. It comes when the session hasn't had this revision yet: in the first prompt, and after the person or an agent changes it |
| The person's files, in that prompt | an `image` block for an image of up to 5 MB, when the program says it takes images (`promptCapabilities.image`); otherwise a `resource_link` to a copy in the agent's folder, under `files/`, with its name |
| `session/update`, `agent_message_chunk` | `delta` with `kind: "text"` |
| `session/update`, `agent_thought_chunk` | `delta` with `kind: "thought"` |
| `session/update`, `agent_message_chunk` with an `image`, a `resource` with a `blob`, or a `resource_link` to a `file://` on the instance's machine | a `file` block: the file is kept as the person's, as if they'd uploaded it |
| `session/update`, `tool_call` | a `tool` block, and `tool.started` |
| `session/update`, `tool_call_update` | `tool.updated`, or `tool.finished` when its status is `completed` or `failed` |
| `session/update`, `plan` | a `plan` block (replaced whole each time) |
| `session/request_permission` | `approval.needed`, or answered at once by the agent's permission mode, with `approval.answered` `by: "auto"` |
| The person's `approve` | the permission request's answer, with the `allow_once`, `allow_always`, `reject_once` or `reject_always` option that matches `allow` and `always` |
| The person's `cancel` | `session/cancel` |
| `session/prompt`'s result, with its `stopReason` | `message.created` for the reply, then `run.finished`: `end_turn` is `done`, `cancelled` is `cancelled`, `max_tokens` and `max_turn_requests` are `limit`, and an error or a crash is `failed` |

### Sandboxes

Only the owner's agents run as the instance's own user. Everyone else's run in a sandbox of their own ([PLAN.md](../PLAN.md#sandboxes-where-members-agents-run)), on an instance started with `loom serve --sandboxes podman` or `--sandboxes docker`. On one without, their runs are `failed`, saying so.

- **The sandbox** is a container, `loom-<user>`, from loom's image (`ghcr.io/aaen-studios/loom`, or `--sandbox-image`), made when the person's first agent starts and kept running. It sees the person's folder, `sandboxes/<user>/` in the data folder, and the instance's downloaded agent programs, `acp/`, read-only, both at the same paths as outside, and nothing else of the instance's. It has the internet, but none of the server's loopback but the gate:
  - **In rootless Podman,** pasta gives it the internet and the gate's port on 127.0.0.1.
  - **In Docker,** it's on a network of loom's own, `loom`, and reaches the gate at that network's gateway. It runs as the instance's own user, not root, with no capabilities and `no-new-privileges`. It can reach whatever else the server listens on at that gateway, so the owner keeps everything but the gate off it.
- **Its limits** are 2 GB of memory, 2 CPUs and 512 processes, where the system lets the instance's user set them. Its own layer can hold at most 10 GB, where the storage allows a size (overlay on XFS with project quotas). The person's folder is kept to 10 GB too: a run doesn't start while it holds more, and is `failed`, saying so.
- **The agent program** runs in it with `exec`, in `sandboxes/<user>/agents/<agent>/`, whatever `folder` the agent names. Its environment is the person's own secrets, each by its name, never the program's settings, which are the owner's, unless the owner shares them with a budget ([`acp.share`](#commands)). Their own secrets always win. Once a secret changes, their agents that aren't answering start again with it at their next message.
- **Loom's tools** reach it as the image's `loom mcp`, with `LOOM_URL` set to the gate: a listener on 127.0.0.1 (or Docker's gateway), which passes on only `GET /api/mcp/tools`, `/api/mcp/servers` and `/api/mcp/deno` and `POST /api/mcp/call`, `/api/mcp/allow` and `/api/mcp/done`, and answers anything else 404. The person's MCP servers run in the sandbox too, started by `loom mcp` in Code Mode or by the agent without it, and so do the scripts, in the Deno the sandbox sees in `acp/`. Plugins' tool servers, which run from the instance's own folders, aren't given to sandboxed agents.
- **Files** go in and out through the sandbox. The instance writes an agent's instructions, skills and the person's files into its folder with `exec`, and reads a `file://` link the agent sends the same way, so a link the agent makes can't lead the instance to its own files.
- **Removing** the person removes their sandbox and their folder.

## One message's whole trip

Elliot asks an agent, from the desktop app, whether the build passed. The agent checks with a shell command, which needs his approval. While it's answering, the PC's Wi-Fi drops for a few seconds.

**1. Writing it.** The UI hands the core `{"t":"send", …}`. The core gives the message a client id, `m_4f2`, stores it in its outbox and its cache (shown as "sending"), and sends it:

```
→ {"t":"send","cid":"m_4f2","conversation":"c_9","text":"Did the build pass?"}
```

**2. Stored.** The instance stores the message and starts a run, then answers. The `ack` and the events can arrive in either order; the core matches them by `cid` and ids.

```
← {"t":"ack","cid":"m_4f2","message":"msg_310","run":"run_77"}
← {"t":"event","seq":1042,"kind":"message.created","at":1795187131000,"conversation":"c_9",
   "message":{"id":"msg_310","conversation":"c_9","role":"person","from":"d_3","at":1795187131000,
              "blocks":[{"kind":"text","text":"Did the build pass?"}]}}
← {"t":"event","seq":1043,"kind":"run.started","at":1795187131200,"run":"run_77","conversation":"c_9",
   "agent":"a_2","revision":4,"message":"msg_311","answering":"msg_310"}
```

The core drops `m_4f2` from its outbox, and stores `seq` 1043 as its cursor.

**3. The agent starts answering.** The instance sent the message to the agent's ACP session with `session/prompt`. Its `agent_message_chunk`s become deltas:

```
← {"t":"delta","run":"run_77","message":"msg_311","block":0,"kind":"text","pos":0,"text":"Let me check"}
← {"t":"delta","run":"run_77","message":"msg_311","block":0,"kind":"text","pos":12,"text":" the last run on main."}
```

**4. A tool, and a question.** The agent wants to run `gh run list`. Its permission mode is `ask`, so the instance asks Elliot:

```
← {"t":"event","seq":1044,"kind":"tool.started","at":1795187132000,"run":"run_77","call":"tc_3",
   "tool":"execute","title":"gh run list --branch main --limit 1","input":{"command":"gh run list --branch main --limit 1"}}
← {"t":"event","seq":1045,"kind":"approval.needed","at":1795187132010,"run":"run_77","call":"tc_3",
   "tool":"execute","title":"Run a command","detail":"gh run list --branch main --limit 1","can_always":true}
```

His phone gets a push with Allow and Deny (Phase 5). He answers on the PC:

```
→ {"t":"approve","call":"tc_3","allow":true,"always":false}
← {"t":"event","seq":1046,"kind":"approval.answered","at":1795187135000,"call":"tc_3","allow":true,"always":false,"by":"d_3"}
```

**5. The Wi-Fi drops.** The command runs on the instance. While it does, the PC loses its connection. The core notices (the socket closes, or 45 seconds pass with nothing) and starts reconnecting: 0.5 s, 1 s, 2 s. Meanwhile, on the instance:

- the tool finishes: `seq` 1047, `tool.finished`;
- the agent writes more of its reply, in block 2: "Yes. The last build on main" so far.

The instance can't send any of it to the PC, and doesn't need to. The event is in the log, and the reply so far is kept with the run.

**6. Back online.** The core's third try gets through. Its cursor is 1046, the last event it stored.

```
→ {"t":"hello","token":"…","versions":[1],"after":1046,"device":{"active":true}}
← {"t":"welcome","version":1,"user":"u_1","device":"d_3","seq":1047,"reset":false}
← {"t":"event","seq":1047,"kind":"tool.finished","at":1795187136000,"call":"tc_3","status":"done",
   "output":[{"kind":"text","text":"completed  success  CI  main  push  2m3s  14:02"}]}
```

Then the reply so far, from `pos` 0, one delta per block. Block 0 is text the core already has, so it skips it. Block 1 is the tool call, which doesn't stream. Block 2 is new:

```
← {"t":"delta","run":"run_77","message":"msg_311","block":0,"kind":"text","pos":0,"text":"Let me check the last run on main."}
← {"t":"delta","run":"run_77","message":"msg_311","block":2,"kind":"text","pos":0,"text":"Yes. The last build on main"}
← {"t":"synced","seq":1047}
```

**7. The rest, live.**

```
← {"t":"delta","run":"run_77","message":"msg_311","block":2,"kind":"text","pos":27,"text":" passed at 14:02, in 2 minutes."}
```

**8. Finished.** The agent's `session/prompt` returns with `stopReason: "end_turn"`. The instance stores the whole reply, and ends the run:

```
← {"t":"event","seq":1048,"kind":"message.created","at":1795187140000,"conversation":"c_9",
   "message":{"id":"msg_311","conversation":"c_9","role":"agent","agent":"a_2","run":"run_77","at":1795187140000,
              "blocks":[{"kind":"text","text":"Let me check the last run on main."},
                        {"kind":"tool","call":"tc_3"},
                        {"kind":"text","text":"Yes. The last build on main passed at 14:02, in 2 minutes."}]}}
← {"t":"event","seq":1049,"kind":"run.finished","at":1795187140100,"run":"run_77","status":"done",
   "usage":{"input":5210,"output":118}}
```

The core replaces what the deltas built with the finished message, stores `seq` 1049, and the UI shows the reply as done.

**Had the app been closed** through all of this, it would have connected with `after: 1043`, got 1044 to 1049 as events, and no deltas, since the run was over. It ends up with the same cache. Any of Elliot's other devices that was open the whole time got every frame above as it happened, apart from the `ack`, which only goes to the device that sent the message.

## Later versions

Fields and frames that come with later phases. Where they only add, they come within version 1.

- **Device tools** (Phase 3): the instance sends `{"t":"device.call", …}` with the request, and the signed approval when there is one, and the device answers `{"t":"device.result", …}`. Each device offers its tools in `hello`.
- **Noise** (Phase 6): done, within version 1 ([Over Noise](#over-noise)).
