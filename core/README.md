# core

Calx: the apps' core, built with the server as one static library that the desktop and iPhone apps link. It keeps the WebSocket to the instance, logs in, keeps the SQLite cache and the outbox, and catches up through the event log.

Only JSON crosses its C boundary:

```c
uint8_t *loom_core_start(const uint8_t *config_json, size_t len, loom_event_fn on_event, uint8_t *ctx);
void      loom_core_send(uint8_t *core, const uint8_t *json, size_t len);
void      loom_core_stop(uint8_t *core);
```

[`spike.calx`](spike.calx) is the LOO-4 spike's stand-in: it streams a made-up reply to the desktop app through `on_event`, from a thread of its own, to show the boundary works. The real core replaces it in Phase 2.
