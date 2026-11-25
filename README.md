# loom

loom is an app for chatting with your own AI agents from your computer and your phone. The agents do real work on a server of yours, or on your computer if you don't want a server.

Each agent runs on an agent program you already use, through the [Agent Client Protocol](https://agentclientprotocol.com): Claude Code, Codex, OpenCode, Gemini CLI and others. loom handles everything around them: accounts and the people you invite, devices, chats, memory, skills, routines, tasks, approvals, a browser, plugins, push, backups, and moving the whole thing between machines.

## What it does

- **Chats with your agents.** A reply reads as one answer, with what the agent did on the way folded under it. You can pin messages, branch a chat, ask on the side, queue or steer while it works, and move a chat to another agent program.
- **Agents that can do anything the app can.** Every action in the app is also a tool, so an agent can make other agents, set up routines and plugins, invite people, install loom on a new server and move your instance there.
- **Asks before it acts.** Each agent has a permission mode: ask first, ask when it's risky, or don't ask. Rules on commands, paths, sites and tools override the mode, and every action goes in an activity log.
- **Your devices as tools.** Agents can run commands, read files and take screenshots on your computers, and on servers you reach over SSH or Tailscale, with each tool off, ask or allow.
- **A browser for each agent.** You can watch it, take over, and record what you do to teach the agent a task.
- **Memory, skills, routines and tasks.** Agents keep notes, learn skills, run on schedules or webhooks, and take on long jobs in chats of their own.
- **Sandboxes.** On Linux, an agent can run in a sandbox with only the network hosts you allow. Members' agents always do.
- **Several servers in one app.** The desktop app signs in to as many as you like, and can run one itself.
- **Updates.** Releases are signed. The app checks while it's open, and one click updates the app and your servers.

## Getting it

**Windows.** Download `loom-setup.exe` from the [latest release](https://github.com/aaen-studios/loom/releases/latest). It installs for your user only, with no admin rights. On first start, choose to run loom on this computer, or sign in to a server.

**Mac**, with Apple silicon. Download `loom-mac.dmg` from the [latest release](https://github.com/aaen-studios/loom/releases/latest) and drag loom into Applications. The first time, macOS won't open it: choose Open Anyway in System Settings → Privacy & Security. It keeps itself up to date from there.

**A Linux server**, as root:

```sh
curl -fsSL https://github.com/aaen-studios/loom/releases/latest/download/install.sh | sudo sh
```

This makes a `loom` user and a systemd service. With `--domain loom.example.com` it puts Caddy in front for HTTPS, and without one it serves on your tailnet with Tailscale. It prints a one-time setup code, which you enter in the app to become the instance's owner.

**Let an agent do it.** In the app, **Add a server** has **Copy prompt**: give that prompt to one of your agents, and it installs loom on one of your hosts, brings your OpenBot agents over if you have any, and signs the app in. You say yes once.

**Docker.** Build the image from the [`Dockerfile`](Dockerfile), then:

```sh
docker run -d --name loom -p 127.0.0.1:8790:8790 -v loom-data:/data loom
docker logs loom   # the setup code
```

Put a proxy with TLS in front of it.

**Your other devices.** **Add a device**, on a device signed in, shows a QR code for a phone and a code to paste on a computer, and the new device joins once you say yes there. A phone reaches your computer over Noise, end to end, on your Wi-Fi or over Tailscale. Push notifications go through an opt-in relay and are sealed end to end. The iPhone app isn't released yet.

**The server on a Mac.** `loom-macos` in each release is the `loom` server for macOS.

## Status

loom 0.2 is a rewrite. The server and the Windows app are in daily use, and the Mac app is new. The iPhone app is being built. [PLAN.md](PLAN.md) is the plan, [protocol/protocol.md](protocol/protocol.md) is the wire protocol, and work is tracked on the [Acta board](https://acta.aaenz.no/aaen-studios/loom).

The previous loom (0.1: a Rust engine with its own model loop, a React UI and Tauri) is at the [`v0.1`](https://github.com/aaen-studios/loom/tree/v0.1) tag. Its updater offers 0.2, which replaces it and leaves your 0.1 data in `~/.loom` where it is.

## How it's built

| Part | Built with | What it does |
|---|---|---|
| Server | [Calx](https://calxlang.com) | Accounts, agents, chats, memory, routines, loom's tools and push. Runs on a server, or inside the desktop app |
| Core | Calx, as a static library | Connection, sign-in, sync and the offline cache, shared by the desktop and iPhone apps |
| Desktop app | Rust and [GPUI](https://www.gpui.rs) | Windows and macOS. Draws screens and reaches the system; everything else is in the core |
| iPhone app | Swift and SwiftUI | Links the same core |

The apps hold no logic, so the desktop and the phone can't disagree about data or the protocol.

## Building

You need [`calx`](https://calxlang.com) on your PATH, the version in [`.calx-version`](.calx-version), and for the desktop app, a recent stable Rust. `calx scripts/calx_pin.calx` says whether yours is that one, and how to install it if it isn't.

```sh
calx build server/main.calx -o loom      # the server: loom serve, loom install, …
cd desktop && cargo run --release        # the desktop app; it builds the core itself
calx scripts/check.calx                  # every Calx check and test, as CI runs them
```

The [desktop README](desktop/README.md), [server README](server/README.md) and [tests README](tests/README.md) have more.

## Layout

| Folder | What's in it |
|---|---|
| [`server/`](server/) | The server, and the `loom` program |
| [`core/`](core/) | The apps' core |
| [`shared/`](shared/) | Code the server and the core share: the protocol's types, the command list, the tools |
| [`protocol/`](protocol/) | The wire protocol |
| [`desktop/`](desktop/) | The GPUI app, and its installer in `desktop/setup/` |
| [`ios/`](ios/) | The iPhone app |
| [`push/`](push/) | The opt-in push relay |
| [`design/`](design/) | Design tokens, generated into Rust, Swift and CSS, and the logo |
| [`site/`](site/) | The website, [loom.rip](https://loom.rip) |
| [`tests/`](tests/) | Fake agent programs and hosts, protocol tests, end-to-end tests and benchmarks |
| [`scripts/`](scripts/) | Checks, tokens, icons and releases |

## License

MIT. The name and logo aren't covered by it, so forks use their own.
