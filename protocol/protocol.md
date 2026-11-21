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
- [The event log](#the-event-log)
- [Messages and streaming](#messages-and-streaming)
- [Events](#events)
- [Commands](#commands)
- [Plugins](#plugins)
- [Errors and close codes](#errors-and-close-codes)
- [Versions](#versions)
- [How ACP maps onto loom](#how-acp-maps-onto-loom)
- [One message's whole trip](#one-messages-whole-trip)
- [Later versions](#later-versions)

## Transport

- **HTTP/1.1** for what isn't a live session: setup, login, invites, files and webhooks.
- **One WebSocket** (RFC 6455) per device for everything live, at `/ws`.
- **TLS** comes from whatever is in front of the instance (Caddy, a Cloudflare tunnel). The instance itself listens on 127.0.0.1. A desktop instance's own app connects over loopback without TLS. Phones reaching a desktop instance use a Noise handshake ([Later versions](#later-versions)).
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
  "setup": false
}
```

`setup` is `true` until the owner account exists. `key` is checked on every later connection ([PLAN.md: Connecting devices](../PLAN.md#connecting-devices)).

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

An invite (Phase 10). The same body and answer as `/api/setup`, without `code`. Errors: `bad_code`, `expired`, `used_up`, `username_taken`.

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

For `loom mcp` alone, which the instance starts for each agent (below, [How ACP maps onto loom](#how-acp-maps-onto-loom)). They take that agent's tool token, `Authorization: Bearer <token>`, not a device's, and an app never calls them. `tools` gives `{"tools": [...]}`, MCP's tool definitions from `shared/tools.calx`. `call` takes `{"name", "arguments"}` and gives an MCP tool result, `{"content": [{"type": "text", "text"}], "isError"?}`. A `message_agent` call is answered when the other agent's reply is done.

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
{ "t": "welcome", "version": 1, "user": "u_1", "device": "d_3", "seq": 1046, "reset": false }
```

- `version`: the one version this connection uses.
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

`run` is the run it started, if the conversation's agent was asked to answer, which it always is in version 1.

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

### `error`

Something the app sent couldn't be done. The socket stays open.

```json
{ "t": "error", "code": "unknown_conversation", "message": "There's no conversation c_44.", "ref": "m_4f2" }
```

`ref` is the `cid`, `call` or `run` of the frame it answers, when there is one.

## Devices as tools

The person's devices are tools for their agents while their apps run ([PLAN.md](../PLAN.md#your-devices)). The device decides, not the instance:

- **Each tool is off until the person turns it on, on that device**: `off`, `ask` each time, or `allow`. The device tells the instance (the `device` frame), so agents see it in `list_devices`, and checks it again itself before it acts on a `device.call`.
- **A tool that asks** shows in the agent's chat as `approval.needed` with `device` (the device's id) and `request`, and `can_always: false`. A yes is signed by the device it's given on (`approve`'s `approval`). The device doing the work acts only on its own key's signature of the exact request. A yes from another device, or one the instance makes up, is refused, and the agent hears so. Approving on one device for another comes with linking devices to each other, which exchanges their keys without the instance in between.
- **A request is done once.** The device keeps the calls it did, and refuses a request more than 15 minutes from its clock.
- **The tools:** `run_command` (`command`, in PowerShell on Windows and zsh on a Mac, up to 2 minutes), `read_file` (`path`), `list_files` (`path`, folders with a `/`), `open` (`target`, a file or a link, with its usual program), `screenshot` (the main screen, as a JPEG up to 1568 pixels on its longest side), `read_clipboard` (its text) and `write_clipboard` (`text`). Output past 20,000 bytes is cut.
- **A screenshot** comes back in `device.result` as `image` (base64) and `mime`. The agent's tool call gets it as an MCP `image`, and the chat keeps it as a file block in `tool.finished`'s `output`.
- Each call goes in the activity log as kind `device`, by the device that did it.

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

- `role` is `person` or `agent`. A person's message has `from` (a device id) instead of `agent` and `run`. A message one agent sent another with `message_agent` is a `person` message too, in the other agent's conversation, with the sending agent's id as `from`.
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
| `conversation.created` | `conversation` (the whole conversation: `id`, `agent`, `title`, `at`) | A conversation is made |
| `conversation.updated` | `conversation` (`id` and what changed) | Renamed, or its agent changed |
| `conversation.deleted` | `conversation` (the id) | Deleted |
| `message.created` | `conversation`, `message` (the whole message) | A person's message is stored, or an agent's reply is finished |

### Runs

A run is one turn of an agent, answering one message.

| Kind | Fields | When |
|---|---|---|
| `run.started` | `run`, `conversation`, `agent`, `revision`, `message` (the reply's id), `answering` (the person's message id) | The agent was given the message |
| `run.finished` | `run`, `status` (`done`, `cancelled`, `failed` or `limit`), `error` (for `failed`), `usage` (`input`, `output`, `cost` where the agent reports them) | The turn is over, after `message.created` for its reply |

### Tool calls

| Kind | Fields | When |
|---|---|---|
| `tool.started` | `run`, `call`, `tool` (a kind: `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, `other`), `title`, `input`, `locations` (files and lines it touches) | The agent started a tool |
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
| `notify` | `agent`, `title`, `body`, and for a routine's reply, `conversation` | An agent sent its person a notification with its `notify` tool, or a routine with notify on finished a run (its name as `title`, the start of the reply as `body`). The apps show it, even while someone's looking |
| `agent.deleted` | `agent` (the id) | Deleted |
| `routine.updated` | `routine` (the whole routine, as `routine.list` gives it) | Made, changed, or run: `last` says how its newest run is going |
| `routine.deleted` | `routine` (the id) | Deleted |
| `plugin.updated` | `plugin` (as `plugin.list` gives it) | A plugin was put, or its person allowed what it asks |
| `plugin.deleted` | `id` | Deleted |
| `plugin.data` | `plugin`, `key` | A panel's data under `key` changed. A panel showing it on another device reads it again |
| `device.added` | `device` (`id`, `name`, `kind`, `at`) | A login, on any of the person's devices |
| `device.updated` | `device` (`id` and what changed) | Renamed |
| `device.revoked` | `device` (the id) | Signed out. The revoked device's own socket is closed with 4001 |

## Commands

Commands are defined once in `shared/` as a list: each one's name, its arguments, what it answers, and whether it asks first when an agent uses it. The app's `cmd` frames and the agents' tools in `loom mcp` both come from that list ([PLAN.md](../PLAN.md#agents-can-do-anything-the-app-can)).

Version 1's commands:

| Name | Arguments | Answers |
|---|---|---|
| `conversation.create` | `agent`, `title`? | `conversation` |
| `conversation.rename` | `conversation`, `title` | — |
| `conversation.delete` | `conversation` | — |
| `messages.list` | `conversation`, `before`? (a message id), `limit`? (up to 100) | `messages`, newest last, and `more` |
| `agent.create` | `name`, `acp_agent`, `instructions`, `folder`?, `mcp`? (the names of the person's MCP servers it gets), `permission`? (the permission mode: `ask`, the default, asks about everything; `auto` allows the tools that only look, of kind `read`, `search`, `think` and `fetch`, and asks about the rest; `allow` allows everything; or a list of kinds to allow, like `"read,search,edit"`), `color`? (one of the eight thread colours in `design/tokens.toml`; left out, they go round in turn) | `agent` |
| `agent.update` | `agent`, and what changes | `agent`, with its new `revision` |
| `memory.list` | `agent`? (that agent's notes and the shared ones; every note without it), `query`? (words, any of them, best match first; newest first without), `limit`? (up to 500, 100 if left out) | `memories`: each `id`, `agent` (empty for a shared note), `shared`, `text`, `by` (the agent or device that kept it) and `at` |
| `memory.put` | `text`, `agent`? (empty or left out: shared by all the person's agents) | `id` |
| `memory.delete` | `id` | — |
| `link.create` | — | `code` (one-time, for [`POST /api/link`](#post-apilink)), `expires` (Unix ms, five minutes on), `instance` and `key` (the instance's id and public key, as `/api/info` gives them). The core adds `qr`, the QR code's text. Only a device can ask, never an agent |
| `push.register` | `token` (the device's APNs token, hex), `env` (`production`, or `development` for a debug build) | Nothing back. Only a device, for itself. A device signed out loses it |
| `push.unregister` | — | Nothing back. This device stops taking push |
| `profile.get` | — | `profile`: `text`, `revision`, `by` (the device or agent that wrote it) and `at`, or `null` when there's none yet |
| `profile.put` | `text` (the whole profile, up to 4000 bytes; empty clears it) | `profile`, as `profile.get` gives it, with its new `revision` |
| `routine.list` | — | `routines`, oldest first: each `id`, `name`, `agent`, `prompt`, `schedule` (null for a routine that only runs by webhook), `hook` and `secret` (with a webhook: [`POST /hooks/<id>`](#post-hooksid)), `when` (in words, such as `every 30 minutes, and by webhook`), `tz` (an IANA name), `on`, `notify`, `catch_up`, `next` (when it runs next, Unix ms, or 0 while it's off), `conversation` (where it last posted), `last` (null before its first run, then `at`, `status`: `running`, `done`, `failed`, `cancelled` or `skipped`, `run`, and `error`), `by` and `at` |
| `routine.put` | To make one: `name` (up to 100 bytes), `agent` (by id or name), `prompt` (up to 20,000 bytes), `schedule` (`{"every": minutes}`, 5 to 10,080; `{"daily": "HH:MM"}`; or `{"weekdays": "HH:MM"}`, Monday to Friday; a time of day on a 24-hour clock in `tz`, summer time included), `webhook` (true for a webhook, which gets a new `hook` and `secret`; false takes it away), at least one of the two, and optionally `tz` (an IANA name, such as `Europe/Oslo`; the device's time zone if left out, or UTC when loom doesn't know the device's; one loom doesn't know is refused), `notify` (false), `on` (true) and `catch_up` (true: a run missed while the instance was off happens once when it starts; false: it waits for its next time). To change one: its `id` and what changes | `routine`. Its next run stays unless its schedule or time zone changed |
| `routine.run` | `id` | `run`. It runs now, whether it's on or not, unless its last run is still going (`busy`) |
| `routine.delete` | `id` | — |
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
| `activity.list` | `agent`?, `before`? (an entry's `id`), `limit`? (up to 200, 50 if left out) | `activity`, newest first: each `id`, `agent`, `kind` (`tool`, `approval`, `agent`, `message`, `notify`, `memory`, `routine` or `device`), `text`, `by` (a device, an agent, `auto` for the permission mode, or empty for the agent itself) and `at`. It's kept for good, unlike events |
| `agent.revisions` | `agent` | `revisions`, newest first: each `revision`, `by` (the device or agent that made it), `at`, and `agent` as it was then |
| `agent.rollback` | `agent`, `revision` | `agent`: a new revision with that one's name, ACP agent, instructions, folder and colour. The permission mode stays as it is |
| `agent.delete` | `agent` | — |
| `device.rename` | `device`, `name` | — |
| `device.revoke` | `device` | — |
| `acp.list` | — | `programs`: the agent programs installed on the instance, each with `name`, `command`, `source` (the ACP Registry id it came from, or empty for one added with `loom acp add`), `version`, and `settings`: the names of its settings, never their values |
| `acp.registry` | `again`? (`true` to read it again now, not from the last 10 minutes) | `agents`: the [ACP Registry](https://agentclientprotocol.com/get-started/registry)'s, each with `id`, `name`, `version`, `description`, `website`?, `icon`?, `how` (how it would run on the instance: `binary`, `npx`, `uvx`, or null when it can't), and `installed` (the version installed) if it is; and `platform`, such as `windows-x86_64`. The answer comes once the registry has been read |
| `acp.install` | `id` (a registry id), `env`? (settings to give it, `{NAME: value}`) | `program`, as `acp.list` gives it, once it's installed. A binary is downloaded into the instance's `acp/<id>/`, checked against the registry's SHA-256 and unpacked; an npx or uvx agent is run with `npx -y <package>` or `uvx <package>`. Installing one that's installed updates it, keeping its settings. Only the owner installs. `install_failed` says why it didn't |
| `acp.settings` | `name`, `env` (`{NAME: value}` sets one, `{NAME: null}` removes it) | `program`. Settings are the environment variables the program starts with: an API key, a model. They're kept sealed under `secret.key`, as secrets are, and only their names come back. Agents that aren't answering start again with them at their next message. Only the owner |
| `acp.remove` | `name` | —. Removes the program and, for one installed from the registry, its folder. Only the owner |
| `mcp.list` | — | `servers`: the person's MCP servers, each with `name`, and `command`, `args` and `env`, or `url` and `headers`. A value in `env` or `headers` is text, or `{"secret": NAME}` |
| `mcp.put` | `name` (lowercase letters, digits and hyphens; not `loom`), and `command` with `args`? and `env`?, or `url` (http or https) with `headers`? | `server`. Each agent whose `mcp` names it gets it in its next new session, with its secrets filled in, beside loom's own. One over `url` goes only to agent programs that offer `mcpCapabilities.http` |
| `mcp.delete` | `name` | — |
| `secret.list` | — | `secrets`: the person's secrets, each `name` and `at`. Never their values |
| `secret.put` | `name` (letters, digits, `_`, `-` and `.`), `value` | `name`. It's sealed with XChaCha20-Poly1305 under the instance's `secret.key`, bound to the person and the name, and opened only to start an agent's MCP server |
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

What it asks for is its `asks`: `tools` when it has a tool server, `agents`, and `cmd:<name>` for each command. A panel never gets the commands that would let it allow itself more, run programs on the instance, or reach the person's devices, secrets and sign-in: `plugin.put`, `plugin.allow`, `plugin.delete`, `link.create`, `push.register`, `push.unregister`, `device.revoke`, `device.rename`, `secret.list`, `secret.put`, `secret.delete`, `mcp.put`, `acp.install`, `acp.remove` and `acp.settings`. Until its person allows something, it waits, and the plugin goes without it.

- **A tool server** the person allowed is in every one of their agents' new sessions, as the MCP server `plugin-<id>`. It runs from a folder holding the revision's files, with `LOOM_PLUGIN_DIR` set to that folder.
- **A panel** talks to the app through one door, `await loom.send(op, args)` (desktop/src/panel.rs). The app answers these, and refuses everything else:

| `op` | `args` | Allowed |
|---|---|---|
| `data.get` | `key` | Always: `plugin.data.get` for this plugin |
| `data.put` | `key`, `value` | Always: `plugin.data.put` for this plugin |
| `cmd` | `name`, `args` | When `cmd:<name>` is allowed. The command's `value` comes back |
| `message_agent` | `agent`, `text` | When `agents` is allowed. The agent's reply comes back, as with `message_agent` |

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
| `busy` | The conversation's agent is still answering. Version 1 has one run per conversation at a time; the message is stored, and the app can send again after `run.finished`. Or `acp.install` for an agent that's being installed already |
| `unsupported` | `acp.install`: the agent has no build for the instance's computer, and npx or uvx, which it could run with otherwise, isn't there |
| `install_failed` | `acp.install`: the download, its check or unpacking failed. The message says which |
| `registry_unavailable` | `acp.registry`: the ACP Registry couldn't be read |
| `unknown_file` | A `send` names a file this person hasn't uploaded |
| `storage_full` | A file would take the person over the space the owner gives each person |
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
| Every session gets `mcpServers` with one server, `loom`: the `loom` program (or the desktop app's own `loom.exe`) with the argument `mcp`, and `LOOM_URL` and that agent's `LOOM_MCP_TOKEN` in its environment. It gives the agent loom's own tools: `message_agent`, `list_agents`, `create_agent`, `update_agent`, `edit_instructions`, `list_acp_agents`, `search_messages`, `list_skills`, `read_skill`, `save_skill`, `notify`, `remember`, `recall`, `forget`, `update_profile` (writes the person's whole profile anew), `list_devices`, `use_device` ([Devices as tools](#devices-as-tools)), `schedule` and `list_routines` (`routine.put` and `routine.list`; a routine is for the calling agent unless it names another), `list_plugins`, `read_plugin` and `save_plugin` ([Plugins](#plugins)), `delete_agent`, `set_permission`, `delete_skill`, `delete_routine` and `delete_plugin`. They act with the person's rights. The last five ask the person every time, whatever the permission mode: the call shows as a tool (`delete` or `other`) in the run that made it, with `approval.needed` (`can_always: false`), and is answered once the person does. With no run going, they refuse; an agent can't delete itself or loosen its own mode | what the tools do: `message.created`, `agent.updated`, `notify` and the rest, as if the person had done it. A yes is done as the device that gave it, so `by` names it |
| A conversation's first message to an agent program: `session/new`, or after a restart, `session/resume` (where the agent offers `sessionCapabilities.resume`) or `session/load` (where it offers `loadSession`) with the session the conversation had. What `session/load` plays back is dropped, and if the agent can't pick the session up, a new one starts | nothing: the apps see the same conversation |
| Where the person is writing from, in that prompt | a first text block, `[loom: This message comes from … It's 2026-11-21 13:05 UTC.]`, with the device's name, what it is and its time zone, the agent that sent it with `message_agent`, or the routine that sent it (`the person's routine "Morning brief", run every 30 minutes`), whose message has the routine's id as `from`. It comes when the session hasn't been told yet, or when it changes, never in the instructions |
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
- **Push** (Phase 5): sending to APNs once ES256 lands in Calx (CLX-406). `push.register` and `push.unregister` come first.
- **Noise** (Phase 6): a phone reaching a desktop instance runs a Noise handshake against the instance's key from the QR code before `hello`, and every frame after it is encrypted.
- **Moving** (Phase 7): the `instance.moved` event, signed with the instance's key, with the new address.
