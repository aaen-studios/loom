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
- [`spike.calx`](spike.calx): the LOO-4 spike's stand-in, which the desktop spike still links. LOO-25 moves the desktop app to `lib.calx`.

## Config

`loom_core_start` takes `{"server": "https://loom.example.com", "token": "…", "after": 1046}`. `after` is the newest event `seq` the app has. Login (LOO-23) and the cache (LOO-24) will fill these in themselves.

## Between the app and the core

The app sends the core protocol frames (`send`, `cmd`, `approve`, `cancel`, …), which go on to the instance as they are, and these of the core's own:

| From the app | Means |
|---|---|
| `{"t":"app.wake"}` | The system woke from sleep. The core opens a new connection at once |
| `{"t":"app.network"}` | The network changed. The same |
| `{"t":"core.stop"}` | Sent by `loom_core_stop`. The core closes the connection and ends |

The core tells the app every frame the instance sends (`welcome`, `event`, `delta`, `synced`, `ack`, `result`, `error`, …), and these of its own:

| From the core | Means |
|---|---|
| `{"t":"core.conn","state":"connecting"}` | Opening the socket, or `hello` is sent and `welcome` hasn't come |
| `{"t":"core.conn","state":"live"}` | Welcomed. Catch-up follows, then `synced` |
| `{"t":"core.conn","state":"offline","retry_ms":540,"why":"…"}` | The connection ended. It tries again after `retry_ms` |
| `{"t":"core.conn","state":"signed_out","why":"…"}` | The instance refused the token (4001). The app asks for a login |
| `{"t":"core.conn","state":"update_needed","why":"…"}` | No protocol version in common (4002) |
| `{"t":"error","code":"offline","for":"q_2"}` | A frame came while the core wasn't connected. The outbox (LOO-24) will keep sends instead |
| `{"t":"error","code":"bad_config","message":"…"}` | The config has no `http://` or `https://` server |

Not done yet: treating 45 s with nothing from the instance as a dead connection. The Calx WebSocket client answers pings without showing them to the program, so it waits on CLX-395.

[`tests/protocol/core_conn.calx`](../tests/protocol/core_conn.calx) runs the connection against a real instance.
