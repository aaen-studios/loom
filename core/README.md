# core

Calx: the apps' core, built with the server as one static library that the desktop and iPhone apps link. It keeps the WebSocket to the instance, logs in, keeps the SQLite cache and the outbox, and catches up through the event log.

Only JSON crosses its C boundary:

```c
uint8_t *loom_core_start(const uint8_t *config_json, size_t len, loom_event_fn on_event, uint8_t *ctx);
void      loom_core_send(uint8_t *core, const uint8_t *json, size_t len);
void      loom_core_stop(uint8_t *core);
```

## Files

- [`lib.calx`](lib.calx): the C boundary. `calx build --staticlib core/lib.calx` makes the library and `loomcore.h`. `loom_core_start` runs the connection on a thread of its own, and a second thread hands what it says to `on_event`. `loom_core_stop` waits for both threads, so no `on_event` comes after it returns.
- [`conn.calx`](conn.calx): the connection to the instance (LOO-22). It opens one WebSocket to `/ws`, says `hello` with its cursor, then forwards catch-up and live frames to the app. When the connection ends, it opens again after 0.5 s, doubling up to 30 s, plus up to 20% at random. It opens again at once on wake or a network change. It stops for good on 4001 (signed out) and 4002 (update needed).
- [`login.calx`](login.calx): login, setup and logout over the instance's HTTP API (LOO-23). The core's loop starts each request and checks it on every turn, so nothing waits while the instance hashes a password. The device token is kept in the system's credential store with std.keyring (Credential Manager, the Keychain), under the service `loom` and the instance's address. A 4001 close forgets it.
- [`device.calx`](device.calx): this device as a tool for agents (LOO-37). Its Ed25519 key and the person's choice for each tool (off, ask or allow) are kept in the cache. It tells the instance both on welcome, signs the approvals given here, and acts on a `device.call` only as the person chose here, and for a tool that asks, only with this device's own signature of the exact request. Commands run in PowerShell or zsh without holding up the loop.
- [`cache.calx`](cache.calx): the local SQLite cache (LOO-24), `core.db` in the app's data folder: agents, conversations, messages, devices, the runs still going, their tool calls and the questions still open. Each event is applied in one transaction with the cursor, its `seq`, and before the app hears of it, so the two never drift apart. A snapshot after `reset: true` replaces it all, and a login as someone else, or to another instance, clears it. The outbox is here too: a `send` waits with its client id until the instance acks it, and goes again on each reconnect.

## Config

`loom_core_start` takes `{"server": "https://loom.example.com", "data": "C:\Users\…\AppData\Roaming\Loom"}`, both optional. `data` is the folder for the cache, which is kept in memory without one. With `server`, or the one the cache last knew, the core looks for a token kept for it and connects; without a token, it waits for a login. Tests also give `token`, and `keyring`, a service of their own.

