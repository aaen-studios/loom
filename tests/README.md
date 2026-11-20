# tests

- **The fake ACP agent** ([`fake_agent/main.calx`](fake_agent/main.calx)): a Calx program that speaks ACP over stdio, as `claude-agent-acp` does, and plays the turn its prompt spells out, one step a line: text to say, `/think`, `/plan`, `/tool`, `/ask` (a tool call that asks permission), `/slow` and `/sleep`, `/stop` with a stop reason, `/fail` and `/crash`. So sessions, approvals, cancelling and crashes are tested without a real model. The comment at its top lists every step. Run it with `calx tests/fake_agent/main.calx`.
- **Protocol tests:** Calx scripts that drive a real instance through the WebSocket, starting with Phase 1's: log in, chat, drop the connection mid-reply, reconnect, catch up.
- **End-to-end tests:** the real apps against an instance with the fake agent.

Unit tests live beside the code, as `@test` functions run with `calx test`. `calx scripts/check.calx` runs them all.
