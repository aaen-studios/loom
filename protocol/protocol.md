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
- [The event log](#the-event-log)
- [Messages and streaming](#messages-and-streaming)
- [Events](#events)
- [Commands](#commands)
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

### Later routes

`POST /hooks/<id>` for webhooks (Phase 4).

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

## Frames from the instance

### `welcome`

```json
{ "t": "welcome", "version": 1, "user": "u_1", "device": "d_3", "seq": 1046, "reset": false }
```

- `version`: the one version this connection uses.
- `seq`: the newest event `seq` the person has right now. Catch-up runs from `after` up to this.
- `reset`: `true` when the app's cursor is too old (its events were pruned) or ahead of the instance's (the instance was restored from an older backup). The app clears its cache, and a [`snapshot`](#snapshot) follows instead of the missed events.

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

### `error`

Something the app sent couldn't be done. The socket stays open.

```json
{ "t": "error", "code": "unknown_conversation", "message": "There's no conversation c_44.", "ref": "m_4f2" }
```

`ref` is the `cid`, `call` or `run` of the frame it answers, when there is one.

## The event log

- **Every change** the instance makes for a person is an event with a `seq`, counted per person from 1. Each `seq` is one more than the last, with no gaps.
- **An event is stored before it's sent.** An app that has `seq` N has everything up to N.
- **The app stores the `seq`** of the last event it applied, in the same transaction as the change it made to its cache. That's its cursor, and it sends it as `after` on the next `hello`.
- **Every device of a person** sees the same events in the same order, including events about what that device did itself.
- **Old events are pruned** after 30 days, keeping at least the newest 10,000. A device further behind than that gets `reset: true` and a snapshot.
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

- `role` is `person` or `agent`. A person's message has `from` (a device id) instead of `agent` and `run`.
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
| `approval.needed` | `run`, `call`, `tool`, `title`, `detail` (what exactly would happen: the command, the diff, the file), `can_always` | The agent asks first, and its permission mode sends the question to the person |
| `approval.answered` | `call`, `allow`, `always`, `by` (the device that answered, or `auto` when the permission mode did) | Someone answered, on any device. Every other device stops showing the question |

An approval nobody answers stays open while the run does. Cancelling the run answers it with `allow: false`, `by: "cancel"`.

### Agents and devices

| Kind | Fields | When |
|---|---|---|
| `agent.created`, `agent.updated` | `agent` (the whole agent, with its `revision`) | Made or changed, by a person or by itself |
| `agent.deleted` | `agent` (the id) | Deleted |
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
| `agent.create` | `name`, `acp_agent`, `instructions`, `folder`?, `permission`?, `color`? (one of the eight thread colours in `design/tokens.toml`; left out, they go round in turn) | `agent` |
| `agent.update` | `agent`, and what changes | `agent`, with its new `revision` |
| `agent.delete` | `agent` | — |
| `device.rename` | `device`, `name` | — |
| `device.revoke` | `device` | — |
| `acp.list` | — | `programs`: the agent programs installed on the instance, each with `name` and `command` |
| `messages.search` | `query`, `agent`?, `after`? and `before`? (Unix ms), `limit`? (up to 100, 30 if left out) | `results`, best first, each with its `message` and a `snippet`: the words around the match, with each matched word between U+0002 and U+0003. Every word of `query` must be in the message, the last as a prefix, so it works while typing. It searches text blocks and file names, in conversations that aren't deleted. A person's message counts as its conversation's agent's |

Arguments marked `?` are optional. "—" means `value` is `{}`.

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
| `busy` | The conversation's agent is still answering. Version 1 has one run per conversation at a time; the message is stored, and the app can send again after `run.finished` |
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
- **Memory, routines and notifications** (Phase 4): `routine.*` and `notification` events.
- **Push** (Phase 5): the `push.register` command.
- **Noise** (Phase 6): a phone reaching a desktop instance runs a Noise handshake against the instance's key from the QR code before `hello`, and every frame after it is encrypted.
- **Moving** (Phase 7): the `instance.moved` event, signed with the instance's key, with the new address.
