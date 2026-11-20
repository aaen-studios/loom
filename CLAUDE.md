# Loom

Acta board: aaen-studios/loom

- [PLAN.md](PLAN.md) is the plan, and [protocol/protocol.md](protocol/protocol.md) the wire protocol. Read them before changing anything they cover, and keep them true when the code changes.
- Loom waits for Calx. When Calx is missing something, it gets a card on `aaen-studios/clx` and the loom work that needs it waits. No workarounds in loom, and no other language for the server, the core or the scripts.
- The apps hold no logic. GPUI (`desktop/`) and Swift (`ios/`) draw screens and reach the system. Everything else lives in the Calx core and server.
- v0.1 (Rust engine, React UI, Tauri) is at the `v0.1` tag, for reference only. Nothing from it is carried over unless the plan says so.
