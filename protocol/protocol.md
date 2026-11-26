# The loom protocol

Version 2, and version 1, which an instance still speaks ([Versions](#versions)). This is what an app (always through its core) and an instance say to each other. It's the contract between `core/` and `server/`, and its types live once, in `shared/`, which both import.

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
- [Rules on what's touched](#rules-on-whats-touched)
- [The queue](#the-queue)
- [Tasks](#tasks)
- [A backup brain](#a-backup-brain)
- [Group chats](#group-chats)
- [Agents run commands](#agents-run-commands)
- [The agent's browser](#the-agents-browser)
- [Files in a reply](#files-in-a-reply)
- [Teaching a task, and proposals](#teaching-a-task-and-proposals)
- [Deleting a message](#deleting-a-message)
- [The event log](#the-event-log)
- [Messages and streaming](#messages-and-streaming)
- [Events](#events)
- [Commands](#commands)
- [Emoji](#emoji)
- [Plugins](#plugins)
- [Errors and close codes](#errors-and-close-codes)
- [Over Noise](#over-noise)
- [Push](#push)
- [Updating the instance](#updating-the-instance)
- [Versions](#versions)
- [Usage limits](#usage-limits)
- [How ACP maps onto loom](#how-acp-maps-onto-loom)
- [One message's whole trip](#one-messages-whole-trip)
- [Later versions](#later-versions)

## Transport

- **HTTP/1.1** for what isn't a live session: setup, login, invites, files and webhooks.
- **One WebSocket** (RFC 6455) per device for everything live, at `/ws`.
- **TLS** comes from whatever is in front of the instance (Caddy, a Cloudflare tunnel). The instance itself listens on 127.0.0.1. A desktop instance's own app connects over loopback without TLS. Phones reaching a desktop instance use a Noise handshake, over one WebSocket that carries their HTTP requests too ([Over Noise](#over-noise)).
- **One JSON object per WebSocket text frame**, UTF-8. Files go over HTTP. The one binary message is a [`browser.frame`](#browserframe), from the instance, from version 2 on: its head, a JSON object as a text frame would be, a newline, and the picture. An app sends none.
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
  "protocol": [1, 2],
  "version": "0.2.0",
  "setup": false,
  "moved": null
}
```

`version` is the version of loom the instance runs. `setup` is `true` until the owner account exists. `key` is checked on every later connection ([PLAN.md: Connecting devices](../PLAN.md#connecting-devices)). `moved` is the [`moved`](#moved) frame once the instance has moved, and null until then.

Once the instance has moved, every other route answers 410 with `{"error": {"code": "moved", …}, "moved": <the moved frame>}`. While it's moving, every route but a `GET` answers 503 with `moving`.

### `GET /api/version`

No login. Whether loom answers at an address, for a script or an agent setting a server up, before there's anything to log in to.

```json
{ "name": "loom", "version": "0.2.0", "protocol": [1, 2], "set_up": false }
```

`set_up` is `true` once the owner account exists (the opposite of `/api/info`'s `setup`). It answers the same once the instance has moved.

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

Errors: `bad_password` (also for a username that doesn't exist, so the two can't be told apart, and for an account a [link code](#link-codes) made, which has no password until the person sets one), `rate_limited`.

### `POST /api/link`

A device joins with a one-time code: the one another device of the same account shows under Add a device, as a QR code or a code to paste ([Adding a device](#adding-a-device)), or a [link code](#link-codes) an agent handed the app. No login.

```json
→ { "code": "K7QF-M2XD", "device": { "name": "Elliot's MacBook", "kind": "macos", "tz": "Europe/Oslo" } }
← 202 { "waiting": "<what it asks again with>", "check": "4821", "expires": 1795262400000, "error": { "code": "update_app", … } }
```

A code a device showed doesn't sign in on its own: the new device waits, with `check` to show, until the person says yes on a device signed in, and asks again with [`POST /api/link/wait`](#post-apilinkwait). `error` is there for an app from before this, which reads any answer but 200 as one, and so says to update it. A [link code](#link-codes) was said yes to already, and signs in at once:

```json
→ { "code": "loom-link-3f2a…", "device": { "name": "iPhone", "kind": "ios", "tz": "Europe/Oslo" } }
← { "user": "u_1", "device": "d_4", "token": "<device token>" }
```

A link code that makes the owner takes `username` too, and the answer has `"owner": true`. Errors: `bad_code` (also for one that's been used or replaced, and it counts toward `rate_limited` as a wrong password does), `expired`, and for an owner's code, `bad_username` (the code still works) and `already_set_up`.

### `POST /api/link/wait`

No login. A device that used a code asks whether the person has said yes, every two seconds, and is answered at once:

```json
→ { "waiting": "<from POST /api/link>" }
← 202 { "waiting": true, "check": "4821", "expires": 1795262400000 }
← 200 { "user": "u_1", "device": "d_4", "token": "<device token>" }
```

202 while no one has answered, and 200, once, with the new device's token after a yes, as a login gives it; the devices hear `device.added`. Errors: `refused` (403, a no), `expired` (410, no answer in five minutes), and `bad_code` (403, for what no device was given, or one answered already; it counts toward `rate_limited` as a wrong password does).

### Adding a device

How a new device (a Mac, a PC or a phone) joins an account with nothing to type but a code and nothing to say but yes on a device the person has already ([PLAN.md: Connecting devices](../PLAN.md#connecting-devices)).

1. **The code.** Add a device, on any device signed in, sends [`link.create`](#commands). The code is two groups of four letters and digits of Crockford's base 32 (`K7QF-M2XD`), read in any case, without the dash or spaces, with O as 0 and I or L as 1 (`shared/addcode.calx`). Only its SHA-256 is kept. It works once, for ten minutes, and a new one doesn't end the last.
2. **Where it goes.** The core makes two things of it, with the address it reaches the instance at: the QR code, `loom://add?a=<address>&c=<code>&i=<instance>&k=<its key>`, which a phone scans and which opens the app on a computer as a link, and the code to paste, `K7QF-M2XD@loom.example.com` (`https://` left out). An older app's `loom://link?…` reads the same. Its address is one of the network's, or a [`noise://`](#over-noise) place, for an instance in a desktop app with phones on.
3. **The check.** The new device posts the code to [`POST /api/link`](#post-apilink), and shows the four digits it gets back. Each of the person's devices hears [`link.asked`](#events) with the new device's name and kind and the same digits, and asks `Add "Elliot's MacBook"?`. A yes or a no on one ([`link.answer`](#commands)) is every device's, with `link.answered`. Only a device answers, never an agent or a plugin, so a code that leaked, or was guessed, adds nothing without the person.
4. **Signed in.** The new device asks again with [`POST /api/link/wait`](#post-apilinkwait) until it's answered, and keeps the token it gets in the credential store, as any login does. A no, or five minutes without an answer, ends it; the person makes a new code.

Signing in with the address, a username and a password stays, for the first device and when every device is lost. A server on a tailnet (a `….ts.net` name or an address in 100.64.0.0/10) that this device can't reach, with no Tailscale address of its own, is refused as `tailscale`, `This server is only reachable on Tailscale, and this device isn't on it`, rather than a connection that failed.

### `POST /api/link/new`

No login. A [link code](#link-codes) that makes the owner, for the setup code, on an instance no one has set up yet. `loom install --json` asks for one and gives it as `link_code`.

```json
→ { "setup_code": "<setup code>" }
← { "code": "loom-link-3f2a…", "expires": 1795262400000, "owner": true }
```

Errors: `bad_code` (it counts toward `rate_limited` as a wrong password does), `already_set_up`.

### Link codes

How an agent sets loom up for its person with nothing for them to type but yes ([PLAN.md: Several servers in one app](../PLAN.md#several-servers-in-one-app)). A link code is `loom-link-` and 32 hex digits. Only its SHA-256 is kept. It works once, for 15 minutes, and it's the only thing the agent handles: never a password or a token.

- **Where they come from.** On an instance no one has set up: [`POST /api/link/new`](#post-apilinknew) with the setup code, `loom install --json`'s `link_code`, or `loom link` on the server. These make the owner, and there's one at a time: a new one replaces the last. For an account that's there: `loom link [NAME]` on the server (the owner's without a name), or [`link.create`](#commands) with `user` from the owner's or an admin's device, as a password reset is (the owner for anyone, an admin for members). These sign in to that account, and go in the people log.
- **How they're used.** The agent calls `add_server` with `address` and `link_code`. It asks the person, as loom's own tools that ask do, titled `Add the server ADDRESS to your app, and sign in there`. Their yes sends `server.offer`, and the device that gave it hears `server.offered` with the code and their username here, which its core passes on as `core.offer` ([core/README.md](../core/README.md#between-the-app-and-the-core)). The app starts a core for the new server, which posts the code to [`POST /api/link`](#post-apilink) with that username and the device, and keeps the token it gets in the credential store, as any login does. An agent's `install_server` on one of the person's hosts never hands the agent the code at all: the device that ran the install says `core.offer` to its own app itself ([Installing loom on a host](#installing-loom-on-a-host)).
- **The owner a code makes** has the username the app gave (`owner` if it gave none) and no password. They need none in the apps that are signed in, and their phones join with a QR code as ever. A login with a username takes none until they set one with [`password.set`](#commands), on a device, or `loom user password` on the server. Once there's an owner, the setup code and every owner's code are gone.
- **Limits.** Wrong codes count as wrong passwords do: 10 in 15 minutes from one address, and the next gets `rate_limited` with `Retry-After`. So does a wrong setup code to `/api/link/new`.
- **The import that follows.** `loom import openbot|grokbot`, run on the server with no token where it can open `--data` (as `sudo -u loom loom import openbot --data /var/lib/loom`), makes itself a script token for the owner that only `/api/scripts/import` takes, for 15 minutes, and drops it once the import answers ([`/api/scripts/*`](#apiscripts)). Whoever can open the instance's folder could do more than that already. Before there's an owner, it says to add the server with the link code first.

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
{ "file": "9f86d0…", "name": "shot.png", "mime": "image/png", "size": 48213, "width": 1280, "height": 800 }
```

- `file` is the file's SHA-256 in lowercase hex. The instance keeps each file once, by it, so sending one again costs nothing and gives the same answer.
- `name` is the name made safe for any system: no folders, and nothing Windows refuses.
- `mime` is what the bytes say for PNG, JPEG, GIF, WebP and PDF, else the `Content-Type` sent, else what the name's extension says.
- `width` and `height`, for a PNG, JPEG, GIF or WebP, are its size in pixels as it shows, read from its header: a JPEG its EXIF turns a quarter has them swapped. They're left out for any other file, and for a picture whose header can't be read. A `file` block has them too, so an app lays a picture out before it's fetched; one kept before 0.2.10 has them once it's sent again, and until then an app goes by its thumbnail's shape.
- `413` with `storage_full` when the file would take the person over the space the owner gives each person (`loom files limit`).

A file goes in a message by its `file`, in [`send`](#send)'s `files`.

### `GET /files/<sha256>`

Login needed, as above. The file, with its type, for whoever uploaded it or got it from an agent, and `404` for anyone else. With `?thumb`, a thumbnail of a PNG or JPEG image instead, at most 512 pixels a side, turned the way a photo's EXIF says it shows (the thumbnail keeps no EXIF), as a JPEG, or a PNG when it has transparency. It's made just after the upload; until it's there, and for files that have none, `?thumb` gives the file itself. Both are cached for good (`Cache-Control: private, immutable`), since a file's name is its content, except the file given while its thumbnail is still being made: that one has `X-Loom-Thumb: not-yet` and `Cache-Control: no-store`, and the core shows it without keeping it as the thumbnail.

### `GET /emoji/<id>`

Login needed, as above. The picture of a person's own emoji ([Emoji](#emoji)), a PNG 128 pixels square, for anyone signed in to the instance, since everyone in a chat sees the emoji in it. Cached for good (`Cache-Control: private, immutable`): a renamed emoji keeps its picture. `404` for one deleted, until `emoji.restore` brings it back.

### `GET /api/mcp/tools` and `POST /api/mcp/call`

For `loom mcp` alone, which the instance starts for each agent (below, [How ACP maps onto loom](#how-acp-maps-onto-loom)). They take that agent's tool token for that conversation's session, so a call finds the run it came from, `Authorization: Bearer <token>`, not a device's, and an app never calls them. `tools` gives `{"tools": [...]}`, MCP's tool definitions from `shared/tools.calx`. `call` takes `{"name", "arguments"}` and gives an MCP tool result, `{"content": [{"type": "text", "text"}], "isError"?}`. A `message_agent` call is answered when the other agent's reply is done.

In Code Mode ([PLAN.md](../PLAN.md#code-mode)), `loom mcp` also uses these, with the same token:

- `GET /api/mcp/servers`: `{"servers": [...], "icons": {...}}`, the agent's MCP servers, as `session/new` would have had them (`{name, command, args, env}` or `{type: "http", name, url, headers}`, with each `env` and `headers` a list of `{name, value}` and its secrets filled in), and, unless the agent runs in a sandbox, the person's plugins' tool servers it allowed, each named `plugin-<id>`; and for each server whose icons the instance keeps ([PLAN.md](../PLAN.md#cards-in-the-chat)), what they're known by, `{NAME: KEY}`.
- `POST /api/mcp/icon` with `{"server", "key", "icon", "icon_dark"}`, once `loom mcp` has started one of the person's servers whose `serverInfo.icons` aren't the ones known: what they're known by (`server/icons.calx`'s `key`, empty for none), and the one for a light background and the one for a dark, each fetched where the agent runs and given in base64, empty where there's none. The instance checks each is a picture, keeps it as the person's file and puts it on the server's badge; one that isn't is left out. `404` for a server that isn't the person's, `400` for one over 256 KiB. `{}`.
- `GET /api/mcp/deno`: where Deno is, `{"path"}`, or `{"error"}` when it can't be had here. The first time, the instance fetches it on its installer's thread, and the request waits until it's there. With `?here`, it says where only once it's been fetched, and never fetches it: `loom mcp` asks so to start a Deno ahead of a script.
- `POST /api/mcp/allow` with `{"script": true, "arguments": {"code"}}`, before each script runs. In the permission mode `ask`, it's one of loom's own tools that asks, titled `Run a script`, of kind `execute`, with `server` `loom`, `tool` `run_code`, the script as `arguments` and `act` 0 as `detail`, and `can_always: false`, since each script is a different one, and answered once the person does. In any other mode it's answered at once. The answer is as below.
- `POST /api/mcp/allow` with `{"server", "tool", "read_only", "arguments"}`: whether a script's call to that server's tool may go. It's put to the agent's permission mode as kind `read` (`read_only`) or `other`; one the mode allows is answered at once, and otherwise it's one of loom's own tools that asks ([How ACP maps onto loom](#how-acp-maps-onto-loom)), titled `Use TOOL on the MCP server SERVER` (or `on the plugin ID`), with the call's `server`, `tool`, `arguments` and `act` as `detail`, and the server's `icon` and `icon_dark` when it has them, for its badge on the question, and `can_always`, unless one of the person's rules asked, and answered once the person does. A yes with `always` also adds a rule allowing `SERVER.TOOL` for the agent, with `rules.updated`, so its next calls go at once. With 200 rules already, the call still goes, and the activity log says the rule couldn't be kept. The answer is an MCP tool result: `isError` with why for a no, or else with `act`, the activity log's entry for the call (0 for one that only reads).
- `POST /api/mcp/done` with `{"act", "ok", "why"}`: how that call went, which settles its entry as `done` or `failed`. Only for an `act` that `allow` gave this token.
- `POST /api/mcp/token` with `{"server"}`, after one of the person's servers answered 401 to its token ([PLAN.md](../PLAN.md#signing-in-to-mcp-servers)): the instance renews the person's sign-in with its refresh token and answers `{"headers": [{"name": "Authorization", "value": "Bearer …"}]}` once it has, or `{"sign_in": true}` when there's no sign-in to renew or the authorization server won't. `404` for a server that isn't the person's, `400` for one that doesn't sign in (it's started by a command, or has an `Authorization` header of the person's own).
- `POST /api/mcp/sign_in` with `{"server"}`: the server needs the person to sign in. A sign-in with no refresh token is dropped (one with a refresh token was renewed through `token` first, so it stays), and the agent's reply in that run gets a `sign_in` card for it, once a reply, unless the reply has the server's `mcp_add` card, which has Sign in itself. `{}`.

After an `add_mcp_server` call, `loom mcp` asks for `GET /api/mcp/servers` again at the session's next search or script, and starts the servers it hasn't, so a server the person added is the agent's in the same session.

### `/api/scripts/*`

The person's own scripts ([PLAN.md](../PLAN.md#scripts)), with the script token `scripts.on` gave, as `Authorization: Bearer <token>`. Only from the instance's computer, straight: a request from anywhere but a loopback address, or with `X-Forwarded-For`, `Forwarded`, `X-Real-IP`, `Via` or `X-Loom-Bridge` (which the phones' bridge adds to everything it forwards, and takes off what a phone sent), gets `403` with `not_here`. A missing or wrong token gets `401`. A token `loom import` made itself from the data folder ([Link codes](#link-codes)) is taken only by the route it's for, `import`: any other gets `403` with `not_allowed`, and once its 15 minutes are over, `401`. A status token (`scripts.status_on`) is taken only by `GET /api/scripts/status`, and a script token or one `loom import` made isn't taken there: either way, anything else gets `403` with `not_allowed`. A status token opens nothing outside `/api/scripts/*` either: not `/ws`, `/files` or `/api/mcp/*`.

- `GET /api/scripts/status`, with a status token: the person's agents, routines and the last 24 hours' routine runs, for a dashboard on the instance's computer. Names, colours, shapes, counts, states and times only, never what anyone said, a prompt, instructions, memories, a run's error or a secret: `{"agents": [{"id", "name", "color", "shape", "drawing"?, "working"}], "routines": [{"id", "name", "on", "last"}], "runs": [{"at", "status", "routine"}]}`. An agent's `color` is a thread colour's name or `#rrggbb`, its `shape` one of the eight, `drawn` with its `drawing` (as `agent.update` keeps it), or `""` for the plain thread, and `working` is whether a run of it goes on now. A routine's `last` is its last run's `status` and `at` (Unix ms), or null before it has run. `runs` are the runs routines started in the last 24 hours, oldest first and at most 2,000 of the newest, each with when it started (Unix ms), how it went (`running` while it runs, then as its routine says: `done`, `quiet`, `failed`, or as its turn ended: `cancelled`, `limit`) and its routine's name (empty once the routine is deleted).
- `POST /api/scripts/ask` with `{"agent", "text", "conversation"?}`: the agent (by name or id) is asked, in its chat or the conversation named, as a person's message with `from: "script"`, up to 20,000 bytes. The answer comes once its run ends: `{"reply", "conversation", "message"}`, or `502` with `agent_failed`. `409` with `busy` while it's answering in that conversation, `404` for an agent or conversation that isn't the person's.
- `POST /api/scripts/routine` with `{"routine"}`, a routine's name or id: `routine.run`'s answer.
- `GET /api/scripts/approvals`: `{"approvals"}`, what waits for the person's yes, each `call`, `agent`, `agent_name`, `title` and `on_a_device`.
- `POST /api/scripts/approve` with `{"call", "allow"}`: answers one, as `script` (`approval.answered`'s `by`). One `on_a_device` is refused with `403`: a call to a device, whose yes a device signs, and a token an agent asked to have saved to a file (`token.make_to_file`), which a script mustn't make itself.
- `POST /api/scripts/import` with `{"from", "path"?, "carry_provider"?}`: `import.run`'s answer, as `script`, or `400` with its error. `loom import openbot|grokbot [PATH] [--carry-provider]` sends it.

### `POST /hooks/<id>`

A routine's webhook, for something outside to run it, such as GitHub Actions when a build fails. `<id>` is the routine's `hook`, and its `secret` comes one of two ways: as `Authorization: Bearer <secret>`, or as GitHub's webhooks sign, with `X-Hub-Signature-256: sha256=<hex>`, the HMAC-SHA256 of the body under the secret. The body can be anything. As UTF-8, up to 20,000 bytes of it, it goes to the agent after the routine's prompt, in a code block, with `X-GitHub-Event` named when it comes.

- `202` with `{"run"}`: it's running, or with `{"run", "waiting": true}`, it starts once its chat is free ([`routine.run`](#commands)).
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
 │◀───────────────────────── synced (seq, runs)     │
 │◀═════════════ events and deltas, live ══════════▶│   and the app's frames
```

1. The app opens `/ws` and sends `hello` straight away. Anything else first closes the socket with 4000.
2. The instance checks the token and the versions, and answers `welcome`, or closes with [4001 or 4002](#errors-and-close-codes).
3. It sends every event after the app's cursor, in order ([The event log](#the-event-log)).
4. For each of the person's runs still going, it sends what the reply has so far, as deltas from position 0 ([Messages and streaming](#messages-and-streaming)).
5. It sends `synced`, with the runs still going. From then on, events and deltas come as they happen.

The app may send frames as soon as it has `welcome`. It doesn't wait for `synced`.

**Keeping it alive.** The instance sends a WebSocket ping every 20 seconds. A connection that hasn't answered for 60 seconds is closed. The app treats 45 seconds with nothing from the instance as a dead connection, and reconnects.

**Reconnecting.** The app reconnects after 0.5 s, then doubles the wait each time, up to 30 s, with up to 20% random jitter. It tries again at once when the system wakes from sleep or the network changes. Each time it sends `hello` with the last `seq` it stored, and nothing else is needed to pick up where it was.

## Frames from the app

### `hello`

```json
{ "t": "hello", "token": "<device token>", "versions": [1, 2], "after": 1041, "device": { "tz": "Europe/Oslo", "active": true } }
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
- `form`: the person's answers to a `form` card (below): `{message, card, answers}`, the reply the card is in, the card's `id`, and the answers, by each field's label. The instance checks them against the card (`server/cards.calx`): a text answer is cut at 1,000 bytes, a choice is one of its options, a toggle `true` or `false`, a number a number or text that reads as one; a required field left empty, or a form already answered, is refused with `bad_form`, saying why, and the core takes the send out of its outbox. The message's `text` is then the instance's, one line a field, `Label: answer` (a toggle `Yes` or `No`, an optional field left empty left out), and it keeps `form` with the answers as they were checked. The card keeps them as `answers`, and the reply comes again in `message.updated`. The agent reads the text, with a note after it: `[loom: This message is your person's answer to your form card_1, by each field's label: {"Name":"Ada","Size":"Large"}]`.

- `reply`: the message it replies to ([PLAN.md](../PLAN.md#replying-to-a-message)), `{message, quote?, ask?}`: a message's `id` in the same conversation; `quote`, the words the person selected in it, for a reply to that part alone; and `ask: true` with a `quote`, for asking about it. Other fields are ignored: the core fills in the rest from its cache, as the instance will keep it, so the message waiting in the outbox shows its quote. One whose message isn't there or is in another chat is refused with `bad_reply`, and the core takes the send out of its outbox. The message keeps it as its own `reply`, below, and the agent reads it with the message, quoted line by line after `> `, in a note: `[loom: This message replies to your message msg_311 (with log.txt):`, `to this part of your message msg_311`, `to their own message msg_9`, or `Your person asks about this part of your message msg_311`.

Answered by [`ack`](#ack), or [`error`](#error) with `ref` set to the `cid`.

### `approve`

The person's answer to an [`approval.needed`](#approvals) event.

```json
{ "t": "approve", "call": "tc_3", "allow": true, "always": false }
```

`always: true` also allows this kind of request from this agent from now on, where `approval.needed` had `can_always`: for the agent program's own request, by its `allow_always` option, and for a Code Mode script's call to an MCP server's or a plugin's tool, by a rule allowing that tool (`server.tool`) for the agent, made as `rule.put` makes one ([Rules on what's touched](#rules-on-whats-touched)), so it's on the Rules page to remove. An approval that was already answered, by this device or another, gets an `error` with code `already_answered`.

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

With them, its `hosts`: the person's servers it reaches with ssh ([Devices as tools](#devices-as-tools)), each with its `name`, `target`, `via` (`ssh` or `tailscale`) and its `tools`, where only `run_command` counts. The instance keeps at most 50, and only names and targets of letters, digits and `.-_@:` not starting with `-` or `@`:

```json
{ "t": "device", "key": "3d4017c3…", "tools": { … }, "hosts": [ { "name": "vps", "target": "root@vps", "via": "ssh", "tools": { "run_command": "ask" } } ] }
```

### `device.result`

The answer to a [`device.call`](#devicecall): `ok`, and `output`, the command's output (with its exit status) or the file, or why it wasn't done.

```json
{ "t": "device.result", "call": "tc_9", "ok": true, "output": "hi from loom\nExit status 0." }
```

### `device.progress`

How a [`device.call`](#devicecall) that takes several steps is going, before its `device.result`: an install of loom on one of the person's hosts ([Installing loom on a host](#installing-loom-on-a-host)). `step` is `install`, `import`, `link` or `check`, `status` is `running`, `done` or `failed`, and `said` says it in words. The instance changes the step's line on the plan card with each `done` or `failed`.

```json
{ "t": "device.progress", "call": "tc_9", "step": "install", "status": "done", "said": "Installed loom 0.2.0 at https://vps.tail1.ts.net:8443 (through Tailscale)" }
```

## Frames from the instance

### `welcome`

```json
{ "t": "welcome", "version": 2, "loom": "0.2.0", "user": "u_1", "device": "d_3", "seq": 1046, "reset": false, "key": "<the instance's public key, base64>", "relay": "<the push relay's key, base64>", "has_password": true }
```

- `has_password`: whether the account has a password. One a [link code](#link-codes) made has none until the person sets one with `password.set`, so the apps offer **Set a password** while it's `false`.

- `version`: the one version this connection uses.
- `loom`: the version of loom the instance runs, as `GET /api/info` gives it. The core compares it with the newest release ([Updating the instance](#updating-the-instance)).
- `update`: only to an owner or admin, while an update of the instance is under way or has failed: how it stands, as [`instance.update`](#instanceupdate) says it.
- `relay`: only while the instance pushes through the [push relay](#through-the-push-relay). It's the relay's X25519 public key, which the core seals the phone's APNs token with before `push.register`.
- `key`: the instance's Ed25519 public key, as `GET /api/info` gives it. The app keeps the first one it sees, checks a [`moved`](#moved) frame's signature with it, and expects the same key at the new place.
- `seq`: the newest event `seq` the person has right now. Catch-up runs from `after` up to this.
- `reset`: `true` when the app's cursor is too old (its events were pruned, which includes a new device's `0` once any have been), more than 10,000 events behind (a new device's `0` on a busy account too), or ahead of the instance's (the instance was restored from an older backup). The app clears its cache, and a [`snapshot`](#snapshot) follows instead of the missed events.

### `snapshot`

Only after a `welcome` with `reset: true`. Everything the app needs to start again: the conversations, the agents and the newest messages of each conversation, as they are now. Older messages are fetched with the `messages.list` command.

```json
{ "t": "snapshot", "seq": 1046, "agents": [ … ], "conversations": [ … ], "messages": [ … ], "partial": ["c_3", "c_1"] }
```

- `agents` and `conversations` are all of them.
- `messages` are up to the newest 50 of each conversation, the conversation with the newest message first, while the frame stays within 768 KiB, under a phone's 1 MiB frame over Noise. A conversation gets its newest messages or none, never some in the middle.
- `partial` names each conversation with messages that `messages` leaves out. The core fetches those with `messages.list` when the app opens one. A snapshot without `partial`, from an older instance, may have left out any.

Its `seq` is up to 5,000 events before the `welcome`'s, or the oldest kept, and the events after it follow as usual, applied over the snapshot. They bring what a snapshot doesn't carry: the questions still open and the newest tool calls. Each applies over the newer state as it would have when it came, so the cache ends as the instance is. The runs still going come with `synced`, including one that started before those events.

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
{ "t": "synced", "seq": 1046, "runs": [ { "run": "run_77", "conversation": "c_9", "agent": "a_2", "message": "msg_311", "answering": "msg_310" } ], "groups": [ { "id": "g_1", "name": "Work", "collapsed": false, "at": 1700000000000 } ] }
```

`runs` is every one of the person's runs still going, as `run.started` gave each (without `revision`), with `routine` for a routine's quiet run, which the apps don't show as working. The app's cache makes its runs exactly these, so a run whose `run.started` or `run.finished` it never kept (the event was pruned, came before a snapshot's, or the cache couldn't write it) shows as it is: an agent shows it's working for as long as its turn goes, and stops once it's over.

`groups` is every one of the person's sidebar folders, in the order they were made, as `group.list` gives them ([PLAN.md](../PLAN.md#the-sidebar)). The cache makes its folders exactly these, so one that came while an older app was in use is there too. An instance from before folders leaves it out.

`emoji` is every one of the person's own emoji, as `emoji.list` gives them ([Emoji](#emoji)). The cache keeps them whole, as each `emoji.changed` brings them. An instance from before emoji leaves it out.

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

A call to one of this device's tools, from an agent's `use_device`. `request` is the call as text: `call`, `device` (this device's id), `host` (for a command on one of the person's hosts this device reaches), `tool`, `args`, `agent` and `at` (Unix ms). It's what an approval signs, and what the device runs. `approval` is there when the person said yes to a tool that asks.

```json
{ "t": "device.call", "call": "tc_9", "request": "{\"call\":\"tc_9\",\"device\":\"d_1\",\"tool\":\"run_command\",\"args\":{\"command\":\"echo hi\"},\"agent\":\"a_1\",\"at\":1795262400000}", "approval": { "key": "3d4017c3…", "signature": "9a0f…" } }
```

The device answers with [`device.result`](#deviceresult).

### `browser.state`

An agent's browser opened or closed, was taken over or handed back, or is asking for the person: `agent`, `open`, `person` (whether the person has it), `asked` (what `browser_ask_person` said, while it waits; otherwise empty) and `secret` (what `browser_ask_secret` asks for, while its card waits; the card in the chat is where it's answered), `recording` (whether what the person does is being recorded, `browser.record`) and `steps` (how many steps the recording has). Every live session of the person's gets it, not only those watching. It isn't an event, so it isn't kept: an app that connects asks with `browser.watch`, with `on` false for a browser it doesn't watch, which answers with its state and watches nothing.

### `browser.frame`

A picture of the page, to each session watching that agent's browser: `agent`, and `width` and `height`, the page's pixels, which `browser.input`'s `x` and `y` are in, whatever the picture's own size. They come as the page changes, at most about twelve a second, each no bigger than the biggest any session watching it shows it at (`browser.watch`'s `width` and `height`), and never bigger than the page. None made at the size before comes after `browser.watch` answers.

From version 2 it's a binary message: the head, `{"t":"browser.frame","agent":"a_1","width":1264,"height":705}`, a newline, then the JPEG's bytes. In version 1 it's a text frame with the JPEG in base64 as `data`. The JPEG is as the browser wrote it: Chrome on Linux writes a scan for each of its three colors, which an app's decoder has to read as well as a JPEG in one scan.

### `instance.update`

How an update of the instance stands, to every owner and admin connected, each time it changes ([Updating the instance](#updating-the-instance)). It isn't an event, so it isn't kept: a device that connects later gets it in `welcome`'s `update`.

```json
{ "t": "instance.update", "state": "waiting", "version": "0.2.1" }
```

- `state`: `downloading` (the manifest is read and checked, and the program fetched and checked), `waiting` (it's staged, and goes in once nothing runs), `restarting` (it's in, and the instance is stopping to start it), `failed` (with `why`), or `latest` (this is the newest already).
- `version`: the release it's for, once it's known. `why`: why it failed. `now`: it stops what runs rather than waiting. `auto`: the instance started it itself.

### `connect`

A server the person is connecting from the catalog moved to its next step ([`catalog.connect`](#commands)), to every live session of theirs: `connect`, with `entry`, `server`, `step` (`adding`, `sign_in`, `agents`, `failed` or `done`), `said` (why it failed, or why a sign-in didn't work) and `tools` (what it offers, from `agents` on). It isn't an event, so it isn't kept: a device that connects later finds a connect under way in `catalog.list`'s `connecting`, and a connect is kept only until the instance stops.

```json
{ "t": "connect", "connect": { "entry": "t3", "server": "t3", "step": "agents", "said": "", "tools": ["t3_thread_list"] } }
```

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
- **A screenshot** comes back in `device.result` as `image` (base64) and `mime`. The agent's tool call gets it as an MCP `image`, with a last text block naming it for `attach_files` (below, [Files in a reply](#files-in-a-reply)), and the chat keeps it as a file block in `tool.finished`'s `output`.
- Each call goes in the activity log as kind `device`, by the device that did it.
- **The person's hosts** ([PLAN.md](../PLAN.md#your-hosts)) are devices too, each kept with the device that reaches it, which says so in its `device` frame's `hosts`. `list_devices` gives each as `{"id": "d_1/vps", "name": "vps", "kind": "host", "via": "ssh", "target": "root@vps", "through": "Desk PC", "device": "d_1", "tools": {"run_command": "ask"}}`. `use_device` on a host takes only `run_command`. Its `device.call` goes to the device that reaches it, with `host` in the request beside `device`, so the yes signs which host as well. That device acts by the host's own `off`, `ask` or `allow`, and runs `ssh -o BatchMode=yes -o ConnectTimeout=15 <target> -- <command>`, with no input, up to 2 minutes. Its output and exit status come back as a command's do, and ssh's own failure (exit status 255) says why it couldn't connect.

### Installing loom on a host

An agent's `install_server` with `host` puts loom on one of the person's hosts and adds it to their app, on one yes ([PLAN.md: Several servers in one app](../PLAN.md#several-servers-in-one-app)). The host must have commands `ask` or `allow`, and there must be a run to ask in.

1. **The plan, as a card.** The instance puts a checklist card in the agent's reply, titled `loom on HOST`, with a line for each step: install (with HTTPS through Caddy at `domain`, or through Tailscale), import (with `import: "openbot"`), link, and check.
2. **One question.** Then `approval.needed`, with `device` and `request` as for any call to a device, and `can_always: false`, even when the host's commands are allowed, since it's several commands. Its `detail` has the `plan` and the `commands` it runs there, word for word. `request`'s `tool` is `install_loom` and its `args` are the plan: `domain` (`""` for Tailscale), `import` (`""` or `openbot`), `from_user` and `carry_provider` with an import, and `username`, the person's here, for the owner the link makes there. The device that reaches the host signs it, and acts on nothing without that signature.
3. **The device runs it.** It makes each command from the signed plan itself (`shared/hosts.calx`), never from what the instance sends, and runs it through ssh as a command on the host is run, saying how each step goes with [`device.progress`](#deviceprogress):
   - `install`: `curl -fsSL …/install.sh | sh -s -- --json --https-port auto [--domain DOMAIN]`, as root or through `sudo -n`, up to 20 minutes. Its last line of JSON gives `address`, `version`, `via` and the `link_code`, which stays on the device.
   - `import`: `loom import openbot --from-user USER --data /var/lib/loom --json [--carry-provider]`, as root, which stages OpenBot's data for the owner the link makes ([PLAN.md: Importing](../PLAN.md#importing-from-openbot-and-grok-bot)). A failure here doesn't stop the rest.
   - `link`: the device's core says [`core.offer`](../core/README.md#between-the-app-and-the-core) to its own app, with the address, the link code and the username, and the app signs in there as it does after `add_server`. With no link code (the server had an owner already), the app adds it for the person to sign in to. A server that answers only on itself (`via: "local"`) can't be reached, and the install stops here.
   - `check`: [`GET /api/version`](#get-apiversion) on the new server, from the device, every second for up to two minutes, until it says `set_up: true`.
4. **The answer.** `device.result` says what was done, a line or so for each step, or what failed and what to do next. It never has a link code or a setup code. Each step's `done` or `failed` changes the card's line.

## Agent defaults

The person's defaults are what every agent of theirs follows, live, unless it sets that field itself ([PLAN.md](../PLAN.md#agent-defaults)). They're kept on the instance per person, so every device has them: an app asks with `defaults.get` once it's connected, and `defaults.updated` keeps it up to date.

- `brain`: the program `agent.create` uses when it names none. An agent that's made keeps its program.
- `backup`: the program an agent's turns move to when its own runs out of usage or doesn't start, for an agent with no `backup` of its own ([A backup brain](#a-backup-brain)). `""` is none.
- `config`: per program, the options its agents' sessions are set to, by id. An agent's own `config` wins, option by option. A default its program doesn't offer is passed over, so the session stays on what the program starts on.
- `permission`: the mode of an agent with no `permission` of its own. Loom's own risky tools ask whatever it is.
- `instructions`: told to every agent, beside the person's profile ([How ACP maps onto loom](#how-acp-maps-onto-loom)).
- `busy`: what a `send` to an agent that's answering does, `queue` (it waits, [The queue](#the-queue)) or `steer`. An agent's own `busy` wins.
- `router_brain` and `router_model`: the program and the model the group chats' router runs on ([Group chats](#group-chats)). `""` for the program is the one loom picks, and for the model `haiku`.
- An agent's field goes back to following the default with `agent.update` and `null` (or `""`): `permission`, `busy`, `backup`, or an option in `config`.
- An agent made before there were defaults keeps the `permission` it was made with, as its own, until the person sets it to follow the default; its model and effort already follow it wherever it hasn't picked its own. One made with `agent.create` and no `permission` follows the default from the start, but one an agent makes (`create_agent`, or `run_command` with `agent.create`) or an import brings has `ask` of its own.
- Agents read them at once, and `defaults.put` asks the person first ([Agents run commands](#agents-run-commands)), since it changes every agent that follows them.

## Rules on what's touched

Beside its permission mode, an agent works under the person's rules on what an action touches ([PLAN.md](../PLAN.md#rules-records-and-limits)): its own, and the person's for every agent, kept on the instance per person. `server/rules.calx` reads and matches them.

- **A rule** has `then`, `allow`, `ask` or `deny`, and one or more conditions, all of which must hold:
  - `site`: a host a browser tool or a fetch goes to. `github.com` is that host and every one under it, `*.bank.no` a glob; an address is kept as its host. A port counts only when the rule names one.
  - `path`: a file path, as a glob (`/srv/**/*.pem`, `**/.env`), with `~` for the instance's home folder. A path with no wildcards is that file or folder and everything in it, and one with no root is matched at any depth. Paths are normalized first, so `..` can't get round a rule. `*` and `**` match hidden names too, so `/srv/secrets` covers `/srv/secrets/.key`.
  - `command`: a shell command, by its first words (`git push` is `git push` and `git push origin`, not `git pushy`) or as a glob (`rm -rf *`). A line is checked command by command, split where the shell would run another (`;`, `&&`, `|`, `$( )`, backticks), and with what only wraps a command (`FOO=1`, `sudo`, `env`, `nohup` and the like) taken off for deny and ask.
  - `tool`: an MCP tool as `server.tool`, a glob (`github.delete_*`), or a server's name alone for all its tools. Loom's own tools are `loom.<name>`.
  - `effect`: `read` (the action only looks: an ACP tool of kind `read`, `search`, `think` or `fetch`, an MCP tool with `readOnlyHint`, one of loom's tools that only reads) or `write` (everything else).
  - `started_by`: who started the turn: `person` (from one of their devices), `owner`, `admin` or `member` (the person, by their role), `agent` (another agent, with `message_agent`), `routine` (on its schedule, by hand, or a step of its flow), `webhook`, `script` or `plugin`.
- **Deny first.** Every rule about the action counts: a deny wins, then an ask, then an allow. With none, the permission mode decides, as before. An allow on paths or commands holds only when every path and every chained command matches it, and never for a command with `$( )`, backticks or `>`; a deny or an ask holds when any does.
- **What can't be read denies.** A kept rule that isn't one `rule.put` would make denies everything. So does an action a rule is about whose site, path, command or starter can't be read: an address with no host, a command that's neither text nor a list of words.
- **Where they apply.** An ACP agent's `session/request_permission`: a deny answers no at once (`approval.answered` with `by: "rule"`, `rule` and `why`), an ask asks the person even where the mode allows, with `rule` in `approval.needed` and `can_always: false`, so the program keeps asking, and an allow answers yes (`by: "rule"`). The command is the tool call's `rawInput.command` (text, or a list of words, where `["bash", "-lc", SCRIPT]` is `SCRIPT`), its paths are `rawInput`'s `file_path`, `path` or `notebook_path` and its `locations`, relative ones in the agent's folder, its site is `rawInput.url`, and a tool titled `mcp__server__tool` is `server.tool`. A Code Mode script's call to an MCP server or a plugin: a deny throws in the script, an ask asks, with `can_always: false`, and an allow goes. One of loom's own tools: a deny refuses it, and an ask puts it to the person first, as a tool that deletes is ([How ACP maps onto loom](#how-acp-maps-onto-loom)), and a yes runs it; an allow changes nothing, since loom's own tools follow loom's own rules. Its site is `browser_open`'s `url`, or for the other browser tools the page the browser is on; `use_device`'s are its command and path. A refusal names its rule in the activity log, as `rule r_3: …`.
- **The what-if.** `rule.check` says what an action would get, with the rules and the permission mode as they are now: the action as a rule's conditions name it (`path` can be a list), `agent` (by id or name; without, only the rules for every agent and the default mode), and `kind` (the ACP tool kind the mode sees; without, `execute` for a command, `fetch` for a site, `read` or `other` for a tool, and `read` or `edit` for a path, by its effect).
- **Agents** read the rules (`rule.list`) and ask what-ifs at once. Adding or changing one asks the person, and removing one asks too ([Agents run commands](#agents-run-commands)). An agent's own rules go when it's deleted.

## The queue

A conversation has one turn at a time. A `send` while its agent is still answering waits in the conversation's queue ([PLAN.md](../PLAN.md#work-that-waits-its-turn)), kept in the instance's database, and goes once the turn ends: as the person's message, with the id the `ack` gave, and a run of its own. One goes at a time, first first. A turn that ends `done` or `failed` lets the next go. One the person stops (`cancel`) or that runs out (`limit`) leaves the queue waiting, until they send again or steer. A turn the instance's restart cut short finished `failed`, so once it's running again the next goes.

- Every change to a queue is a `queue.changed` event: `conversation`, and `queue`, the whole queue, first first, each item its `id`, `text`, `files`, `from` (the device that sent it), `at`, and `steered` once it's been steered.
- `queue.list` (`conversation`) gives it as `queue`. `queue.edit` (`id`, `text`) changes an item's text, in its place; it can't leave one with nothing. `queue.order` (`conversation`, `ids`) puts the items in that order, the ones it names first and the rest after, as they were. `queue.drop` (`id`) drops one. `not_found` is an item that's gone, sent or dropped.
- `queue.steer` (`id`) sends one now: it goes first, and the running turn is stopped (`session/cancel`, so its run finishes `cancelled`). Then it goes, in the same ACP session, so the agent has what it did so far, and its prompt starts with a `[loom: …]` block that says it was stopped for this message. With no turn running, it goes at once.

## Tasks

A task is a long job an agent works on in a conversation of its own, so its chat with the person stays free ([PLAN.md](../PLAN.md#work-that-waits-its-turn)). The conversation has `task` (the task's id) in `conversation.created` and the `conversations` list, and it's never the one an app shows as the agent's chat, or the one a `send` that names only `agent` goes to. Everything else about it is a conversation's: the person can read it, and a `send` naming it, or its queue, goes into the task. A branch (`branch_of`), the side questions' conversation (`side`), a routine's thread of its own (`routine`) and another agent's thread with it (`asked_by`) are never the agent's chat either.

Another agent's thread with an agent is the conversation `message_agent` keeps for the two: one for each asking agent and agent, made at the first message and used for every one after, titled `From <the asking agent's name>`, with `asked_by` the asking agent's id. A plugin's panel keeps one the same way (`plugin.message`), with `asked_by` `plugin:<id>`. It's the person's, as all their agents' conversations are, to read and write in, but no app shows it as the agent's chat, and a routine or webhook posting in the agent's chat, a script's ask that names only the agent, and a `send` that names only `agent` all go to the agent's chat with the person, however new the thread is ([PLAN.md](../PLAN.md#agents)). An instance from before `asked_by` marks the threads it kept when it starts, and tells the devices with `conversation.updated`.

- A task is `id`, `agent`, `conversation`, `title`, `status`, `run` (its run now, or its last), `by` (the device or agent that started it), `attempts`, `at` and `updated` (Unix ms), `result` once it's `done` (the start of the agent's last answer, up to 2000 bytes), and `error` while it's `failed`, or `paused` by a limit.
- `status` is `running`, `paused`, `done`, `failed` or `cancelled`. Any run in the task's conversation is the task's: it's `running` while one goes, and the run's end ends it. `done` is done, `failed` is failed, and a run stopped any other way (the person's `cancel`, a steer, `limit`) leaves it `paused`, unless `task.cancel` stopped it, which makes it `cancelled`.
- Every change is a `task.updated` event, with the whole `task`.
- `task.start` (`agent`, `prompt`, `title`?: the prompt's first line if left out) makes the conversation, puts the prompt in it as the person's message, and starts a run. `task.list` (`before`?: a task id, `limit`?: 50 unless it says, up to 200) gives `tasks`, newest first, after `before` when it's given, but not those of deleted agents, and `more`: whether there are older ones. The desktop app shows the newest 50, and older ones a page at a time.
- `task.pause` (`id`) stops a running one, which stays `paused`. `task.resume` (`id`) carries a paused one on, in the same ACP session, with a run whose prompt is only a `[loom: …]` block that says so. `task.cancel` (`id`) stops a running or paused one for good. `task.retry` (`id`) tries a `failed` or `cancelled` one again the same way, as its next `attempts`. A task in the wrong state for the command is refused `not_now`, and one whose agent is working on it already `busy`.
- A task that was `running` when the instance stopped carries on once it starts again: its run was `failed` by the restart, and a new one starts with a `[loom: …]` block that says the instance restarted.
- An agent starts one with `start_task` (`title`, `prompt`), with itself as `by`. It may have three running that it started itself.

## A backup brain

An agent's turn moves to its backup program when its own can't answer, and the person can move a chat to another program by hand ([PLAN.md](../PLAN.md#a-backup-brain)).

- **The backup** is the agent's own `backup` (a program's name), or the default's `backup` where it has none; `"none"` is none, whatever the default says. `agent.create`, `agent.update` and `defaults.put` refuse one that isn't installed (`not_found`) or isn't the person's to use (`not_allowed`).
- **A turn moves** when its program's `session/prompt` or `session/new` error says its usage, quota or rate limit ran out, when the person is past the tokens the owner shares for it, or when it didn't start: it wouldn't run, its `initialize` failed, it ended before it was ready, or `session/new` failed. If the agent has a backup that's another program, and this turn hasn't tried it yet, the turn goes on there: the same run and the same reply, whose next block is a `notice` (a delta of kind `notice`, then in `message.created`), one line such as "Claude Code ran out of usage, so Codex answers this chat now." Otherwise the run finishes as before (`failed`, or `limit` for the shared tokens).
- **The chat stays there.** It's moved to the backup as by `conversation.move`, with `conversation.updated` (`id`, `program`), but with no summary waiting for its next turn, since this turn's prompt starts with it already. It stays until the person moves it back.
- **`conversation.move`** (`conversation`, `program`) puts a chat on another program: `conversation.updated` with `program`, and the chat's `program` from then on. Its next turn there starts its reply with a `notice` ("Moved from Codex to Claude Code, with a summary of the chat so far.") and its prompt with loom's summary. It's `busy` while a turn is running in the chat, `not_found` for a program that isn't installed, `not_allowed` for one the person can't use, and changes nothing for the program the chat is on already.
- **Loom's summary** is a `[loom: …]` block made from the chat itself, not by a model: what the person asked (the first line of each of their 12 newest messages, and how many came before), then the newest 6 messages word for word, each cut at 1,200 characters, in 8,000 bytes at most, and that `search_messages` finds the rest. It's given once, with the first prompt on the new program, which starts a session of its own there. A chat that comes back to a program it was on before starts a fresh session there too, since the old one missed what the other program said.
- **On another program than its own**, an agent's sessions are set to the person's default `config` for that program, not the agent's own `config`, and what that program offers doesn't become the agent's `options`.

## Group chats

A group chat is a conversation where several of the person's agents take part, and a router picks which one answers each message ([PLAN.md](../PLAN.md#group-chats)). It's a conversation like any other (`send`, `messages.list`, the queue, pins, `conversation.rename` and `conversation.delete` work on it as they do on a chat), with `agent` `""` and `room`:

```json
{ "id": "c_12", "agent": "", "title": "Money", "at": 1795187131000,
  "room": { "name": "Money", "members": ["a_2", "a_5"], "color": "amber", "default": "a_2",
            "corrections": [ { "id": "k_3", "text": "what did the VPS cost in October?", "to": "a_5", "from": "a_2", "at": 1795187139000 } ] } }
```

- `members`: the agents in it, by id, in the order the person picked them. Everyone's has `everyone: true` and lists every one of the person's agents as they are now, so it changes when one is made or deleted, with `conversation.updated`.
- `color`: one of the thread colours, `#rrggbb`, or `""` for none. `default`: the member a message goes to when the router can't pick, or `""` for the one that answered last.
- `corrections`: the newest 20 the person made with `room.resend`, newest last: `text`, the start of their message (300 bytes at most), `to`, the agent it should have gone to, and `from`, the one the router picked.
- `conversation.created`, `conversation.updated` (with the whole `room` as it is now) and the `conversations` list carry it. `title` is its name.
- **Everyone** is made by the instance for a person once they have two agents, when one of their devices connects, with `conversation.created`. It can be renamed but not deleted (`not_allowed`), and its members can't be changed.

**A message there.** A `send` into a group chat is `ack`ed as any is, with its `run`. Then one member is picked, and `run.started` comes for that run with that member as `agent` and `routed`, how it was picked:

```json
"routed": { "by": "router", "why": "money question" }
```

`by` is `router`; `mention`, for a message whose text mentions a member (`[@Hermes](loom:agent/a_5)`, or `@Hermes` typed), the first one it mentions, with no router call; `handoff`, with `from`, the agent that handed it on with `hand_off`; `person`, from `room.resend`; or `default`, when the router named no member, said nothing within 8 seconds of being asked (or its program and session didn't start within a minute) or couldn't run, so the message went to the chat's `default`, the one that answered last, or its first member. A room with one member always goes to it, `by` `default`. The reply's `message.created` has `routed` too, and `agent`, as every reply has. The apps draw it as one line over the reply: "→ Ledger · money question", "→ Hermes · you asked", "→ Hermes · handed off by Ledger", "→ Hermes · you picked".

**The router** is a session on an agent program, with no tools and no MCP servers: the person's `router_brain` (left `""`, their default brain when its name or registry id has `claude`, or else the first program that does, or else their default brain), set to `router_model` (`haiku` until they change it) with `session/set_config_option` where the program offers a `model`. Its prompt is one `[loom: route …]` block: what it's for, the rules (one member, always; follow-ups stay with the one that answered last until the subject changes), then as JSON the `members` (each `name`, `for`, `skills` and `mcp`), `last` (the name of the one that answered last), `recent` (the chat's newest 8 messages, each `who` and `text`, 400 bytes each), `examples` (the corrections, each `text` and `to` by name) and `message`. It answers with `{"agent": "NAME", "why": "a few words"}`, which the instance finds in its words; a name that isn't a member is no pick. A session takes 20 picks, then the next starts a fresh one, as does one on another model than `router_model` now. Its usage counts toward the person's as a turn's does, as program `router_brain`, kept as the run `route:RUN` (RUN the run its pick started) in `usage`, which `room.summary` gives with the chat's. **Ready ahead:** once the person has a group chat, the router's program starts and makes a session (`session/new`, then `session/set_config_option` to `router_model` where it's offered), before any pick, on `hello`, `room.create`, `messages.list` of a group chat without `before`, and a `defaults.put` that changes `router_brain` or `router_model`; and after a pick that leaves its session with 20, the next one is made at once. A pick that comes meanwhile waits for it. The program stops after 30 idle minutes, as an agent's does.

**The member that answers** answers in a session of its own for the group chat, kept under the seat `CONVERSATION/AGENT` in `acp_sessions`. Its prompt starts with a `[loom: …]` block of what was said in the chat since it last answered there, newest last, each line `Your person:`, or another member's name and a colon, then its words (the newest 40 messages, 24,000 bytes at most), and once a session, the chat's name and its members, and that `hand_off` gives a message to another of them. Where its seat's session would go on afresh (nearly full, its tools changed, idle for 12 hours, or not picked up again after a restart), only that member's does: the block before it is `[loom: Your part in this group chat goes on from an earlier session of yours, which grew too long, so this one starts fresh. … The chat up to your last reply there, newest last: … What was said since is below. For anything older, search_messages finds it.]`, each message `Your person:`, `You:` for its own, or the other member's name. The chat gets no `fresh` mark. The group chat's `context` is the seat's of the member that answered there last, with `agent`: a seat's goes to the apps while its member answers, or when it's the one the chat shows already.

**`hand_off`** (`agent`, a member by name or id; `why`?) is one of loom's tools, only in a group chat. The agent's turn ends as it would; then the message it answered goes to that member, as a new run with `routed` `{by: "handoff", from, why}`. A message is handed on 3 times at most, and never to an agent that had it already: the tool says so, and the turn ends as it would. Outside a group chat it's refused, saying to use `message_agent`.

**`agent.for`.** An agent has `for`, one line of what it's for (200 characters at most), which `agent.create` fills from its instructions' first line that isn't a heading when it isn't given, and `agent.update` changes. Not a revision. The router reads it.

## Agents run commands

An agent runs the commands below as its person's app does, with their rights ([PLAN.md](../PLAN.md#agents-can-do-anything-the-app-can)), through two of loom's own tools:

- `list_commands` gives each command an agent can run: `name`, `args` (in words), `does`, and `how`: `runs at once` or `asks your person first`.
- `run_command` (`name`, `args`, an object) runs one. Its result is the command's `value` as JSON, or its error's message. Which run at once and which ask is in `server/commands.calx`.
- **At once**: `conversation.create`, `rename`, `branch` and `move`, `messages.list`, `search` and `pinned`, `message.pin`, `agent.create` (always with `permission: "ask"`, and from an isolated agent, `isolated` and its own `network` unless it names a narrower one), `agent.update`, `agent.revisions`, `agent.rollback`, `activity.list`, `memory.list` and `put`, `table.list`, `create`, `write`, `rows` and `delete_rows`, `profile.get` and `put`, `defaults.get`, `rule.list` and `rule.check`, `routine.list`, `put`, `run`, `runs`, `next` and `calendar`, `task.*`, `queue.list`, `edit`, `order` and `drop`, `skill.list`, `get` and `put`, `plugin.list`, `get`, `put` and `revisions`, `mcp.list` and `mcp.test`, `secret.list`, `acp.list` and `acp.registry`, and `device.list`. What changes something is in the activity log as "Ran" and the command's name. A task started by another agent is told so, and an agent can start three running tasks for each agent.
- **Asks first**, every time, as one of loom's own tools that asks ([How ACP maps onto loom](#how-acp-maps-onto-loom)), titled with what the command does, with its `args` as `detail`: every `.delete` and `.remove`, and `device.revoke` (tool `delete`); `instance.move` (tool `move`); and everything else (tool `other`), among them `mcp.put`, `acp.install`, `acp.settings`, `acp.share` and `acp.members`, `invite.*`, `user.list` and `user.role`, `device.rename`, `link.create`, `instance.phones`, `instance.relay`, `instance.update`, `instance.updates` and `server.offer`, any command not listed, `rule.put`, `defaults.put` (one with a `permission` looser than the agent's own is refused before it asks), and `agent.create` with a `folder` or `agent.update` with a `permission` (`null` included, which follows the default), a `folder`, `isolated` or `network` (one that would give the agent more than the asking agent has, out of a sandbox or a wider network, is refused before it asks). A yes runs it as the device that gave it, so commands only a device may send work. `acp.install` answers once it's installed.
- **A token saved to a file**: `token.make_to_file` (`kind`, `path`, `replace`?) has loom make a token for the instance's owner and write it to a file on the instance's computer, without the agent seeing it, for a dashboard or a script there. Only `kind: "status"` for now, the token `scripts.status_on` makes ([`/api/scripts/*`](#apiscripts)); any other is refused, saying so. `path` is absolute, with no `..`, in a folder that's there, with no link at the file or any folder on the way, and not inside the instance's data folder (by the path as given and where it really is). There's no file there unless `replace` is `true`, and while the person has a status token, only `replace` ends it. What doesn't hold is refused at once, as `token-file`, asking no one, and so is an agent whose person isn't the owner. Otherwise it asks every time, on one of the person's devices (tool `other`, `can_always: false`, never answered by a script: [`/api/scripts/*`](#apiscripts)), titled `Save a new status token to <path>, as <agent> asks`, with `, ending the status token you have` and `and writing over the file there` where they're so, and its `args` as `detail`. On a yes it's all checked again, then the token is made and written to a new file only its owner can read (mode 600, made so: `O_EXCL`, never through a link), synced to the disk, or with `replace`, to one beside it that's moved over the old, so there's never a part of a file; on Windows, where only the read-only bit is a mode, the file takes its folder's permissions. A folder loom's user can't write in says so, and nothing is left. Only its SHA-256 is kept, with the path and the agent's name (`scripts.get`'s `status_path` and `status_by`), and with `replace` the person's last status token ends. The agent's call is answered only `Saved to <path>.`: the token is never in a tool's result, a message, the activity log (`Saved a new status token to <path>`) or an event.
- **Never**: `account.delete`, `password.reset`, `password.set`, `link.answer` (only the person says yes to a new device), `secret.put` (a secret's value is never an agent's) and `secret.sites`, `mcp.sign_in` and `mcp.signed_in` (only the person signs in), `catalog.list`, `catalog.connect` and `mcp.agents` (the person connects servers from Spools and picks their agents; an agent proposes one with `add_mcp_server`), `plugin.allow`, `plugin.cmd`, `plugin.data.*`, `plugin.message` and `chat.ask` (agents have `message_agent`), `push.*`, `browser.*` (agents have their `browser_` tools), `proposal.*` (agents have `propose_skill`, `propose_note` and `fix_skill`; keeping, putting off or undoing one is the person's), `queue.steer` and `scripts.*`. An agent can't delete itself, loosen its own permission mode or change where it runs itself (`own-place`), or give another a looser mode, or more of a network, than its own.

## The agent's browser

Each agent has a browser of its own ([PLAN.md](../PLAN.md#the-agents-browser)): a Chrome, Edge or Chromium on the instance's computer in Chrome's own headless mode (`--headless=new`), which the instance drives over the DevTools protocol, one per agent, on a thread of its own. `LOOM_BROWSER` names the one to run; otherwise it's the first Chrome, Edge or Chromium installed in the usual places or on PATH, a snap's last, and on Windows where the registry's App Paths say. A Mac with none fetches the headless shell loom pins, checked against its SHA-256, on the first browser call, and a Linux server without one fetches Chrome for Testing's newest; that call's result starts with a note saying so, and the person's activity has a `browser` entry. Its tools are loom's own, `loom.browser_*` in Code Mode, not an MCP server's. An instance's first start on this release removes the MCP server named `browser` that the MCP servers page's old Add a browser set up (`npx` with `-y @playwright/mcp@latest` and no `env`), and each agent's pick of it, with an `agent.updated` for each agent changed and an `agent` entry in the activity; any other server stays.

- **When it runs.** It starts on the agent's first browser call. It stops, with every process it started, once nothing uses it: when a turn of the agent's ends and no other turn of its is going, no session watches it without `follow`, and the person hasn't taken it over or isn't recording; or once those end after the turn, as when the last such session stops watching or the person hands it back. It also stops on `browser_close`, when the agent is deleted or moved or the instance stops, and after 5 minutes with no call and nothing from the person who has it, watched or not, unless a call or an ask waits. A call waiting on it then is answered `The browser closed.` The next call starts it again; unless that call is `browser_open` or `html_preview`, it first opens the page it was on (an `http`, `https` or `file` page, not after `browser_close`), and its result starts with a text `[loom: Your browser had closed since you last used it, so it opened URL again, where you were. Refs from before are gone: take them from a new outline.]`. Stopping, it asks the browser to close (`Browser.close`), waits up to 5 s for it to end, and then ends it and every process it started. A browser ends with its instance however the instance ends, killed or crashed too, except the desktop app's on Linux and macOS. On starting, an instance stops each browser it left running from before, by the DevTools address in its profile's `DevToolsActivePort`, and waits up to 5 s for it to stop listening there.
- **What it needs on Linux:** a browser that isn't a snap, and fonts, as `fontconfig` lists them: any, ones for Chinese, Japanese and Korean, and emoji. `loom status` gives them as `browser`: `{"path", "missing": [...]}`. A browser that's missing, or doesn't start for lack of a library, fails the call saying what to install; with no fonts at all, the first call's result starts with a `[loom: …]` text saying pictures show no text.

- **The profile** is `browsers/<user>/<agent>` in the instance's data, not the agent's folder, so cookies and logins last between calls but the agent can't read them. Deleting the agent, or removing the person, stops its browser and removes the profile.
- **The tools:** `browser_open` (`url`), `browser_snapshot`, `browser_click` (`ref`), `browser_type` (`ref`, `text`, `submit`?, `clear`?, true unless false), `browser_press` (`key`: `Enter`, `Tab`, `Escape`, `Backspace`, `Delete`, the arrows, `Home`, `End`, `PageUp`, `PageDown`, `Space`, or one character), `browser_scroll` (`direction`, `down` or `up`), `browser_back`, `browser_screenshot` and `browser_close` (closes it now; `Your browser wasn't open.` if it wasn't, and refused while the person has it).
- **The outline** that `browser_snapshot` gives, and `browser_open`, `browser_click`, `browser_press`, `browser_back` and a submitting `browser_type` give after they act, is the page's accessibility tree: a line for each part that means something, indented under what it's in, as `- role "name": value [states] [ref=e12]`, after a first line `Page: <title> (<address>)`. Parts that only hold others are left out unless they're named. A ref lasts until the next outline. It's cut at 40,000 bytes.
- **A screenshot** is an MCP `image`, a JPEG of the 1280 × 800 page, with the page's line, and a last text block naming it for `attach_files` ([Files in a reply](#files-in-a-reply)).
- **What it opens:** `http`, `https`, `file` and `about` pages. An agent in a sandbox (a member's, or one marked isolated) opens only `http` and `https`, and its browser loads nothing from the instance's computer (`localhost`, `127.*`, `[::1]` and every IPv6 address written as one) or a private network (`10.*`, `172.16–31.*`, `192.168.*`, `169.254.*`, `0.*`). Downloads are refused. One closed to the network reaches only the hosts on its list, through its egress ([Sandboxes](#sandboxes)), and `browser_open` refuses another at once.
- **Watching it.** `browser.watch` (`agent`, `on`? true unless false, `follow`? false, `width`? and `height`?) has this session watch that agent's browser, or stop; a session watches one at a time, and stops when it closes. A watch keeps the browser open after the agent's turn; one with `follow`, as an app's small window that shows while the agent browses, doesn't. `width` and `height` are the size the app shows the page at, in the screen's pixels; left out, the page's own. Watching again with another `follow` or size changes it. It answers with the browser's state, as `browser.state` has it. While anyone watches, the instance sends each watching session `browser.frame`s as the page changes, a JPEG of it each, under 1 MiB, the browser drawing them at the biggest size any of them shows it at (the screencast's `maxWidth` and `maxHeight`), so a small window gets small ones. It doesn't open a browser that isn't.
- **Taking over.** `browser.take` (`agent`) gives the person the browser, if it's open: the agent's browser calls wait meanwhile, in order. `browser.input` (`agent`, `kind`, and for each kind: `click` and `move` with `x` and `y`, the page's own pixels as the frames are; `wheel` with `x`, `y` and `dy`; `key` with `key`, as `browser_press` takes it, and `modifiers`?, a list of the keys held with it: `shift`, `ctrl`, `alt` and `meta` (⌘ on a Mac); `text` with `text`) is a click, a scroll, a key or text typed, refused with `not_yours` unless they've taken over. Ctrl or ⌘ with A, C, X, Z (with Shift too) or Y selects all, copies, cuts, undoes or redoes, whichever computer the app and the instance are on; a paste comes as text, from the app's own clipboard. A session's `browser.*` commands are handled in the order they came, so an app sends `browser.take` and the click that took it over one after the other, without waiting. `browser.give` (`agent`) hands it back: the calls that waited go on, and each `browser_ask_person` gets its answer. `not_open` is a browser that isn't open, and `not_found` an agent that isn't theirs.
- **`browser_ask_person`** (`why`) asks the person to take over, for a login, a code or a CAPTCHA, with a `notify` (so a phone gets it too) and `browser.state`'s `asked`. Its result comes once they hand the browser back: how many clicks and keys they gave, and the page's outline. It needs the browser open.
- **Secrets go in unseen.** `browser_fill_secret` (`ref`, `secret`, `submit`?) types one of the person's secrets (`secret.put`) into a field. The first time a secret goes to a site (its host and port, or a file on the instance's computer), it asks the person in the agent's chat, as the tools that delete do (`approval.needed`, titled "Fill your secret NAME on HOST"), and a yes keeps that site for that secret. Either way the agent's result says it's in, never what it was, and every outline after names a filled secret `[secret NAME]` (or `[secret what your person typed]`) wherever the page shows it. A screenshot shows the page as it is.
- **Asking for one on a card.** `browser_ask_secret` (`why`; `ref`, `kind`?, `masked`? and `save`?, or `fields`?, up to 4 fields on the page at once, each with its `ref`, `label`?, `kind`?, `masked`? and `save`?; `submit`?) asks the person on a `secret_ask` card in the agent's reply ([Messages and streaming](#messages-and-streaming)), with a `notify` and `browser.state`'s `secret`. A field's `kind` is `password` (the default: masked, and offered to keep), `username` (an email or a user name: shown, and offered to keep) or `code` (a one-time code: shown, and never offered); `save` is the name it's offered to be kept as, made from the page's host when it's left out (`ntnu-password` for `innsida.ntnu.no`), and empty to offer none. It's refused when the agent isn't answering in a chat, or answers for a routine, whose turn the person doesn't see. `browser.secret` (`agent`, and `fields`, one `{value, save?}` for each of the card's fields in order, or `value`, the only one's, as an app from before cards sends it) answers it. With every value given, each with a `save` is kept as that secret, as `secret.put` keeps one, with the card's site allowed for it, so `browser_fill_secret` asks no one there; and they're typed into their fields, the last submitted with `submit`, if the page is still on that site. With none, it's a no. One given and another not, or a `save` that can't name a secret, is `bad_request`, and the card still asks. It answers with the browser's state and `saved`, the names kept. The agent's result says it's in, and what it was kept as, never what it was (`Filled in what your person typed. They kept it as ntnu-password for innsida.ntnu.no, so from now on browser_fill_secret fills it in there without asking.`), and a no is `Your person said not now, so nothing went in.` An ask its turn ends on, or whose browser closes, is answered `The turn ended before your person answered.` or `The browser closed.`, and its card says `closed`.
- **What the agent's told of secrets.** Each session is told once, with its first prompt, never to ask for a password or a code in the chat, and to fill one or ask for it with these tools. A browser call's result, but `html_preview`'s and one that failed, gets a last text block, `[loom: …]`, naming the secrets the person keeps for the page's host (`secret.list`'s `sites`), on the first call there since the browser was on another host or started; and, the first time a page there shows a field for a password or a one-time code, saying to fill one with `browser_fill_secret` or ask with `browser_ask_secret`, never in the chat. It never has a value.
- Each `browser_open`, each ask and each hand back goes in the activity log as kind `browser`.
- **Teaching a task.** `browser.record` (`agent`, `on`? true unless false) starts recording what the person does in the browser they've taken over (`not_yours` before they have), or stops it; handing the browser back, or its closing, stops it too. It answers with the browser's state. See [Teaching a task, and proposals](#teaching-a-task-and-proposals).
- **Site notes.** A note tied to a site (`memory.put`'s or `remember`'s `site`) is added to the result of each of the agent's browser calls but `html_preview`'s and one that failed, as a last text block, `[loom: Your notes for SITE, kept for when you're on it: …]`, each note as `- ID: TEXT` (`(shared)` after the id for a shared one): the agent's own and the shared ones tied to the page's site or one it's under (a note for `example.com` is shown on `mail.example.com`), newest first, 20 at most. They come once a visit: again only after the browser has been on another site, or has closed.
- **`html_preview`** (`html`, the whole page, up to 2 MiB; `width`? 1280 and `height`? 800, the window in pixels; `full_page`? false) draws the page in a new background tab of the agent's browser (`Target.createTarget`, then `Page.setDocumentContent`), with the same blocked addresses for a sandboxed person's, waits for it to load and 300 ms more, brings the tab to the front for its picture (`Page.bringToFront`: a tab in the background draws frames only now and then, so a picture of it waits seconds, or never comes), closes it, and brings the agent's page back to the front. Its result is an MCP `image`, a JPEG of the window, or of the whole page up to 16,000 pixels tall, and a line saying the size and what the console said: logs, errors thrown and the browser's own entries, as `level: text`, up to 50 lines of 400 bytes.
- **`html_render`** (`html`, up to 2 MiB; `title`, up to 80 bytes) keeps the page as the person's file, `<title>.html` of type `text/html`, and adds it to the reply of the turn that made the call as a `file` block ([PLAN.md](../PLAN.md#pages-in-the-chat)). It's refused when the agent isn't answering in a chat, or the person's files are at their limit. It goes in the activity log as kind `page`. An app shows a `text/html` file block as a page, opened beside the chat in a web view walled off as a [panel](#plugins) is, with nothing through the door.

## Files in a reply

An agent puts files it made in its reply ([PLAN.md](../PLAN.md#files-and-images)), each kept as the person's file and added as a `file` block.

- **`attach_files`** (`paths`, up to 20) adds them after what the turn has said so far. Each is a path in the agent's folder (relative or whole) or the shared folder, with links followed and `..` resolved, as `message_agent`'s `files` are, up to 64 MiB, or the name of a screenshot the agent took. One already in the reply isn't added again. The result says which went in and why the others didn't: not a file there, too big, or past the person's space. It's refused when the agent isn't answering in a chat, or answers for a routine. It goes in the activity log as kind `file`.
- **Screenshots.** Each picture a browser call gives (`browser_screenshot`, `html_preview`) and each device `screenshot` is kept for a while, the agent's newest 10 since the instance started, as `browser-screenshot-N.jpg` or `device-screenshot-N.jpg`, and its result ends with a text block saying so: `[loom: This picture is kept for a while as browser-screenshot-3.jpg: attach_files with that name puts it in your reply.]`.
- **Named in the answer.** When a turn ends, each file its answer names, by its whole path (`/home/me/out/chart.png`, `C:\out\chart.png`), a `file://` link or in code, goes at the end of the reply: one in the agent's folder or the shared folder that the agent made or changed during the turn, or a screenshot it took then, of a type an agent makes for its person (a picture, a PDF, an Office or OpenDocument document, RTF, CSV or TSV, text or Markdown, sound or video, or a zip), up to 25 MiB, ten a reply, none in it already and none past the person's space.
- **Told once a session**, with its first prompt: `[loom: To give your person a file you made, … put it in your reply with attach_files: …]`.

## Teaching a task, and proposals

The person teaches an agent a task by doing it in the agent's browser, and agents propose skills, notes and fixes to skills that failed from their chats. A skill waits for the person's yes; a note or a fix follows the agent's permission mode ([PLAN.md](../PLAN.md#sharing-and-learning)).

- **Recording.** While `browser.record` is on, each click, text, key and scroll the person gives with `browser.input` is kept as a step, before it goes to the page, and so is each page they're on: `{"kind": "page", "title", "url"}`; `{"kind": "click", "on"}`, what's under the click, named as an outline names it (`button "Sign in"`); `{"kind": "type", "into", "text"}`, what was typed into the field that has the focus, one step while typing goes on in the same field, with a Backspace taking its last character back out; `{"kind": "key", "key"}`, a key held with others named with them, as `Ctrl+A` or `Shift+Tab`; and `{"kind": "scroll", "direction"}`, one for a run of scrolls one way. 300 steps at most, each text cut at 500 bytes. What goes into a password field, or one marked for a password or a one-time code, is never kept: the step is `{"kind": "type", "into", "secret": true}`. A secret filled into the browser shows as `[secret NAME]`, and when the recording ends, each of the person's secrets that's anywhere in its steps is too. `browser.state` says `recording` and how many `steps` there are as they come.
- **The write-up.** A recording that ends with something done in it is kept, with an id `rec_N`, in the activity log as kind `browser`, and starts a [task](#tasks) for the agent, "Write up the task you were shown", whose prompt has the steps, a line each, and asks for a skill with `save_skill` and the recording's id. In that task, `save_skill`, with `recording` or without, proposes the skill instead of saving it, tied to the recording, whose steps come with it in `proposal.list`. Once a recording's skill waits, writing it up again is refused until the person decides.
- **Proposing.** `propose_skill` (`name`, `description`, `body`, `why`?) and `propose_note` (`text`, `site`?, `shared`?, `why`?) keep a proposal, with `proposal.created` to their devices and an activity entry of kind `memory`. A skill's name and description are checked as `skill.put` checks them. One made in a turn of a chat goes under that turn's reply (its `message`), or for an agent looking back over its chat, under the reply it looked back from, and the reply's `learned` has it. 50 may wait at once; past that, a proposal that would wait is refused. `proposal.ask` asks an agent to look over its chats for more.
- **Kept on its own, or asked.** A note, and a fix, is kept at once when the agent's permission mode doesn't ask about everyday changes (`allow`, or a list of kinds with `edit`): its status is `kept` and `by` is `auto`, and the activity log has it as `Remembered: …` or `Fixed the skill …`. In any other mode it waits for the person, as does a note with `shared`, in every mode. The tool's result says which: `Kept: …` or `Proposed. …`.
- **Skills that failed.** `skill_failed` (`name`, `step`, `why`) keeps that the agent's following of one of the person's skills failed at a step, the newest failure of each skill, in the activity log as kind `memory`. `fix_skill` (`name`, `body`, the whole skill fixed, `description`?, `why`?) is refused unless the agent said that skill failed and hasn't proposed a fix for it since, and for a body that's the skill as it is; it proposes a fix, `kind: "fix"`, whose `fix` has `name`, `body`, `description` (`""` for as it is), `failed` (`step` and `why`) and `lines`, the change from the skill as it is (see `learned` in [Messages and streaming](#messages-and-streaming)). A fix that's kept puts the skill as the fix has it, keeping it as it was as `was` and `was_description`, with `lines` made again from the skill as it was then.
- **Looking back over a chat.** A turn of the person's (`started_by` `person`) that ends `done` in one of their agent's chats, a branch too, but not a task's conversation, a routine's or another agent's thread or side questions, pauses the chat: 4 minutes later, with nothing more said in it, the agent gets a turn in the chat's session whose prompt is one `[loom: Your chat with your person has paused. …]` block, asking for notes with `propose_note` and a fix with `fix_skill` for each skill it said failed in that chat, which it names. Its `run.started` has `quiet: true`, and its reply isn't kept, as a routine's quiet run's isn't. A message the person sends while it goes on stops it, and goes at once, without `steered`. In a group chat, each member whose turn of the person's ended `done` there since it last paused gets that turn in its own session there, one after another, the prompt beginning `[loom: The group chat you answer in with your person and their other agents has paused. …]` instead, and what it keeps goes under its own last reply there; `hand_off` refuses in it. A chat whose turn is still going when it would pause waits. A restart forgets a pause.
- **Deciding.** The person keeps or drops each with `proposal.keep` (changing it first, if they like) and `proposal.drop`, puts waiting ones off with `proposal.later`, and undoes a kept note or fix with `proposal.undo`, and every device hears `proposal.updated`, and `message.updated` for the reply it's under. Every note and fix kept is in the activity log with its `proposal`, and `undo` while it can be undone. Agents can't run `proposal.*`.

## Deleting a message

`message.delete` (`message`) deletes a message from its chat, the person's or an agent's, for every device ([PLAN.md](../PLAN.md#deleting-a-message)). The instance keeps a mark in its place, the message with no blocks and with `deleted`, and nothing else of it: its blocks (words, files, cards), `links` and `pinned` go, and it's out of search. A file it had stays kept as the person's file. Its answer is `{message}`, the mark, and every device gets the mark as `message.updated`:

```json
{ "id": "msg_311", "conversation": "c_9", "role": "agent", "agent": "a_2", "run": "run_77", "status": "done", "at": 1764062520, "blocks": [], "deleted": { "at": 1764062601, "by": "d_1" } }
```

- `deleted` has `at`, when, and `by`, the device or agent that deleted it. The apps draw the mark as a quiet line: "You deleted a message", or "Casper's message was deleted" for an agent's.
- Each message that replied to it has its `reply` kept as who it answered, with `text: ""` and `deleted: true`, and comes again in `message.updated`; the apps' quote says the original was deleted.
- Deleting a mark answers with it as it is. A reply still being written is refused with `busy`, and a message that isn't there with `not_found`.
- The agent is told once, with the next message in that chat that starts a run, in a note before the rest: `[loom: Your person deleted these messages from this chat: msg_311, your reply that began "The build passed at 14:02."; msg_9, their message. They're gone: don't use, repeat or bring up what they said.]`, each with up to 200 bytes of how it began. In a group chat each member is told once, with its own next run there, and another member's reply is named as `Casper's reply`.
- An agent can run it through `run_command`, and it asks the person first, as every `.delete` does.

## The event log

- **Every change** the instance makes for a person is an event with a `seq`, counted per person from 1. Each `seq` is one more than the last, with no gaps.
- **An event is stored before it's sent.** An app that has `seq` N has everything up to N.
- **The app stores the `seq`** of the last event it applied, in the same transaction as the change it made to its cache. That's its cursor, and it sends it as `after` on the next `hello`.
- **Every device of a person** sees the same events in the same order, including events about what that device did itself.
- **Old events are pruned** after 30 days, keeping at least each person's newest 10,000. The instance looks once an hour. A device further behind than that, or more than 10,000 events behind, gets `reset: true` and a snapshot.
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
  "status": "done",
  "blocks": [
    { "kind": "thought", "text": "The user means the CI run on main." },
    { "kind": "tool", "call": "tc_3" },
    { "kind": "text", "text": "Yes. The last build on main passed at 14:02." }
  ]
}
```

A reply that names files in its text, in `code` like `notes/hours.md`, also has `mentions`: each of them (ten at most) that is a file in its agent's folder or the shared folder, with `path` as the reply wrote it, `name`, `size` and `mime` (by its name). The apps show each where it's named, or as a chip under the reply, and `agent.file` opens one. A PNG or JPEG of 25 MiB at most that isn't among the reply's `file` blocks (by name and size) has `thumb` too: a thumbnail of it, at most 512 pixels a side, kept as the person's file, to fetch with `file.get`, without the picture itself. It's made after the reply is kept and comes with `message.updated`; a picture named again as it was has it at once. The apps show those pictures in the reply as its files' pictures are, and open them in the viewer, through `agent.file`.

A person's message can mention another of their agents, or a file, as a link in its text ([PLAN.md](../PLAN.md#mentions)): `[@Casper](loom:agent/a_2)`, or `[hours.md](loom:file/a_1/notes%2Fhours.md)` with the file's agent and its path as `agent.files` gives it, with space, `(`, `)`, `%`, `[`, `]`, `<`, `>` and `\` escaped as `%XX`. The message keeps the links as they are. The agent reads each as `@Casper` or `` `notes/hours.md` ``, with a note after the text: `[loom: In this message, @Casper is Casper, another of your person's agents: message_agent with agent "Casper" reaches it; `notes/hours.md` is a file in your folder.]`. The agent mentioned in its own chat reads `@Main is you`. A file that isn't in the agent's folder or the shared one isn't named in the note. A skill picked from the composer's + menu is `[review](loom:skill/review)` ([PLAN.md](../PLAN.md#the-composers--menu)): the agent reads its name, `review`, and the note says `review is a skill your person picked for it: read it with read_skill and follow it`, for one of the person's skills.

A reply its agent kept or proposed notes or fixes from has `learned`: each of them, oldest first, as `proposal.list` gives it without a fix's skill as it was or a skill's body: `id`, `agent`, `kind` (`note` or `fix`), `status`, `why`, `by` once decided (`auto` for one the permission mode kept), `later` once the person put it off, and `note` (`text`, `site`, `shared`) or `fix` (`name`, `body`, the skill as fixed, `failed` with the `step` and `why` the agent said, and `lines`, its change: each `{op, text}`, `op` `-` for a line removed, `+` for one added, ` ` for one around them and `…` for lines left out, 400 at most) ([Teaching a task, and proposals](#teaching-a-task-and-proposals)). It comes with the reply, for what the turn kept or proposed itself, and with `message.updated` for what the agent kept looking back over the chat after, and each time one of them is kept, dropped, put off or undone.

A reply whose words end in a Sources list (a line such as `Sources:`, `**Sources**` or `## Sources`, then only links: `- [title](url)`, `- title: url` or `- url`) has it taken out of its last text block, and has `sources` instead: each `{url, title, site}` in order, `title` as the reply wrote it, maybe empty, and `site` the host without `www.` ([PLAN.md](../PLAN.md#links-and-sources)).

A message that links to pages, in its words or its `sources`, has `links` for those the instance has looked up: by each address, `{title, site, icon?, image?}`, `title` the page's own, or its address cleaned up when it has none ([PLAN.md](../PLAN.md#links-and-sources)), `icon` the site's icon, a PNG kept as the person's file, to fetch with `file.get`, and `image` the page's preview picture (its `og:image`, a JPEG or a PNG at most 480 pixels a side, kept the same way), only for a page looked up with `link.preview`. Only https pages of public hosts are looked up, twelve a message at most. One looked up after the message was kept comes with `message.updated`.

A person's message that replies to another has `reply`, what it replies to as it was when sent ([`send`](#send)): `message`, its id; `role`, and `agent` for an agent's or `from` for a person's; `text`, the words quoted, the part selected or the message's words (an agent's reply its last text block that isn't a subagent's, a person's message all its text), up to 2,000 bytes; `part: true` when it's a part, and `ask: true` for asking about it; and for a whole message its `files`, up to four, as its `file` blocks have them without `kind`. The apps draw it over the message's bubble, and a click on it goes to the original. Once the original is deleted, `reply` keeps `message`, `role` and who said it, with `text: ""` and `deleted: true` ([Deleting a message](#deleting-a-message)).

```json
"reply": { "message": "msg_311", "role": "agent", "agent": "a_2", "text": "14:02", "part": true }
```

In a person's message, a bare address is a link they pasted as a piece, which the apps show by its page's title and its site's icon, and `<https://…>` one they wrote or pasted as plain text, which they show as the address itself, untitled ([PLAN.md](../PLAN.md#links-in-the-composer)). An attached file is its name in brackets, `[image.png]`, where it was attached.

```json
"sources": [ { "url": "https://9to5google.com/2026/11/18/neon/", "title": "Gemini 4 Neon", "site": "9to5google.com" } ],
"links": { "https://9to5google.com/2026/11/18/neon/": { "title": "Google launches Gemini 4 Neon", "site": "9to5google.com", "icon": "6c1f…" } }
```

- `role` is `person`, `agent` or `loom`. A `loom` message is a line across the chat from loom itself, no one's words, with one `mark` block and no `agent`, `from` or `run`; the apps draw it as a line, and a transcript, a summary, a reply's quote and the chat's newest message for its cards leave it out. A person's message has `from` (a device id) instead of `agent` and `run`. A message one agent sent another with `message_agent` is a `person` message too, in the other agent's conversation, with the sending agent's id as `from`. One a plugin's panel sent with `plugin.message` has `plugin:<id>` as `from`, and one brought over with `import.run` has `import:openbot` or `import:grokbot`, as its pin's `by` does. An imported agent's reply has `agent` and no `run`. What a routine's agent posted with `routine_post` is an `agent` message with `routine` (the routine's id), `routine_name` (its name when it posted; posts from before have none) and no `run`. The apps say over it which routine it's from, by the name the routine has now, or else `routine_name` ([PLAN.md](../PLAN.md#routines)). A `person` message with a routine's id as `from` is a routine's prompt kept before routines ran quietly: it stays stored, and the apps don't show it.
- A deleted message is a mark, with `deleted` (`at`, `by`) and no blocks ([Deleting a message](#deleting-a-message)).
- An agent's reply to a run has `status`, how its turn ended, as `run.finished` says it: `done`, `cancelled`, `failed` or `limit`. The apps show a reply that ended `done` as one answer, its last words, with what it said and did on the way folded under one line, and keep everything in view for one that ended any other way ([PLAN.md](../PLAN.md#one-answer-at-the-end)). A reply kept before replies said, or an imported one, has none, and reads as `done`.
- Block kinds in version 1: `text` (Markdown), `thought` (the agent's reasoning, shown folded), `tool` (a tool call, whose details are in its `tool.*` events), `plan` (the agent's plan, as `entries` with `content` and `status`), `file` (`file`, `name`, `mime`, `size`, and a picture's `width` and `height`, as [`POST /files`](#post-filesnamename) gives them) and `card` (`card`: an agent's card, below). `mark` is a `loom` message's one block, a line across the chat: `mark` is `compacted`, where the agent program compacted the session's context, with `before` and `after`, the tokens it held, where the program said them ("Context compacted · 182k → 16k"), or `fresh`, where loom carried the chat on in a fresh session because the last one's context was nearly full, or where the person asked for one (`conversation.fresh`) ("A fresh session starts here"). `notice` (`text`) is one line from loom in an agent's reply, not the agent's own words, such as that another program took the chat over ([A backup brain](#a-backup-brain)); the apps show it small and quiet, and a transcript or a summary of the chat leaves it out.
- A `card` block's `card` has `id`, `type` and `title`, and the fields of its type. An agent's (`show_card`) is `choices` (`options`), `form` (`fields`, each `label`, `kind`, `required`, `value`, what it starts as, and for a choice `options`, for a text or a number `placeholder`; and `answers`, by label, once the person has sent them), `checklist` (`items` of `text` and `done`), `table` (`columns`, `rows`), `progress` (`label`, `value`) or `facts` (`items` of `label` and `value`). A form's field `kind` is `text`, `choice` (one of its 2 to 6 options, or `""` while none is picked), `toggle` (`true` or `false`, never required) or `number` (`value` a number or null); up to 8 fields, their labels told apart. A question, `choices` or `form`, goes at the end of its reply, after all its other blocks, whenever the agent made it, and comes only with the finished reply in `message.created`, never as a `card` frame: show_card's `place` is `end` for them unless the agent says `here`, and `here` for the rest unless it says `end`. A card changed by its `id` stays where it is. The apps let a question answer, and its keys pick, only once its reply is finished and while it's the chat's newest message. Loom's own cards carry the server's `badge` (`name`, `initial`, `color`, one of the thread colours picked from the name, and `icon` when the server gave one of its own, the SHA-256 of the person's file kept of it, a PNG to fetch as any other, with `icon_dark` when it gave one for a dark background; `shared/badge.calx`). Its `sign_in` (`server`, `badge`), titled `Sign in to SERVER`, says one of the person's MCP servers needs them to sign in; the apps draw it with **Sign in**, which starts `mcp.sign_in`, and as signed in once `mcp.list` says so. Loom's own `mcp_add`, titled `Add the MCP server SERVER`, is a server the agent proposed with `add_mcp_server` ([PLAN.md](../PLAN.md#signing-in-to-mcp-servers)): `call` (what `mcp.add_answer` names), `server`, `badge`, `url`, or `command` and `args`, `settings` (each `name`, `in`, `env` or `header`, and `value`, its text, or `secret`, the name of a secret it's filled with, and `have`, whether the person has that secret yet; never a secret's value), `state` and `said`, a line in plain words: the names of the tools it offers once it's `ready`, why it `failed`, or why a sign-in didn't work; once it's `ready`, `tools` lists those names too, each on its own, for the apps to show (at most 50). Its `state` is `asking` (the apps draw **Add**, with a masked field for each secret the person doesn't have, and **Cancel**), `adding`, `sign_in` (**Sign in**, which starts `mcp.sign_in` as the `sign_in` card's does, and **Later**), `later` (**Sign in** still), `ready`, `added` (kept, but not tried on the instance: a sandboxed person's), `failed` (kept, but it didn't start) or `cancelled`. Add, Cancel and Later answer only while the reply is being written, and the apps draw `sign_in` and `later` as signed in once `mcp.list` says so. Loom's own `secret_ask`, titled with what `browser_ask_secret` asks for (its `why`), has `call`, `agent`, `site` (the page's host, or `a file on this computer`), the site's `badge` when it has a name (`shared/badge.calx`, from `ntnu` for `innsida.ntnu.no`), `fields`, each a field the person fills in (`label`; `kind`; `masked`, whether it's hidden as it's typed; and `save`, the name it's offered to be kept as, or empty for none), `state` and `saved`, the names what was given is kept as. Its `state` is `asking` (the apps draw each field, with **Save as** and the name filled in where `save` has one, which the person can change or untick; **Fill**, which sends `browser.secret` with each field's value and the name it's kept as, if it's kept; and **Not now**, which sends it with none), `filled`, `declined` or `closed`. It never has a value. A `card` frame, `{"t": "card", "run", "message", "block", "card"}`, puts one in a reply as it's written, or changes it in place.
- A subagent's `tool`, `text` and `thought` blocks have `parent`: the call of the subagent call (a Task or Agent call) that made them, a `tool` block earlier in the same reply. The desktop app draws them inside that call, folded under it with how many there are, and the iPhone app leaves them out; neither draws them in the reply. A subagent's words aren't the reply's: its answer, a push, a task's result, a transcript or a summary leave them out. A block whose `parent` isn't in the reply is drawn as the reply's own.

**While a reply is being written**, its message exists but isn't finished. `run.started` gives its id, and deltas fill its blocks:

```json
{ "t": "delta", "run": "run_77", "message": "msg_311", "block": 2, "kind": "text", "pos": 4, "text": " The last build" }
```

- `block`: which block, counted from 0. A delta for a block the app doesn't have yet starts that block, with `kind`.
- `parent`: for a subagent's words, the subagent call they sit inside, as the block's `parent`.
- `pos`: how many UTF-8 bytes of that block's text came before this piece. The app adds the piece when `pos` equals what it has, skips it when `pos + len(text)` is no more than what it has (it's already there), and treats a gap as a lost connection (it reconnects).
- A tool call doesn't stream. It comes as `tool.started` and `tool.finished` events, and its block points at it.

**When the reply is finished**, `message.created` carries the whole message, and it replaces whatever the deltas built. Then `run.finished` follows. An app that missed every delta still ends up with the right message.

**Joining in the middle of a reply.** After catch-up, before `synced`, the instance sends each run still going as deltas from `pos` 0, one per block, with everything so far. Then the rest comes live.

## Events

Version 1's kinds. Each event also has `seq`, `kind` and `at`.

### Conversations and messages

| Kind | Fields | When |
|---|---|---|
| `conversation.created` | `conversation` (the whole conversation: `id`, `agent`, `title`, `at`, `task` for a task's, `side: true` for the one where an agent answers side questions,, `branch_of` and `branch_at` for a branch: the conversation and the message it goes on from, `routine` for a routine's thread of its own: the routine, `asked_by` for another agent's thread with it: that agent's id, or `plugin:<id>` for a plugin's panel ([Tasks](#tasks)), `program` for one moved to another program than its agent's ([A backup brain](#a-backup-brain)), for a group chat `agent` `""` and `room` ([Group chats](#group-chats)), and `context` once its agent program has said how full its session's context is: `used` tokens of a `size` window, as its last `usage_update` said, `compacted`, when the program last compacted that session (Unix ms; left out for one not compacted since it began), `compacts: true` when the program compacts a session when asked (it offers `/compact`), and in a group chat `agent`, the member whose session it is, the one that answered there last ([How ACP maps onto loom](#how-acp-maps-onto-loom))), and `partial: true` for one brought over by `import.run` with more messages than come as its events | A conversation is made |
| `conversation.updated` | `conversation` (`id` and what changed: `title`, `program`, `asked_by`, a group chat's whole `room`, or `context`) | Renamed, a group chat changed (its name, members, colour, default or corrections), or moved to another program, by hand or to its agent's backup, or, once, an older instance's thread of one agent with another marked as the asker's, or its session's context filled by another percent, changed window, was compacted, or started again from 0 in a fresh session, or in a group chat another member answered |
| `conversation.deleted` | `conversation` (the id) | Deleted |
| `message.created` | `conversation`, `message` (the whole message) | A person's message is stored, or an agent's reply is finished |
| `message.updated` | `conversation`, `message` (the whole message, as it is now) | A stored message changed: it was pinned (it has `pinned`: `at` and `by`) or unpinned (it hasn't), a page it links to was looked up (its `links` has it), or it was deleted (it has `deleted` and no blocks, [Deleting a message](#deleting-a-message)) |
| `message.deleted` | `conversation`, `message` (its id) | A stored message is gone |

### Runs

A run is one turn of an agent, answering one message.

| Kind | Fields | When |
|---|---|---|
| `run.started` | `run`, `conversation`, `agent`, `revision`, `message` (the reply's id), `answering` (the person's message id, or `""` for a turn with none), in a group chat `routed`, how its agent was picked ([Group chats](#group-chats)), and for a routine's quiet run or a step of its flow, `routine` (its id): the apps don't draw that run in the chat, whose reply isn't kept unless it failed ([PLAN.md](../PLAN.md#routines)); for an agent looking back over its paused chat, `quiet: true`, which the apps don't draw either, and whose reply is never kept ([Teaching a task, and proposals](#teaching-a-task-and-proposals)) | The agent was given the message |
| `run.finished` | `run`, `status` (`done`, `cancelled`, `failed` or `limit`), `error` (for `failed`), `usage` (`input`, `output` and `total` tokens, where the agent program reports them with ACP's `usage`) | The turn is over, after `message.created` for its reply. It comes for every run that started, however it ended, even when its reply couldn't be kept. Only the owner's agents run as the instance's own user, unless they're isolated. Anyone else's run in their sandbox ([Sandboxes](#sandboxes)), and on an instance without sandboxes their runs are `failed`, saying so |
| `queue.changed` | `conversation`, `queue` (the whole queue, first first) | A message waits for its agent, or one that waited was edited, reordered, dropped, steered or sent ([The queue](#the-queue)) |
| `task.updated` | `task` (the whole task) | A task started, or its status changed ([Tasks](#tasks)) |
| `table.changed` | `table`, `by` (the agent or device), `dropped`? (`true` when it's gone) | A shared table was made, written to, had rows deleted, or was dropped ([PLAN.md](../PLAN.md#shared-tables)) |
| `server.offered` | `address`, `by` (the device that said yes), and with a [link code](#link-codes), `link_code` and `username` (the person's here) | An agent's `add_server` was allowed: that device's core says `core.offer`, and its app adds the server at `address`, signed in with the code, or for the person to sign in to ([PLAN.md](../PLAN.md#several-servers-in-one-app)) |

### Tool calls

| Kind | Fields | When |
|---|---|---|
| `tool.started` | `run`, `call`, `block` (its `tool` block in the reply, counted as deltas count, so an app draws it there among the words while the reply is written, and the words after it after it), `parent` (for a subagent's call, the subagent call it sits inside: [Messages and streaming](#messages-and-streaming)), `tool` (a kind: `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, `other`), `title` (a few words on one line: [How ACP maps onto loom](#how-acp-maps-onto-loom)), `input` (each string in it cut at 65,536 bytes, with "…", here and in `approval.needed`'s `detail`, so a big one can't make a frame past 1 MiB), `locations` (files and lines it touches) | The agent started a tool |
| `tool.updated` | `call`, and what changed: `title`, `output`, `locations` | The agent reported progress |
| `tool.finished` | `call`, `status` (`done`, `failed`, `denied`, `cancelled`), `title` (when what finished it named it, made as `tool.started`'s), `output` (blocks, kept short: long output is cut, with `cut: true`) | The tool ended, or its turn did first. A call nothing finished when its run ends, however it ends, ends with it, before the reply is kept: `denied` for one a rule or the person said no to, as one its agent program never reported on after, and otherwise `cancelled`, with an empty `output`. So a kept reply never shows a call still running. A run a restart cut short has its open calls ended so too when the instance starts again, before its `run.finished`. The apps show a `cancelled` call as stopped |

### Approvals

| Kind | Fields | When |
|---|---|---|
| `approval.needed` | `run`, `call`, `tool`, `title`, `detail` (what exactly would happen: the command, the diff, the file), `can_always`, `risky` (loom counts it risky, `rules.risky`: it deletes, changes an agent's permission mode or moves the instance, or its command deletes a folder, wipes a disk, throws away git history or drops tables; the apps have Allow held down until it fills, about a second, rather than clicked, and offer no Always allow; the core's `shown` gives it the effect `deletes`, "Risky", unless it says Deletes), `rule` (when one of the person's rules asked, its id), and for a call to a device, `device` and `request` ([Devices as tools](#devices-as-tools)) | The agent asks first, and its permission mode or a rule sends the question to the person |
| `approval.answered` | `call`, `allow`, `always`, `by` (the device that answered, `auto` when the permission mode did, or `rule` when one of the person's rules did, with `rule`, its id, and for a deny `why`) | Someone answered, on any device. Every other device stops showing the question |

An approval nobody answers stays open while the run does. Cancelling the run answers it with `allow: false`, `by: "cancel"`.

### Agents and devices

| Kind | Fields | When |
|---|---|---|
| `agent.created`, `agent.updated` | `agent` (the whole agent, with its `revision`), and for `agent.updated`, `by` (the device that changed it, or the agent's id when an agent did, with a tool) | Made or changed, by a person or by an agent |
| `defaults.updated` | `defaults` (the whole defaults, as `defaults.get` gives them), `by` (the device that changed them; a yes to an agent's `defaults.put` is done as the device that gave it) | The person's agent defaults changed. Every agent that follows a field follows the new one from its next prompt ([Agent defaults](#agent-defaults)) |
| `rules.updated` | `rules` (all of the person's, as `rule.list` gives them), `by` (the device that changed them, or the one that said yes to an agent's) | A rule was added, changed or removed. Each agent works under the rules as they are from its next action ([Rules on what's touched](#rules-on-whats-touched)) |
| `notify` | `agent`, `title`, `body`, and for a routine's, `conversation` | An agent sent its person a notification with its `notify` tool, or a routine's agent posted with `routine_post` while it notifies (its name as `title`, the start of the post as `body`), or its run failed, or loom itself told the owner of an update it made, with no `agent` ([Updating the instance](#updating-the-instance)). The apps show it, even while someone's looking |
| `agent.deleted` | `agent` (the id) | Deleted |
| `group.updated` | `group` (the whole folder: `id`, `name`, `collapsed`, `at`), `by` (the device that changed it) | A sidebar folder was made, renamed, or opened or shut ([PLAN.md](../PLAN.md#the-sidebar)) |
| `group.deleted` | `group` (the id), `by` | Deleted. Its agents came out of it first, each with `agent.updated` |
| `emoji.changed` | `emoji` (all of the person's own, as `emoji.list` gives them), `by` (the device that changed them, or the one that said yes to an agent's) | One was added, renamed, deleted or brought back ([Emoji](#emoji)) |
| `routine.updated` | `routine` (the whole routine, as `routine.list` gives it) | Made, changed, or run: `last` says how its newest run is going |
| `routine.deleted` | `routine` (the id) | Deleted |
| `proposal.created` | `proposal` (as `proposal.list` gives it) | An agent proposed a skill, a note or a fix, which waits for the person or, for a note or a fix its permission mode keeps, is kept already ([Teaching a task, and proposals](#teaching-a-task-and-proposals)) |
| `proposal.updated` | `proposal` (as `proposal.list` gives it) | The person kept, dropped, put off or undid one, on any device |
| `flow.updated` | `flow`, as `flow.list` gives each | A routine's flow started, or one of its steps started or ended. Once `finished`, it's no longer listed |
| `plugin.updated` | `plugin` (as `plugin.list` gives it) | A plugin was put, or its person allowed what it asks |
| `plugin.deleted` | `id` | Deleted |
| `plugin.data` | `plugin`, `key` | A panel's data under `key` changed. A panel showing it on another device reads it again |
| `device.added` | `device` (`id`, `name`, `kind`, `at`) | A login, on any of the person's devices |
| `device.updated` | `device` (`id` and what changed) | Renamed |
| `device.revoked` | `device` (the id) | Signed out. The revoked device's own socket is closed with 4001 |
| `link.asked` | `ask`: `id`, `name`, `kind`, `check` and `expires` (Unix ms) | A device used a code to be added, and waits for a yes ([Adding a device](#adding-a-device)). An ask past `expires` is let go |
| `link.answered` | `ask` (the id), `allow`, `by` (the device that answered) | Answered on one of the person's devices, so the others stop asking |
| `account.updated` | `has_password` | The person set their first password, on one of their devices |
| `limits.updated` | `program` (its name), `limits` (as `acp.list` gives them) | The program reported other limits of the person's plan, or another login ([Usage limits](#usage-limits)) |
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
| `conversation.move` | `conversation`, `program` | `conversation`, as it is now, with `program`, and `conversation.updated`. Its next turn there starts with loom's summary of the chat ([A backup brain](#a-backup-brain)). `busy` while a turn runs in it |
| `conversation.compact` | `conversation`, `agent`? (in a group chat, the member whose session it is; left out, the one its `context` names) | `run`: a quiet turn of the program's `/compact` alone in the chat's session, with nothing loom tells an agent before it, which waits for its next turn ([PLAN.md](../PLAN.md#agents-run-on-acp), "One endless chat"). Its `run.started` has `quiet: true`, it leaves no reply, and the compaction comes as the `compacted` line and `conversation.updated` with `context`, as one the program makes itself. A message sent meanwhile waits for it, whatever the agent's `busy`. A session that its next turn leaves for a fresh one (nearly full but for this, its tools changed, idle for hours, or a fresh one asked for) has nothing to compact, and the turn ends having done nothing. `busy` while a turn runs in the chat, `bad_request` with nothing in its session yet or a fresh one asked for, and for a member that isn't in the group chat; `not_supported` when its program doesn't compact when asked |
| `conversation.fresh` | `conversation`, `agent`? (as for `conversation.compact`) | `{}`. The chat's next turn starts a fresh session, given the newest of the chat, its `[loom: …]` block saying `which your person chose to start afresh, so this one starts fresh.` A `fresh` line goes in the chat now, but in a group chat, and the `context` starts again at 0 of its window, not compacted. What the old session says from then on isn't kept. `busy` while a turn runs in the chat |
| `conversation.branch` | `message`, `title`? (left out, "Branch: " and the message's first words) | `conversation`, a new one with the same agent, and `conversation.created` with `branch_of` and `branch_at` ([PLAN.md](../PLAN.md#branching-a-chat)). The agent's first turn in it starts with the chat up to that message, its 40 newest, in a `[loom: …]` block. `bad_request` for a message in the side questions' conversation |
| `room.create` | `name` (1 to 80 characters, trimmed), `members` (two or more of the person's agents, by id), `color`?, `default`? (one of `members`) | `conversation`, the group chat, and `conversation.created` ([Group chats](#group-chats)). `bad_request` for a blank name, fewer than two members or a colour that isn't one; `not_found` for an agent that isn't the person's |
| `room.update` | `conversation`, and what changes: `name`, `members`, `color`, `default` (`""` for none) | `conversation`, as it is now, and `conversation.updated` with `room`. Everyone's `members` can't change (`not_allowed`) |
| `room.resend` | `message` (an agent's reply in a group chat), `agent` (another member) | `run`: the person's message that reply answered goes to `agent`, answering after it, with `routed` `{by: "person"}`, and the router's pick is kept as a correction for the chat (its newest 20), with `conversation.updated`. `busy` while a turn runs in the chat, `bad_request` for one that isn't a member or answered already |
| `room.forget` | `conversation`, `correction` (its `id`) | `conversation`, without it, and `conversation.updated` |
| `room.summary` | `conversation` (a group chat) | What its settings show: `tokens_today` and `tokens_month`, what its members' turns there used, and `router_today` and `router_month`, what the router's picks for its messages used, from the start of the day and month in UTC. `bad_request` for an agent's own chat |
| `messages.list` | `conversation`, `before`? (a message id), `limit`? (up to 100) | `messages`, newest last, and `more` |
| `chat.ask` | `conversation` (a chat), `question` (up to 4,000 bytes) | `reply`, the agent's answer, once it's done ([PLAN.md](../PLAN.md#side-questions)). It's asked in the agent's conversation for side questions (made the first time, with `side: true`), with the chat's 40 newest messages in a `[loom: …]` block, and a tool that asks is refused there. `busy` while the last is being answered. Not for agents |
| `message.pin` | `message`, `pinned`? (`false` takes the pin off), `at`? (when it was pinned, for an undo that puts a pin back in its place; no later than now) | `message`, as it is now, and `message.updated` to every device ([PLAN.md](../PLAN.md#pinned-messages)). `bad_request` for a deleted message, whose pin went with it |
| `messages.pinned` | `conversation`, `before`? (a pinned message's id: the pins after it), `limit`? (up to 100, 50 if left out) | `messages`: the pinned ones, the newest pin first; `count`, how many the conversation has; and `more` |
| `message.delete` | `message` | `message`, the mark left in its place, and `message.updated` to every device for it and each message that replied to it. `busy` for a reply still being written, `not_found` for a message that isn't there ([Deleting a message](#deleting-a-message)) |
| `agent.create` | `name`, `acp_agent`? (left out or empty, the person's default brain; `bad_request` with none), `backup`? (the program its turns move to when its own runs out of usage or doesn't start; `"none"` for none; left out or empty, the default's), `instructions`, `folder`?, `mcp`? (the names of the person's MCP servers it gets), `permission`? (the permission mode: `ask` asks about everything; `auto` allows the tools that only look, of kind `read`, `search`, `think` and `fetch`, and asks about the rest; `allow` allows everything; or a list of kinds to allow, like `"read,search,edit"`. Left out, the agent has none of its own and follows the person's default, `ask` until they change it ([Agent defaults](#agent-defaults))), `busy`? (`queue` or `steer`; left out, the default's), `color`? (one of the twelve thread colours in `design/tokens.toml`, or one of its own as `#rrggbb`, which each app makes light or dark enough for its theme; anything else is `bad_request`. Left out, the first eight go round in turn), `shape`? (the shape its thread takes in the sidebar: one of the eight that come with loom, `loop`, `coil`, `bow`, `heart`, `star`, `knot`, `snail` and `yarn`, which the apps have as design tokens; `drawn`, for the agent's own `drawing`; or left out or `""` for the plain thread. Anything else is `bad_request`), `drawing`? (one the person drew, as 8 to 4000 points `[x, y]` along one stroke, in any units, y down. The instance smooths it, spaces 96 points evenly along it and fits it to a box from -1 to 1 by its longer side, keeps that as the agent's `drawing`, and makes it the shape unless `shape` names another. A drawing that isn't one, all in one place say, is `bad_request`), `code_mode`? (`false` gives the agent every tool plainly, and its MCP servers, in place of [Code Mode](../PLAN.md#code-mode)'s `search_tools` and `run_code`; `true` if left out, as for an agent without it), `isolated`? (`true` runs it in a sandbox ([Sandboxes](#sandboxes)); a member's agents always are, and `false` from a member is `not_allowed`), `network`? (in a sandbox, the hosts it may reach: names such as `github.com`, `*.npmjs.org` for every name under one, or addresses, at most 200, each kept lowercase; `[]` for none, and `null` or left out for everything. A network on an agent that isn't isolated is `bad_request`, and so is a host that isn't one). `isolated: true` or a network on an instance without sandboxes is `not_here`: it refuses rather than pretend | `agent`, with `permission` and `busy` only when they're its own, `isolated`, and `network` only when it isn't everything |
| `agent.update` | `agent`, and what changes, `code_mode` among them, which its next session follows. `for` is its one line of what it's for, which the group chats' router reads (200 characters at most; not a revision). `shape` and `drawing` are as for `agent.create`; `""` goes back to the plain thread, and a drawing stays when another shape is picked, so `drawn` picks it again. `config` sets the agent's options by id, like `{"model": "opus", "effort": "high"}`: a value its program doesn't offer is refused, and `null` (or `""`) takes the option back to following the person's default for its program, or what the program starts on without one. Each session is set to it, over the default, before its next prompt. `permission` and `busy` set its own, and `null` (or `""`) takes either back to following the default ([Agent defaults](#agent-defaults)). `backup` sets its backup program, `"none"` for none, and `null` (or `""`) follows the default's. `isolated` and `network` are as for `agent.create`, and `isolated: false` takes its network too. A change that moves it into another sandbox, or out of one, is `busy` while it's answering; otherwise its programs and its browser stop, so its next message starts them where it runs now, and a sandbox of its own it had goes, with its egress. A new list for an agent closed to the network holds from its next connection. Each change of where it runs goes in its activity, as "Runs …" | `agent`, with its new `revision`. An agent has `options` once its program has started a session, its own or another of its person's agents' on that program (which the instance shares with each of them as it learns them, and at start): what it offers as ACP session config options, each `id`, `name`, `category` (`model`, `thought_level`, `mode` or another), `value` (what a new session starts on) and `options` (each `value`, `name` and maybe `description`). When they change, `agent.updated` comes with the agent's id as `by` |
| `memory.list` | `agent`? (that agent's notes and the shared ones; every note without it), `query`? (words, any of them, best match first; newest first without), `limit`? (up to 500, 100 if left out) | `memories`: each `id`, `agent` (empty for a shared note), `shared`, `text`, `by` (the agent or device that kept it), `at` and `site` (the site it's tied to, or empty) |
| `memory.put` | `text`, `agent`? (empty or left out: shared by all the person's agents), `site`? (an address or a host it's tied to, kept as its host and port, lowercase, without `www.`: the agent is shown it when its browser is on that site, or one under it) | `id` |
| `memory.delete` | `id` | — |
| `scripts.get` | — | `on`, `since` (when they were turned on, Unix ms, or null), `status_since` (when the status token was made, or null), `status_path` and `status_by` (where an agent had it saved with `token.make_to_file`, and that agent's name, both empty for one made here) and `port`, where the instance listens on its computer ([PLAN.md](../PLAN.md#scripts)). The person's alone, from a device |
| `scripts.on` | — | `token`, a new script token, shown this once, and `port`. Only its SHA-256 is kept, and it ends the last one |
| `scripts.off` | — | — . The token stops working |
| `scripts.status_on` | — | `token`, a new status token, shown this once, and `port`. It only reads `GET /api/scripts/status`, apart from scripts, which may stay off. Only its SHA-256 is kept, and it ends the last one |
| `scripts.status_off` | — | — . The status token stops working |
| `table.list` | — | `tables`: each `name`, `description`, `owner` (the agent that made it, or empty for the person), `at` and `rows`, how many ([PLAN.md](../PLAN.md#shared-tables)) |
| `table.create` | `table` (1 to 64 letters, digits, hyphens and underscores), `description`? (up to 500 bytes) | `table`, as `table.list` gives it, owned by the caller. `exists` for a name that's taken, `limit` past 100 tables |
| `table.write` | `table`, `rows` (1 to 500, each an object of its fields, up to 16 KiB as JSON; with `id`, it changes that row) | `ids`, each row's. Changing a row takes its writer, the table's owner or the person, or it's `forbidden`. `limit` past 100,000 rows in all. Nothing's written unless every row can be |
| `table.rows` | `table`, `where`? (fields and the strings, numbers, `true`, `false` or `null` they equal), `order_by`? (a field, or `id`, the default: the order they were written in), `desc`?, `limit`? (50 if left out, up to 200), `offset`? | `owner`, `rows` (each `id`, `by`, the agent that last wrote it or empty for the person, `at`, `updated` and `data`, its fields; and `untrusted: true` when `by` is an agent that isn't the owner) and `more` |
| `table.delete_rows` | `table`, `ids` | `deleted`, how many. Each row as `table.write` changes it, or none is deleted |
| `table.drop` | `table` | — . The person's alone: an agent's `drop_table` asks them |
| `link.create` | `user`? | `code` (one-time, `K7QF-M2XD`, for [`POST /api/link`](#post-apilink), [Adding a device](#adding-a-device)), `expires` (Unix ms, ten minutes on), `instance` and `key` (the instance's id and public key, as `/api/info` gives them), and while phones are on, `noise`: `{"on":true,"port":47420,"key":"<its Noise key, base64>"}` ([Over Noise](#over-noise)). The core adds `qr`, the QR code's text, which is the `loom://add?…` link too, and `text`, the code to paste. While phones are on, its address is a `noise://` place ([Over Noise](#over-noise)) with the instance's Noise key and this computer's IPv4 addresses on its port, in the order to try them: the local network's (10/8, 172.16/12, 192.168/16), then Tailscale's (100.64/10). Interfaces that are down, loopback, or networks inside the machine (Hyper-V's and WSL's switches, Docker's and Podman's bridges) are left out. Only a device can ask, never an agent. With `user`, someone else's id, it's a [link code](#link-codes) for their account instead, `code` and `expires` (15 minutes on) alone: from the owner for anyone but the owner, from an admin for members |
| `push.register` | `token` (the device's APNs token, hex), `env` (`production`, or `development` for a debug build). Or, [through the push relay](#through-the-push-relay): `sealed` (the token and env sealed for the relay, base64) and `box` (the phone's X25519 public key, base64). The core sends these in place of `token` whenever `welcome` had a `relay` | Nothing back. Only a device, for itself. A device signed out loses it. A sealed token while the relay is off is refused `not_here` |
| `push.unregister` | — | Nothing back. This device stops taking push |
| `profile.get` | — | `profile`: `text`, `revision`, `by` (the device or agent that wrote it) and `at`, or `null` when there's none yet |
| `profile.put` | `text` (the whole profile, up to 4000 bytes; empty clears it) | `profile`, as `profile.get` gives it, with its new `revision` |
| `defaults.get` | — | `defaults`: `brain`, `backup`, `config`, `permission`, `instructions`, `busy`, `router_brain`, `router_model`, `revision`, `by` and `at` ([Agent defaults](#agent-defaults)) |
| `defaults.put` | What changes, the rest as it was: `brain` (an installed program the person may use, or `null` for none; else `not_found` or `not_allowed`), `backup` (the same, for the program agents without one of their own move to), `config` (`{"claude": {"model": "opus", "effort": "high"}}`: per program, its options by id; an option `null` or `""` goes, and a program `null` loses all of its), `permission` (`ask`, `auto`, `allow` or a list of kinds; `null` is `ask`), `instructions` (up to 8000 bytes; empty clears them), `busy` (`queue` or `steer`), `router_brain` (the program the group chats' router runs on, checked as `brain` is; `null` for the one loom picks) and `router_model` (the model it's set to; `null` is `haiku`) ([Group chats](#group-chats)) | `defaults`, as `defaults.get` gives them, with the next `revision`, and `defaults.updated` to every device. In the activity log, kind `agent` |
| `rule.list` | `agent`? (by id or name: its own rules and every agent's; all of them without) | `rules`, oldest first: each `id`, `agent` (`""` for every agent), `then`, its conditions, `note`?, `by`, `at` and `said`, the rule in words, such as "Deny commands like git push, when a webhook started the turn" ([Rules on what's touched](#rules-on-whats-touched)) |
| `rule.put` | To add one: `then` (`allow`, `ask` or `deny`), at least one of `site`, `path`, `command`, `tool`, `effect` (`read` or `write`) and `started_by` (`person`, `owner`, `admin`, `member`, `agent`, `routine`, `webhook`, `script` or `plugin`), each up to 500 bytes, `agent`? (by id or name; left out, every agent), `note`?. A person has 200 at most (`full`). To change one: its `id` and what changes, `null` taking a condition off | `rule`, as `rule.list` gives it, and `rules.updated` to every device. `bad_request` for a rule with nothing to match or a value that isn't one, `not_found` for an agent or a rule that isn't there. In the activity log, kind `agent` |
| `rule.delete` | `id` | Nothing back, and `rules.updated` to every device. `not_found` for one that isn't the person's |
| `rule.check` | The action: `site`?, `path`? (one, or a list), `command`?, `tool`?, `effect`?, `started_by`? (`person` if left out), and `agent`?, `kind`? | `outcome` (`allow`, `ask` or `deny`, or `loom` when no rule is about one of loom's own tools, which follow loom's own rules), `decided` (`rule`, `mode` or `loom`), `rule` (the rule that decided, as `rule.list` gives it, or `null`), `why` (in words), `mode` (the agent's permission mode, its own or the default's) and `kind` |
| `routine.list` | — | `routines`, oldest first: each `id`, `name`, `agent`, `prompt`, `schedule` (null for a routine that only runs by webhook), `hook` and `secret` (with a webhook: [`POST /hooks/<id>`](#post-hooksid)), `when` (in words, such as `every 30 minutes, and by webhook`), `tz` (an IANA name), `on`, `notify`, `catch_up`, `next` (when it runs next, Unix ms, or 0 while it's off), `upcoming` (its next three runs, Unix ms, the first `next`, or empty while it's off), `conversation` (where it last posted), `last` (null before its first run, then `at`, `status`: `waiting` (for a turn in its chat to end), `running`, `done` (its agent posted, or its answer went on to its flow), `quiet` (its agent posted nothing: no message, no notification), `failed`, `limit`, `cancelled` or `skipped` (with why in `error`: missed while the instance was off, its chat busy for 10 minutes, or the instance restarted while it waited), `run`, and `error`), `failures` (failed runs in a row since its last change; at 5 it turns itself off, with a notification), `flow` (null, or its steps: each `name`, `agent` (the id), `prompt` and `after`), `by` and `at` |
| `routine.put` | To make one: `name` (up to 100 bytes), `agent` (by id or name), `prompt` (up to 20,000 bytes), `schedule`, one of: `{"every": minutes}`, 5 to 10,080, from when it's made; `{"times": ["HH:MM", ...], "days": [...]}`, at those times (1 to 24) on those days, or every day without `days`; `{"every": minutes, "from": "HH:MM", "to": "HH:MM", "days": [...]}`, every 5 to 1,440 minutes on the clock from `from` up to `to` (which can't come before it; midnight and 23:59 if left out) on those days; or, as routines had them first, `{"daily": "HH:MM"}` and `{"weekdays": "HH:MM"}`, Monday to Friday. Days are `mon` to `sun` (or spelled out, in any case), and an empty list isn't one. Times are on a 24-hour clock in `tz`, summer time included: a time the clock skips runs when it jumps past it, and one it shows twice runs the first time. A new kind is kept in order, its times sorted and its days Monday first, without `days` for every day, `webhook` (true for a webhook, which gets a new `hook` and `secret`; false takes it away), at least one of the two, and optionally `tz` (an IANA name, such as `Europe/Oslo`; the device's time zone if left out, or UTC when loom doesn't know the device's; one loom doesn't know is refused), `notify` (true: each post its agent makes with `routine_post` comes as a notification, and so does a failure), `on` (true), `catch_up` (true: a run missed while the instance was off happens once when it starts; false: it waits for its next time), `post` (`chat`, its agent's chat, the default; or `thread`, a thread of its own: a conversation marked with the routine, made at its first run and kept for the next, which is never the agent's chat; anything else is `bad_request`) and `flow` (what happens to its answer, [Routine flows](../PLAN.md#routine-flows): up to 20 steps, each `name` (up to 40 bytes, not `routine`), `agent` (by id or name), `prompt` (up to 20,000 bytes) and `after`, the steps it waits for: `routine` or steps before it, the step before it if left out; an empty list takes it off; one that loops back is `bad_request`, and an agent that isn't there `not_found`). To change one: its `id` and what changes | `routine`. Its next run stays unless its schedule or time zone changed |
| `routine.run` | `id` | `run`. It runs now, whether it's on or not, unless its last run is still going (`busy`, and that time is kept in its runs as `skipped`). While a turn goes on in its chat, the person's or another routine's, `waiting: true` comes too: the run starts once that turn has ended and what the person queued behind it has gone, and is `skipped` if the chat is still busy after 10 minutes, or if the routine runs again first, on its schedule, by hand, a webhook or a script ([PLAN.md](../PLAN.md#routines)). The same holds for a run on its schedule or its webhook |
| `routine.runs` | `id`, `before`? (a run's id), `limit`? (1 to 100, 20 if left out) | `runs`, newest first, the newest 100 kept: each `run`, `routine`, `conversation`, `by` (`schedule`, `webhook`, or the device or agent that ran it), `at` (when it was due), `status` (as `last` has it; a time it didn't run is kept as `skipped`, with why in `error`, also when its run before was still going), `error`, `ended`, `started` (when its turn started, which is later than `at` for one that waited for its chat; left out for one kept before runs had it, or that never started), `said` (what its agent answered, up to 4,000 bytes), `posted` (what it posted with `routine_post`, each up to 4,000 bytes), `hint` (for one that failed or was skipped, what the person can do about it, in a sentence or two) and, for a routine with a flow, `steps`, as each ended: its `name`, `agent`, `status`, `error`, `said` and `posted`. `not_found` for a routine that isn't there |
| `routine.next` | `schedule` (as `routine.put` takes it), `tz`? (the device's time zone if left out, as `routine.put` takes it) | `next`, when it would run the next three times from now (Unix ms), `when`, the schedule in words, and `tz`, the zone it used. For the routine form's "Next" while the person changes it, before it's kept. `bad_request` for a schedule that isn't one, or a time zone loom doesn't know |
| `routine.calendar` | `from`, `to` (Unix ms, `to` after `from` and up to 8 days on) | `routines`: each routine that's on and runs between the two, oldest first, with its `id`, `times` (when it runs from `from` up to `to`, Unix ms, at most 500) and `more` (true when it runs more often than that). One every N minutes counts on and back from its `next`. For the week on the Routines page |
| `routine.stop` | `id` | —. The routine's running flow stops: the step that's running is stopped and the rest are skipped. `not_running` when it has none |
| `flow.list` | — | `flows`, the routine flows running: each `id`, `routine`, `name` (the routine's), `stopped`, `finished` and `steps`, each `name`, `agent` (the id; the routine's own is first, named `routine`) and `status`: `waiting`, `running`, `done`, `failed` or `skipped` |
| `routine.delete` | `id` | — |
| `task.list` | `before`? (a task id), `limit`? (up to 200; 50 unless it says) | `tasks`, newest first, and `more` ([Tasks](#tasks)) |
| `task.start` | `agent`, `prompt`, `title`? | `task` |
| `task.pause`, `task.resume`, `task.cancel`, `task.retry` | `id` | Nothing back for `pause` and `cancel`; `run` for `resume` and `retry`. `not_now` when the task isn't in a state for it |
| `skill.list` | `bodies`? (`true` for each skill's text too) | `skills`, by name: each `name`, `description` (when it's for) and `at`, and `body` with `bodies` |
| `skill.get` | `name` | `skill`: `name`, `description`, `body`, `at` |
| `skill.put` | `name` (lowercase letters, digits and hyphens, up to 64), `description`, `body` (Markdown) | `skill`. It reaches every one of the person's agents: through `list_skills` and `read_skill`, and as `.claude/skills/<name>/SKILL.md` in each agent's folder, which Claude agents load by themselves |
| `skill.delete` | `name` | — |
| `proposal.list` | `status`? (`waiting`, the default, `kept`, `dropped`, `undone` or `all`) | `proposals`, newest first, 200 at most: each `id`, `agent` (who proposed it), `kind` (`skill`, `note` or `fix`), and `skill` (`name`, `description`, `body`), `note` (`text`, `site`, `shared`, and once kept, `memory`, the note's id) or `fix` ([Teaching a task, and proposals](#teaching-a-task-and-proposals)), `why` (where it came from, in the agent's words), `status`, `at`, `message` (the reply it's under) when it's under one, `later` once the person put it off, and once decided, `decided` and `by` (the device, or `auto` for the permission mode); and for a skill written from a recording, `recording` and its `steps`. The person's alone, from a device |
| `proposal.keep` | `id`, and what the person changed: `skill` (any of `name`, `description` and `body`), `note` (any of `text`, `site` and `shared`) or `fix` (`body`, `description`) | `proposal`, kept. A skill is put as `skill.put` puts one, and refused as it refuses one; a note is kept as the agent's (or shared, with `shared`), by the agent, tied to its site; a fix puts the skill as it has it, `bad_request` when the skill isn't there any more. `not_now` for one that isn't waiting |
| `proposal.drop` | `id` | `proposal`, dropped. Nothing is kept. `not_now` for one that isn't waiting |
| `proposal.later` | `ids` | `proposals`, the waiting ones of them, each now with `later`: put off on the card under their reply, they wait on the Proposals page |
| `proposal.undo` | `id` | `proposal`, `undone`: a kept note is forgotten, and a kept fix's skill put back as it was (`was`, `was_description`), or `changed` when the skill isn't as the fix left it. `not_now` for one that isn't kept, `bad_request` for a skill. In the activity log as `Forgot what was remembered: …` or `Undid the fix to the skill …` |
| `proposal.ask` | `agent` | `task`: a task in which the agent looks over its chat (its 40 newest messages are in its prompt, and `search_messages` finds the rest) and proposes skills and notes with `propose_skill` and `propose_note` |
| `plugin.list` | — | `plugins`, by id: each `id`, `name`, `version`, `description`, `panel` (`entry`) or null, `tools` (`command`, `args`) or null, `asks`, `allowed`, `waiting` (what's asked and not allowed), `revision`, `by`, `at` ([Plugins](#plugins)) |
| `plugin.get` | `id` | `plugin`, and `files`: `{path: text}` |
| `plugin.put` | `id` (lowercase letters, digits and hyphens, up to 64, starting with a letter), `files` (`{path: text}`, `plugin.toml` among them; 200 files and 2 MB at most) | `plugin`, a new revision. From a device, what it asks is allowed. From an agent, only what was allowed before and is still asked stays allowed, and the rest waits. `bad_plugin` says what's wrong with the manifest or a path |
| `plugin.allow` | `id` | `plugin`, with everything it asks allowed. Only a device: an agent can't |
| `plugin.revisions` | `id` | `revisions`, newest first, each as `plugin.list` gives it |
| `plugin.delete` | `id` | —. Its panel's data goes too |
| `plugin.data.get` | `plugin`, `key` (1 to 200 characters) | `value`, or null |
| `plugin.data.put` | `plugin`, `key`, `value` (any JSON; null forgets it) | —. A panel keeps 1 MB at most (`too_large`) |
| `plugin.cmd` | `plugin`, `name`, `args`? | The command's `value`, when the person allowed the plugin `cmd:<name>`, or `not_allowed`. It's how a panel's door sends commands |
| `plugin.message` | `plugin`, `agent` (a name or id), `text` (up to 20,000 bytes) | `reply`: the agent's answer, once its run ends, when the person allowed the plugin `agents`, or `not_allowed`. The plugin and the agent keep one conversation, `From <plugin's name>` with `asked_by` `plugin:<id>`, never the agent's chat ([Tasks](#tasks)), where the message is the person's, with `plugin:<id>` as `from`. `busy` while the agent is still answering the last one, and `agent_failed` when its run fails |
| `activity.list` | `agent`?, `before`? (an entry's `id`), `limit`? (up to 200, 50 if left out) | `activity`, newest first: each `id`, `agent`, `kind` (`tool`, `approval`, `agent`, `message`, `notify`, `memory`, `routine`, `device`, `browser`, `import` or `token`), `text`, `by` (a device, an agent, `auto` for the permission mode, or empty for the agent itself) and `at`. An action recorded before it happened, kind `tool`, also has `status` (`started`, then `done`, `failed` or `refused`), `input` (what it was given, as JSON text) and `why` (for `failed` and `refused`; a refusal's starts with its rule: `never`, `own-mode`, `no-more-than-its-own`, `not-itself`, `chain-loop`, `chain-depth`, `person-said-no`, `device-tool-off`, `token-file` or `needs-a-chat`), and `ended` once it has. A note or a fix that was kept, kind `memory`, has `proposal`, its id, and `undo`, whether `proposal.undo` can still undo it. Every call to one of loom's tools that acts is recorded, as its name (`run_command NAME` for a command), and so is each tool call of the agent's own, as its title; a call that only reads (`list_`, `read_`, `search_messages`, `recall`, `query_rows`, `browser_snapshot` and the like, or a command ending `.list`, `.get`, `.search` or `.revisions`) isn't. The person's secrets, in any entry, show only as `[secret NAME: N characters]`, and a member of `input` named like a password or a token as `[N characters]`. One still `started` when the instance stops is marked `failed` when it starts again. It's kept for good, unlike events |
| `activity.export` | `agent`? | `csv`, the activity log as CSV, oldest first, with a header row and the columns in this order: `id`, `at`, `agent` (by name), `kind`, `what`, `by`, `status`, `why`, `input` and `ended`, the times in UTC as RFC 3339; and `entries`, how many. The newest 100,000 at most. An agent's asks the person |
| `server.offer` | `address` (`http://` or `https://`), `link_code`? | `{}`, and `server.offered` to the person's devices, with the code and their username. Only from a device: an agent's `add_server` asks the person, and their yes sends it |
| `instance.move` | `to` (the new place's `http://` or `https://` address), `code` (the move code `loom receive` printed there) | `address`, once the instance is there. Only the owner, and not while an agent is answering. Until then, sends and commands get `moving`, and routines wait. The instance goes as `loom export` writes it, in chunks of 4 MB, each sealed with XChaCha20-Poly1305 under a key from the code, which also gives the bearer token for the new place's `POST /api/move/chunk?n=<n>` and `POST /api/move/done`. Then every device gets [`moved`](#moved). `move_failed` says why one didn't happen. An agent's `move_instance` asks the person first |
| `import.scan` | `openbot`?, `grokbot`? (a folder to look in for each, in place of where the app keeps its data on the instance's computer) | `sources`, one for each app: `from` (`openbot` or `grokbot`), `name`, `found`, and once found, `where` (its folder), `counts` (`agents`, `conversations`, `messages`, `files`, `memories`, `routines`, `skills`, `mcp`, `tables` and `rows`) and `text`, the counts in words, such as `2 agents, 3 messages`, and for OpenBot `provider`: the provider its Claude uses, from Claude Code's `settings.json` (`env.ANTHROPIC_BASE_URL`, and `ANTHROPIC_AUTH_TOKEN` or `ANTHROPIC_API_KEY`), as `{"program": "claude", "endpoint", "token", "text"}` (`token` says whether there is one), such as "OpenBot's Claude uses the endpoint https://… with a token.", or null when it has none of its own; the token itself never comes; or, when not found, `why` ([PLAN.md](../PLAN.md#importing-from-openbot-and-grok-bot)). Only the owner, since it reads the instance's computer (`not_allowed`) |
| `import.run` | `from` (`openbot` or `grokbot`), `path`? (a copy of that app's data folder, such as on a server; left out, where it keeps its data on the instance's computer), `carry_provider`? (`true` once the person said yes to "Use the same for Claude Agent?", which the app asks when `import.scan` gave a `provider`) | `came` (how many of each kind came now: `agents`, `conversations`, `messages`, `files`, `pins`, `memories`, `routines`, `skills`, `mcp`, `tables`, `rows`, and `folders`, which only an import staged from another user brings: the folders in their home that the agents named, now in the person's shared folder), `already` (how many were here from an earlier import, and stayed as they are), `not` (what couldn't come or changed on the way, each in a sentence), `text` (what came, in words), `from` and `where`; with `carry_provider`, `provider`: `{"carried": true, "program", "endpoint"}`, or `{"carried": false, "why"}` when OpenBot's Claude has no provider of its own or there's no Claude agent program here (one whose name or registry id has `claude`). The instance copies those settings into that program's itself, sealed as `acp.settings` keeps them, and starts it again with them; no agent or app reads the token. The other app's files are only read: its SQLite opened read-only, and its secrets never at all, but for that provider when the person says yes. A `path` staged by `loom import openbot --from-user` (a folder with `loom-stage.json`) has the token removed once it has run. Each thing is kept with the other app's id, so importing again brings only what's new. Agents come on the agent program named for their provider, or the first one installed, with the permission mode `ask`, `config.model` (a Claude model by its family: `opus`, `sonnet`, `haiku`), the nearest thread colour and their pin; conversations, messages, memories and pins with their own times, and as their events: `agent.created`, `conversation.created` (a thread beside an agent's chat as a branch of it, or as a routine's own), and `message.created` for each conversation's 50 newest, older ones coming with `messages.list`; routines on loom's schedules, with what changed in `not`; skills, MCP servers without their environment and headers, and shared tables and their rows, none of them over one of the person's own of the same name. Recorded in the activity log, kind `import`. `not_found` when the app's data isn't there. Only the owner |
| `instance.phones` | `on`? (true or false) | `on`, and while they are, `port` and `key` ([Over Noise](#over-noise)). With `on`, turns phones on or off: they reach the instance on the network over Noise, and stay on across a restart. Without it, says how things are. Only the owner, from a device, and only on an instance in the desktop app (or `loom serve --lan`): elsewhere `not_here` |
| `instance.relay` | `url`? (the push relay's address, such as `https://push.loom.example`; empty turns it off) | `on`, and while it is, `url` and `key` (the relay's X25519 public key, base64). Anyone sees it. With `url`, the owner, from a device, turns the [push relay](#through-the-push-relay) on or off. The instance asks the address for its key at once, and refuses it `unreachable` when it doesn't answer as a relay. Phones that connect after that seal their tokens for that key |
| `instance.update` | `now`? (`true` stops what runs, rather than waiting for it to end) | `state` (`downloading`, or how one under way stands), and `version` once it's known; then `instance.update` frames to every owner and admin. Owners and admins; `not_here` in a container or the desktop app ([Updating the instance](#updating-the-instance)) |
| `instance.updates` | `auto`? (`true` lets the instance update itself, the owner's alone) | `version`, `auto`, `may`, `owner`, `can`, `why`?, and to an owner or admin `update` ([Updating the instance](#updating-the-instance)) |
| `invite.create` | `role`? (`member`, the default, or `admin`), `uses`? (1 to 100, default 1), `days`? (1 to 30, default 7) | `invite`, `code` (for [`POST /api/join/<code>`](#post-apijoincode), shown once), `role`, `uses`, `expires`. Owners and admins, from a device; only the owner makes admins. On an instance in the desktop app, `not_here`: it's its owner's alone |
| `invite.list` | — | `invites`: each open one's `invite`, `role`, `uses` left, `expires`, `by` and `at`, newest first. Owners and admins |
| `invite.delete` | `invite` | — |
| `user.list` | — | `users`: each person's `user`, `username`, `role` and `at`, oldest first. Owners and admins |
| `user.role` | `user`, `role` (`member` or `admin`) | —. The owner only, and never for the owner |
| `user.remove` | `user` | —. Removes the person and everything of theirs, and their sessions close with 4001 once this is answered. The owner removes anyone but the owner, an admin only members, and not while one of their agents is answering (`busy`) |
| `account.delete` | — | —. The person removes their own account, as `user.remove` does. Not the owner |
| `password.reset` | `user` | `code` (for [`POST /api/reset/<code>`](#post-apiresetcode), shown once), `expires` (a day on). The owner resets anyone but the owner, an admin only members. Each new link replaces the person's last |
| `password.set` | `password` | `{}`, and `account.updated` to every device of the person's: the first password of an account a [link code](#link-codes) made, which has none (`welcome`'s `has_password` is `false`). Only from a device, never an agent or a plugin. Errors: `has_password` (a new one comes with a reset link, or `loom user password`), `weak_password` (under 8 characters) |
| `people.log` | `before`? (an entry's `id`), `limit`? (50; up to 200) | `entries`, newest first: each `id`, `kind` (`joined`, `invited`, `role`, `removed`, `left`, `reset`, `password` or `programs`), `text` (what happened, in words, with usernames), `by` (the username of who did it, or empty when it came from the server's command line or the person is gone) and `at`. Owners and admins. It keeps who joined and how, invites made and taken back, role changes, removals, accounts deleted, reset links made and used, accounts and passwords from the command line, and who the owner gives each agent program and its shared settings to |
| `agent.revisions` | `agent` | `revisions`, newest first: each `revision`, `by` (the device or agent that made it), `at`, and `agent` as it was then |
| `agent.file` | `agent`, `path` (as a reply's `mentions` has it) | `file`: the file, kept as the person's, as a `file` block, to fetch with `file.get`. Only from the agent's folder or the shared folder, with links followed and `..` resolved, and 25 MiB at most |
| `link.preview` | `url` | `link`: `{title, site, icon?, image?}` as a message's `links` has it, for a link pasted in the composer ([PLAN.md](../PLAN.md#links-in-the-composer)). Answered once the page is looked up, with its preview picture, or at once when it has been. A page that isn't looked up (not https, or not on a public host) is `{title: "", site}` at once. Words that aren't an http or https address are refused (`bad_request`) |
| `agent.files` | `agent`, `query`? | `files`, for the composer's @ picker: up to 20 files in the agent's folder and the shared one whose name, or else path, has `query` in it in any case, names that start with it first, then shorter paths. Each `path` (as `agent.file` takes it: relative in the agent's folder, absolute in the shared one), `name` and `where` (its folder, or `shared`). Hidden folders, `node_modules`, `target` and `__pycache__` are left out, and 20,000 entries are looked at at most |
| `files.recent` | `limit`? (20; up to 50) | `files`, for the composer's + menu: the files sent or received in the person's chats, newest first, each once, from their 2,000 newest messages in chats not deleted. Each as its `file` block has it (`file`, `name`, `mime`, `size`), with `at`, when the message was, and its `conversation` |
| `agent.summary` | `agent` | What its settings panel counts: `memories` (its own notes), `skills` (the person's, which every agent has), `tables` (the shared tables it owns), `files` and `bytes` (in its folder, but for the skills loom writes there; 20,000 files at most), `routines`, and `tokens_today` and `tokens_month` (what its runs used, from the start of the day and month in UTC) |
| `agent.rollback` | `agent`, `revision` | `agent`: a new revision with that one's name, ACP agent, instructions, folder, colour and shape. The permission mode stays as it is |
| `agent.pin` | `agent`, `pinned`? (`false` takes the pin off) | `agent`, with `pinned`: when it was pinned. Pinned agents go at the top of the list, in that order. Not a revision; `agent.updated` tells every device |
| `agent.order` | `agents` (the agents' ids, in the order the person dragged them into) | `agents`, and `moved`: how many changed place. Each keeps its place as `order`, and the apps list agents by it, pinned ones first; one with no `order` yet comes after, in the order it was made. Nothing else moves an agent in the list. Not a revision; `agent.updated` tells every device of each that moved. `not_found` for an agent that isn't there |
| `agent.group` | `agent`, `group`? (a folder's id; `""` or left out takes it out of any) | `agent`, with `group` while it's in one. It shows in that folder, so it's no longer pinned or hidden. Not a revision; `agent.updated` tells every device. `not_found` for an agent or a folder that isn't there ([PLAN.md](../PLAN.md#the-sidebar)) |
| `agent.hide` | `agent`, `hidden`? (`false` shows it again) | `agent`, with `hidden`: when it was hidden. A hidden agent is unpinned, keeps its folder, and is listed under Hidden; pinning it shows it again. Not a revision; `agent.updated` tells every device |
| `group.list` | — | `groups`: the person's sidebar folders, in the order they were made, each `id`, `name`, `collapsed` and `at` |
| `group.create` | `name` (1 to 80 characters, trimmed) | `group`, open. `group.updated` tells every device. `bad_request` for a blank or long name |
| `group.update` | `group`, `name`?, `collapsed`? | `group`, as it is now, and `group.updated` to every device when it changed. Opening or shutting a folder is kept too, so every device shows it the same |
| `group.delete` | `group` | —. Its agents stay, out of any folder, each with `agent.updated`, then `group.deleted` |
| `emoji.list` | — | `emoji`: the person's own, oldest first, each `id`, `name`, `file` (its picture, the person's file), `animated` and `at`, and `most`, how many one person may have (200) ([Emoji](#emoji)) |
| `emoji.add` | `file` (a picture the person sent, with `POST /files`), `name` | `emoji`, and `emoji.changed` to every device. The picture is a PNG or a JPEG of at most 256 KB and 1024 pixels a side, made 128 pixels square. `bad_name` for a name that isn't 2 to 32 of `a`–`z`, `0`–`9` and `_` (it's taken in lowercase, without colons), `name_taken`, `too_many` past 200, `bad_picture` for anything else, a GIF, a WebP or a moving PNG among them, for now, and `not_found` for a file the person doesn't have |
| `emoji.rename` | `emoji`, `name` | `emoji`, as it is now, and `emoji.changed`. `bad_name`, `name_taken`, `not_found` |
| `emoji.delete` | `emoji` | —, and `emoji.changed`. It's kept, for its undo, but not listed, counted or shown |
| `emoji.restore` | `emoji` | `emoji`, back with its id, so the messages it's in show it again, and `emoji.changed`. `name_taken` when another has its name now, `too_many` |
| `agent.delete` | `agent` | —. A sandbox of its own, for an agent closed to the network, goes with it, and its egress |
| `device.list` | — | `devices`, the person's devices signed in, oldest first: each `id`, `name`, `kind`, `at` (when it signed in), `seen` (when it was last connected, Unix ms, or 0 for never) and `online` (connected now) |
| `device.rename` | `device`, `name` | — |
| `device.revoke` | `device` | —. Removes it: its token stops working, and its socket closes with 4001 |
| `link.answer` | `ask` (a `link.asked`'s id), `allow` | `{}`, and `link.answered` to every device of the person's. The person's yes or no to a device asking to be added ([Adding a device](#adding-a-device)). Only from a device, never an agent or a plugin. Errors: `not_found`, `expired`, `answered` |
| `acp.list` | — | `programs`: the agent programs installed on the instance, each with `name`, `title`? (the name people know it by, the ACP Registry's for the agent it is, as `acp.registry`'s `programs` matches it: "Claude Agent" for `claude-acp`; left out when the registry has none for it), `command`, `source` (the ACP Registry id it came from, or empty for one added with `loom acp add`), `version`, and `settings`: the names of its settings, never their values, and `share` when the owner shares them ([`acp.share`](#commands)): `tokens`, `per`, and for someone it applies to, the tokens `left` of theirs this day or month. To the owner, a program kept to some people has `members`, their ids, and so does a share kept to some. Anyone else gets only the programs they have ([`acp.members`](#commands)). And `owner`: whether they're the owner, and `sandboxes`: whether the instance runs sandboxes, which an isolated agent needs. A program that has reported the limits of the person's plan has `limits` too ([Usage limits](#usage-limits)) |
| `acp.registry` | `again`? (`true` to wait for it to be read again now) | `agents`: the [ACP Registry](https://agentclientprotocol.com/get-started/registry)'s, each with `id`, `name`, `version`, `description`, `website`?, `icon`? (its address), `icon_svg`? (the icon itself, an SVG in currentColor, which the instance fetched with the registry; one that couldn't be fetched or isn't an SVG is left out), `how` (how it would run on the instance: `binary`, `npx`, `uvx`, or null when it can't), and `installed` (the version installed) if it is; `programs`, which registry agent each program on the instance is, by its name (`{"claude": "claude-acp"}`): its own id, or the agent named for it; and `platform`, such as `windows-x86_64`. The instance answers at once with what it last read, which it keeps in `acp/registry.json` across restarts, and once that is 10 minutes old, reads it again behind the answer, for the next one. The first time, or with `again`, the answer comes once it has been read. It fetches every icon at once |
| `acp.install` | `id` (a registry id), `env`? (settings to give it, `{NAME: value}`) | `program`, as `acp.list` gives it, once it's installed. A binary is downloaded into the instance's `acp/<id>/`, checked against the registry's SHA-256 and unpacked; an npx or uvx agent is run with `npx -y <package>` or `uvx <package>`. Installing one that's installed updates it, keeping its settings. Only the owner installs. `install_failed` says why it didn't |
| `acp.settings` | `name`, `env` (`{NAME: value}` sets one, `{NAME: null}` removes it) | `program`. Settings are the environment variables the program starts with: an API key, a model. They're kept sealed under `secret.key`, as secrets are, and only their names come back. Agents that aren't answering start again with them at their next message. Only the owner |
| `acp.share` | `name`, `tokens` (each person's budget, or 0 to stop sharing), `per`? (`day`, the default, or `month`, in UTC), `members`? (people's ids, or null for everyone; without it, who it goes to stays as it was) | `program`. Shares the program's settings with everyone but the owner, or only the `members`, whose agents otherwise get only their own secrets ([Sandboxes](#sandboxes)). A person with their own secret for every one of the settings uses theirs, with no budget. Otherwise each turn on the shared settings counts the tokens its program reports (ACP's `usage`), and once a person's add up to `tokens` this day or month, their runs on the program finish `limit` until it begins again. The turn that crosses it finishes. Agents that aren't answering start again at their next message. Each change goes in the people log. Only the owner |
| `acp.members` | `name`, `members` (people's ids, or null for everyone) | `program`. Gives the program to everyone, the default, or only the `members`. Anyone else doesn't see it in `acp.list` and can't make an agent on it or switch one to it (`not_allowed`), and a run of an agent of theirs already on it finishes `failed`, saying the owner hasn't given it to them. The owner always has every program. Agents that aren't answering start again at their next message. Each change goes in the people log. Only the owner |
| `acp.remove` | `name` | —. Removes the program and, for one installed from the registry, its folder. Only the owner |
| `mcp.list` | — | `servers`: the person's MCP servers, each with `name`, and `command`, `args` and `env`, or `url` and `headers`. A value in `env` or `headers` is text, or `{"secret": NAME}`, with `prefix` when it has one: text sent before the secret, such as `Bearer `. One over `url` that loom knows signs in ([PLAN.md](../PLAN.md#signing-in-to-mcp-servers)) has `sign_in`: `needed`, or `signed_in` with `signed_in_at` and, when the sign-in said whom, `signed_in_as`. One with a client of the person's own has `client_id`, and its `client_secret` as `{"secret": NAME}`, or `client_secret_set` for one given as text, which isn't shown. Each has how it stands, for the Installed list ([PLAN.md](../PLAN.md#the-integrations-catalog)): `status`, `connected` (its last test reached it), `sign_in` (it needs the person to sign in), `failing` (its last test didn't reach it, with `why`) or `untested` (no test since it was added or changed, or since a sign-in it waited for), with `tested_at` and, while it's `connected`, `tools`, what that test found; `agents`, the ids of the person's agents that have it; and `catalog`, the id of the catalog entry it was connected from, or whose fixed address or program it has |
| `mcp.put` | `name` (lowercase letters, digits and hyphens; not `loom`), and `command` with `args`? and `env`?, or `url` (http or https) with `headers`?, `client_id`? and `client_secret`? (text or `{"secret": NAME}`; left out with the same `client_id`, it stays as it was) | `server`. A value in `env` or `headers` is text, or `{"secret": NAME}` with `prefix`? (text, put before the secret's value). Each agent whose `mcp` names it gets it in its next new session, with its secrets filled in, and its sign-in as `Authorization: Bearer`, beside loom's own. One over `url` goes only to agent programs that offer `mcpCapabilities.http`. A new `url` drops its sign-in, and another address or program drops what its last test found |
| `mcp.test` | `name` | `tools`: the names of the tools it offers, and `signed_in` (with `signed_in_as`, when it's known) for one it reached with the person's sign-in. The instance starts it as an agent's session would, with its secrets and its sign-in, renewing a token that ends soon or that the server answers 401 to, asks for its tools and stops it, answering once it has; up to a minute for one that's slow to start. `sign_in_needed` for one that answers 401 with no sign-in, or none it takes. `mcp_failed`, with why, for one that names a secret the person doesn't have (each by name), doesn't start, or doesn't answer. A sandboxed person's servers run only in their sandboxes, so theirs are `not_allowed` |
| `mcp.sign_in` | `name`, `redirect` (where the browser comes back to: `http://127.0.0.1:PORT/…`, `http://localhost:PORT/…`, `https://…` or `loom:…`, which the device that asks catches) | `server`, `url`: the address to open in the person's browser. The instance finds where the server signs in, signs loom up there as a client for `redirect` unless it has one, and makes a PKCE verifier it keeps and a state, open for 15 minutes. `sign_in_failed`, with why, for one it can't sign in to: it doesn't ask for a sign-in, its metadata can't be found or doesn't check PKCE, or it lets no app sign up and the server has no `client_id`. `not_found`; `bad_request` for a server started by a command, or with an `Authorization` header of the person's own. Never an agent's |
| `mcp.signed_in` | `query`: what came back to the redirect, as it came (`code`, `state`, and `iss` or `error` and `error_description`) | `server`, and `signed_in_as` when the sign-in said whom. The code is swapped for tokens, which are kept sealed as the person's. `sign_in_failed` for a state that isn't one the instance made for this person and still open (each is taken once), an `iss` that isn't where it sent the browser, an error the authorization server sent back, or a code it didn't swap. Agents that aren't answering start again with it at their next message. Never an agent's |
| `mcp.add_answer` | `call` (an `mcp_add` card's), `answer` (`add`, `cancel`, or `later` for one waiting on a sign-in), `secrets`? (`{NAME: value}`, with `add`: each secret the server names that the person doesn't have) | `{}`. Add keeps each secret as `secret.put` does, the server as `mcp.put` does, and gives it to the agent that asked (`agent.update`'s `mcp`, as the device), then tries it as `mcp.test` does: the card goes on to `ready`, to `sign_in` for one that answers 401 with no key of the person's own, or to `failed`, and the agent's `add_mcp_server` call is answered once it's `ready`, `failed`, or the person says Later. Cancel keeps nothing. `bad_request` naming a secret that isn't filled in, `not_found` for a card that isn't the person's or whose turn is over, `not_now` for one already answered. Only from a device: it's never a command an agent, a plugin or a script runs |
| `mcp.sign_out` | `name` | —. Drops the tokens, keeps the client to sign in with again, and tells the authorization server, when it has a revocation endpoint. `not_found` when the person isn't signed in |
| `mcp.delete` | `name` | —. Signs out of it too, and drops what its last test found and the catalog entry it came from |
| `mcp.agents` | `name`, `agents` (the ids of the person's agents that should have it) | `server`, `agents`. Gives the server to each agent in `agents` that doesn't have it, and takes it from each that has it and isn't, each as `agent.update` changes its `mcp`, as the device. A connect from the catalog waiting at `agents` for it is `done`. `not_found` for a server or an agent that isn't the person's. Only from a device |
| `catalog.list` | — | `entries`: Spools, the integrations catalog loom ships ([PLAN.md](../PLAN.md#the-integrations-catalog)), in its order, Acta first. Each has `id`, `name`, `category`, `description` (one line), `maker`, `website`?, `logo`? (none has one yet), `badge` (as a card's, from its `server`), `server` (the name it's added as unless the person gives another), `runs` (`remote`, reached over HTTP, or `instance`, a program the instance starts), `url` (with a `{field}` for each field) or `command` and `args`, `fields` (each `name`, `label` and `placeholder`?: what the person fills in), `sign_in` (`oauth`, `secret` or `none`), `secrets` (for `secret`: each `name`, `label`, `placeholder`? and `have`, whether the person has that secret), `note`? (one line), `tools` (each `name` and `kind`, `read` or `change`), `tools_from` (where they were read), `tools_checked` (false where the names couldn't be checked against the server or its own docs), `servers` (the person's servers made from it), `connected` (whether there are any) and `connecting`, a connect under way for it, as `catalog.connect` gives it. And `categories`, those with entries, in their order. Only from a device |
| `catalog.connect` | `id` (an entry's), `fields`? (`{name: value}` for each of its fields), `secrets`? (`{NAME: value}` for each of its secrets: needed for one the person doesn't have, and replacing one they have), `name`? (the server's, for a second one from the same entry, such as `t3-vps`) | `connect`: `entry`, `server`, `step`, `said` and `tools`. The entry becomes a server as `mcp.put` takes it, each `{field}` filled in (an address field is `https://` or `http://`, a host and a port, with no path); its secrets are kept as `secret.put` keeps them and the server as `mcp.put` does, marked as the entry's, then it's tried as `mcp.test` tries it, from settings as an agent's `mcp_add` card does from a chat. It answers at `adding`, and each step after comes as a [`connect`](#connect) frame: `sign_in` for one that answers 401 with no key of the person's own (the device signs in with `mcp.sign_in` and `mcp.signed_in`, and it's tried again once they have, or the sign-in's error is its `said`), then `agents`, with the tools it found (the apps ask which agents get it, and `mcp.agents` makes it `done`), or `failed`, with why as `said`. A sandboxed person's goes straight to `agents`, untried. Connecting one the person has from the entry again, without `fields`, tries it as it is (Reconnect). `not_found` for an entry that isn't there, `bad_request` for a field or a secret left out (by its label) or a field that isn't right, `exists` for a name another of the person's servers has. Only from a device |
| `secret.list` | — | `secrets`: the person's secrets, each `name`, `at`, and `sites`, the hosts it may be filled in on in the agents' browsers (`""` for a file on the instance's computer). Never their values |
| `secret.put` | `name` (letters, digits, `_`, `-` and `.`), `value` | `name`. It's sealed with XChaCha20-Poly1305 under the instance's `secret.key`, bound to the person and the name, and opened only to start an agent's MCP server, or to fill it into an agent's browser (`browser_fill_secret`). Putting one again by its name changes its value, and it keeps its sites |
| `secret.sites` | `name`, `sites` (hosts, such as `innsida.ntnu.no` or `localhost:8080`, or `""` for a file on the instance's computer) | `name` and `sites`, as kept: where the secret may be filled in from now on, in place of where it could before. `not_found` for a secret the person doesn't have. Never an agent's |
| `secret.delete` | `name` | —. Its sites go with it |
| `messages.search` | `query`, `agent`?, `after`? and `before`? (Unix ms), `conversation`?, `limit`? (up to 100, 30 if left out; with `conversation`, up to 500, 500 if left out) | `results`, best first, or with `conversation`, that chat's alone, in its order, oldest first, the newest when there are more (what finding in a chat steps through, [PLAN.md](../PLAN.md#finding-in-a-chat)), each with its `message` and a `snippet`: the words around the match, on one line and without the Markdown they were written in (a link as its text, no code ticks, bold, strike, list or heading marks), with each matched word between U+0002 and U+0003. Every word of `query` must be in the message, the last as a prefix, so it works while typing. It searches text blocks and file names, in conversations that aren't deleted. A person's message counts as its conversation's agent's |

Arguments marked `?` are optional. "—" means `value` is `{}`.

## Emoji

([PLAN.md](../PLAN.md#emoji).) A built-in emoji goes in a message's text as itself, the Unicode character, so it reads the same everywhere: to an agent, copied, in another app. One of the person's own goes as a token with its name and its id, `<:party_cat:e_3>`, or `<a:party_cat:e_3>` for one that moves, and the apps draw its picture from `GET /emoji/<id>`. A token is `<`, `a` for one that moves, `:`, its name (2 to 32 of `a`–`z`, `0`–`9` and `_`), `:`, its id, and `>`. In code, in backticks or a fenced block, nothing is a token or an emoji's name.

- **What the agent reads.** Each token as `:name:`, in the message and in what it replies to. Each session is told, once and again when the person's emoji change, `[loom: In the chat, write an emoji as itself or by its name between colons, as :sob: or :thumbsup:, and it shows as the emoji. Your person's own emoji, written the same way: :party_cat:, :sob_cat:.]`.
- **What it writes.** A reply's words are kept with each `:name:` the instance knows turned into the emoji: a built-in name (`:sob:`, `:+1:`, `:thumbsup_tone3:` for a skin tone) into the character, and one of the person's own into its token. A name it doesn't know, a time such as `12:30:45`, and anything in code stay as they are. The deltas stream the words as the agent wrote them, and the reply's `message.created` has them turned.
- **Notifications** say a token as `:name:`, since a notification draws no pictures.

The built-in names come from [`shared/emoji.calx`](../shared/emoji.calx), which the core and the instance both use: gemoji's Discord-style names (`sob`, `joy`, `+1`, `thumbsup`, `skull`) first, then CLDR's name in snake case (`loudly_crying_face`), and CLDR's keywords for finding one. The core answers the composer with them (core/README.md, "Between the app and the core").

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

What it asks for is its `asks`: `tools` when it has a tool server, `agents`, and `cmd:<name>` for each command. A panel never gets the commands that would let it allow itself more, borrow another plugin's permissions, run programs on the instance, or reach the person's devices, secrets and sign-in: `plugin.put`, `plugin.allow`, `plugin.delete`, `plugin.cmd`, `plugin.message`, `instance.move`, `instance.update`, `instance.updates`, `instance.phones`, `instance.relay`, `invite.create`, `invite.delete`, `user.role`, `user.remove`, `account.delete`, `password.reset`, `password.set`, `link.create`, `link.answer`, `push.register`, `push.unregister`, `device.revoke`, `device.rename`, `secret.list`, `secret.put`, `secret.delete`, `mcp.put`, `mcp.test`, `mcp.sign_in`, `mcp.signed_in`, `mcp.sign_out`, `acp.install`, `acp.remove`, `acp.settings`, `acp.share`, `acp.members`, `import.scan` and `import.run`. Until its person allows something, it waits, and the plugin goes without it.

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
| `mcp_failed` | `mcp.test`: the server names a secret the person doesn't have, didn't start, or didn't answer. The message says which |
| `sign_in_needed` | `mcp.test`: the server answers 401, and the person has no sign-in to it that it takes. `mcp.sign_in` signs them in |
| `sign_in_failed` | `mcp.sign_in` or `mcp.signed_in`: the sign-in couldn't be started or finished. The message says why |
| `exists` | `catalog.connect`: another of the person's MCP servers has that name |
| `unknown_file` | A `send` names a file this person hasn't uploaded |
| `bad_form` | A `send`'s `form` doesn't fit its card: a required field left empty, an answer that isn't one of its options or a number, a card that isn't a form or isn't in that reply, or a form answered already. The message says which |
| `bad_reply` | A `send`'s `reply` names a message that isn't there, or is in another conversation |
| `storage_full` | A file would take the person over the space the owner gives each person |
| `moving` | The instance is moving: nothing changes until it's at the new place |
| `moved` | `instance.move` on an instance that has moved already |
| `move_failed` | `instance.move`: the export, sending it, or the new place failed. The message says which, and the instance goes on where it is |
| `not_here` | `instance.phones` on an instance on a server, which phones reach at its own address, or `push.register` with a sealed token while the push relay is off, or `instance.update` on an instance that doesn't update itself here |
| `unreachable` | `instance.relay`: the address didn't answer, or isn't a push relay |
| `expired`, `answered` | `link.answer`: the device stopped waiting to be added, or another device answered it |
| `internal` | A bug in the instance. It's logged there |

The core says a few of its own in `core.login`, for an instance it couldn't sign in to: `unreachable` (no answer; the message says why), `tailscale` (the server is only on a tailnet, and this device has no Tailscale address: [Adding a device](#adding-a-device)), `bad_qr` (a code that isn't loom's) and `busy` (a login is on its way already).

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
| `b` | A binary message, as `/ws` carries it | A [`browser.frame`](#browserframe) of a session of version 2, from the instance |
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

## Updating the instance

An instance updates its own program from loom's signed releases ([PLAN.md](../PLAN.md#updates-and-versions), `server/update.calx`), as `loom update` does on its command line.

- **Behind or not.** `welcome` says the instance's version as `loom`. The desktop core that looks for app updates reads the newest release's manifest about every ten minutes while the app runs (give or take a minute, and with `If-None-Match`, so one that didn't change is a 304), and once its signature checks, tells the app `core.release`, newer than the app or not. The app hands it to each of its servers' cores, and each says `core.server`: the instance's version, the newest one, and whether the instance is `behind` it, by the same rule as `shared/release.calx`'s `newer`. The instance in the desktop app is never behind: it updates with the app. A core whose instance is behind asks it `instance.updates` once on each connection, and `core.server` says too whether this person `may` update it, whether it `can` where it runs (and `why` not), and its `update` under way. The app's first core gathers every server's `core.server` and the app's own update into `core.updates`, what the update icon shows (the app's with `can` false and `why` where it can't put a new version in place, as a Mac app opened from its disk image), and decides what updating everything does: each server's core sends `instance.update`, and the app goes last (core/README.md, "Between the app and the core").
- **`instance.update`** (`now`?): an owner or admin (`not_allowed` for anyone else) has the instance fetch the newest release on a thread of its own, check the manifest's signature and the program's SHA-256 as `loom update` does, and put it beside its program as `.new`. It answers at once with how it stands (`state`, as the frame above), and every owner and admin hears the frame as it goes on. Once nothing runs (no turn, a task's or a flow step's included, nothing waiting on the loop to start, no install, move or login being checked, and no browser the person has taken over or is recording in), it renames its program to `.old`, the new one into its place, says `restarting`, closes every socket with 1001 and the listener, and starts again as the new program. With `now: true` it doesn't wait for turns: each is stopped as `cancel` stops it (its open approvals answered no, `by: "cancel"`, and its reply so far kept), and the restart finishes it as one a restart cut short (`run.finished` `failed`, "the instance restarted"), so its task carries on, its flow's step goes again, and its queue goes on. A turn that doesn't stop within 15 seconds is cut short the same way. An update asked for while one is under way joins it.
- **How it comes back.** Run by systemd (`loom install`'s unit says `LOOM_SERVICE=systemd`, or systemd's `INVOCATION_ID` is set), it exits with 75 and the unit's `Restart=on-failure` starts the new program. Run by hand, it starts the new program with the same arguments and the port it had, then exits, and the new one waits up to 10 seconds for the port. In a container (`LOOM_SERVICE=docker`, which loom's image sets, or `/.dockerenv` or `/run/.containerenv`) and in the desktop app it doesn't update itself: `not_here`, saying how it is updated instead. So is one whose program sits where it can't write, such as a server set up before `loom install` gave the service a copy of its own.
- **`instance.updates`** (`auto`?): how its updates stand, to anyone: `version`, `auto` (whether it updates itself), `may` (whether this person may update it), `owner`, `can` (whether it can update itself here) and `why` when it can't, and to an owner or admin `update`, how one under way stands. With `auto`, the owner, from a device, turns self-updates on or off (`not_allowed` for anyone else, `not_here` where it can't).
- **Self-updates.** With them on, the instance looks a minute after it starts and every half hour after. A newer release is staged as above, says `waiting` with `auto: true`, and goes in once nothing has run, and nothing has come from an app, for five minutes. The new program finds what it was to be in the database, checks that the program running is the one it staged, by its SHA-256, and tells the owner once, as a [`notify`](#agents-and-devices) from loom itself (no `agent`, `title` "loom"): "loom updated itself to 0.2.1.", when the owner's first device connects after it, live, or two minutes after it starts, whichever comes first. One that failed is told once for each release, as "loom couldn't update itself to 0.2.1: …".
- **Tested against a release of its own.** `loom serve --test-release MANIFEST KEY` takes a test's manifest and the public key it's signed with, in hex, in place of loom's releases. It's refused unless `--test-fast-hashes` is given too, which no real instance runs with, so one stray flag can't point an instance at releases loom didn't sign ([tests/protocol/server_update.calx](../tests/protocol/server_update.calx)).

## Versions

- **The app offers, the instance picks.** `hello` lists every version the app speaks, and `welcome` names the one the connection uses: the newest both have.
- **An instance speaks its newest version and the one before**, so an app one release behind still works.
- **Fields are added without a new version**, since unknown fields are ignored. A new version is only for a change an older app would get wrong.
- `GET /api/info`'s `protocol` lists the versions too, so an app can say "this instance needs an update" before it logs in.
- **Version 2** sends [`browser.frame`](#browserframe) as a binary message, the JPEG as it is, rather than as JSON with it in base64, a third bigger. Nothing else changed: an instance sends a session of version 1 the frame as before.

## Usage limits

The limits of the person's plan with a program, as the program reports them ([PLAN.md](../PLAN.md#usage-limits)), are kept as the person's for that program. `acp.list` gives them with the program as `limits`, and each change is a `limits.updated` event:

- `plan`: the plan's name, as the program says it ("Claude Max"), or empty when it hasn't said.
- `session` and `week`, each where the program says it: `used`, in percent, and `resets`, when it starts again (Unix ms). A window past `resets` reads as 0% until the program says otherwise.

`limits` is `{}` for a login that has no plan, as with an API key, and a program that has never said anything of it has none. Only Claude's adapter reports them now (below). A program on settings the owner shares keeps none, since the plan is the owner's.

## How ACP maps onto loom

Inside the instance, each agent runs as an ACP agent program ([PLAN.md](../PLAN.md#agents-run-on-acp)). The apps never see ACP. What the agent says becomes loom's own frames:

| From the agent (ACP) | In loom |
|---|---|
| The instance sends the person's message to the session with `session/prompt` | `run.started` |
| Every session gets `mcpServers` with one server, `loom`: the `loom` program (or, in the desktop app, its own small `loom-mcp` program, which it writes into its folder) with the argument `mcp`, and `LOOM_URL` and that agent's `LOOM_MCP_TOKEN` in its environment. It speaks MCP 2026-07-28 to a request whose `_meta` says so (`server/discover` too, and `UnsupportedProtocolVersionError`, -32022, naming its versions, for another version), and after initialize the version of the legacy era the agent program asks for, 2025-11-25 to 2024-11-05, or else 2025-11-25; in Code Mode it speaks to each of the agent's servers in the version that server does ([PLAN.md](../PLAN.md#mcp-versions)). In Code Mode, the default ([PLAN.md](../PLAN.md#code-mode)), `LOOM_CODE_MODE=1` is there too, and it's the only server: it offers `search_tools` (`query`?, `limit`? up to 30: the TypeScript signatures of the tools that match, or every tool's name without a query) and `run_code` (`code`, `timeout`? in seconds, up to 300: runs the script in Deno with no permissions, and gives what it logged and returned, or `isError` with why it failed), through which the agent reaches the tools below, its MCP servers' and its person's plugins', and beside them, listed directly too, `add_server`, `install_server`, `list_devices`, `use_device`, `show_card`, `search_messages`, `propose_note`, `skill_failed` and `fix_skill`, the same in every session. Their MCP `annotations` say `search_tools` only reads (`readOnlyHint`) and neither destroys anything or reaches outside by itself (`destructiveHint` and `openWorldHint` false), and `propose_note`, `skill_failed` and `fix_skill` are marked as `run_code` is, in Code Mode or not, so an agent program that goes by them, as Codex does, doesn't ask before them: loom asks for each call in a script, and in `ask` before the script too. Each agent program's environment also has `OPENCODE_CONFIG_CONTENT`, OpenCode's config, with a `permission` rule allowing `loom_search_tools`, `loom_run_code`, `loom_propose_note`, `loom_skill_failed` and `loom_fix_skill`, added to the config its settings give, which keeps its own rules for them. A call by one of the tools' own names still works. With `code_mode: false` on the agent, the session also gets the agent's MCP servers and the plugins' tool servers, and `loom mcp` lists the tools below itself. It gives the agent loom's own tools: `message_agent`, `list_agents`, `create_agent`, `update_agent`, `edit_instructions`, `list_acp_agents`, `search_messages`, `list_skills`, `read_skill`, `save_skill` (proposed instead, in a task writing up a recording: [Teaching a task, and proposals](#teaching-a-task-and-proposals)), `propose_skill` and `propose_note`, `skill_failed` and `fix_skill` ([Teaching a task, and proposals](#teaching-a-task-and-proposals)), `notify`, `remember` (`text`, `shared`?, `site`?), `recall`, `forget`, `update_profile` (writes the person's whole profile anew), `list_devices`, `use_device` ([Devices as tools](#devices-as-tools)), `browser_open`, `browser_snapshot`, `browser_click`, `browser_type`, `browser_press`, `browser_scroll`, `browser_back`, `browser_screenshot`, `browser_close`, `browser_ask_person`, `browser_fill_secret` and `browser_ask_secret` ([The agent's browser](#the-agents-browser)), `schedule` and `list_routines` (`routine.put` and `routine.list`; a routine is for the calling agent unless it names another), `routine_post` (`text`, up to 20,000 bytes), `routine_quiet`, `routine_failed` (`why`) and `pause_routine` (`why`), only while the agent answers for a routine ([PLAN.md](../PLAN.md#routines); `routine_post` in a step of its flow too), `start_task` ([Tasks](#tasks)), `list_plugins`, `read_plugin` and `save_plugin` ([Plugins](#plugins)), `list_commands` and `run_command` ([Agents run commands](#agents-run-commands)), `install_server` (with `host`, `domain`?, `import`?, `from_user`? and `carry_provider`?: puts loom on one of the person's hosts and signs their app in there, on one yes on the device that reaches it ([Installing loom on a host](#installing-loom-on-a-host)); without `host`, the command that puts loom on a new server, to run by hand), `delete_agent`, `set_permission`, `delete_skill`, `delete_routine`, `delete_plugin`, `move_instance` and `add_server` (`server.offer`, with `address` and a [link code](#link-codes) as `link_code`, which their yes signs the app in with). They act with the person's rights. The last seven ask the person every time, whatever the permission mode: the call shows as a tool (`delete`, `move` or `other`) in the run that made it, with `approval.needed` (`can_always: false`), and is answered once the person does. With no run going, they refuse; an agent can't delete itself or loosen its own mode, or give another a looser mode than its own (ask, then auto, then allow). `message_agent` takes `agent` and `text`, `task`, `bounds` and `good_answer`, the last three each under its heading (`The task: …`, `Its bounds: …`, `A good answer: …`), and `files`: up to 20 paths, of 25 MiB each at most, in the agent's own folder or the shared folder, with links followed and `..` resolved. Each is kept as the person's file and goes with the message as a `file` block, as the person's attachments do; a path anywhere else is refused. A chain of asks, an asked agent asking another, goes at most four deep and never back to an agent already in it; the tool refuses with the chain's names | what the tools do: `message.created`, `agent.updated`, `notify` and the rest, as if the person had done it. A yes is done as the device that gave it, so `by` names it |
| A conversation's first message to an agent program: `session/new`, or after a restart, `session/resume` (where the agent offers `sessionCapabilities.resume`) or `session/load` (where it offers `loadSession`) with the session the conversation had. When one of the person's apps connects (`hello`), the program of the conversation of their newest turn that wasn't a routine's starts, and that conversation's session is made or picked up then, before its message, unless it would go to a fresh one. Each has `_meta` `{"claudeCode": {"options": {"allowedTools": ["mcp__loom__search_tools", "mcp__loom__run_code", "mcp__loom__propose_note", "mcp__loom__skill_failed", "mcp__loom__fix_skill"]}}}`, so Claude's adapter doesn't ask before Code Mode's tools, or before what an agent keeps from a chat, which loom keeps or asks about itself, which other programs ignore. What `session/load` plays back is dropped, and if the agent can't pick the session up, a new one starts, given the newest of the chat as a full one's is, the text block saying `which couldn't be picked up again, so this one starts fresh.` A session keeps the tools it started with (Code Mode's or plain): once the agent's `code_mode` changes, the conversation's next message goes to a new session instead, as when one nearly fills, whose prompt starts with the newest of the chat and that its tools changed | nothing: the apps see the same conversation |
| Where the person is writing from, in that prompt | a first text block, `[loom: This message comes from … It's 2026-11-21 13:05 UTC.]`, with the device's name, what it is and its time zone, the agent that sent it with `message_agent`, or the routine that sent it (`the person's routine "Morning brief", run every 30 minutes`). It comes when the session hasn't been told yet, or when it changes, never in the instructions. A routine's quiet run, and a step of its flow, has no message of its own, and its prompt always comes with a text block telling the agent the person sees nothing of the turn but what it posts with `routine_post` |
| The shared folder, in a session's first prompt | a text block, `[loom: Your person's agents share the folder PATH. …]`, once a session: `shared/USER` in the data folder, or `shared` in the person's own folder for an agent in a sandbox, made when it's first given (so the owner's isolated agents share one apart from the rest of the owner's). Every agent of the person reads and writes it, and `message_agent` hands over files from it ([PLAN.md](../PLAN.md#sharing-and-learning)) |
| A fresh session for a full one | when the agent's last `usage_update` (`used` of `size` tokens) says its session is 80% full or more, the conversation's next run starts a new session, whose prompt begins with a text block, `[loom: Your chat with your person goes on from an earlier session, which grew too long, so this one starts fresh. … The newest of it, newest last: … For anything older, search_messages finds it.]`, with the chat before the run's message. The activity log has it, as `session` ([PLAN.md](../PLAN.md#agents-run-on-acp)). A `usage_update` that comes with no run going, as after Claude's adapter compacts an idle session, counts the same. Where the run's conversation goes on in the fresh session, a `loom` message with a `fresh` mark goes in the chat before its reply, and the conversation's `context` starts again at 0 of the window it had |
| `session/update`, `usage_update`, whether a run is going or not | the conversation's `context`, `used` of `size` tokens, kept with it, so it's there after a restart, and sent with `conversation.updated` when its percent or its window changes, or whether its program compacts when asked. It says nothing of what the tokens are, so the apps don't either |
| `session/update`, `available_commands_update` with `compact` among them | the program compacts a session when asked: the `context` of a chat on it has `compacts: true`, for Compact now (`conversation.compact`), which sends a prompt of `/compact` alone |
| `session/update`, `tool_call` or `tool_call_update` with `_meta.claudeCode.toolName` `compact` | a compaction of the session's context, as Claude's adapter reports one to a client without ACP's compaction updates, never a `tool` block: once it's `completed`, a `loom` message with a `compacted` mark goes in the chat, its `before` and `after` from `rawOutput`'s `preTokens` and `postTokens`, and where a later report adds them, `message.updated` fills them in, and the conversation's `context` has `compacted`, when it was. One that `failed` leaves none. During a run the line comes before its reply; one while idle, after the chat's last message |
| A fresh session for a long-idle one | when a turn last ended in the conversation's session 12 hours ago or more, its next run starts a new session the same way, the text block saying `which had been idle for hours, so this one starts fresh. Files and pages you looked at then may have changed: look again before relying on them.` ([PLAN.md](../PLAN.md#agents-run-on-acp), "Long-idle sessions are closed") |
| A fresh session the person asked for | after `conversation.fresh`, the conversation's next run starts a new session the same way, the text block saying `which your person chose to start afresh, so this one starts fresh.`, whatever the old one held. Its `fresh` line went in the chat when it was asked for |
| A `usage_update` with `_meta["_claude/rateLimit"]` (Claude's adapter: Claude Code's rate limit info, its `unifiedWindows` `five_hour` and `seven_day`, each `utilization` from 0 to 1 and `resetsAt` in Unix seconds) | the program's `limits`, `session` and `week` in percent and Unix ms, with `limits.updated` when they changed ([Usage limits](#usage-limits)) |
| `_auth/status_update` (Claude's adapter, as it logs in: `authStatus`, with `kind` `account` and the plan's `label`, such as "Claude Max") | the program's `limits.plan`; any other `kind`, as an API key's, makes them `{}` |
| The instructions every agent follows, in that prompt | a text block, `[loom: Instructions your person gives every agent of theirs. Follow them, as you do your own.` and the defaults' `instructions` `]`, after the shared folder's. It comes when the session hasn't had them as they are now: in the first prompt, and after they change. When they're cleared, a block says they were taken back |
| The person's profile, in that prompt | a text block, `[loom: About the person, as they and their agents keep it. update_profile changes it.` and the profile's text `]`, after the one above. It comes when the session hasn't had this revision yet: in the first prompt, and after the person or an agent changes it |
| The person's files, in that prompt | an `image` block for an image of up to 5 MB, when the program says it takes images (`promptCapabilities.image`), read and put in base64 on the program's own thread as the prompt goes out, not on the instance's loop; otherwise a `resource_link` to a copy in the agent's folder, under `files/`, with its name |
| `session/update`, `agent_message_chunk` | `delta` with `kind: "text"` |
| `session/update`, `agent_thought_chunk` | `delta` with `kind: "thought"` |
| `session/update`, `agent_message_chunk` with an `image`, a `resource` with a `blob`, or a `resource_link` to a `file://` on the instance's machine | a `file` block: the file is kept as the person's, as if they'd uploaded it |
| `session/update`, `tool_call` | a `tool` block, and `tool.started` (one that asked permission first keeps the block and the call its request made). Its `title` is a few words on one line: `rawInput.description` where there is one (the few words Claude writes for a shell command, which Claude's adapter titles with the whole command), or else the title's first line, cut at 100 bytes with "…" when there's more, or with no title, its kind and the file it touches ("Edit food-log.csv", "Run a command"). The command itself stays in `input` |
| `session/update`, `tool_call_update` | `tool.updated` (its `title` made as `tool.started`'s, when the update has a title or a description), or `tool.finished` when its status is `completed` or `failed`, with that `title` too when the update that finishes it names it |
| `session/update`, `tool_call`, `tool_call_update`, `agent_message_chunk` or `agent_thought_chunk` with `_meta.claudeCode.parentToolUseId` | a subagent's: Claude's adapter tags what a subagent does with the `toolCallId` of the Task or Agent call that started it. Its block, `tool.started` and `delta` have that call as `parent`, so the apps draw it inside that call, and its words are never the reply's. A parent the run never heard of is left out, and the block is the reply's own |
| `session/update`, `user_message_chunk` | nothing: only the person's own messages are theirs, and a prompt the program gives itself or a subagent never shows as one |
| `session/update`, `plan` | a `plan` block (replaced whole each time) |
| `session/request_permission` | For a call its program never sent a `tool_call` for, first a `tool` block and `tool.started`, made from the request's `toolCall` as from a `tool_call`, so the call shows with its question; a `tool_call` for it later keeps them. Then the person's rules on what it touches ([Rules on what's touched](#rules-on-whats-touched)): a deny or an allow answers it at once, with `approval.answered` `by: "rule"`. Then `approval.needed`, or answered at once by the agent's permission mode, with `approval.answered` `by: "auto"`. A rule that asks asks even where the mode allows |
| The person's `approve` | the permission request's answer, with the `allow_once`, `allow_always`, `reject_once` or `reject_always` option that matches `allow` and `always` |
| The person's `cancel` | `session/cancel` |
| `session/prompt`'s result, with its `stopReason` | `message.created` for the reply, then `run.finished`: `end_turn` is `done`, `cancelled` is `cancelled`, `max_tokens` and `max_turn_requests` are `limit`, and an error or a crash is `failed` |

### Sandboxes

Only the owner's agents run as the instance's own user, and of those, only the ones that aren't isolated. Everyone else's run in a sandbox of their own, and so do the owner's marked `isolated` ([PLAN.md](../PLAN.md#sandboxes-where-members-agents-run)), on an instance started with `loom serve --sandboxes podman` or `--sandboxes docker`. On one without, their runs are `failed`, saying so.

- **The sandbox** is a container, `loom-<user>`, from loom's image (`ghcr.io/aaen-studios/loom:<version>`, the instance's own version, or the untagged newest for a build from source, or `--sandbox-image`), made when the person's first agent starts and kept running. When the instance starts, each sandbox (an agent's own too) whose image isn't the one it names now, by the image's id, is removed, so the next agent there makes it again from the new one: after an update, its `loom` is the new loom's. Where the image isn't here yet, the instance fetches it on a thread (`pull`) and the sandboxes go on as they are; once it's here, each stale one is removed as soon as no agent program runs in it, with its egress, and is made again from the new one when it's next used. One that can't be fetched leaves them as they are until the next start. It sees the person's folder, `sandboxes/<user>/` in the data folder, and the instance's downloaded agent programs, `acp/`, read-only, both at the same paths as outside, and nothing else of the instance's. It has the internet, but none of the server's loopback but the gate:
  - **In rootless Podman,** pasta gives it the internet and the gate's port on 127.0.0.1.
  - **In Docker,** it's on a network of loom's own, `loom`, and reaches the gate at that network's gateway. It runs as the instance's own user, not root, with no capabilities and `no-new-privileges`. It can reach whatever else the server listens on at that gateway, so the owner keeps everything but the gate off it.
- **Its limits** are 2 GB of memory, 2 CPUs and 512 processes, where the system lets the instance's user set them. Its own layer can hold at most 10 GB, where the storage allows a size (overlay on XFS with project quotas). The person's folder is kept to 10 GB too: a run doesn't start while it holds more, and is `failed`, saying so.
- **The agent program** runs in it with `exec`, in `sandboxes/<user>/agents/<agent>/`, whatever `folder` the agent names. An isolated agent of the owner's runs the same way, in `loom-<owner>`, with the program's settings, which are the owner's own. Its environment is the person's own secrets, each by its name, never the program's settings, which are the owner's, unless the owner shares them with a budget ([`acp.share`](#commands)). Their own secrets always win. Once a secret changes, their agents that aren't answering start again with it at their next message.
- **Loom's tools** reach it as the image's `loom mcp`, with `LOOM_URL` set to the gate: a listener on 127.0.0.1 (or Docker's gateway), which passes on only `GET /api/mcp/tools`, `/api/mcp/servers` and `/api/mcp/deno` and `POST /api/mcp/call`, `/api/mcp/allow`, `/api/mcp/done`, `/api/mcp/token`, `/api/mcp/sign_in` and `/api/mcp/icon`, and answers anything else 404. The person's MCP servers run in the sandbox too, started by `loom mcp` in Code Mode or by the agent without it, and so do the scripts, in the Deno the sandbox sees in `acp/`. Plugins' tool servers, which run from the instance's own folders, aren't given to sandboxed agents.
- **Files** go in and out through the sandbox. The instance writes an agent's instructions, skills and the person's files into its folder with `exec`, and reads a `file://` link the agent sends the same way, so a link the agent makes can't lead the instance to its own files. The folder, the instructions and the skills that changed go in with one `exec`: `mkdir -p` for the folder, then `tar -x --no-same-owner -f - -C /` on a tar stream of the files, so the image needs `sh`, `mkdir` and `tar`.
- **Removing** the person removes their sandboxes and their folder.
- **An agent closed to the network** (a `network` list, or `[]`) has a sandbox of its own, `loom-<user>-<agent>`, made as the person's is, but with `--network none`, so it has nothing but its own loopback, and with `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` (each also in lowercase) set to `http://127.0.0.1:3128`, `NO_PROXY` to `localhost,127.0.0.1,::1`, and `NODE_USE_ENV_PROXY=1`. It sees the person's folder and `acp/` as theirs does. Once it's running, the instance runs `loom egress` in it with `exec -i`, which listens on 127.0.0.1:3128 there and carries each connection over its standard input and output, in frames: a byte saying what it is (`O` opens a connection, `D` carries its bytes, `C` closes it), the connection's number and the length of what follows, each a big-endian u32, and that many bytes. The instance starts it again a second after it ends. Its `loom mcp` has `LOOM_URL` set to `http://127.0.0.1:3128`.
  - **What gets out.** A request whose target is a path, as `loom mcp`'s are, goes on to the gate. Anything else is an HTTP proxy's request: `CONNECT host:port`, answered `200 Connection established` once it's open, or a plain `http://` page, sent on with its path alone. A host that isn't on the list is `403`, saying what is; so is one whose address, once it's open, is on the server itself or a network of its own (loopback, link-local, multicast, and the private ranges: 10/8, 172.16/12, 192.168/16, 100.64/10 and fc00::/7, unless the list names that address). A head over 64 KiB is `431`, and a host that can't be reached `502`. An agent has 512 connections open at most. A new list holds from the next connection.
  - **Its browser** runs on the instance's computer, so the instance's end of the egress also listens on its loopback, on a port of its own, and the browser is started with that as its proxy (`--proxy-server`, `--proxy-bypass-list=<-loopback>`, `--disable-quic`, and no WebRTC but through the proxy). It takes only a proxy's requests (`400` for loom's tools), held to the same list, and `browser_open` refuses a page whose host isn't on it at once (rule `network`).
  - **What it can't do yet.** In Code Mode, `loom mcp` starts the agent's MCP servers in the sandbox, and one reached over HTTP waits for std.http to go through a proxy (CLX-429): until then, it can't be reached from an agent closed to the network, as nothing else can. Programs that don't honour `HTTPS_PROXY` reach nothing.
- **Where it can't be kept**, it isn't pretended. An instance without `--sandboxes` refuses `isolated: true` and a `network` (`not_here`), and so does `--sandboxes` itself on Windows, where a sandbox can't see the instance's folders at the same paths.

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
← {"t":"synced","seq":1047,"runs":[{"run":"run_77","conversation":"c_9","agent":"a_2","message":"msg_311","answering":"msg_310"}]}
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

Fields and frames that come with later phases. Where they only add, they come within the version they're in.

- **Device tools** (Phase 3): the instance sends `{"t":"device.call", …}` with the request, and the signed approval when there is one, and the device answers `{"t":"device.result", …}`. Each device offers its tools in `hello`.
- **Noise** (Phase 6): done, within version 1 ([Over Noise](#over-noise)).
