# Loom: the plan

A self-hostable agent app. You chat with your own AI agents from your PC and phone, and they do real work on your server, or on your PC if you don't want a server. It's built for how Elliot works, and open for anyone to run.

This is a rewrite. Loom v0.1 (a Rust engine with its own model loop, a React UI, desktop only) is replaced from the ground up. Its design is not carried over either: the new design starts from a blank page.

- **A passion project, not a business.** MIT license, no CLA, open for anyone to use and contribute to. The name and logo aren't covered by the MIT license, so forks use their own.
- **A real test project for Calx.** The server and the app core are Calx. When Calx is missing something, it gets added to Calx.
- **Repo:** `aaen-studios/loom`. On the VPS as a T3 project.
- **Board:** Acta, [`aaen-studios/loom`](https://acta.aaenz.no/aaen-studios/loom).
- **Protocol:** [protocol/protocol.md](protocol/protocol.md).

A word used throughout: an **instance** is one running loom server and its people, on a server or inside the desktop app. (A placeholder word; see [Open questions](#open-questions).)

## Principles

1. **Loom waits for Calx.** When loom needs something Calx doesn't have, it's built into Calx first (std, compiler or runtime), and then loom uses it. No workarounds in loom, and no other language to get around it. Each gap becomes a card on the Calx board and blocks the loom work that needs it.
2. **One core, a native app per platform.** The desktop app is GPUI (Rust) on Windows and macOS, and the iPhone app is Swift. Both link the same Calx core, so they can't disagree about data, sync or the protocol. Only their screens differ, and the phone does less than the desktop where that suits a phone.
3. **The apps hold no logic.** GPUI and Swift draw screens and reach the system. Everything else lives in Calx.
4. **Replace OpenBot for Elliot first.** Matching everything OpenBot does isn't the goal. Doing what Elliot uses, better, is.
5. **Self-hosted, for several people.** An instance has accounts. The owner invites people, and each person's data is their own.
6. **A server is optional.** The desktop app can run an instance itself, with the same server code, for people who don't want a server. Only the desktop apps can. The phone always connects to an instance somewhere else.
7. **Agents can do anything the app can.** Every action in the app is a command in the protocol, and every command is also a tool. An agent can set up agents, routines, ACP agents, plugins and invites, and even install loom on a new server and move the instance there.
8. **Everything moves.** An instance moves whole between computers and servers, and the apps follow it on their own.

## Design

Not decided here, and nothing from v0.1 or elsewhere is assumed. The only technical requirements:

- The look lives as design tokens in one file. A Calx script generates them for each place that uses them: Rust for the desktop app, Swift for the iPhone app, and CSS for the site, the docs and panels.
- Panels get the tokens as CSS variables, so plugins look like part of the app.
- The main parts to design: the chat, a streaming reply, a tool call, an approval, the agent list, settings, a panel.

## Architecture

| Part | Technology | Role |
|---|---|---|
| Server | Calx, on a server or inside the desktop app | Accounts, agents, conversations, memory, routines, loom's own tools, push. Every agent runs on an ACP agent program (Claude, Codex, OpenCode, Gemini CLI and others) |
| App core | Calx, built as a static library with C functions | Connection, login, sync, offline cache. Linked into the app on all three platforms. On Windows and macOS the same library also carries the server |
| Desktop app | GPUI (Rust), on Windows and macOS | Drawn on the GPU (DirectX on Windows, Metal on macOS). Low memory, fast start, no web view for the app itself. Tray, notifications, starting at login and the updater come from small Rust crates |
| iPhone app | Swift and SwiftUI | Native from the start. Widgets, Live Activities, the push extension and Shortcuts sit in the same Xcode project |

```
desktop: GPUI (Rust) ─┐
                      ├─ C calls ─ core (Calx) ─ SQLite cache
iPhone:  Swift ───────┘
                                                               │
                                       WebSocket: TLS, or Noise end to end
                                                               │
                    ┌──────────────────────────────────────────┴──────────────────┐
       an instance on a server                                   an instance in the desktop app
  domain → Caddy or a tunnel → server (Calx)                the same server, in the app's library
                    └──────────────────────────────────────────┬──────────────────┘
                                                               ├ SQLite, files
                                                               ├ ACP agents: Claude, Codex, OpenCode, Gemini CLI, …
                                                               │    with loom's tools through `loom mcp`, and MCP servers
                                                               ├ plugins
                                                               └ APNs (iPhone push)
```

Native features (push, Face ID, haptics, the share sheet, notifications, the camera) are each app's own work: Rust crates and the Windows and macOS APIs on desktop, Apple's frameworks on iPhone. The core asks for them with a message and gets the answer back the same way.

### The core's C boundary

The core is the same Calx code on every platform, and only JSON crosses into Rust or Swift:

```c
typedef void (*loom_event_fn)(const uint8_t *json, size_t len, uint8_t *ctx);

uint8_t *loom_core_start(const uint8_t *config_json, size_t len, loom_event_fn on_event, uint8_t *ctx);
void      loom_core_send(uint8_t *core, const uint8_t *json, size_t len);
void      loom_core_stop(uint8_t *core);
```

- The app calls `calx_init` once, which starts the Calx runtime (`calx build --staticlib` makes it and the header). `loom_core_start` then starts the core's own thread, which runs std.net's event loop. The config holds the data folder and the server's address. It gives back the core: Calx has no global variables, so what the core keeps lives in a box the app holds and passes to the other two calls.
- The app passes JSON to `loom_core_send`, and the core answers through `on_event` on its own thread. The app hands each event to its UI thread: a GPUI entity update on desktop, the main actor on iPhone.
- What happens to the app (sleep, going to the background on iPhone, a new network) comes in as messages too, such as `{"t":"app.background"}`.
- The UI never talks to the server itself. Everything goes through the core, so the cache and the outbox always know what happened.

### What the core does

- Keeps one WebSocket open to the server, reconnects with backoff, and notices sleep and network changes.
- Logs in, and keeps the device token in the system's credential store (Windows Credential Manager, Apple's Keychain).
- Keeps a local SQLite cache of conversations, messages and agents, so the app opens at once and can be read offline.
- Keeps an outbox. A message written offline gets a client id and goes out on reconnect. The server ignores a client id it has already seen, so nothing is sent twice.
- Catches up through the event log (below).

### An instance in the desktop app

People who don't want a server run their instance in the desktop app. On first start the app asks: connect to an instance, or run one here.

- **The same server code, in the same library.** The server is compiled into the app's static library beside the core, since a process can hold only one Calx runtime. With `{"mode":"local"}` in its config, `loom_core_start` also starts the server on its own thread, listening on 127.0.0.1 on a free port with a token made for that launch. The core then connects to it as it would to any instance. One protocol and one code path, so whatever works on a server works here.
- **Data** goes in the app's data folder (`%APPDATA%\Loom`, `~/Library/Application Support/Loom`), and the agents' folders go there or wherever you choose.
- **Just you, no password.** Your account on the computer is the lock. A desktop instance is for its owner and their own devices. To share it with other people, move it to a server.
- **Agents work on your computer.** They're the ACP agents installed there, with your own logins, and their commands run in PowerShell on Windows and zsh on macOS.
- **The `loom` program comes along.** The app installs the same `loom` program a server runs, beside itself. ACP agents need it for `loom mcp`, and it gives the desktop instance the same command line as a server.
- **It keeps running.** The app starts when you log in and lives in the tray, so routines run while the window is closed. Quitting from the tray stops the instance. Routines missed while it was off run when it starts again, or are skipped, as each routine says.
- **Your phone** connects to it by scanning a QR code in the desktop app ([below](#connecting-devices)). An iPhone can't run an instance itself, since iOS stops apps in the background.
- **Moving.** A desktop instance moves to a server, or back, whenever you like ([Moving an instance](#moving-an-instance)).

## Sync: an event log with a cursor

Every change the server makes for a person is an event with a sequence number, counted per person and stored in `events`. The app remembers the last number it has. When it connects it sends that number, gets everything after it, and then gets live events as they happen.

- One design covers live updates, reconnecting, the offline cache and several devices at once.
- Streamed text (`delta`) goes out live, with no number. The finished message is stored as one event. A device that connects in the middle of a reply gets the reply so far, then the rest live.
- Old events are pruned after a while. A device too far behind gets a fresh snapshot instead.

## Accounts and invites

An instance has accounts, and the owner decides who's in. (Only an instance on a server has other people. A desktop instance is just its owner, as above.)

The data is kept per person from the first day, so a second person never needs a migration. The screens for inviting and managing people come later, in Phase 10, when other people start using loom. Until then, accounts are made from the command line.

- **Setup.** On first start the server prints a one-time setup link. Whoever opens it creates the owner account.
- **Roles.** Owner, admin and member. Owners and admins invite and manage people.
- **Invites.** An invite is a link (`https://<instance>/join/<code>`) that opens the app. It's good for one use or several, it expires, and it says which role it gives. The person picks a username and password, and they're in. Open sign-up stays off unless the owner turns it on.
- **Login.** Username and password, hashed with argon2id. Each login makes a device token (32 random bytes, kept on the server as a SHA-256 hash), so the password is used only to log in. Two-factor with TOTP comes later, and Calx already has the HMAC it needs.
- **Devices.** Each person sees their devices, and can rename them, revoke any of them, or sign out everywhere.
- **Forgotten passwords.** An owner or admin makes a reset link. The owner's own reset comes from the server's command line.
- **Deleting.** People delete their own account in the app, which removes their data (the App Store requires this). The owner can remove people.
- **Taking your data.** Each person can export their own conversations, agents and files, apart from moving the whole instance.

What's private and what's shared:

| Private to each person | Shared across the instance |
|---|---|
| Conversations, memory, routines, their own agents, their own secrets (their Acta key, say) | Agents the owner or an admin shares, MCP servers the owner sets up for everyone, skills shared to the instance |

What the owner controls for each member:

- **Agents and keys.** Which ACP agents they can use, and whether those can use API keys the owner shares. Shared keys come with a token budget per day or month, counted from the usage the agents report, and loom stops a session that goes over. Members can always bring their own keys and logins.
- **MCP servers** the owner sets up for everyone, and which members get them.

A member's agents always run in that member's sandbox (below).

Secrets (MCP keys, tokens) are encrypted in the database with a key only the server holds. The key is `secret.key` in the data folder, made on the first start and readable only by the instance's user, never in the database. Each secret is sealed with XChaCha20-Poly1305 and a random nonce, bound to its person and name. So are the other things only the instance should read: an ACP program's settings, its API keys among them, bound to the program, and a routine's webhook secret, bound to the routine. Settings kept before sealing came are sealed on the next start.

### Sandboxes: where members' agents run

Every ACP agent runs commands and reads and writes files. On a server, all of the instance's data belongs to the `loom` user: the database, everyone's files, the key that opens the secrets. A member's agent running as `loom` could read all of it, so it would in effect be the owner.

- **The owner's agents** run as `loom` on a server, or as the owner in the desktop app. They can do anything.
- **A member's agents** always run in that member's sandbox: a container of their own (rootless Podman, or Docker). It has:
  - their own folder, and nothing else of the instance's;
  - limits on CPU, memory and disk;
  - the internet, but not the server's own local services, such as the instance itself or anything else listening on 127.0.0.1.

  The member's ACP agents and their logins live in the sandbox, and they can install what they like there. Loom's own tools reach in through `loom mcp`, with only that member's rights.
- **An instance with members needs Podman or Docker** on its server. The install script sets it up.

## Connecting devices

A device joins an instance in one of three ways:

- **Log in.** Type the instance's address, then a username and password. For instances on a server.
- **Scan a QR code** from a desktop app that's already connected, as WhatsApp links a computer. This works with any instance, and it's the only way into one that runs in a desktop app. The new device gets its own device token for the same account.
- **An invite link**, for someone who doesn't have an account yet, on an instance on a server.

The QR code holds the instance's id and public key, the addresses to reach it, and a one-time code that expires after a few minutes. The core makes what goes in it, and the desktop app draws it. The phone scans it with Apple's own camera code reader (AVFoundation or VisionKit).

**Each instance has a key pair**, made when it's created, and that key is its identity. The apps check it on every connection, on top of TLS. That's what lets an instance move to a new address without the apps trusting a stranger.

**Reaching an instance in a desktop app:**

- **On the same Wi-Fi**, directly. If the PC's address changes, the phone finds it again with mDNS.
- **Away from home**, through Tailscale, which people set up themselves. Loom passes no traffic through a server of its own (the only relay it has is the [opt-in one for push](#the-push-relay-opt-in)). A home PC sits behind the router, so a phone on mobile data can't reach it on its own. Tailscale puts the PC and the phone on one private network. Loom sees the PC's Tailscale address (a `100.x` address, or a `….ts.net` name) and puts it in the QR code beside the Wi-Fi one, so the phone tries Wi-Fi first and Tailscale next. Any other VPN works the same way.
- **Encrypted end to end** either way, with a Noise handshake against the key from the QR code. Tailscale encrypts too, but loom doesn't count on it, and on Wi-Fi nothing else does.

## Agents

Agents live in the database, and are made and edited in the app.

- **What an agent has:** a name and a look, the ACP agent it runs on with that agent's model and mode ([Agents run on ACP](#agents-run-on-acp)), instructions (Markdown, like an `AGENTS.md`), its permission mode, its MCP servers, its skills, and a workspace folder. MCP servers are set up once, on the app's **MCP servers** page, and each agent picks the ones it gets on its edit page. A setting can name one of the person's secrets (`ACTA_KEY=secret:ACTA_KEY`), which is filled in only for the agent's process.
- **Revisions.** Each change to an agent is saved as a revision, and each run records the revision it used. You can see which instructions gave an answer, and roll back.
- **Agents edit themselves.** An agent can change its own instructions with a tool, as OpenBot's agents (and v0.1's Atelier mode) do. The change shows up as a revision by that agent.
- **Skills.** Named Markdown bundles with files, loaded only when an agent needs them. Personal or shared.
- **Agents talk to each other.** `message_agent` starts or continues a conversation with another of your agents, so a main agent can hand work to the others. A message from an agent never counts as your approval for a risky action.
- **As files.** An agent can be exported and imported as a folder (`agent.toml`, `instructions.md`, `skills/`), for backups, git, and moving between instances. OpenBot's agents come in the same way, from their `AGENTS.md`.

### Agents can do anything the app can

- **One list of commands.** Every command the app sends (make an agent, change a routine, add an ACP agent, install a plugin, make an invite, export, move) is defined once in `shared/`. Loom's own tools are made from that same list, so each new feature is a tool from its first day.
- **The same rules as you.** An agent works with its person's rights and no more. Anything that sends, pays, deletes, lets people in, changes permissions or moves the instance asks first. An agent can't raise its own permission mode or answer its own approval.
- **Scriptable to the end.** The server's command line (`loom install`, `loom receive`, `loom status` and the rest) does everything the app does, and prints JSON when asked, so an agent working over SSH reads it reliably.

Say you ask: "Set up loom on this VPS and move me there." The agent:

1. Connects with `ssh`, using the address and key you give it. That's an approval.
2. Runs the install script. It downloads the server for that machine, makes the `loom` user and the service, and sets up Caddy for TLS if there's a domain, or Tailscale if there isn't.
3. Starts `loom receive` there, and reads the move code it prints.
4. Asks you once: "Move this instance to the new server?" Then it moves it, and your apps follow.

## Plugins

Agents and people can extend loom itself:

- **Panels:** small web apps inside loom, such as a dashboard, a tracker or a form. They're HTML, CSS and JavaScript, shown in a web view of their own (below), the only web view in either app. Panels live on the instance, so they show on every device, the phone too.
- **Tools:** an MCP server in any language, Calx scripts included, that the instance runs.
- **A manifest**, `plugin.toml`, gives the name, version, permissions, and the panel and tool server it has.
- **Agents write plugins.** An agent makes or edits a plugin with tools, as it edits its own instructions. Each change is a revision. A plugin that asks for new permissions waits for your approval.
- **Plugins are files**, like agents. They export as folders and move with the instance.
- **The App Store.** Apple's guideline 4.7 sets conditions for HTML5 mini apps and plug-ins in an app. Check them before submitting.

### The panel sandbox

A panel is code that an agent wrote, or that came from someone else, running inside the app. The app can do everything you can: it talks to the core, which holds your login. If a panel's JavaScript could reach the app, it could act as you, or send your data anywhere. So each panel is walled off:

- **Its own web view,** and nothing else runs in it. On desktop that's WebView2 on Windows and WKWebView on macOS, embedded in the GPUI window through `wry`. On iPhone it's a WKWebView. The panel is loaded from a custom scheme with an origin of its own, and the web view has no bindings into the app beyond the one below.
- **No network.** A content security policy (`connect-src 'none'` and the like), plus the web view refusing any request outside the panel's own scheme, stops it fetching anything or sending anything out.
- **One door.** The panel talks to the app only through one message handler (`window.ipc` from `wry`, a `WKScriptMessageHandler` on iPhone). The app checks each message against what the panel's manifest asked for and you allowed: its own storage, reading certain data, calling certain commands, messaging an agent. It answers only those.
- **Storage through the door.** A panel's data belongs on the instance, where it syncs to every device and moves with it, so the web view's own storage is turned off.

An embedded web view is a native window of its own, laid over the GPUI window, so GPUI can't draw menus or popovers on top of a panel. Panels are laid out so nothing needs to overlap them. Phase 7 starts with a panel that tries to break out on each platform, and checks that it can't reach the app or the network.

On Windows that test is `loom --panel-test` (`desktop/src/panel_test.rs`), in CI. Its first runs showed two things. The policy and WebView2's request filter each stop every HTTP request on their own. WebRTC goes around both, and around the proxy too, and WebView2 has no switch for it, so a script that runs first in every frame, blank frames included, takes it away. The Mac and the iPhone get the same test with their WKWebViews.

Later: Calx library plugins in the server (Calx's own `@export` plugins), once loom can build them with the Calx it was built with.

## Server

- **One Calx process** on std.http.server (std.net's event loop underneath, WebSockets included), listening on 127.0.0.1. On the VPS, the domain comes in through the Cloudflare tunnel and Caddy, which handle TLS. Self-hosters put Caddy or their own proxy in front.
- **HTTP routes** for what isn't a live session: `/api/setup`, `/api/login`, `/api/join/<code>`, `/hooks/<id>` for webhooks, and `/ws` for the WebSocket.
- **Slow work** (password hashing, files, reading each agent program) runs on worker threads, through std.thread and its queues, so the event loop never waits.
- **Its own Unix user** on a server, `loom`, with its own folder. "Shell on the VPS" means what that user can do. (In the desktop app it runs as you.)
- **Config** in one TOML file: the address to listen on, the data folder, the instance's ACP agents, push settings. Secrets come from a separate file only `loom` can read.

## Agents run on ACP

Loom doesn't run models itself. Every agent runs on an ACP agent program, and loom is its client through the [Agent Client Protocol](https://agentclientprotocol.com) (JSON-RPC over stdio). The agent program runs the model, its own loop, its shell and file tools, and its context. Loom runs everything around it: people, devices, conversations, memory, routines, approvals, push, plugins and moving.

**Why only ACP.** Claude Code, Codex and OpenCode are better agent loops than loom would build (v0.1 built its own, and keeping up was most of the work), and they keep getting better without loom doing anything. Subscriptions work through them. Loom's own work goes into what they don't have. A model loop of loom's own can still be added later as one more kind of agent, if it's ever needed, since the apps only ever see events.

**Which agents, for which models.** More than 50 agents speak ACP. Between them they cover:

| You want | Use |
|---|---|
| Claude, on your own login or an Anthropic API key | Claude, through Zed's `claude-agent-acp` |
| OpenAI's models, on a ChatGPT login or an API key | Codex, through `codex-acp` |
| Custom providers: OpenRouter, Ollama, LM Studio, DeepSeek, Groq, Mistral and more | OpenCode or Goose |
| Gemini | Gemini CLI |

- **Adding an agent.** Loom installs agents from the [ACP Registry](https://agentclientprotocol.com/get-started/registry): on the instance's machine for the owner, or in a member's sandbox. A binary release for that machine is downloaded into the instance's `acp/` folder, checked against the registry's SHA-256 and unpacked; otherwise the agent runs through `npx` or `uvx`. Installing one again updates it. An agent's settings (its model, its provider, an API key) live in loom, which passes them to the agent as environment variables when it starts it, and never sends their values to the apps. Keys are kept with the other secrets. The app's **Agent programs** page lists what's installed, with its settings, and the registry to install from.
- **Loom never logs in for an agent.** Each ACP agent uses the login it already has on that machine, set up by the person it belongs to (for Claude, their own Claude Code login). Loom has no "log in with Claude.ai" or anything like it.
- **Each person's login is their own.** The owner's agents run as `loom` on a server, or as you in the desktop app. A member's run in their [sandbox](#sandboxes-where-members-agents-run), with the member's own logins. One person's subscription is never used for anyone else.
- **Claude through ACP.** Claude's adapter is one ACP agent like the rest, and loom runs it as OpenBot does, on the person's own Claude Code login. Elliot's own instance uses it from the start. Anthropic's Agent SDK docs say: "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK." The rule is about what a product *offers*, so the public loom:
  - supports ACP agents in general, and lets people add whichever they have;
  - doesn't bundle Claude's adapter, log anyone into Claude, or advertise Claude subscriptions;
  - offers Claude through an API key in its docs.

What loom gives an agent when it starts a session:

- `cwd`: the agent's workspace folder.
- A system prompt built from the agent's instructions, in `append` mode, where the agent supports it. Where it doesn't, the instructions go in the folder as `AGENTS.md` or `CLAUDE.md`.
- `mcpServers`: `loom mcp` (below), and the person's MCP servers. The agent connects to them itself, so loom needs no MCP client of its own.
- What changes during a conversation (the time, which device you're on, a routine's context) goes in with the next prompt, never as an edit to the instructions.

What comes back:

- `session/update` notifications (message chunks, thoughts, tool calls, plans) become loom events, so every agent looks the same in the apps.
- `session/request_permission` becomes an approval in the app, and a push on the phone.
- After a restart, loom picks the session up again with `session/resume` or `session/load`, where the agent supports them.
- Models and modes the agent offers as config options show in the app.
- The usage the agent reports (tokens and cost) is counted per person.

A turn, from start to end:

1. A message comes in. The instance stores it and sends it to the agent's session with `session/prompt`, starting the agent if it isn't running.
2. The agent's updates stream out to the apps, as `delta`s and events.
3. When the agent asks permission, the instance answers by itself or asks you, as the agent's permission mode says.
4. When the turn ends, the instance stores the finished message as one event.

Each step is saved as it happens, so a restart resumes the session where it was. A turn can be stopped from the app, with `session/cancel`.

Each agent program runs as its own process. The instance reads each one on a thread of its own, through std.process.

### Permissions

- **Loom starts each agent in a mode that asks** before commands and edits, never in one that skips asking, unless the owner picks that for one agent.
- **Loom answers the agent's requests** by the agent's permission mode: `ask` (every request comes to you), `auto` (loom allows the tools that only look: reading, searching, thinking and fetching, and asks about the rest), `allow` (everything), or a list of the kinds to allow.
- **Loom's own tools follow loom's rules.** Sending, paying, deleting, letting people in, changing permissions and moving always ask.
- **Where the agent runs limits what it can do anyway:** as `loom` on a server, in a member's sandbox, or as you in a desktop instance.

### Loom's own tools

`loom mcp` serves them, as an MCP server over stdio. It's the `loom` program in MCP mode, and in the desktop app it's `loom.exe mcp`, the app itself. The instance gives it to every ACP session. It connects back to the instance with a token for that one agent, passed in its environment, and it acts with that person's rights and no more. The tools are defined once, in `shared/tools.calx`.

| Tool | What it does |
|---|---|
| `message_agent` | Talks to another of your agents |
| `remember`, `recall`, `forget`, `update_profile` | Memory (below) |
| `schedule`, `list_routines` | Make or change a routine, and list them ([Routines](#routines)) |
| `notify` | Sends you a notification |
| `edit_instructions` | Changes the agent's own instructions |
| `list_agents`, `create_agent`, `update_agent`, `list_acp_agents`, `search_messages` | Loom's commands. None of them changes a permission mode or deletes |
| `delete_agent`, `set_permission`, `delete_skill`, `delete_routine` | The risky ones. Each asks you every time, whatever the agent's permission mode, and is done as the device you answered on. An agent can't delete itself or loosen its own mode |
| `list_skills`, `read_skill`, `save_skill` | Skills: read when a task fits one, and saved by an agent for every agent. They're also written into each agent's folder as Claude Code skills |
| Device tools | Your PC and phone, where you allow it ([Machines and devices](#machines-and-devices)) |
| Every loom command | Agents, routines, plugins, settings, export, moving ([Agents can do anything the app can](#agents-can-do-anything-the-app-can)) |

- **Shell, files and web fetch** come from the agent itself.
- **MCP servers** (Acta, Cloudflare, Composio for mail and the rest) are passed to the agent, which connects to them itself.
- **A browser** comes through a browser MCP server, as OpenBot's screenshots do.

## Machines and devices

An agent can work on two kinds of machine: the one its instance runs on, and the devices you talk to it from. Both sit behind permissions, and your devices are off until you turn them on.

### The instance's own machine

- **The owner's agents** run commands and use files there: as the `loom` user on a server, or as you in the desktop app. Members can only do so inside a [sandbox](#sandboxes-where-members-agents-run).
- **What an agent may do without asking** is set by its permission mode (`ask`, `auto`, or a list). The risky actions always ask.
- **No root by default.** On a server, `loom` isn't an administrator. The owner can give it more, such as `sudo` for chosen commands or the Docker group, and the settings say plainly what each one gives away. `loom serve` refuses to run as root, or as an elevated Administrator on Windows, since every agent would get those rights too, and says how to run it otherwise. `--allow-root` serves anyway. `loom status` says which user it runs as.

### Your devices

Your PC and phone can be tools too, even when the instance runs somewhere else. You chat from your PC while the instance is on the VPS, and ask "what's taking up space on my C: drive?" or "take a screenshot of what I'm looking at".

- **Agents know where you're writing from.** Each message says which device it came from: its name, its kind (Windows PC, Mac, iPhone), its time zone, and whether you're using it right now. Agents get this as a mid-conversation system message when it changes. You can turn it off.
- **Device tools are opt-in.** Each device offers its tools to the instance over the same WebSocket. Every tool is off until you turn it on for that device. It can be off, ask each time, or allowed. Running commands starts at "ask each time", and the switch to "allowed" says what that means.
- **A hacked instance can't fake your yes.** Each device has its own key. When you approve a device tool, the device you approve it on signs the approval, and the device doing the work checks that signature before it acts. So you can approve a command on your PC from your phone, but the instance can't approve it for you. For now a device trusts only its own key, so a tool that asks is approved on the device itself. Approving on the phone for the PC comes with linking devices (Phase 5), which exchanges their keys directly, never through the instance.
- **Each device does only what it was asked.** It runs exactly the request it showed you, shows what it did, and every device action goes in the activity log.
- **Your devices are yours alone.** A member's agents can reach only that member's own devices.
- **Reachable while the app runs.** A desktop app in the tray is always reachable. The iPhone app is reachable only while it's open, since iOS stops apps in the background. The agent can send a push asking you to open it.

| Device | What it can offer |
|---|---|
| Windows PC, Mac | Running commands, files in the folders you pick, screenshots, the clipboard, opening a file or a link, notifications. Later: controlling the screen, mouse and keyboard (v0.1 had this on Windows) |
| iPhone | Location, a photo or file you pick, the camera, the clipboard, opening a link. Later (Phase 9): the calendar, reminders, contacts, Health, running a Shortcut |

Commands and files run in the core (std.process and std.fs). Screenshots (`xcap`) and the clipboard (GPUI's own) on desktop come from the GPUI app: the core checks the call as for any tool, then asks the app. Location, the camera and photos on iPhone come from Apple's frameworks in the Swift app. The core asks the app for them and passes the answers back.

In a desktop instance, the instance's machine and your PC are the same machine, so device tools are for your other devices: the phone, and other computers.

## Memory

- `remember`, `recall` and `forget` keep and search notes in SQLite with full-text search (FTS5), per person and per agent. A note can be shared by all the person's agents. `recall` finds any of its words, best match first. The person reads, adds and deletes them on the agent's **Memory** page.
- A short profile of each person (a few lines, up to 4000 bytes) starts every session, and comes again when it changes. Agents rewrite it with `update_profile` as they learn, and the person edits it on the **About you** page. Each change is a new revision, with who made it.
- No embeddings to start. They come in if full-text search isn't enough.

## Routines

- **On a schedule:** every N minutes (5 at least), daily at a time, or on weekdays at a time, in the person's own time zone, summer time included. A routine takes the time zone of the device that made it.
- **On a webhook:** `POST /hooks/<id>`, with the routine's secret as a bearer token or signing the body as GitHub's webhooks do (`X-Hub-Signature-256`). "The build failed" comes from GitHub Actions posting there, not from a timer. What was posted goes to the agent after the prompt. A routine can have a schedule, a webhook, or both. The instance has to be reachable from where the post comes from: a server is, and a desktop instance is through Tailscale Funnel or the like.
- A routine sends its agent a prompt in the agent's chat, which is its newest conversation, so it shows where the person reads that agent. The message says it's from the routine, and the agent is told so. With notify on, the reply comes as a notification. If the last run hasn't finished, that time is skipped.
- Missed while the instance was off, a routine runs once when it starts again, or waits for its next time, as the routine says (`catch_up`). Either way, missed runs never pile up.
- The person makes, changes, pauses, runs and deletes them on the **Routines** page, and agents do the same with `schedule`, `list_routines` and `delete_routine`, which asks first.

## Notifications and push

- **Desktop.** The app lives in the tray and keeps the WebSocket open, so the core shows system notifications itself. No push service needed.
- **iPhone.** APNs, with an ES256-signed token, over HTTP/2. Approvals come with Allow and Deny buttons.
- **Elliot's instance sends push to Apple itself**, with the key of the Apple developer account that signed the app.
- **Other people's instances can't do that.** Apple only accepts a push signed with that key. Elliot signs the TestFlight and App Store builds, and his key can't be handed out, since it would let anyone push to every loom user. So other instances have three choices:
  - **No push**, which is the default. The phone shows notifications while the app is open, and desktop apps get them as usual.
  - **The push relay**, if the owner turns it on (below).
  - **Their own build** of the iPhone app, with their own Apple account and key. The docs explain how.

### The push relay (opt-in)

A small Calx service at `push.<domain>` that only sends notifications. It passes no other traffic: reaching a desktop instance from outside goes through Tailscale. It's off for every instance until its owner turns it on in settings. The settings screen says exactly what the relay gets.

- **It can't read notifications.** Each one is encrypted for the phone it goes to. A small Swift notification service extension on the iPhone decrypts it before it shows, so neither the relay nor Apple sees the text.
- **It can't be used to spam phones.** The app seals its Apple device token with the relay's public key before giving it to the instance. The instance never sees the real token, and can push only to phones that joined it. Each instance signs its requests with its own key, and the relay limits how many each can send.
- **It keeps almost nothing.** No accounts, no stored tokens, no logs of content. It opens the sealed token, sends the push to Apple, and forgets it. The only thing it keeps is a counter per instance key for the rate limit. It still sees the instance's IP address while a request comes in, so it gets a short privacy policy.
- **The key is kept apart.** The relay uses an Apple push key of its own, separate from the one Elliot's instance uses, on its own small server. If it were ever stolen, Elliot revokes that key in Apple's developer portal and makes a new one, and nothing else is touched.
- **It's cheap.** Pushes are tiny and Apple charges nothing for them, so one small server carries a lot.

It needs no Calx beyond what APNs and the QR code already need (ES256, HTTP/2, X25519 and AEAD).

## Moving an instance

An instance moves whole, in any direction: desktop to server, server to desktop, server to server.

- **What moves:** everything. Accounts, devices, agents and their revisions, conversations, memory, routines, skills, plugins, ACP agent settings, secrets, MCP setups, agents' folders and usage. ACP agents' own session files come too, where the agent keeps them on disk. Where it doesn't, the conversation carries on from a summary.
- **What doesn't move:** logins that belong to a machine, such as an ACP agent's Claude Code login. The new instance lists which agents need them, and the person logs in again there. Members' sandboxes move as their folders and are built again on the new server. A desktop instance has no sandboxes, so moving there turns them off.
- **How:** the new place runs `loom receive`, which prints a one-time move code. The old instance connects with it, pauses runs and routines, and streams everything over an encrypted connection: a snapshot of the database, and the folders as a zstd tar stream. The new instance checks it all, then takes over.
- **Secrets** travel encrypted with a key from the move code, never in plain text, and are encrypted again with the new instance's key.
- **The apps follow.** The old instance sends every device a move event, signed with its key, with the new address. The apps switch over, keep their device tokens and cache, and catch up from the same event number. A device that was offline finds the old address answering "moved to …" for 30 days, or until the owner shuts it down.
- **Backups are the same export**, written to a file, by hand or on a schedule.

## Files and images

- **Both ways.** People send files and images to agents (drag and drop, paste, the share sheet on the iPhone), and agents send files back: screenshots, reports, exports.
- **Kept once.** Files are stored on the instance under their SHA-256, so the same file is never kept twice. Devices download a file when it's shown, and cache it.
- **Thumbnails** are made on the instance with std.image.
- **Photos from the iPhone** go up as JPEG, since the models agents use don't take HEIC.
- **Space.** Files count toward each person's storage, which the owner can limit.

## Search

Full-text search over every conversation, with FTS5 as memory uses, filtered by agent and date.

## Security

- **What a model reads is data, not orders.** Web pages, mail and tool results can carry instructions meant to trick an agent. Nothing in them can approve a risky action or change a permission. Only a person can, in the app.
- **Secrets stay out of prompts.** Loom never puts a secret in a prompt or a transcript, only its name. The keys an ACP agent needs (its API key, its MCP servers' keys) go to that agent's process alone, and only for its own person. The agent's own shell could still read its own environment, which is one more reason a member's agents run in a sandbox that holds only that member's keys.
- **An activity log** of everything agents do: each tool call, what it changed, and the approval that allowed it. People see their own, and the owner sees the whole instance's.
- **Logins** are rate-limited by address and by account, and a revoked device token stops working at once.
- **Plugins** run sandboxed (panels) or as their own processes (tools), with only the permissions they were given.
- **Reporting a reply.** Any reply can be reported, as the App Store asks. The report goes to the instance's owner.

## Updates and versions

- **Desktop apps** update themselves from signed releases (an Ed25519 signature on each release, checked before the swap), with loom's own small updater. The iPhone app updates through TestFlight, and later the App Store.
- **Servers** update with `loom update`, which checks the release's signature, swaps the program and restarts. The owner can let it update by itself.
- **How a release is signed.** A tag `v…` builds every program, and `scripts/release.calx` writes `manifest.json` with each file's SHA-256 and `manifest.json.sig`, the Ed25519 signature of the manifest's exact bytes, with the release key (the CI secret `LOOM_RELEASE_KEY`; its public half is in `shared/release.calx`). The desktop core and `loom update` read the latest release's manifest, check the signature and then each file's hash, and only then use it. The desktop app is installed per person (`%LOCALAPPDATA%\Programs\loom`), so it swaps its own `loom.exe` without asking for an administrator.
- **Protocol versions.** `hello` says which protocol versions the app speaks, and the instance answers with the one it will use. An instance supports its newest version and the one before, so an app a release behind still works, and the app says when it has to update.
- **The database** carries its schema version, and the instance upgrades it on start, after a backup. A move between instances on different versions goes through the same upgrade.

## Testing

- **Unit tests** in every Calx module, as `@test` functions run with `calx test`.
- **A fake ACP agent** that plays scripted turns (replies, tool calls, permission requests, crashes), so sessions, approvals and reconnects are tested without a real model or spending money.
- **Protocol tests** drive a real instance through the WebSocket, as Phase 1's test script does, and grow into the main suite.
- **End-to-end tests** run the real app against an instance with the fake ACP agent.
- **CI** builds and tests the server, the library and the app on Linux, Windows and macOS on every push.

## Storage

One SQLite database on the server, with these tables: `users`, `invites`, `link_codes`, `devices` (with each device's public key), `device_grants`, `sandboxes`, `plugins`, `plugin_revisions`, `acp_agents`, `acp_settings`, `agents`, `agent_revisions`, `skills`, `mcp_servers`, `conversations`, `acp_sessions`, `messages` (content blocks as JSON, with FTS5), `files`, `runs`, `tool_calls`, `approvals`, `activity`, `reports`, `events`, `memories` (with FTS5), `routines`, `push_tokens`, `secrets`, `usage`, `schema_version`. Files themselves sit in the data folder, named by their SHA-256.

## Protocol

JSON frames over one WebSocket, after logging in over HTTP:

```
→ {"t":"hello","token":"…","after":1041}
← {"t":"event","seq":1042,"kind":"message.created",…}                     catch-up
→ {"t":"send","cid":"m_4f2","conversation":"c_9","text":"Did the build pass?"}
← {"t":"ack","cid":"m_4f2","message":"msg_310","run":"run_77"}
← {"t":"delta","run":"run_77","text":"Checking"}                           live only, no seq
← {"t":"event","seq":1043,"kind":"approval.needed","run":"run_77","call":"tc_3","tool":"shell",…}
→ {"t":"approve","call":"tc_3","allow":true}
← {"t":"event","seq":1044,"kind":"tool.finished",…}
← {"t":"event","seq":1046,"kind":"run.finished","run":"run_77"}
```

Device tools travel the same WebSocket the other way: `{"t":"device.call",…}` from the instance, carrying the signed approval when there is one, and `{"t":"device.result",…}` back.

The protocol's types are one Calx module in `shared/`, which the server and the core both import, so the two can't drift apart. The whole protocol goes in `protocol/protocol.md` in Phase 0.

## Repo layout

```
loom/
  PLAN.md
  design/     design tokens and anything the new design needs
  protocol/   protocol.md, the wire protocol
  shared/     Calx: protocol types and the command list, used by the server and the core
  server/     Calx: the instance, and the `loom` program
  core/       Calx: connection, login, sync, cache, built with the server as a static library
  desktop/    Rust: the GPUI app for Windows and macOS
  ios/        Swift: the iPhone app, its widgets, push extension and the Xcode project
  push/       Calx: the opt-in push relay (Phase 10)
  site/       the public site and the docs
  tests/      the fake ACP agent, protocol and end-to-end tests
  scripts/    Calx: build, release, install
```

**How the Calx code fits together.** The repository's root is the source root of every Calx module, so a file's module is its path: `server/main.calx` is `module server.main`, and imports `shared.protocol` from `shared/protocol.calx`. That needs no packages, so loom has no `calx.toml` until something outside the repository is a dependency (Calx has packages now, CLX-370). Scripts in `scripts/` stand alone. `calx scripts/check.calx` checks the formatting of every `.calx` file, checks each one builds, runs every file's tests and checks the design tokens are up to date. CI runs it on Linux and Windows on every push and pull request, and macOS joins with the Mac build.

## What Calx needs

Checked against the Calx board on 2026-11-20: every card below is done, so Phases 1 and 2 wait on nothing in Calx. The last of them merged at 21:55 that day, so the 22:16 nightly is the first with all of them. Calx work runs one phase ahead: each item without a card gets one on [`aaen-studios/clx`](https://acta.aaenz.no/aaen-studios/clx) when the phase before its own starts, so it's ready in time. If it isn't, it still blocks its phase.

| Calx needs | Today | Card | Loom needs it for | Phase |
|---|---|---|---|---|
| A WebSocket server in std | **Done**: `server.upgrade` in std.http.server | CLX-322 | `/ws` | 1 |
| A WebSocket client in std, `ws://` and `wss://` | **Done**: std.http.websocket | CLX-320 | The test client (1), the core (2) | 1–2 |
| argon2id in std.crypto, with BLAKE2b in std.hash | **Done**: password `hash`, `verify_password` and `needs_rehash` in std.crypto | [CLX-384](https://acta.aaenz.no/aaen-studios/clx#CLX-384) | Passwords | 1 |
| `@export_c`, so C can call Calx, and a static library that carries the runtime | **Done**: `calx build --staticlib`, with a C header and `calx_init`/`calx_shutdown` | [CLX-334](https://acta.aaenz.no/aaen-studios/clx#CLX-334) | The core's C boundary, linked into the desktop and iPhone apps | 2 |
| Calling a C function pointer from Calx | **Done**: `extern "C" fn` values | [CLX-333](https://acta.aaenz.no/aaen-studios/clx#CLX-333) | The `on_event` callback | 2 |
| std.net event loops on two threads of one process, inside a library | **Done**: each thread polls a `Net` of its own, in a static library too | [CLX-386](https://acta.aaenz.no/aaen-studios/clx#CLX-386) | The core and a local instance's server side by side | 2 |
| Static libraries Rust's MSVC toolchain links, on Windows | **Done**: built for Visual Studio's linker and `/MD`. `/MT` hosts are [CLX-391](https://acta.aaenz.no/aaen-studios/clx#CLX-391), which loom doesn't need, since Rust uses `/MD` | (with CLX-334) | The Windows app | 2 |
| The system's credential store (std.keyring) | **Done**: Credential Manager, the Keychain and the Secret Service | [CLX-380](https://acta.aaenz.no/aaen-studios/clx#CLX-380) | Keeping the device token | 2 |
| AEAD encryption: AES-256-GCM or ChaCha20-Poly1305 | std.crypto has AES-CFB8 and RSA | — | Encrypting secrets in the database | 3 |
| Killing everything a process started (a process group on Linux and macOS, a job object on Windows) | **Done**: `Child.stop` and `Child.kill_tree` | [CLX-376](https://acta.aaenz.no/aaen-studios/clx#CLX-376) | Stopping a runaway agent and all it started, on a server and on a PC | 3 |
| Ed25519 signing in std.crypto | **Done** | [CLX-379](https://acta.aaenz.no/aaen-studios/clx#CLX-379) | Device keys and signed approvals for device tools, the instance's key, signed releases | 3 |
| Named time zones in std.time | **Done**: `TimeZone`, with the IANA tz database built in | [CLX-404](https://acta.aaenz.no/aaen-studios/clx#CLX-404) | Routines daily or on weekdays at a time, in each person's zone | 4 |
| An iOS target, for devices and the simulator, as a static library Xcode links | Being built, on the Mac | [CLX-408](https://acta.aaenz.no/aaen-studios/clx#CLX-408) | The iPhone app. Started early, during Phases 3–4 (below) | 5 |
| Cross-compiling: building for another system or processor | Not documented; builds are for the machine they run on | [CLX-409](https://acta.aaenz.no/aaen-studios/clx#CLX-409) | iOS builds from a Mac, Linux arm64 and other release builds | 5 |
| std.net and std.http on iOS: kqueue, and URLSession since iOS has no libcurl | WinHTTP on Windows, libcurl on Linux and macOS | [CLX-410](https://acta.aaenz.no/aaen-studios/clx#CLX-410) | The core on iPhone | 5 |
| ES256 (ECDSA P-256) signing | Being built | [CLX-406](https://acta.aaenz.no/aaen-studios/clx#CLX-406) | APNs tokens | 5 |
| HTTP/2 in std.http | **Done** | [CLX-383](https://acta.aaenz.no/aaen-studios/clx#CLX-383) | APNs, which speaks only HTTP/2 | 5 |
| A library's panic reports to the host instead of ending the app | Being built | [CLX-407](https://acta.aaenz.no/aaen-studios/clx#CLX-407) | A bug in the core shouldn't close the app | 5 |
| A static library for macOS that links with Rust | Not checked yet | — | The Mac app | 6 |
| X25519 in std.crypto, and a Noise handshake built on it, Ed25519 and AEAD | None | [CLX-411](https://acta.aaenz.no/aaen-studios/clx#CLX-411) | End-to-end encryption to a desktop instance, and the push relay's sealed tokens | 6 |
| UDP sockets with multicast in std.net, and mDNS | No UDP in std.net | [CLX-412](https://acta.aaenz.no/aaen-studios/clx#CLX-412) | Finding a desktop instance on the Wi-Fi when its address changes | 6 |
| Choosing the CPU a release build targets, such as x86-64-v2 | **Done**: the `--cpu` level | [CLX-381](https://acta.aaenz.no/aaen-studios/clx#CLX-381) | Installing loom on any VPS | 7 |
| Writing and reading tar as a stream, to and from a file or a connection | std.tar works in memory; being built | [CLX-413](https://acta.aaenz.no/aaen-studios/clx#CLX-413) | Exports and moves of many gigabytes | 7 |

Also done: std.http reports a server that doesn't speak HTTP/1.1 or HTTP/2 as a protocol error rather than `BadUrl` ([CLX-385](https://acta.aaenz.no/aaen-studios/clx#CLX-385)), `Child.read_line(0)` takes output already in the pipe ([CLX-358](https://acta.aaenz.no/aaen-studios/clx#CLX-358)), packages with `calx.toml` ([CLX-370](https://acta.aaenz.no/aaen-studios/clx#CLX-370)), and handling SIGTERM, Ctrl+C and a service stop ([CLX-377](https://acta.aaenz.no/aaen-studios/clx#CLX-377)), for `loom serve` to stop cleanly.

Already there: WebSockets both ways, FTS5 in std.sqlite's build (SQLite 3.54.1), `calx test` for the tests, std.image for thumbnails. For ACP, std.process can start an agent, write to it, and read its output a line at a time with a timeout (`read_line`), which is enough with a thread per agent.

Not needed: a TLS server in Calx, since Caddy or Cloudflare handles TLS in front of the server. And since agents run on ACP, loom makes no model calls and needs no MCP client, so std.http needs neither streaming bodies nor more methods than GET and POST.

## Phases

| # | Phase | Contents | Calx first | Done when |
|---|---|---|---|---|
| 0 | Design and start | Retire v0.1 (tag it, clear the tree). The new design from scratch, desktop first (the phone's comes with Phase 5). MIT license, README, PLAN.md, protocol.md, repo layout, Acta board | — | A mock of the main desktop screens in light and dark, and protocol.md shows one message's whole trip, including a reconnect |
| 1 | Server MVP | The owner account, a second account from the command line, login, devices, one agent on Claude through ACP on your own login, the `loom` program, SQLite with data kept per person, the event log, the WebSocket, protocol versions, the fake ACP agent, tests and CI | — (argon2id and WebSockets are done) | A Calx test script logs in, chats with Claude, drops the connection, reconnects and catches up, and the second account sees none of it |
| 2 | Windows app | First, a throwaway test: a tiny Calx static library linked into a GPUI window on Windows, streaming a Markdown reply through `on_event`. Then the GPUI app with the core linked in: chat list, streaming replies with Markdown and code blocks, local cache, outbox, tray, notifications, starting at login, files and images, search, the updater. An instance in the app, for no server at all | — (all done) | You use it instead of the test script, it survives sleep and Wi-Fi changes, and it works with no server |
| 3 | Agents and tools | Agents and revisions in the app, installing ACP agents from the registry, their settings and keys, permissions and approvals, `loom mcp` with loom's tools for every command, MCP servers, a browser through MCP, skills, `message_agent`, the activity log, the `loom` user, which device you're writing from, your PC as a device tool with signed approvals | AEAD (process trees and Ed25519 are done) | An agent does a real task, sets up another agent and a routine by itself, and asks before a risky step |
| 4 | Memory and routines | Memory with full-text search, profiles, routines on a schedule and on webhooks, notifications | Time zones | A failing GitHub build shows up on your PC, and routines run every 30 minutes and daily |
| 5 | iPhone | The Swift app on TestFlight, with the phone's own smaller set of screens, connecting to an instance on a server, push through APNs, adding the phone by QR code from the desktop app, the iPhone's device tools. **OpenBot is switched off** | iOS target, cross-compiling, std.net and std.http on iOS, ES256, library panics (HTTP/2 is done) | Loom on your phone with push on the lock screen, and OpenBot off |
| 6 | Mac, and phones on desktop instances | The GPUI app on macOS, with an instance in it too. The phone reaching a desktop instance: on the Wi-Fi with mDNS, and from anywhere through Tailscale, encrypted with Noise | macOS static library, X25519 with a Noise handshake, UDP and mDNS | The Mac app works, and a phone reaches an instance on a PC both at home and away |
| 7 | Moving and plugins | The panel break-out test first, then export, import and backups, moving an instance with the apps following, the install script, `loom update`, `loom` on the command line with JSON output, panels, tool plugins, agents writing plugins | Streaming tar (the CPU choice is done) | An agent installs loom on a fresh VPS and moves your instance there, and your apps follow without you touching them |
| 8 | Your own integrations | T3 Code threads, Acta, the VPS dashboard and deploys, built in | As they come up | It does things OpenBot can't |
| 9 | iPhone extras | Widgets, Live Activities in the Dynamic Island, Action Button, Siri and Shortcuts, Control Center. The calendar, reminders, contacts, Health and Shortcuts as device tools | As they come up | Whenever you want, one by one |
| 10 | For others | Inviting and managing people (invites, roles, budgets, member sandboxes), release builds for every platform, Docker image, docs, setting up agents and keys, a guide to Tailscale, the opt-in push relay with encrypted notifications, a guide to building the iPhone app with your own Apple account | — | Someone else runs their own instance, on a server or on their PC, and invites someone to it |

### How the work runs

- **Calx runs one phase ahead.** When a phase starts, the Calx cards for the next one start too, so loom rarely waits.
- **The riskiest parts get tested first.** Everything rests on a Calx static library working inside a GPUI app, so Phase 2 opens with a throwaway test of exactly that on Windows. It also shows early whether GPUI is ready for this on Windows. If it isn't, the fallback is Tauri for desktop, with the same core. The iOS target is the biggest unknown in the plan, so its Calx work starts during Phases 3–4, before Phase 5 needs it.
- **The shortest path to daily use.** Claude through ACP from Phase 1, the iPhone in Phase 5, and the screens for other people last.

### Leaving OpenBot

Agents start moving over in Phase 4, once routines and MCP work. OpenBot is switched off at the end of Phase 5, once loom is on the phone. By then loom covers:

- Agents on Claude through ACP, on Elliot's own Claude Code login (see the subscription rule under [Agents run on ACP](#agents-run-on-acp)).
- Several agents, each with their own instructions, folder and skills, which edit their own instructions.
- A main agent that hands work to the others, and coding work to T3 Code through its CLI.
- Routines every 30 minutes and daily at a set time, which post into a chat and notify.
- Mail across several inboxes, through Composio's MCP server.
- Agents that work in git repos on the VPS and run Calx tools there.
- A browser, with screenshots.
- Asking before risky actions: sending, paying, deleting.

## iPhone: getting it onto your phone

- **For yourself, no App Store needed.** With Apple's developer account (about $99, ~1,050 kr a year) you can install from Xcode (it lasts a year), or use TestFlight internal testing (up to 100 people, no review, builds last 90 days). TestFlight is easiest.
- **On the App Store later.** Self-hosted is a well-known category (Home Assistant, Jellyfin, Nextcloud). Apple wants a demo server and account for review, a privacy policy, account deletion in the app, and a way to report AI responses.
- **The Xcode project** lives in `ios/` and is committed to git. Prefer a generated project (XcodeGen or Tuist) over a hand-edited `.xcodeproj`, so merges stay readable.

## Desktop on GPUI: what to know

GPUI is Zed's UI framework. It makes nothing in this plan impossible, but it's young, and some things are loom's own work:

- **Which GPUI.** The `gpui` crate on crates.io stopped at 0.2.2. Zed's main branch is published as `gpui-pre`, and **GPUI Kit** (`gpui-kit`) pins a `gpui-pre` and re-exports it with `gpui-base` and `gpui-component`. Loom depends on `gpui-kit` alone, pinned exactly, and takes each new version on purpose. It needs a recent stable Rust (1.100 when the spike ran).
- **The widgets mostly exist.** `gpui-component` has Markdown with code blocks (`TextView::markdown`, selectable), a virtual chat list that follows the tail (`MessageScroller`), and a composer (`Textarea`, growing, Enter to send and Shift+Enter for a new line). Loom styles them and builds what's missing.
- **What a web app shell gave for free is now small crates:** the tray (`tray-icon`), notifications (`notify-rust`), starting at login, screenshots, the clipboard, the updater. `tray-icon` pulls in GTK on Linux, which loom doesn't target.
- **Screen readers.** GPUI now has AccessKit on Windows, and `gpui-component`'s widgets set roles and labels. Check each screen with Narrator and VoiceOver as it's built.
- **Licences.** GPUI, `gpui-component` and GPUI Kit are Apache-2.0, which fits loom's MIT. Zed's editor and terminal crates are GPL, so they can't be used. A terminal panel would use `alacritty_terminal` (Apache-2.0).

### What the Windows spike showed (LOO-4)

The throwaway test that opens Phase 2: a stand-in core (since replaced by `core/lib.calx`) built with `calx build --staticlib` by `desktop/build.rs`, linked into a GPUI Kit window, streaming a Markdown reply through `on_event` from the core's own thread, below 3000 older Markdown messages.

- **It works.** The core's 71 deltas arrive over 1.1 s, and GPUI draws each one as it comes, with code blocks, lists and inline code.
- **Release build:** the window is up in about 0.55 s, the process holds about 85 MB, and the exe is 26 MB.
- **The composer** takes typing and Enter, sends to the core, and the list follows the new message. Dropping files gives their paths (`ExternalPaths`). The tray icon and a notification come from the crates above, on GPUI's own message loop.
- **Not yet checked by hand:** an IME (Japanese, say), and Narrator. Both are on the Phase 2 desktop chat task.
- **Geist isn't bundled yet,** so text falls back to a system font.
- **For the real core:** `loom_core_stop` must join the core's threads before it returns, so no `on_event` call comes after the app frees its side.

GPUI is ready for this on Windows, so the Tauri fallback isn't needed.

## What happens to v0.1

Nothing from v0.1's code carries over: the engine was Rust with its own model loop, and the UI was React. It stays reachable at a tag (`v0.1`) for reference. Some of what it had may come back later in the new stack, each decided on its own:

- The real terminal, the editor and the git panel, as panels or as their own features.
- Computer use on Windows, as a device tool (see [Your devices](#your-devices)).
- Image generation, as an MCP tool rather than a provider feature.
- Semantic search over a coding workspace, if full-text search isn't enough.

## Later ideas

- Group conversations, where several people and agents share one chat.
- A model loop of loom's own, as one more kind of agent, if ACP agents ever stop being enough.
- Agents controlling a PC's screen, mouse and keyboard, as a device tool.
- A web client: the core built for `wasm32`, running in a browser tab with nothing to install. It would need a UI of its own.
- Android: a Kotlin app on the same core, once Calx has an Android target.

## Open questions

- **The word for an instance.** "Instance" is a placeholder.
- **The domain**, for the site, invites and the push relay.
- **The panel break-out test** at the start of Phase 7 ([The panel sandbox](#the-panel-sandbox)).
- **The Calx checks** marked "Not documented" in [What Calx needs](#what-calx-needs).
