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
- [`cache.calx`](cache.calx): the local SQLite cache (LOO-24), `core.db` in the app's data folder: agents, conversations, messages, devices, the runs still going, their tool calls and the questions still open. Each event is applied in one transaction with the cursor, its `seq`, and before the app hears of it, so the two never drift apart. A snapshot after `reset: true` replaces it all, and a login as someone else, or to another instance, clears it. The outbox is here too: a `send` waits with its client id until the instance acks it, and goes again on each reconnect.

## Config

`loom_core_start` takes `{"server": "https://loom.example.com", "data": "C:\Users\…\AppData\Roaming\Loom"}`, both optional. `data` is the folder for the cache, which is kept in memory without one. With `server`, or the one the cache last knew, the core looks for a token kept for it and connects; without a token, it waits for a login. Tests also give `token`, and `keyring`, a service of their own.

With `"mode": "local"` the core first starts an instance of its own ([PLAN.md, "An instance in the desktop app"](../PLAN.md#an-instance-in-the-desktop-app)): the server code, on a thread, listening on 127.0.0.1 on a free port, with its data in `{data}/instance`. It finds the ACP agents installed on the computer, makes a device for "This computer" owned by `username` (default `me`) of kind `kind` (default `windows`), and connects with that device's token, which is new every launch. Its cache is `{data}/local.db`, apart from `core.db`, so an instance elsewhere keeps its own. `loom_core_stop` stops the instance too.

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
| `{"t":"error","code":"offline","for":"q_2"}` | A frame other than `send` came while the core wasn't connected, so it didn't go |
| `{"t":"error","code":"not_sent","ref":"m_…","message":"…"}` | A send to an agent the instance couldn't make a conversation with, such as one deleted meanwhile. It leaves the outbox |
| `{"t":"error","code":"cache","message":"…"}` | The cache couldn't be read or written. An event that wasn't applied isn't in the cursor either, so the next connection gets it again |
| `{"t":"error","code":"bad_config","message":"…"}` | The config's `server` isn't an `http://` or `https://` address |
| `{"t":"error","code":"local","message":"…"}` | The instance on this computer didn't start, in `"mode": "local"`. The core waits for a login, as without a server |

Not done yet: treating 45 s with nothing from the instance as a dead connection. The Calx WebSocket client answers pings without showing them to the program, so it waits on CLX-395.

[`tests/protocol/core_conn.calx`](../tests/protocol/core_conn.calx) runs the connection against a real instance.
