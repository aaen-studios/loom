# Loom

A self-hostable agent app. You chat with your own AI agents from your PC and phone, and they do real work on your server, or on your PC if you don't want a server.

- Every agent runs on an existing agent program through the [Agent Client Protocol](https://agentclientprotocol.com): Claude Code, Codex, OpenCode, Gemini CLI and others. Loom runs everything around them: accounts, devices, conversations, memory, routines, approvals, push, plugins, and moving between machines.
- The server and the app core are [Calx](https://calxlang.com). The desktop app is GPUI, on Windows and macOS. The iPhone app is Swift.
- A server is optional. The desktop app can run the whole thing itself.

## Status

**Being rewritten, Phase 0.** Nothing runs yet. [PLAN.md](PLAN.md) is the plan, and [protocol/protocol.md](protocol/protocol.md) is the wire protocol. Work is tracked on the [Acta board](https://acta.aaenz.no/aaen-studios/loom).

The previous loom (v0.1: a Rust engine with its own model loop, a React UI, Tauri) is at the [`v0.1`](https://github.com/aaen-studios/loom/tree/v0.1) tag.

## Layout

| Folder | What's in it |
|---|---|
| [`protocol/`](protocol/) | The wire protocol between the apps and an instance |
| [`shared/`](shared/) | Calx: the protocol's types and the command list, used by the server and the core |
| [`server/`](server/) | Calx: the server, and the `loom` program |
| [`core/`](core/) | Calx: the apps' core, a static library the desktop and iPhone apps link |
| [`desktop/`](desktop/) | Rust: the GPUI app for Windows and macOS |
| [`ios/`](ios/) | Swift: the iPhone app |
| [`push/`](push/) | Calx: the opt-in push relay |
| [`site/`](site/) | The public site and the docs |
| [`design/`](design/) | Design tokens, generated into Rust, Swift and CSS |
| [`tests/`](tests/) | The fake ACP agent, protocol and end-to-end tests |
| [`scripts/`](scripts/) | Calx: build, release, install |

## License

MIT. The name and logo aren't covered by it, so forks use their own.