With `"mode": "local"` the core first starts an instance of its own ([PLAN.md, "An instance in the desktop app"](../PLAN.md#an-instance-in-the-desktop-app)): the server code, on a thread, listening on 127.0.0.1 on a free port, with its data in `{data}/instance`. It finds the ACP agents installed on the computer, makes a device for "This computer" owned by `username` (default `me`) of kind `kind` (default `windows`), and connects with that device's token, which is new every launch. Its cache is `{data}/local.db`, apart from `core.db`, so an instance elsewhere keeps its own. `loom_core_stop` stops the instance too.

With `"version": "0.3.0"`, the core keeps the app up to date (PLAN.md, "Updates and versions"). A little after it starts, then every six hours, it reads the newest release's `manifest.json` and `manifest.json.sig` from `update` (GitHub's latest release, unless the config names another manifest; an empty `update` turns it off). If the signature is the release key's (`shared/release.calx`) and the release is newer, it downloads the file for `platform` (default `windows-desktop`) into `{data}/update/<version>/`, checks its SHA-256 against the manifest, and tells the app `core.update`. The app swaps it in. Builds from source leave `version` out.

## Between the app and the core

The app sends the core protocol frames (`send`, `cmd`, `approve`, `cancel`, …), which go on to the instance as they are, and these of the core's own:

| From the app | Means |
|---|---|
| `{"t":"login","id":"l_1","server":"https://…","username":"…","password":"…","device":{"name":"Desk PC","kind":"windows","tz":"Europe/Oslo"}}` | Logs in (`POST /api/login`). `server` may be left out once one is known |
| `{"t":"setup", …, "code":"…"}` | Sets a new instance up (`POST /api/setup`), with the code it printed |
| `{"t":"logout"}` | Revokes this device's token (`POST /api/logout`), forgets it, and signs out |
| `{"t":"send","conversation":"c_9","text":"…"}` | A message the person wrote. It goes in the outbox, with a `cid` if it has none, and out when the connection is up. With `agent` instead of `conversation`, it goes to that agent's newest conversation, which the core makes first if there's none: the desktop app shows each agent as one ongoing chat |
| `{"t":"core.get","id":"g_1","what":"messages","conversation":"c_9","before":"msg_40","limit":50}` | Reads the cache: `what` is `conversations`, `agents`, `devices`, `runs`, `tools`, `approvals`, `outbox` or `messages`. Older messages than the cache has come from the `messages.list` command, whose result the cache keeps |
| `{"t":"app.active","active":false}` | Someone started or stopped looking at the app. The core notifies only while no one is, and tells the instance (the `device` frame) |
| `{"t":"app.wake"}` | The system woke from sleep. The core opens a new connection at once |
| `{"t":"app.network"}` | The network changed. The same |
| `{"t":"file.upload","id":"u_1","path":"C:\\Users\\…\\shot.png"}` | Sends a file to the instance (`POST /files`), up to 64 MB. `core.file` answers with its `file`, which goes in a `send`'s `files` |
| `{"t":"file.get","id":"f_1","file":"9f86d0…","thumb":true}` | Fetches a file, or with `thumb` its thumbnail, into `{data}/files/`, and `core.file` says where. A file goes in a folder of its own, under its `name` if one is given, so the program that opens it knows its kind. One it has already is answered at once |
| `{"t":"update.check","id":"u_2"}` | Looks for an update now. `core.update` answers, with the id |
| `{"t":"core.stop"}` | Sent by `loom_core_stop`. The core closes the connection and ends |

The core tells the app every frame the instance sends (`welcome`, `event`, `delta`, `synced`, `ack`, `result`, `error`, …), after the cache has taken it. Events it has applied already aren't sent again. Deltas go on only when they add text: a repeat is dropped, and a gap means one was lost, so the core reconnects and gets the reply again from position 0. And these of its own:

| From the core | Means |
|---|---|
| `{"t":"core.conn","state":"connecting"}` | Opening the socket, or `hello` is sent and `welcome` hasn't come |
| `{"t":"core.conn","state":"live"}` | Welcomed. Catch-up follows, then `synced` |
| `{"t":"core.conn","state":"offline","retry_ms":540,"why":"…"}` | The connection ended. It tries again after `retry_ms` |
| `{"t":"core.conn","state":"signed_out","why":"…"}` | No token yet ("not logged in"), a logout, or the instance refused the token (4001), which is forgotten. The app asks for a login |
| `{"t":"core.conn","state":"update_needed","why":"…"}` | No protocol version in common (4002) |
| `{"t":"core.login","id":"l_1","ok":true,"user":"u_1","device":"d_3","kept":true}` | Logged in, and connecting. `kept` is false, with `why`, when the credential store couldn't keep the token, so the next start asks again |
| `{"t":"core.login","id":"l_1","error":{"code":"bad_password","message":"…","retry_after":60}}` | The login failed: the instance's error code, `unreachable`, `bad_server` or `busy`. `retry_after` comes with `rate_limited` |
| `{"t":"error","code":"keyring","message":"…"}` | The credential store couldn't be read at start |
| `{"t":"core.queued","cid":"m_…","conversation":"c_9","text":"…","at":…}` | A `send` is in the outbox. The `ack` with its `cid` follows once the instance has it |
| `{"t":"core.data","id":"g_1","what":"messages","items":[…],"more":true}` | The answer to `core.get`, oldest first for messages. `more` says the cache has older ones |
| `{"t":"core.notify","title":"Builder","body":"…","agent":"a_2","conversation":"c_9","kind":"run.finished"}` | Worth a system notification: a reply that finished or failed, or a question waiting (`approval.needed`). Only for live events while no one is looking, not for catch-up |
| `{"t":"core.file","id":"u_1","file":{"file":"9f86d0…","name":"shot.png","mime":"image/png","size":48213}}` | A `file.upload` went up. For a `file.get`, `"path":"…"` instead: where the file is. Either can have `error`, with `code` and `message`, instead |
| `{"t":"core.update","version":"0.3.0","path":"C:\\Users\\…\\update\\0.3.0\\loom.exe","notes":"…"}` | A newer release is downloaded and checked, at `path`. To an `update.check` that found nothing new, `"latest":true` and this version instead, or `error` |
| `{"t":"error","code":"offline","for":"q_2"}` | A frame other than `send` came while the core wasn't connected, so it didn't go |
| `{"t":"error","code":"not_sent","ref":"m_…","message":"…"}` | A send to an agent the instance couldn't make a conversation with, such as one deleted meanwhile. It leaves the outbox |
| `{"t":"error","code":"cache","message":"…"}` | The cache couldn't be read or written. An event that wasn't applied isn't in the cursor either, so the next connection gets it again |
| `{"t":"error","code":"bad_config","message":"…"}` | The config's `server` isn't an `http://` or `https://` address |
| `{"t":"error","code":"local","message":"…"}` | The instance on this computer didn't start, in `"mode": "local"`. The core waits for a login, as without a server |

A connection that died without a close, as a sleeping laptop's does, is noticed after 45 s with nothing from the instance (the socket pings after 25 s of quiet and waits 20 s more), and the core reconnects.

[`tests/protocol/core_conn.calx`](../tests/protocol/core_conn.calx) runs the connection against a real instance.
