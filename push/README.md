# push

Calx: `loom-push`, the opt-in push relay, for instances that can't send to Apple themselves. It sends only notifications, sealed for the phone they go to, and keeps nothing but a rate-limit counter per instance key, in memory. See [PLAN.md: The push relay](../PLAN.md#the-push-relay-opt-in), and [the protocol's Push section](../protocol/protocol.md#through-the-push-relay) for what goes over the wire.

```
loom-push key AuthKey_ABC123.p8 --key-id ABC123 --team TEAM123456 --topic no.aaenz.loom [--data DIR]
loom-push serve [--data DIR] [--port 8791] [--host 127.0.0.1] [--per-hour 600]
loom-push version
```

- **`key`** sets the relay's own APNs key, the `.p8` file from Apple's developer site, with its id, the team and the app's bundle id. It's kept in `DIR/apns.json`, readable only by the relay. Restart the relay to use it.
- **`serve`** listens on `--host` (127.0.0.1 by default: put Caddy or another proxy in front of it for HTTPS at `push.<domain>`). On the first start it makes `DIR/seal.key`, the X25519 key phones seal their APNs tokens with. Keep that file: phones registered with the old key stop getting push when it changes. Each instance key sends at most `--per-hour` notifications an hour.
- `DIR` is `loom-push-data` in the current folder unless `--data` says otherwise.

It answers two requests:

| Request | Answer |
|---|---|
| `GET /v1/key` | `{"key": "<its X25519 public key, base64>"}` |
| `POST /v1/push` | `{}` once APNs took it. Errors as `{"error": {"code", "message"}}`: 401 `stale` or `bad_signature`, 429 `too_many`, 400 `bad_token`, 410 `gone` (APNs says the phone doesn't take push with that token any more), 503 `no_apns_key`, 502 `apns` |

`tests/protocol/relay.calx` runs it against a fake APNs, with an instance and a phone's core.
