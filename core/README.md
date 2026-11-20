# core

Calx: the apps' core, built with the server as one static library that the desktop and iPhone apps link. It keeps the WebSocket to the instance, logs in, keeps the SQLite cache and the outbox, and catches up through the event log.

Only JSON crosses its C boundary:

```c
int  loom_core_start(const char *config_json, size_t len, loom_event_fn on_event, void *ctx);
void loom_core_send(const char *json, size_t len);
void loom_core_stop(void);
```

Starts in Phase 2, once Calx can build it (CLX-333, CLX-334).
