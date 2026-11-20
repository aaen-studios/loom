# tests

- **The fake ACP agent:** a Calx program that speaks ACP over stdio and plays scripted turns (replies, tool calls, permission requests, crashes), so sessions, approvals and reconnects are tested without a real model.
- **Protocol tests:** Calx scripts that drive a real instance through the WebSocket, starting with Phase 1's: log in, chat, drop the connection mid-reply, reconnect, catch up.
- **End-to-end tests:** the real apps against an instance with the fake agent.

Unit tests live beside the code, as `@test` functions run with `calx test`.
