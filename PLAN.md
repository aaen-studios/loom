# Loom: the plan

A self-hostable agent app. You chat with your own AI agents from your PC and phone, and they do real work on your server, or on your PC if you don't want a server. It's built for how Elliot works, and open for anyone to run.

This is a rewrite. Loom v0.1 (a Rust engine with its own model loop, a React UI, desktop only) is replaced from the ground up. Its design is not carried over either: the new design starts from a blank page.

- **A passion project, not a business.** MIT license, no CLA, open for anyone to use and contribute to. The name and logo aren't covered by the MIT license, so forks use their own.
- **A real test project for Calx.** The server and the app core are Calx. When Calx is missing something, it gets added to Calx.
- **Repo:** `aaen-studios/loom`. On the VPS as a T3 project.
- **Board:** Acta, [`aaen-studios/loom`](https://acta.aaenz.no/aaen-studios/loom).
- **Protocol:** [protocol/protocol.md](protocol/protocol.md).
- **Site:** [loom.rip](https://loom.rip), from [`site/`](site/README.md).

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
- **Settings look like settings**, as the system settings of macOS, iOS and Windows do: a page's title with its one action beside it, then groups, each a box of rows with what a row is on the left and its control on the right (a switch, a choice of a few, a button), and nothing else. No line under a title or above a group that only says again or explains what it names, and no internal names (ids, keys, environment variables) beside what people see, unless it's what they type; a format goes in the field's placeholder. A line under a row is for its value or state, or the one short thing someone would get wrong without it, such as that removing something deletes what's in it. A group's heading is a name ("Model and effort"), not a sentence. A program shows by the name people know it by, the ACP Registry's ("Claude Agent", not "Claude-acp"), which `acp.list` gives as its `title`. A page with nothing on it yet says so, with the way to make the first. Corners are boxy but soft, from the tokens.
- **The person's settings are grouped**, down the popup's left under a search that finds a page by its name or what's on it: **You** (About you, Appearance, Chat, App & notifications), **Agents** (Defaults, Rules, Brains, Skills, Proposals, MCP servers, Plugins), **Work** (Routines, Tasks), and Import. Appearance, Chat and App & notifications belong to the device, kept in its `prefs.json`, since each device is used its own way: the theme (the system's, light or dark), how big the chat's words are, and the fonts (loom's Geist and Geist Mono, or the system's); whether an agent's replies are grey bubbles (the default) or plain text in the column, while the person's messages are bubbles in the agent's colour: with Vibrant bubbles, the default, the colour itself, light in the light theme and deep in the dark, or else a breath of it; how much of an agent's work a reply shows (its tool calls hidden, failed ones too since the agent says when a failure matters, which is the default, folded into one line for each run, or every call; its thoughts hidden, which is the default, folded or open) and whether Ctrl+Enter sends, and whether the sidebar shows search (hidden by default, and Ctrl+F searches either way); and starting at login, staying in the tray when closed, keeping the computer awake while an agent works, notifications with or without what was said and a sound, and updates, checked now or put in on their own while loom is in the tray and no agent works.
- <a id="one-answer-at-the-end"></a>**It's a chat: one answer at the end.** While an agent's turn runs, what it says between its tool calls doesn't pile up as bubbles: one quiet line shows the step it's on (the first line of what it last said, the tool it's calling, or that it's thinking), and opens to show the steps so far. When the turn ends, its last words are the reply, wherever in the turn they came, and everything before them (what it said on the way, its thoughts and tool calls, as the person's prefs show them) folds under one quiet line, "3 steps", that opens with a click, in the order it came; what it said on the way shows there smaller and quieter than the answer, never as a bubble, so it never reads as a message, the person's or the agent's. While the turn runs, its tool calls sit among its words where they came, so what it writes after a call comes after it. A subagent's tool calls and words sit inside the call that started it, folded under that call's title with how many steps it took, and open with a click; they are never the reply's words. So most turns end as one answer. Cards, files, plans, notices and words that ask the person something (ending in a question mark) stay in view where they came, and questions and approvals are their own rows. A turn that was stopped, failed or ran out keeps everything it had in view, as its message's `status` says, so a reloaded chat reads the same. A tool call never says it's still running once its turn is over: one its agent program never finished ends with the turn, or when a restart cut the turn short, as the instance starts again, and shows a quiet "stopped" (or "denied", for one a rule or the person said no to). The desktop app's model decides what is a step and what is the answer (`model::parts`), as the iPhone app's does (`parts` in `Model.swift`), and their screens only draw it.
- **Bubbles join**, as a messaging app's do: bubbles from the same sender one after another, an agent's in one reply or the person's messages in a row, sit close, and the corners on the sender's side are smaller where one meets the next (`radius.bubble-joined`): every bubble but the last at the bottom, every bubble but the first at the top. An agent's first is small at its top left anyway, by its thread, as a speech bubble's tail.
- **Messages sit close, and the time shows where the chat paused.** A line with when it went on ("Today 14:05") crosses the chat over its first message, after a pause of half an hour or more, and on a new day. The agent's name shows over the first reply after each line, not over the rest. Hovering a message shows its time beside it. Bubbles don't join across a line.
- **Right-click works where it would in any app.** An agent in the sidebar opens the agent menu at the pointer; a message has Copy, Pin and Branch from here; a server has its settings, Rename and Copy address; and the space under the agents has New agent.
- **The window is loom's own** to its edges: its minimise, maximise and close are loom's, drawn in its style, and act as the system's do (on Windows they're the system's hit areas, so Snap layouts work). macOS keeps its traffic lights.
- **The logo is the knot with the wordmark beside it.** On the first screen and the opening, the agents' threads weave through it, over and under.
- **Scrolling is smooth.** A mouse wheel's notch, which the system gives in lines, is caught before anything scrolls and played out in pixels over the next frames, easing out, as a browser does, everywhere in the window. Trackpads and touch, already in pixels, pass as they are.

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

The core is the same Calx code on every platform, and only JSON crosses into Rust or Swift, but for a picture of an agent's browser, which crosses as the instance sent it: its JSON head, a newline and the JPEG ([protocol.md](protocol/protocol.md#browserframe)):

```c
typedef void (*loom_event_fn)(const uint8_t *json, size_t len, uint8_t *ctx);

uint8_t *loom_core_start(const uint8_t *config_json, size_t len, loom_event_fn on_event, uint8_t *ctx);
void      loom_core_send(uint8_t *core, const uint8_t *json, size_t len);
void      loom_core_stop(uint8_t *core);
bool      loom_push_open(const uint8_t *keyring, size_t klen, const uint8_t *sealed, size_t slen, loom_event_fn on_text, uint8_t *ctx);
```

- `loom_push_open` is the one call that needs no core. The iPhone's notification service extension uses it to open a notification that came [through the push relay](#the-push-relay-opt-in).
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

People who don't want a server run their instance in the desktop app. On first start the app offers to begin, which runs one here, or to sign in to an instance on a server. Then it makes the first agent with the person: a brain (the agent program it runs on), a name (or one from the die) and a colour.

- **The same server code, in the same library.** The server is compiled into the app's static library beside the core, since a process can hold only one Calx runtime. With `{"mode":"local"}` in its config, `loom_core_start` also starts the server on its own thread, without holding up the app's window, listening on 127.0.0.1 on the port it had last time (kept in its data, as `port`, so the person's scripts find it in the same place), or a free one if that's taken, with a token made for that launch. The core then connects to it as it would to any instance. One protocol and one code path, so whatever works on a server works here.
- **Data** goes in the app's data folder (`%APPDATA%\Loom`, `~/Library/Application Support/Loom`), and the agents' folders go there or wherever you choose.
- **Just you, no password.** Your account on the computer is the lock. A desktop instance is for its owner and their own devices. To share it with other people, move it to a server.
- **Agents work on your computer.** They're the ACP agents installed there, with your own logins, and their commands run in PowerShell on Windows and zsh on macOS.
- **The `loom` program comes along.** The app installs the same `loom` program a server runs, beside itself. ACP agents need it for `loom mcp`, and it gives the desktop instance the same command line as a server.
- **It keeps running.** The installed app starts when you log in (its first start turns that on; a dev or test build never does) and lives in the tray, so routines run while the window is closed. On a Mac that's the menu bar, and starting at login is a LaunchAgent, which only a loom in Applications turns on. There's one loom to a person: starting it again while it runs, in the tray or after an update, brings that loom's window forward instead of starting another. Quitting from the tray stops the instance. Routines missed while it was off run when it starts again, or are skipped, as each routine says.
- **Your phone** connects to it by scanning a QR code in the desktop app ([below](#connecting-devices)). An iPhone can't run an instance itself, since iOS stops apps in the background.
- **Moving.** A desktop instance moves to a server, or back, whenever you like ([Moving an instance](#moving-an-instance)).

### Several servers in one app

One app signs in to as many instances as you like: the one on this computer, a server at home, one at work.

- **A core for each.** Each server has a core of its own in the same Calx runtime, with its own data folder (the first in the app's, each other one under `servers/`), cache, cursor and token. A core locks its folder while it runs, so a second loom started on the same folder doesn't write the same cache: it reads from the instance alone and says so. The app keeps which are signed in, in `server-list`, and only the first runs the instance on this computer or updates the app.
- **Each is its ball of yarn.** The sidebar's bottom row has a soft ball of yarn for each server, as each agent has its thread: its colour and its strands chosen from the server's name, resting while it's connected, its strands turning while it connects, and grey while it can't be reached. The one on screen has its name (its address, or a name the person gave it on this device) and how it is beside it, so it reads as the server you're on, and opens the server popup. The settings gear sits beside it. The sidebar collapses to a rail (Ctrl+B), which the app keeps: at its top the control that opens the sidebar, then each agent as its thread, in its shape, a dot on one with a reply waiting and its name beside it under the pointer, and at its foot the update icon over the gear over the yarn. A window too narrow for the sidebar and the chat makes it a rail on its own until there's room again, unless the person collapsed it. Another comes on screen when clicked, and has a dot when one of its agents said something there.
- **The others keep going.** A server away keeps its connection, so its replies notify and its unread marks gather. This PC's tools (screenshots, the clipboard, commands) answer only the server on screen.
- **Adding and removing** are in the server popup: **Add a server** signs in beside the one on screen, with a setup code for a new server, and the server comes on screen once it's in. **Sign out and remove** takes one away, with its folder. The first server's core stays, since it updates the app, but with other servers in the app, signing out of it (or **Stop and remove** for the instance on this computer) takes it out of the sidebar and puts another on screen. The instance's agents and chats stay on disk, and **Add a server** offers **Run here** to bring it back.
- **It opens where you were.** The app keeps which server was on screen (`last-server`) and starts on it.
- **Agents help.** An agent's `add_server` asks its person, and the device that said yes opens Add a server with the address in. With a link code the server gave (`loom install --json`, `loom link`, or an owner's or admin's `link.create` for someone), that yes is all: the app signs in with the code, which on a new server makes the person its owner, so they type nothing and the agent never sees a password or a token. A code works once, for 15 minutes ([protocol.md: Link codes](protocol/protocol.md#link-codes)). The import that follows, `loom import` on the server, makes itself a token that only imports, for 15 minutes.
- **Installing on a host, on one yes.** `install_server` with one of the person's [hosts](#your-hosts) does the whole thing. The plan comes first, as a card in the agent's reply ("Install loom on vps, through Tailscale, import from OpenBot, link it to Desk PC"), then one question, which the PC that reaches the host signs, with every command it'll run there shown. That PC makes those commands itself from the plan it signed (`shared/hosts.calx`), so a hacked instance can't slip one in, and runs them over ssh one after another (`core/install.calx`): the install script with `loom install --json --https-port auto` (or `--domain` for Caddy), then `loom import openbot --from-user` when there's something to import, with OpenBot's provider for Claude carried over unseen (`--carry-provider`). The link code `loom install` gives stays on that PC: its core hands it to its own app, which signs in there and makes the person the owner, and the import comes into their account then. Last, it asks the new server's `/api/version` until it says it's set up. Each step ticks its line on the card as it goes. A step that fails says plainly what failed and what to do next, and nothing after it runs, but for a failed import, which leaves loom working and can be run again. The agent hears what was done, never a code or a password ([protocol.md: Installing loom on a host](protocol/protocol.md#installing-loom-on-a-host)). Without a host, `install_server` gives the command to run by hand.
- **In the server popup.** Add a server and Move sit at the bottom of the server popup, under its settings, since they're things to do rather than settings, and each has **Copy prompt**: a prompt to give one of the person's agents, which does the technical part with these tools. Technical jobs in the app offer one the same way.

## Sync: an event log with a cursor

Every change the server makes for a person is an event with a sequence number, counted per person and stored in `events`. The app remembers the last number it has. When it connects it sends that number, gets everything after it, and then gets live events as they happen.

- One design covers live updates, reconnecting, the offline cache and several devices at once.
- Streamed text (`delta`) goes out live, with no number. The finished message is stored as one event. A device that connects in the middle of a reply gets the reply so far, then the rest live.
- Whether an agent is working comes from the runs still going. Catching up ends with the list of them, so a device whose cache missed a run's start or end shows it as it is. An event the cache can't write starts the connection again, rather than letting the next one move the cursor past it.
- Old events are pruned after a while. A device too far behind gets a fresh snapshot instead. It carries the newest messages of the newest conversations that fit well within a phone's 1 MiB frame, and says which conversations it left messages of out, which the app fetches when it opens one.

## Accounts and invites

An instance has accounts, and the owner decides who's in. (Only an instance on a server has other people. A desktop instance is just its owner, as above.)

The data is kept per person from the first day, so a second person never needs a migration. The screens for inviting and managing people come later, in Phase 10, when other people start using loom. Until then, accounts are made from the command line.

- **Setup.** On first start the server prints a one-time setup link. Whoever opens it creates the owner account. Or an agent sets it up for its person: the setup code (or `loom link` on the server) gives a one-time link code, the agent's `add_server` hands it to the app, and the person's yes there makes them the owner, with their username and no password, and signs the app in. They set a password when they want one, to sign in somewhere with it; phones join by QR code without one ([Several servers in one app](#several-servers-in-one-app)).
- **Roles.** Owner, admin and member. Owners and admins invite and manage people.
- **Invites.** An invite is a link (`https://<instance>/join/<code>`) that opens the app. It's good for one use or several, it expires, and it says which role it gives. The person picks a username and password, and they're in. Open sign-up stays off unless the owner turns it on.
- **Login.** Username and password, hashed with argon2id. Each login makes a device token (32 random bytes, kept on the server as a SHA-256 hash), so the password is used only to log in. Two-factor with TOTP comes later, and Calx already has the HMAC it needs.
- **Devices.** Each person sees their devices, and can rename them, revoke any of them, or sign out everywhere.
- **Forgotten passwords.** An owner or admin makes a reset link. The owner's own reset comes from the server's command line.
- **Deleting.** People delete their own account in the app, which removes their data (the App Store requires this). The owner can remove people.
- **Taking your data.** Each person can export their own conversations, agents and files, apart from moving the whole instance.
- **The people log.** Owners and admins see what happened to people, on the People page: who joined and with whose invite, invites made and taken back, role changes, removals, accounts deleted, reset links made and used, and accounts and passwords set from the server's command line. Each entry says who did it, by name, so it outlives the people in it.

What's private and what's shared:

| Private to each person | Shared across the instance |
|---|---|
| Conversations, memory, routines, their own agents, their own secrets (their Acta key, say) | Agents the owner or an admin shares, MCP servers the owner sets up for everyone, skills shared to the instance |

What the owner controls for each member:

- **Agents and keys.** Which ACP agents they can use, and whether those can use API keys the owner shares, on each program's page under Brains. Each change goes in the people log. Shared keys come with a token budget per day or month, counted from the usage the agents report, and loom stops a session that goes over. Members can always bring their own keys and logins.
- **MCP servers** the owner sets up for everyone, and which members get them.

A member's agents always run in that member's sandbox (below).

Secrets (MCP keys, tokens) are encrypted in the database with a key only the server holds. The key is `secret.key` in the data folder, made on the first start and readable only by the instance's user, never in the database. Each secret is sealed with XChaCha20-Poly1305 and a random nonce, bound to its person and name. So are the other things only the instance should read: an ACP program's settings, its API keys among them, bound to the program, and a routine's webhook secret, bound to the routine. Settings kept before sealing came are sealed on the next start.

### Sandboxes: where members' agents run

Every ACP agent runs commands and reads and writes files. On a server, all of the instance's data belongs to the `loom` user: the database, everyone's files, the key that opens the secrets. A member's agent running as `loom` could read all of it, so it would in effect be the owner.

- **The owner's agents** run as `loom` on a server, or as the owner in the desktop app. They can do anything, unless the owner marks one Isolated.
- **A member's agents** always run in that member's sandbox: a container of their own (rootless Podman, or Docker). It has:
  - their own folder, and nothing else of the instance's;
  - limits on CPU, memory and disk;
  - the internet, but not the server's own local services, such as the instance itself or anything else listening on 127.0.0.1.

  The member's ACP agents and their logins live in the sandbox, and they can install what they like there. Loom's own tools reach in through `loom mcp`, with only that member's rights.
- **Any agent can be isolated**, the owner's too. It's a setting of the agent's: an isolated agent of the owner's runs in a sandbox of the owner's, as a member's does, on the owner's own program settings, and its folder is in the sandbox's.
- **And held to a network.** An agent in a sandbox reaches everything, a list of hosts (`github.com`, or `*.npmjs.org` for every name under one), or nothing. Its sandbox is then one of its own with no network at all, and the only way out is loom's: loom's tools, and a proxy that lets out only the hosts on the list, and never the server itself or a network of its own. Its browser goes through the same proxy. A new list holds from its next connection.
- **An instance with members needs Podman or Docker** on its server. The install script sets it up. So does isolating an agent, and a network of its own. Where they can't be kept, as on a PC with no sandboxes, or on Windows, where a sandbox can't see the instance's folders at the same paths, loom refuses to isolate an agent rather than pretend.

How it's built ([protocol.md, "Sandboxes"](protocol/protocol.md#sandboxes)): `loom serve --sandboxes podman` gives everyone but the owner a rootless Podman container, made from loom's own image, which has Node, git and `loom`. The person's folder and the instance's downloaded agent programs are mounted at the same paths as outside, so paths mean the same in both. Pasta gives it the internet and one port of the server's loopback, the gate, which passes only `loom mcp`'s two routes on to the instance. The person's own secrets are their agent programs' settings, since the program's own settings are the owner's keys. The owner can share those too, with everyone or only some people, with a budget of tokens a day or a month for each person (`acp.share`), and keep a program itself to some people (`acp.members`). The budget is counted from the usage each turn's program reports, which ACP gives only once a turn ends, so the turn that crosses it finishes and the next waits. A person past it waits for the next day or month, or adds their own key. Every file the instance writes into the person's folder, or reads from a link their agent sends, goes through the sandbox, so a link the agent makes there can't lead the instance to its own files. An agent's folder, instructions and skills go in together as a tar stream through one exec, which tar in the sandbox takes apart. `loom install` sets this up for the `loom` user: Podman, pasta and newuidmap, a range of subordinate ids, and lingering, so the service has a user session to run containers in. Without `--sandboxes`, only the owner's agents run, as in the Docker image.

- **`--sandboxes docker`** does the same with Docker, for a server that has it. Docker has no pasta, so its sandboxes sit on a network of loom's own, `loom`, and the gate listens at that network's gateway. They run as the instance's own user rather than root, with no capabilities.
- **Disk.** Each sandbox's own layer gets `--storage-opt size=10g` where the storage allows it, which takes overlay on XFS with project quotas and so isn't the case on most servers. The person's folder is a mount that no storage option covers. So loom measures it in the sandbox before each run, and a run doesn't start while it holds more than 10 GB.
- **A network of its own** is kept by the container, not by the agent: an agent with a list, or with nothing, gets a sandbox of its own, `loom-<user>-<agent>`, made with `--network none`, which has only its own loopback. There `loom egress` listens on 127.0.0.1:3128 and carries each connection over its standard input and output, which `exec` joins to the instance. The instance's end passes a request for loom's tools on to the gate, and takes the rest as an HTTP proxy's: CONNECT to a host on the list, or a plain `http://` page there, checked again once its address is known. HTTP_PROXY and HTTPS_PROXY in the sandbox name it, so the programs that honour them reach the list, and the rest reach nothing. Both runtimes keep `--network none` the same way, so it holds with Podman and Docker alike.

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
- **Only once phones are on.** The instance in the app listens on 127.0.0.1 alone until the person turns phones on. Then a bridge listens on the network too, on a port it keeps, and takes nothing but the encrypted socket, which carries the phone's HTTP requests as well as its session ([protocol.md: Over Noise](protocol/protocol.md#over-noise)).

## Agents

Agents live in the database, and are made and edited in the app.

- **What an agent has:** a name and a look, the ACP agent it runs on with that agent's model and mode ([Agents run on ACP](#agents-run-on-acp)), instructions (Markdown, like an `AGENTS.md`), its permission mode, its MCP servers, its skills, and a workspace folder. MCP servers are set up once, on the app's **MCP servers** page, and each agent picks the ones it gets on its edit page. A setting can name one of the person's secrets (`ACTA_KEY=secret:ACTA_KEY`), which is filled in only for the agent's process.
- **Its settings, grouped**, in the panel beside the chat, without a title for the agent: **Brain** (its name, model and effort, its permission mode and what a message while it answers does, each with **Default** to follow the person's, colour, and its instructions, which Save keeps with its name), **Knows** (its memories, the skills, its tables and its files with their size, each counted and opening where it's kept, then its MCP servers) and **Does** (its routines, and the tokens it used today and this month). `agent.summary` gives the counts.
- <a id="thread-shapes"></a>**Its shape.** In the sidebar, folded or open, an agent's thread takes a shape, so agents tell apart at a glance: the plain thread, which every agent starts with, one of the eight that come with loom (Loop, Coil, Bow, Heart, Star, Knot, Snail and Yarn, each one strand of thread), or one the person draws in one stroke, on the agent's settings beside its colour. The instance smooths a drawing and spaces it evenly, as the eight are (`shared/shape.calx`, which writes those into the design tokens), and keeps it with the agent, so every device draws the same. The shape moves with what the agent is doing, as the plain thread does: each mood bends it along its own line, a little less than a straight thread. Everywhere else, in the chat and its header, the thread stays straight.
- **Revisions.** Each change to an agent is saved as a revision, and each run records the revision it used. You can see which instructions gave an answer, and roll back.
- **Agents edit themselves.** An agent can change its own instructions with a tool, as OpenBot's agents (and v0.1's Atelier mode) do. The change shows up as a revision by that agent.
- **Skills.** Named Markdown bundles with files, loaded only when an agent needs them. Personal or shared.
- **Agents talk to each other.** `message_agent` starts or continues a conversation with another of your agents, so a main agent can hand work to the others. A message from an agent never counts as your approval for a risky action.
- **As files.** An agent can be exported and imported as a folder (`agent.toml`, `instructions.md`, `skills/`), for backups, git, and moving between instances. OpenBot's and Grok Bot's agents come over with everything around them ([Importing from OpenBot and Grok Bot](#importing-from-openbot-and-grok-bot)).

### Agent defaults

- **Live, and an agent can override.** The person's **Defaults** page, under Agents in the settings, sets what every agent of theirs follows until its own settings change that field, so changing a default changes every agent that hasn't. They're kept on the instance per person, so the phone has them too.
- **What they cover:** the brain (agent program) a new agent runs on; the backup program that takes a turn over when an agent's own runs out of usage or doesn't start ([A backup brain](#a-backup-brain)); the model and reasoning effort for each brain's agents, used where the program offers that value; the permission mode, `ask` until the person changes it; instructions every agent follows, told beside the person's profile under their own heading; and whether a message to a busy agent queues or steers ([Work that waits its turn](#work-that-waits-its-turn)).
- **Back to the default.** Each field on an agent's edit page that follows the default says so, and the person sets one back to it there. Agents made before the defaults keep the permission mode they were made with until the person sets it to follow, so nothing loosens on its own; one an agent makes, or an import brings, starts on `ask` of its own.
- **Rules for every agent** sit beside the defaults, on the Rules page, since they're followed by every agent too, but they're kept apart, so a rule never loosens with a default ([Rules, records and limits](#rules-records-and-limits)).
- **Agents read them, the person changes them.** An agent reads the defaults at once, and changing them asks its person first, as a looser permission mode does, since the default mode is every following agent's.

### Agents can do anything the app can

- **One list of commands.** Every command the app sends (make an agent, change a routine, add an ACP agent, install a plugin, make an invite, export, move) is one an agent can send too, with `run_command`, and `list_commands` lists them with their args. `server/commands.calx` says which run at once and which ask, so each new command is an agent's from its first day, asking until it's put among those that don't.
- **The same rules as you.** An agent works with its person's rights and no more. Reading and everyday changes (conversations, notes, routines, tasks, skills, plugins) run at once, and what changes is in the activity log. Anything that deletes, removes, installs, lets people in, reaches outside, gives an agent more (a looser permission mode, a folder) or moves the instance asks first, every time, and is done as the device that said yes. The person's account, password and secrets' values, and what a plugin is allowed, stay theirs. An agent can't raise its own permission mode, delete itself or answer its own approval.
- **Scriptable to the end.** The server's command line (`loom install`, `loom receive`, `loom status` and the rest) does everything the app does, and prints JSON when asked, so an agent working over SSH reads it reliably.

Say you ask: "Set up loom on this VPS and move me there." The agent:

1. Connects with `ssh`, using the address and key you give it. That's an approval.
2. Runs the install script (`curl -fsSL …/releases/latest/download/install.sh | sudo sh`). It downloads the server for that machine and runs `loom install`, which makes the `loom` user and the service, installs a browser and fonts for the agents' browsers if the server lacks them, and sets up Caddy for TLS if there's a domain, or Tailscale if there isn't, on an HTTPS port nothing else on the tailnet uses. `loom install --json` gives the address and the setup code; `GET /api/version` says loom answers there; `loom acp install` and `loom acp settings` give it agent programs; and `--import openbot` brings OpenBot over from the same server.
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
- **No network.** A content security policy (`connect-src 'none'` and the like), plus the web view refusing any request outside the panel's own scheme, stops it fetching anything or sending anything out. The policy also sandboxes the page with scripts only, so its origin is no one's, and so is each blank frame's it makes, which it can't reach into. A click on a link out of the panel does nothing, and on a Mac the web view may reach no host at all.
- **One door.** The panel talks to the app only through one message handler (`window.ipc` from `wry`, a `WKScriptMessageHandler` on iPhone). The app checks each message against what the panel's manifest asked for and you allowed: its own storage, reading certain data, calling certain commands, messaging an agent. It answers only those.
- **Storage through the door.** A panel's data belongs on the instance, where it syncs to every device and moves with it, so the web view's own storage is turned off.

An embedded web view is a native window of its own, laid over the GPUI window, so GPUI can't draw menus or popovers on top of a panel. Panels are laid out so nothing needs to overlap them. Phase 7 starts with a panel that tries to break out on each platform, and checks that it can't reach the app or the network.

That test is `loom --panel-test` (`desktop/src/panel_test.rs`), in CI on Windows and on the Mac. Its first runs showed two things. The policy and WebView2's request filter each stop every HTTP request on their own. WebRTC goes around both, and around the proxy too, and WebView2 has no switch for it, so a script that runs first in every frame, blank frames included, takes it away.

On the Mac its first run got out of WKWebView in two ways. WebRTC and storage from a blank frame: WKWebView runs no script in a blank frame, so the prelude can't take them away there. The policy's sandbox closes that, since the page can't reach into the frame. And a TCP connection, from a link the page clicked: WebKit connects to a link's host as it's clicked, before the policy, the sandbox or anything else sees it. WKWebView has public ways to do what WebView2's filter and proxy do: a content rule list that blocks every load but the panel's own, and a proxy that isn't there for its data store, which keeps nothing. But the link's connection skips the rule list, and macOS never sends an address on the computer or its network to a proxy. So the prelude cancels a click on a link out of the panel: on the window, on every shadow root the page makes, and around a click the page sends into a tree of its own. With that the test passes on the Mac. A link in a closed shadow root that the page's own HTML declares still got through, when it or something in its slot was clicked, as script can't see into one, and the test now has two such links. Only WebKit's private settings turn that connection off, so a panel's web view sets two, each checked for first: `_allowedNetworkHosts`, an empty set, which WebKit checks before every load and before that connection, and the data store's `allowsServerPreconnect`, off. Either one stops it alone. A macOS without the first opens no panels or pages, and says so. The iPhone gets the same test with its WKWebView.

Later: Calx library plugins in the server (Calx's own `@export` plugins), once loom can build them with the Calx it was built with.

## Server

- **One Calx process** on std.http.server (std.net's event loop underneath, WebSockets included), listening on 127.0.0.1. On the VPS, the domain comes in through the Cloudflare tunnel and Caddy, which handle TLS. Self-hosters put Caddy or their own proxy in front.
- **HTTP routes** for what isn't a live session: `/api/setup`, `/api/login`, `/api/link` and `/api/link/new`, `/api/join/<code>`, `/hooks/<id>` for webhooks, and `/ws` for the WebSocket.
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

- **Adding an agent.** Loom installs agents from the [ACP Registry](https://agentclientprotocol.com/get-started/registry): on the instance's machine for the owner, or in a member's sandbox. A binary release for that machine is downloaded into the instance's `acp/` folder, checked against the registry's SHA-256 and unpacked; otherwise the agent runs through `npx` or `uvx`. Installing one again updates it. An agent's settings (its model, its provider, an API key) live in loom, which passes them to the agent as environment variables when it starts it, and never sends their values to the apps. Keys are kept with the other secrets. The app's **Brains** page (an agent program is an agent's brain) lists what's installed, each with its settings, and the registry's agents to install, each with its icon. The instance fetches the icons, small SVGs in currentColor, with the registry, so the apps draw them in their own ink; `acp.registry` gives each as `icon_svg`, and `programs` says which registry agent each program on the instance is, matching one added by hand (`claude`) to the one named for it (`claude-acp`).
- **Programs the desktop app finds.** Where npx is, an instance in the desktop app adds Claude's and Codex's adapters itself, as `claude` and `codex`, each at a version: the registry's, or until it has read the registry, the one loom ships with. It installs that version with npm into `acp/<name>/<version>/` on the installer's thread and runs it with node from then on, so a start doesn't pay for npx (npm's own start, a second, and its look at npm's registry, 15 s once its cache is stale). Until it's in, the program runs through npx at that version. The registry is read again every 6 hours, and a newer version is installed the same way and taken as each program next starts; older versions' folders go once nothing runs them. One the person changes by hand isn't followed any more.
- **Loom never logs in for an agent.** Each ACP agent uses the login it already has on that machine, set up by the person it belongs to (for Claude, their own Claude Code login). Loom has no "log in with Claude.ai" or anything like it.
- **Each person's login is their own.** The owner's agents run as `loom` on a server, or as you in the desktop app. A member's run in their [sandbox](#sandboxes-where-members-agents-run), with the member's own logins. One person's subscription is never used for anyone else.
- **Claude through ACP.** Claude's adapter is one ACP agent like the rest, and loom runs it as OpenBot does, on the person's own Claude Code login. Elliot's own instance uses it from the start. Anthropic's Agent SDK docs say: "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK." The rule is about what a product *offers*, so the public loom:
  - supports ACP agents in general, and lets people add whichever they have;
  - doesn't bundle Claude's adapter, log anyone into Claude, or advertise Claude subscriptions;
  - offers Claude through an API key in its docs.

What loom gives an agent when it starts a session:

- `cwd`: the agent's workspace folder.
- A system prompt built from the agent's instructions, in `append` mode, where the agent supports it. Where it doesn't, the instructions go in the folder as `AGENTS.md` or `CLAUDE.md`.
- `mcpServers`: `loom mcp` (below). In [Code Mode](#code-mode), the default, that's all, and `loom mcp` is the client of the agent's MCP servers and its person's plugins' tool servers. With Code Mode off, they're given too, and the agent connects to them itself.
- What changes during a conversation (the time, which device you're on, a routine's context) goes in with the next prompt, never as an edit to the instructions.

What comes back:

- `session/update` notifications (message chunks, thoughts, tool calls, plans) become loom events, so every agent looks the same in the apps.
- `session/request_permission` becomes an approval in the app, and a push on the phone.
- After a restart, loom picks the session up again with `session/resume` or `session/load`, where the agent supports them. So it does after an agent's program has been idle for 30 minutes, with no run and nothing said, which loom stops: each holds a process, and a Node agent 60-200 MB, and the next message starts it again in a second or two.
- **Ready before the first message.** A program takes a second or two to start, and Claude's adapter as long again for a session. So when one of the person's apps connects, the program of the chat they last wrote in starts, and that chat's session is made or picked up, before they've written: the message goes straight in. It's stopped after 30 idle minutes like any other, so the cost is one program for a while after the person opens an app.
- **Stopping a program stops what it started**: it runs in a process group of its own, which loom ends whole (npx's node, Claude Code, `loom mcp` and its Deno). When the instance stops, for a restart, an update or the app quitting, it ends every program and waits for them, up to 10 seconds.
- **One endless chat.** There are no chat tabs: each agent's chat is one conversation that never ends. The agent's `usage_update` says how full the session's context is, and once it's 80% full, the next message goes to a fresh session instead. That one is given the newest of the chat (up to 24,000 characters) and told `search_messages` finds the rest, so nothing breaks for the person and the whole history isn't sent again. The chat keeps every message, and the activity log says when it happened.
- Models and reasoning efforts the agent offers as session config options show in the app: a picker over the chat and on the agent's edit page. What the person picks is kept with the agent, and loom sets each session to it with `session/set_config_option` before its next prompt.
- The usage the agent reports (tokens and cost) is counted per person.

A turn, from start to end:

1. A message comes in. The instance stores it and sends it to the agent's session with `session/prompt`, starting the agent if it isn't running.
2. The agent's updates stream out to the apps, as `delta`s and events.
3. When the agent asks permission, the instance answers by itself or asks you, as the agent's permission mode says.
4. When the turn ends, the instance stores the finished message as one event, then says the run finished. It says so however the turn ended, even when keeping the message failed, so no app shows the agent working after its turn.

Each step is saved as it happens, so a restart resumes the session where it was. A turn can be stopped from the app, with `session/cancel`.

Each agent program runs as its own process. The instance reads each one on a thread of its own, through std.process.

### Permissions

- **Loom starts each agent in a mode that asks** before commands and edits, never in one that skips asking, unless the owner picks that for one agent.
- **Loom answers the agent's requests** by the agent's permission mode, its own or the person's default ([Agent defaults](#agent-defaults)): `ask` (every request comes to you), `auto` (loom allows the tools that only look: reading, searching, thinking and fetching, and asks about the rest), `allow` (everything), or a list of the kinds to allow.
- **Rules on what's touched come first** ([Rules, records and limits](#rules-records-and-limits)): a rule that denies refuses without asking, one that asks asks even where the mode allows, and one that allows answers yes where the mode would ask.
- **Loom's own tools follow loom's rules.** Sending, paying, deleting, letting people in, changing permissions and moving always ask.
- **Agent programs don't ask before Code Mode's tools.** Loom asks for each call in a script ([Code Mode](#code-mode)), so a program that asked before `search_tools` or `run_code` too would ask twice. Each program is told in its own way. The tools' MCP annotations say `search_tools` only reads, and `run_code` destroys nothing and reaches nothing by itself, which is enough for Codex. Claude's adapter gets them as `allowedTools` in the session's `_meta`. OpenCode gets a rule allowing each in its config (`OPENCODE_CONFIG_CONTENT`, added to a config the program's settings already give, whose own rules for them win). In `ask`, loom asks before each script itself.
- **Where the agent runs limits what it can do anyway:** as `loom` on a server, in a sandbox (every member's agent, and any marked Isolated), or as you in a desktop instance.
- **A question is a card in the chat, in plain words.** The core puts the call in words for both apps (`core/asks.calx`), from nothing but the call: what it does as the title, the call's own title, or for a tool of an MCP server or a plugin, the tool's name in words ("Search issues", "Send email in Gmail" for Composio's `GMAIL_SEND_EMAIL`); the server and the tool under it, small; the services it reaches as badges; and what loom knows it does in a pill, from the kind of tool and an MCP tool's `readOnlyHint` (Only reads, Changes files, Runs a command, Deletes, Can change), with no pill where loom doesn't know. A call whose arguments list the tools it runs, as Composio's `COMPOSIO_MULTI_EXECUTE_TOOL` does, shows them as steps, each with its service's badge, what it does and a few of its arguments as chips. Where the agent gave its reason in an argument (`thought`, `reason` and the like), it's a quote line under them. The rest of its arguments are labelled lines, short values whole and long ones cut, a list or an object said in short, a command or a script as it's written, and all of it under **Details**. Then **Allow**, with **Always allow** under the arrow beside it where it's offered, and **Deny**. Answered, the card is one quiet line saying how until the question goes.

### Loom's own tools

`loom mcp` serves them, as an MCP server over stdio. It's the `loom` program in MCP mode. In the desktop app it's a small program of its own, `loom-mcp` (`server/mcp_main.calx`), which the app carries and writes into its folder's `bin/` once for each build, since the app itself takes twice as long to start (14 ms to 6 on Windows); `loom.exe mcp` still works. The instance gives it to every ACP session. It connects back to the instance with a token for that one agent, passed in its environment, and it acts with that person's rights and no more. The tools are defined once, in `shared/tools.calx`.

| Tool | What it does |
|---|---|
| `message_agent` | Talks to another of your agents |
| `remember`, `recall`, `forget`, `update_profile` | Memory (below) |
| `schedule`, `list_routines` | Make or change a routine, and list them ([Routines](#routines)) |
| `routine_post`, `routine_quiet`, `routine_failed`, `pause_routine` | While answering for a routine, whose turn the person doesn't see: post what's worth telling them into the chat, end the run with nothing to say, say it failed and why, or pause the routine ([Routines](#routines)) |
| `notify` | Sends you a notification |
| `edit_instructions` | Changes the agent's own instructions |
| `list_agents`, `create_agent`, `update_agent`, `list_acp_agents`, `search_messages` | Loom's commands. None of them changes a permission mode or deletes |
| `delete_agent`, `set_permission`, `delete_skill`, `delete_routine` | The risky ones. Each asks you every time, whatever the agent's permission mode, and is done as the device you answered on. An agent can't delete itself or loosen its own mode |
| `list_skills`, `read_skill`, `save_skill` | Skills: read when a task fits one, and saved by an agent for every agent. They're also written into each agent's folder as Claude Code skills. One written from a task the person taught in the browser is proposed instead ([Teach a task](#sharing-and-learning)) |
| `propose_skill`, `propose_note` | A skill or a note from past chats, which waits for the person to keep or drop it ([Teach a task](#sharing-and-learning)) |
| Device tools | Your PC and phone, where you allow it ([Machines and devices](#machines-and-devices)) |
| `list_commands`, `run_command` | Every loom command: agents, routines, tasks, plugins, MCP servers, agent programs, people, devices, moving ([Agents can do anything the app can](#agents-can-do-anything-the-app-can)) |
| `add_server`, `install_server` | Another server in your app (asking first), and loom put on one of your hosts and signed in to, on one yes, or the command that installs it by hand ([Several servers in one app](#several-servers-in-one-app)) |
| `add_mcp_server` | An MCP server for you and the agent, once you say Add on its card, which goes on to Sign in for one that needs it ([Signing in to MCP servers](#signing-in-to-mcp-servers)) |
| `html_preview`, `html_render` | A picture of a page of HTML and what its console said, and the page shown in the chat ([Pages in the chat](#pages-in-the-chat)) |
| `show_card` | A card in the reply: choices that answer, a checklist, a table, progress or facts, changed in place as the work goes on ([Cards in the chat](#cards-in-the-chat)) |
| `list_tables`, `create_table`, `write_rows`, `query_rows`, `delete_rows`, `drop_table` | Tables every agent of the person's shares, each its owner's, with what other agents wrote marked untrusted; dropping one asks first ([Shared tables](#shared-tables)) |

- **Shell, files and web fetch** come from the agent itself.
- **MCP servers** (Acta, Cloudflare, Composio for mail and the rest) are reached through `loom mcp` in Code Mode (below), or passed to the agent, which connects to them itself, with Code Mode off. The MCP servers page tests each: Test starts it as a session would, with its keys or its sign-in, and says the tools it offers and whether it's signed in, or why it didn't start, as a key it names that the person doesn't have, or that it needs them to sign in ([below](#signing-in-to-mcp-servers)). An agent can test one too (`mcp.test`).
- **A browser** is loom's own, below ([The agent's browser](#the-agents-browser)), so you can watch it and take over. Its tools are loom's (`loom.browser_open` and the rest), never an MCP server's, and it comes with loom on every computer, so the MCP servers page doesn't offer one. The Playwright MCP server named `browser` that the page's old **Add a browser** set up (`npx -y @playwright/mcp@latest`, with no settings) is taken out once, when an instance first starts on a release with loom's browser everywhere, and so is each agent's pick of it, with a line in the person's activity. One set up any other way stays.

### Code Mode

An agent that's given every tool's definition fills its context with them before it has done anything, and every result passes through it, even what only goes on to the next tool. So, as Cloudflare's [Code Mode](https://blog.cloudflare.com/code-mode-mcp/) and Anthropic's [code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp) do it, an agent gets two tools in their place, and writes a script that calls the rest.

- **`search_tools`** finds tools by words and gives each as a TypeScript signature, made from its JSON Schema, with what it does and what each argument is for: `loom.notify(args: { title: string; body: string }): Promise<any>`. Without words, it lists every tool's name.
- **`run_code`** runs a TypeScript script. Each tool is an async function: loom's own under `loom` (`await loom.recall({query})`), the agent's MCP servers' under `mcp.<server>` (`await mcp.github.create_issue({...})`), and its person's plugins' tool servers under `plugins.<plugin>`. A name that can't be a JavaScript name has `_` for each character that can't be. A tool gives its structured content, or its text, parsed when it's JSON, and throws when it fails. Only what the script returns and what it logs with `console.log` goes back to the agent, so it can read a whole document, pick out three lines and pass on only those.
- **The script runs in Deno with no permissions:** no network, no files, no environment and no programs, and no npm or remote imports. Its only way out is its standard output, where each call goes to `loom mcp` as a line of JSON and is answered on its standard input. So a key never reaches the script, or the model: the MCP servers get their settings and secrets, as before, and the script only gets what they answer. A small prelude, written by `loom mcp` from Calx (`server/codemode.calx`), makes `loom`, `mcp` and `plugins` and the bridge; loom's own code stays Calx.
- **Deno is fetched on first use**, as agent programs are: the instance downloads a pinned release of `denoland/deno` for its machine into `acp/.deno/<version>/`, checks its SHA-256, and unpacks it. The first script waits for it. Members' sandboxes see `acp/` read-only at the same path, so a member's scripts run in Deno inside their sandbox, with their MCP servers, under the sandbox's limits. That Deno is the build for the instance's machine, which on Linux needs glibc, as loom's own image has.
- **A Deno waits for each script.** Deno takes 50 ms to start, and half a second for a session's first, so `loom mcp` starts one ahead: at the session's first `search_tools`, and after each script, once its result has gone, while the agent reads it. That one runs the prelude and waits; the script comes to it as the first line of its input, a `data:` URL it imports, which needs no permission. A Deno that waits five minutes for nothing ends by itself, and so does one whose `loom mcp` has gone. None is started before Deno has been fetched.
- **Approvals still ask.** Loom's own tools keep their own rules, as when called plainly. A call to an MCP server's or a plugin's tool is put to the agent's permission mode as the agent program's own request would be, of kind `read` for one marked `readOnlyHint` and `other` for the rest: one the mode allows goes at once, and otherwise the person is asked, as with any approval, while the script waits. A no throws in the script. **Always allow** there adds a rule allowing that tool for the agent, as the Rules page makes one, so its next calls go at once and the rule can be removed there; where one of the person's rules asked, it isn't offered, since an allow wouldn't get past that rule. Each call that isn't read-only is in the activity log, with what came of it. In `ask`, which asks about everything, the person also sees each script and says yes before it runs, with no Always allow, since each script is a different one. In any other mode it runs at once, and only its calls are put to the mode. The agent program isn't asked about `search_tools` or `run_code` ([Permissions](#permissions)).
- **Limits:** 60 seconds of the script's own time, or up to 300 when it asks, with time spent waiting on a tool (the person's yes, another agent's reply) not counted; 100 tool calls; 20,000 characters of what it logs and returns, cut beyond; 512 MB of memory; a script of 200 KB.
- **The app's tools stay at hand.** Beside the two, loom's tools that reach the person's app and devices are given directly too, since setting loom up for the person needs them at once and an agent shouldn't have to search for them: `add_server`, `install_server`, `add_mcp_server`, `list_devices`, `use_device` and `show_card`, with `search_messages`, which a fresh session is told finds the rest of the chat. Scripts can call them as well.
- **On for every agent**, with a switch for each (`code_mode` on the agent) back to plain tools, where the agent gets every tool's definition and its servers as before. Calling one of loom's tools by its own name still works in Code Mode.
- **An agent's tools never change inside a session.** An ACP session keeps the tools it started with, and is picked up again after a restart with the same ones. When the agent's Code Mode is switched, its chat's next message goes to a fresh session, as when one fills ([Agents run on ACP](#agents-run-on-acp)), told the newest of the chat and that its tools changed. A server its person adds for it from its card ([below](#signing-in-to-mcp-servers)) is the one thing that comes in: in Code Mode it's only more of what `search_tools` and `run_code` reach, so `loom mcp` starts it in the same session, at its next search or script. With Code Mode off, the agent program has its servers only from a session's start, so its chat's next message goes to a fresh session that has it.

### Signing in to MCP servers

Hosted MCP servers (Acta, Cloudflare, Vercel, Composio) mostly don't take a key in a header: they want the person to sign in, as MCP's authorization says (2025-06-18 and on), with OAuth 2.1. Loom does the client's part on the instance (`server/oauth.calx`), so the person signs in once, in their own browser, and every agent and device uses it.

- **Knowing it needs one.** A server over HTTP that answers 401 wants a sign-in, unless the person gave it an `Authorization` header of their own, a key, when the 401 means the key is wrong. Test then says "needs you to sign in" in place of the error, and the MCP servers page shows **Sign in** on the server's row.
- **Finding where.** The 401's `WWW-Authenticate` names the server's Protected Resource Metadata (RFC 9728), or it's at the well-known place under the server's path, then at its root. That names the authorization server, whose metadata (RFC 8414, or OpenID Connect's discovery, with an issuer's path after the well-known part) gives where to sign in and where tokens come from. One that doesn't say it checks a PKCE challenge (S256) isn't signed in to. An older server with no metadata signs in at its own origin.
- **The client.** Loom signs itself up there as a public client named loom (RFC 7591), for the address the browser comes back to, and keeps that client to sign in with again. An authorization server that lets no app sign up needs a client id the person got from it, and its secret, which the server's form takes (text, or `secret:KEY` for one of their keys).
- **Signing in.** The browser opens the authorization endpoint with a PKCE challenge whose verifier only the instance holds, a state, the scope the 401 or the metadata names (and `offline_access` when the authorization server offers it, for a refresh token), and the server as `resource` (RFC 8707). What comes back to the redirect goes to the instance, which checks the state is one it made for that person and still open (15 minutes), and that the issuer the answer names, when it names one (RFC 9207), is the one it sent the browser to, then swaps the code for tokens with the verifier.
- **Where the browser comes back to: the device that asked.** The desktop app listens on `127.0.0.1` for one request to `/callback` (on port 47251 when it's free, so the address an authorization server knows loom by stays the same, or another), gives the instance that address as the redirect, opens the sign-in page in the person's default browser, and hands the query that comes back to the instance as it came, through its core (`mcp.signed_in`). Loopback redirects are the ones OAuth 2.1 and RFC 8252 say every authorization server takes from a native app, over plain HTTP, on any port. The code is no use without the verifier, which never leaves the instance. It works the same for an instance on a server and the one in the app, since the browser never has to reach the instance: a server behind Tailscale, a VPS whose public address the instance doesn't know, and a desktop instance a phone reaches only through the Noise bridge all sign in the same way. The iPhone catches it the same way: it opens the sign-in page in `ASWebAuthenticationSession`, which shares the person's sign-ins in Safari, with `loom:/callback` as the redirect (the `loom:` scheme is the app's, so a sign-in finished in Safari comes back too), and sends what comes back with the same command. A callback route on the instance itself was the other way: it needs the instance to know an address the browser reaches, over HTTPS for most authorization servers, which a desktop instance only has on loopback and a phone can't reach at all.
- **Kept sealed, as the person's.** The grant, with the client, the tokens and when the access token ends, is sealed under `secret.key` as the person's, bound to them and the server (`mcp_auth`), like their secrets, and never in a log, an event, the activity log or an answer to an app. A sign-in is the person's own, as their MCP servers are. The tokens go to the server as `Authorization: Bearer` for a session (in `loom mcp`'s servers in Code Mode, or the agent program's `mcpServers` without it) and for Test.
- **Renewed.** A token is renewed with its refresh token before it ends (5 minutes before, or halfway through a shorter life), on the instance's sign-in thread, and again when a server answers 401 to it: Test renews it and asks once more, and `loom mcp` asks the instance for a renewed one (`/api/mcp/token`) and makes the call again. One the authorization server won't renew means signing in again. A server moved to another address signs in again too.
- **Signing out** drops the tokens, keeps the client, and tells the authorization server (RFC 7009) when it has a revocation endpoint. Deleting the server does the same.
- **An agent never signs in.** In Code Mode, a server that wants a sign-in tells the agent, in what `search_tools` says it didn't reach and as the error of a call to it, that its person needs to sign in and that it shouldn't try itself, in its browser or any other way, and loom puts a card in its reply, "Sign in to acta", with **Sign in**, which starts the same sign-in from that device and says Signed in once it's done (`/api/mcp/sign_in`). Its tools come once the person has, in that session's next search or script. With Code Mode off, the agent program connects to the server itself, with the token its session started with, so a renewed one reaches it in its next session. `mcp.sign_in` and `mcp.signed_in` are never an agent's; `mcp.sign_out` asks first.
- **A sandboxed person's** sign-ins reach only `https://` addresses, so their server's metadata can't have the instance call the plain services on its own machine.
- **An agent adds one when its person asks.** `add_mcp_server` names the server, by its `url` or the `command` and `args` that start it, with the headers or environment it's given, each text or one of the person's secrets by name, never a secret's value, checked as `mcp.put` checks a server. Nothing is kept yet: loom puts a card in the agent's reply, "Add the MCP server acta", with its address or its command and each setting, and the call waits for the person, as an ask does, and ends with the turn. It asks in every permission mode, since a server is a program that runs, or a place reached, with the person's keys. The person's rules come first, as for any of loom's tools: `loom.add_mcp_server`, with its site or its command, so a rule can deny it. **Add** keeps a key the server needs that the person doesn't have yet, typed into a masked field on the card, as their secret, which the agent never sees, then the server, as the person's own, and gives it to the agent that asked. A member's goes in their own servers, which run in their sandbox, as ever. Then it's tried as Test tries it: the card says it's ready, with its tools, or why it didn't start, and one that answers 401 goes on to **Sign in**, the sign-in above, and **Later**. The card is ready once the person has signed in, or says in plain words why the sign-in didn't work. The agent hears how it ended: added, with its tools; added, and waiting for the person to sign in (after Later); or not added (**Cancel**, or a turn that ended first). A sandboxed person's server isn't tried on the instance, so theirs is just added. Only the person answers a card, on one of their devices (`mcp.add_answer`): not an agent, a plugin or a script.

## Agents that do things

Ideas taken from OpenBot ([CopilotKit's](https://github.com/CopilotKit/OpenBot), MIT, and [nightly-labs'](https://github.com/nightly-labs/openbot), noncommercial, so ideas only), built loom's way: in Calx on the instance, drawn by the apps.

### The agent's browser

- **Each agent has a browser of its own**, which the instance starts when the agent first uses it: Chromium or Chrome on a server, and on a PC the Edge or Chrome that's there (`LOOM_BROWSER` names another). It runs headless with a profile per agent in the agent's data, so logins stay between runs.
- **Nobody installs it.** Windows always has Edge: loom looks for Chrome and Edge in Program Files and the person's own AppData, and then where the registry's App Paths say, as Edge's installer writes them. A Mac uses the Chrome or Edge in Applications, and without either, fetches Chrome for Testing's headless shell for Apple silicon on the agents' first browser call, as Code Mode's Deno is fetched: one build loom pins with its SHA-256 (`server/chrome.calx`), into `browser/` in the instance's data, refused and left out if the download isn't that file. A new pin comes with a release of loom. That first call waits for it, about 100 MB; its result says loom fetched a browser of its own, and so does the person's activity. A Linux server's is below.
- **It runs only while it's used.** It closes when the agent's turn ends, unless you're watching it in the panel or have taken it over, then once you stop; when the agent calls `browser_close`; when the agent is deleted, moved or the instance stops; and, whatever else, after five minutes with nothing done in it. Five, since an agent that stops browsing to think or run commands mostly comes back within a few minutes, and a closed browser costs it its open page and a second or two to start; while an idle Chromium holds a few hundred megabytes. Closing it asks the browser to close, so it saves its profile and the logins in it, and then ends it and every process it started, after five seconds at most. The agent's next browser call starts it again, on the page it was on, and says so; one that comes while it's still closing waits for it, since it holds the profile until it ends. A loom that's killed or crashes takes its browsers with it, everywhere. Only the desktop app's on Linux and macOS, whose instance is in a library that can't start a browser so, run on; it stops them when it starts again on the same data.
- **The same browser on a server with no display.** It's Chrome's own headless mode everywhere, which since Chrome 112 is the browser with a window, drawing pages the same way, only without the window; so a server needs no virtual display, which would be one more process to keep and stop for nothing a picture would show. What a bare Linux server lacks is the browser and fonts: without them a page's words are boxes in its picture. `loom install` installs them with apt: Debian's Chromium, or Google's Chrome on Ubuntu, whose Chromium is a snap that can't keep its profile in `/var/lib/loom`, and the Noto fonts for every script and emoji, with Liberation, as wide as Arial, Times and Courier, so pages lay out as on a desktop. A server set up without them, as one from before `loom install` installed them, fetches its own when an agent first opens its browser, as the loom user and with no root: Chrome for Testing's headless shell, which Google builds for x86-64 and arm64 Linux, into `browser/` in the instance's data, fetched again once it's four weeks old, and the Noto fonts it lacks, with a fontconfig file that adds them to the system's. The libraries that build needs come only as packages, so a server lacking them says which, and that `sudo loom install` adds them. `loom status` says what's missing, and a browser that can't start says what to install. Loom's Docker image has them.
- **The instance drives it over the DevTools protocol** (CDP, a WebSocket), and offers it as loom's tools: `browser_open`, `browser_snapshot` (the page as its accessibility tree, each thing to act on with a ref), `browser_click`, `browser_type`, `browser_press`, `browser_scroll`, `browser_back`, `browser_screenshot` (a picture the agent's model looks at, for what an outline doesn't say: layout, images, charts, a CAPTCHA), `browser_close` and `browser_ask_person`. A ref goes stale when the page changes, and the tool says so.
- **You can watch it.** An app showing an agent with its browser open asks for its screen (`browser.watch`), with the size it shows it at, and the instance sends frames as they come (`browser.frame`, at most about twelve a second), only while someone watches: JPEGs the browser draws no bigger than they're shown, each sent as it is in a binary message rather than as base64 in JSON. A small window's frame is a tenth of the page's at its full size. On the desktop it pops up as a small window at the chat's bottom right while the agent browses, which only follows the agent (`follow`) and goes when its turn does, and opens into the panel beside the chat, where you watch it, and from there full screen, a popup over the window that the same control or Esc puts back; it's a sheet on the phone.
- **You can take over** for a login, a 2FA code or a CAPTCHA: the agent asks with `browser_ask_person`, and you press Take over on its ask, or in the panel, or click into the page. Your clicks, scrolling and keys go to the page (`browser.input`), with what's held with them, so Shift+Tab, Shift+arrows and Ctrl+A (⌘A) work there too, the panel shows plainly that you have it, the agent's browser tools wait meanwhile, and Hand back lets it carry on, with the tool's result saying what you did.
- **Secrets go in without the agent seeing them.** `browser_fill_secret` types one of the person's secrets into a field by name, asking them the first time it goes to a site. When a page wants something it doesn't have, a password, the email they sign in with or a one-time code, `browser_ask_secret` puts a card in its reply: what's asked, for which site, a masked field, with an eye to show what's typed (a code or an email is shown as it's typed, and what the agent asked says which it is when it doesn't say), and **Save as** with a name made from the site, such as `ntnu-password`, which the person can change or untick. **Fill** types it into the page and keeps it, if they said so, with that site allowed, so next time it's filled without asking; **Not now** is a plain no. One card can take several fields on a page at once, an email and a password; a sign-in that asks for them on two pages is two asks. The agent is told it's in, and what it was kept as, never what it was.
- **An agent never asks for a password in the chat**, where it would stay as text. Each session is told so once, the browser tools say so, and when the agent's page wants a password or a code, the call's result says it again, with the names of the secrets the person keeps for that site, so it fills one rather than asking. The secrets in the settings list where each may be filled; the person takes a site off, removes one, or adds one by hand.

### Pages in the chat

- **An agent can check a page it wrote**: `html_preview` draws its HTML in a tab of the agent's browser of its own, off to the side, so the page the agent has open stays as it was, and gives a picture of it and what its console said, errors among them. The tab closes once the picture is taken.
- **And show it**: `html_render` puts the page in the agent's reply, after what it has said so far, as a file of type `text/html` kept as the person's. A report, a chart or a small tool comes as a card in the chat, and opens beside it on the desktop in the same web view as a plugin's panel, walled off the same way: its scripts run, but it reaches no network and has no way into loom, so everything it needs goes inline.

### Cards in the chat

- **Small things an agent makes go in its reply as cards**, which the apps draw in the chat's own look: `show_card` puts one after what the agent has said so far. A card is one of:
  - `choices`: two to six answers to the agent's question, as tiles, two to a row. The one the person clicks goes as their next message, and the tiles answer only while that reply is the chat's newest message. On the desktop each tile shows the key that picks it, Ctrl (⌘ on a Mac) and its number.
  - `checklist`: up to 30 items, each done or not.
  - `table`: up to 8 columns and 50 rows of text.
  - `progress`: how far along something is, from 0 to 1, with a label.
  - `facts`: up to 12 labels, each with its value.
- **A card changes as the work moves along.** Called again with a card's id from the same reply, `show_card` puts the new card in that one's place, so a checklist ticks itself off and a bar fills while the agent works. The apps get each change at once, and the finished reply keeps the card as it was last.
- **The instance tidies what it's given**: text is cut short and kept to one line, and a card that can't be drawn is refused, saying why, so the agent fixes it. Anything bigger, or with its own look, is a page (above).
- **Every card in the chat has one frame**: at its start the badge of the service it reaches, or an icon; its title, with the agent's thread beside it, tugging while the card waits for the person and settled once it's answered; under the title, small, where it goes; at the head's end what it does, in a pill, or its one button; then its rows, and its buttons, the main one filled, with Details at the far end. A card waiting for the person has its edge in the agent's colour. An agent's cards, loom's own, the questions an agent waits on ([Permissions](#permissions)), the secrets an agent asks for ([The agent's browser](#the-agents-browser)) and the asks above the composer all use it, in the chat's own quiet look, with the agent's colour as the one hint of it. A field the person fills in, as the secret card's, is one thing the frame takes, with its label, masked or not, and **Save as** where it can be kept, so a card with several fields is the same.
- **A service's badge is the same everywhere**: its initial on one of the thread colours, picked from its name (`shared/badge.calx`), which both the instance and the core use, so a server has one colour on every card and in both apps. An MCP server that gives its own icons (`serverInfo.icons` in its `initialize`) has them on its badge, on loom's own cards and on the questions about its tools, and the apps draw the icon in the initial's place once they've fetched it, the initial until then. Loom picks the one that suits a badge best (`server/icons.calx`): a PNG or a JPEG near 64 pixels, and the one it gives for a dark background, if it does. An SVG is passed over, since loom can't make a picture of one yet ([CLX-450](https://acta.aaenz.no/aaen-studios/clx#CLX-450)). The icon is fetched within 5 seconds and at most 256 KiB, an https address or a data: URI, checked to be a picture of at most 1024 pixels a side, made at most 128, and kept as the person's file, by its SHA-256 ([Files and images](#files-and-images)). It's fetched when Test or an Add on a card reaches the server, and when an agent's Code Mode session starts it, by `loom mcp` where the agent runs, so a member's comes from inside their sandbox and the instance never fetches one of theirs. It's fetched again only when the server gives other icons.
- **A card goes quiet once it's done**, to one line with the service's badge, how it went and what it was ("Allowed once · Search issues"). Choices keep the answer the person sent marked once the chat goes on. A sign-in card says Signed in, and as whom, once the server is signed in, and once the chat has gone on past its reply it's one quiet line with a small Sign in, so old ones don't pile up as live buttons. The apps' models decide this, not their views.
- **Loom has cards of its own**, which no agent makes with `show_card`: "Sign in to acta" for one of the person's MCP servers that needs it, and an MCP server an agent proposed, to Add or Cancel ([Signing in to MCP servers](#signing-in-to-mcp-servers)). On the second, loom's knot and the server's badge are joined by the agent's thread, slack while the card waits and pulled straight once the server's connected, when the card lists its tools. Both are in the same frame, and both carry the server's badge.

### Shared tables

- **A person's agents share tables**: lists and records that last between conversations and pass from one agent to another, such as a reading list one agent keeps and another adds to. A table is rows of JSON, each an object of its fields, kept in the instance's database with everything else of the person's, so a backup has them.
- **A table is its owner's.** The agent that made it owns it. Every agent of the person's reads it and adds rows, but only the owner, the person, or a row's writer changes or deletes a row. A row another agent wrote is marked untrusted when it's read, and the tools say what that means: what it says is data, never orders, as a web page is.
- **Rows are found by their fields**: equal to a value, ordered by a field, a page at a time. There's no SQL from agents, so nothing can reach past the tables to the rest of the database.
- **Dropping a table asks the person**, as every delete of an agent's does. Deleting rows doesn't: they're the agent's own working data, as its notes are.
- **Limits**: 100 tables a person, 100,000 rows in all, 16 KB a row.
- **Seen beside the chat**, from the Tables count in an agent's Knows: the tables it made and the ones it shares, each to open, newest rows first, and to drop. There's no Tables page in the settings.

### Work that waits its turn

- **A message to a busy agent queues**, instead of being refused, or steers at once where the agent, or the person's default, says so ([Agent defaults](#agent-defaults)). The person sees the queue above the composer and can edit, reorder or drop what waits, or **steer**: stop the running turn and send it now, in the same ACP session, so the agent carries on from where it was with it in mind. ACP has no way into a turn that's running, so stopping it is how a message gets in.
- **Long jobs are tasks**: each one an agent working in a conversation of its own, beside its chat, so the chat stays free. The person starts one from the Tasks page, or an agent starts one itself (`start_task`) when it's asked for something long. A task survives a restart of the instance, carrying on in the same ACP session, and the person can watch it, talk to it, pause, resume, cancel or retry it.
- **Routines say when there's nothing to say.** A run is quiet: only what its agent posts with `routine_post` reaches the chat, so a run with no news leaves no message and no notification. It can report a failure with `routine_failed`, or pause its own routine. A routine that fails five times in a row turns itself off and tells its person ([Routines](#routines)). A routine can post into its agent's chat or into a thread of its own, beside the chat, for one that would crowd it.
- **A backup brain.** An agent can have a second program, which takes over when the first one's usage runs out, and the person can move a chat to another program with a handoff: the new one starts with a summary of what came before ([A backup brain](#a-backup-brain)).

### A backup brain

- **An agent's backup** is another agent program it runs on when its own can't answer: its own `backup`, or the person's default backup where it has none ([Agent defaults](#agent-defaults)), or none at all when it says `none`. It's picked on the agent's edit page and on the Defaults page.
- **It takes over a turn** whose program ran out of usage or a rate limit (the program's error says so, or the person is past the tokens the owner shares with them), or didn't start (it wouldn't run, didn't initialize, ended before it was ready, or couldn't start a session). The same turn goes on in the backup, in the same reply, which says so first in one line, such as "Claude Code ran out of usage, so Codex answers this chat now." A backup that can't answer either fails the turn, as before.
- **The chat stays on the backup** from then on, so it isn't handed back and forth, until the person moves it back. Usage limits last hours, and only the person knows when theirs has come back.
- **Moving a chat by hand**: Runs on, at the foot of the agent's edit page, puts the chat on another program. Its next reply there says so in one line, and the program starts with loom's summary of the chat.
- **The summary is loom's own**, made from the chat itself, not by a model: what the person asked, oldest first, then the newest of the chat word for word, in 8,000 bytes at most, with `search_messages` for anything older. So it costs nothing and works when the old program can't answer. The new program gets it once, as a `[loom: …]` block with its first prompt in the chat, and goes on in its own session from there.
- **A program's picks stay its own.** On another program than its own, an agent's sessions are set to the person's defaults for that program, not to the agent's own model and effort, which are for its own program, and what that program offers isn't kept as the agent's options.

### Scripts

- **The person's own scripts reach their agents**: a script asks an agent and waits for its answer, runs a routine, or answers what waits for the person's yes, through `/api/scripts/` on the instance or loom's command line (`loom ask`, `loom routine run`, `loom approvals`, `loom approve`). A cron job asks for a summary, a build script answers an approval.
- **Off until the person turns them on**, in the app's server settings, which gives a script token, shown once. Only its SHA-256 is kept. Turning scripts off, or on again, ends the last one.
- **Only from the instance's own computer**, straight: a loopback address, and nothing forwarded through a proxy in front of it or the phones' bridge, which say so in their headers. A token copied elsewhere is no use.
- **A status token reads, and does nothing else.** Made in the same settings, apart from scripts, for a dashboard on that computer: it reads the person's agents (names, colours, shapes, whether each works now), routines (on or off, how the last run went) and the last 24 hours' routine runs, through `GET /api/scripts/status`, never what anyone said, a prompt, instructions, memories or secrets. It's taken nowhere else, and a script token doesn't read the status.
- **An agent can have a status token saved to a file without seeing it**, for a dashboard it sets up on the instance's computer (`token.make_to_file`, for the owner only; the script token may follow, through the same command). It names an absolute path, and its person says yes on one of their devices, every time, to a card with the kind, the exact path and the agent, and whether it ends the token they have; a script can't say yes. Loom makes the token, writes it to a new file only its owner can read, never through a link and never part of one, and tells the agent only where it went: the token is in no tool's result, chat, log or event. Paths inside loom's data folder, through a link, or onto a file that's there without `replace` are refused, when it's asked and again on the yes. The server settings show it as saved to that path by that agent, with Revoke as usual.
- **A script speaks for the person.** What it asks goes in the agent's chat as from a script, and its yes is the person's yes, in the activity log as the script's. A call to one of the person's devices is still answered on a device, which signs it. Agents can't turn scripts on.

### Rules, records and limits

- **Rules can read what's touched.** Beside its permission mode, an agent works under the person's rules on the site a browser tool or a fetch goes to, the file path, the command, the MCP tool and its effect (only reading, or changing something), and who started the turn (the person, or the owner, an admin or a member by their role, another agent, a routine, a webhook, a script or a plugin): allow, ask or deny. A rule has one or more of these, all of which must hold, and is for one agent or for every agent. Deny is checked first, then ask, then allow, and with none the permission mode decides. A rule that can't be read denies, and so does an action whose site, path, command or starter a rule is about but can't be read. Commands are checked one by one where a line chains them, and an allow holds only when every one, and every path, matches it. An allow answers an agent program's request by itself; loom's own tools keep their own rules, so for them a rule only asks or denies. Rules aren't revisions of an agent, so no rollback takes one away, and an agent reads them and asks what-ifs at once, while adding, changing or removing one asks its person. The **Rules** page, under Agents in the settings, lists them, adds and removes them, and has a "what if" tester that asks the instance what an action would get and which rule or mode decided (`rule.check`). Unlike the shell's globs, `*` and `**` match hidden files and folders too, so a rule on a folder covers its `.env`.
- **Every action is recorded before it happens**, then marked done or failed. A refusal names its rule, secrets show only as their length, and the log exports as CSV.
- **Agents talking to agents have limits.** A chain of `message_agent` goes at most four deep and can't loop back to an agent already in it. An ask can carry the task, its bounds and what a good answer looks like. An agent can't give an agent it makes or changes more than it has itself.
- **Anyone's agents can be sandboxed**, the owner's too: an agent marked Isolated runs in a sandbox like a member's ([Sandboxes](#sandboxes-where-members-agents-run)), and its network there can be everything, a list of hosts, or nothing, kept by a sandbox of its own with no network but loom's egress, its browser's too. An isolated agent's new agents are isolated, with no more network than its own, and an agent never changes where it runs itself. An instance with no sandboxes refuses to isolate an agent rather than pretend.

### Sharing and learning

- **A shared folder** all of a person's agents can read and write, beside each one's own, and handing a file from one agent to another with `message_agent`. A file an agent names in a reply, such as `notes/hours.md`, shows under it as a chip with its type, which opens it.
- **Teach a task.** Record yourself doing a task in the agent's browser, and the agent writes it up as a skill. Agents also propose skills and notes from past chats, which the person keeps or drops, and a note can be tied to a site, so the agent is shown it when it visits.
  - **Recording.** Once you've taken over the browser, the Record switch beside Hand back keeps what you do as steps: each page you're on, what you click (named as the agent's outline names it, `button "Sign in"`), what you type and into which field, and the keys you press. What goes into a password or one-time-code field is never kept, only that something secret went in there, and any of your secrets typed anywhere else shows as `[secret NAME]`.
  - **The write-up.** Turning Record off, or handing the browser back, starts a task for the agent, beside its chat, with the steps: it writes them up as a skill with `save_skill`, which there proposes the skill rather than saving it. You read it, with what you did beside it, change it if you like, and keep or drop it.
  - **Proposals.** Agents propose skills (`propose_skill`) and notes (`propose_note`) from past chats, on their own or when you ask one to look over its chats, from the **Proposals** page in the settings. Each waits there, a small list, to keep or drop; nothing an agent proposes is applied without your yes, and an agent can't keep its own.
  - **Site notes.** A note can be tied to a site (`remember`'s or a proposed note's `site`, kept as the host, such as `github.com`). When the agent's browser opens a page on that site, or one under it, the outline it gets ends with its notes for there, once a visit.

## Machines and devices

An agent can work on two kinds of machine: the one its instance runs on, and the devices you talk to it from. Both sit behind permissions, and your devices are off until you turn them on.

### The instance's own machine

- **The owner's agents** run commands and use files there: as the `loom` user on a server, or as you in the desktop app. Members, and isolated agents, can only do so inside a [sandbox](#sandboxes-where-members-agents-run).
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

### Your hosts

Servers you own, such as a VPS or a box at home, are devices too, so an agent reaches them through loom instead of finding your `~/.ssh/config` and running ssh as root by itself.

- **Kept with the computer that reaches them.** The keys are on your PC, so a host belongs to that PC's app: an SSH alias from `~/.ssh/config`, a Tailscale machine's name, or `user@name`. The app offers the ones it finds in the ssh config and on the tailnet (`tailscale status --json`), and you add one with a click or by typing it.
- **Like This PC.** Each host is off, ask each time (the start) or allowed, set on that PC. Agents see it in `list_devices` as a host beside the PC that reaches it, and run commands there with `use_device`, which asks you first in the chat as This PC does. The PC signs your yes and checks it before it runs the command, so the instance can't say yes for you.
- **Run on your PC.** The core runs `ssh` there (std.process), with `BatchMode` so it never waits for a password or a new host key nobody could answer, and the command after `--`. A host's name and target are only letters, digits and `.-_@:`, never starting with `-`, so neither is taken for one of ssh's options. The app only draws the Hosts page beside This PC in the server's settings.
- **Commands, and installing loom.** A host runs commands, which is what an agent needs to set a server up. Files and the rest go through commands. Beside them, it takes one plan the person signs to install loom there and sign the app in, which always asks, even on a host whose commands are allowed ([Several servers in one app](#several-servers-in-one-app)).

## Memory

- `remember`, `recall` and `forget` keep and search notes in SQLite with full-text search (FTS5), per person and per agent. A note can be shared by all the person's agents, and tied to a site, so the agent is shown it when its browser is there ([Teach a task](#sharing-and-learning)). `recall` finds any of its words, best match first. The person reads, adds and deletes them on the agent's **Memory** page.
- A short profile of each person (a few lines, up to 4000 bytes) starts every session, and comes again when it changes. Agents rewrite it with `update_profile` as they learn, and the person edits it on the **About you** page. Each change is a new revision, with who made it.
- No embeddings to start. They come in if full-text search isn't enough.

## Routines

- **On a schedule:** every N minutes (5 at least), daily at a time, or on weekdays at a time, in the person's own time zone, summer time included. A routine takes the time zone of the device that made it.
- **On a webhook:** `POST /hooks/<id>`, with the routine's secret as a bearer token or signing the body as GitHub's webhooks do (`X-Hub-Signature-256`). "The build failed" comes from GitHub Actions posting there, not from a timer. What was posted goes to the agent after the prompt. A routine can have a schedule, a webhook, or both. The instance has to be reachable from where the post comes from: a server is, and a desktop instance is through Tailscale Funnel or the like.
- **A run is quiet.** A routine sends its agent a prompt in a turn of the agent's chat, its newest conversation, so the agent has the chat to go on and the person's next message there finds what it said. But the person sees nothing of that turn: the prompt isn't a message in the chat, the agent's answer isn't kept there, and the apps don't draw the run or show the agent working on it, only a question it asks (`run.started` and `synced` say whose it is). The agent is told the routine sent it and that the turn is quiet, with each prompt. When it has something worth saying, it posts it with `routine_post`: that is its message in the chat, which marks the chat unread, and with notify on (the default) a notification. With nothing posted, nothing reaches the chat, and no one is notified. In the chat, a post says which routine it's from ("From the routine Morning brief"), by the name it has now, or the one it had when posted, which the post keeps.
- **A busy chat makes it wait.** A run due while a turn goes on in its chat, the person's or another routine's, waits for that turn to end, and for what the person queued behind it, then starts. A message the person sends during a routine's run waits in the queue, or steers, as any message to a busy agent does. A run that waited 10 minutes is skipped, and so is one the routine's next run reaches first, whether its next time came or someone ran it again. If the routine's own last run hasn't finished, that time is skipped.
- **A failure still reaches the person.** A run whose turn fails, or whose agent says so with `routine_failed`, keeps the agent's answer in the chat, as any failed turn does, and with notify on the person hears why.
- **Each run is kept for the routine.** Its runs, the newest 100, say when each ran and what ran it, how it ended (`done`, `quiet`, `failed`, `limit` or `cancelled`), what the agent answered and what it posted, and for a flow, what each step answered (`routine.runs`). A time it didn't run is kept too, `skipped`, with why: its last run was still going, its chat stayed busy, it was missed while the instance was off, or the instance restarted while it waited. So is a run still `waiting` for its chat. The **Routines** page shows them under each routine, so nothing a quiet run did is lost.
- Prompts kept in the chat before runs were quiet (person messages `from` a routine) stay stored as they were, and the apps don't show them.
- Missed while the instance was off, a routine runs once when it starts again, or waits for its next time, as the routine says (`catch_up`). Either way, missed runs never pile up.
- The person makes, changes, pauses, runs and deletes them on the **Routines** page, and agents do the same with `schedule`, `list_routines` and `delete_routine`, which asks first.
- **Nothing to say, or a failure.** A run that posts nothing ends `quiet`; the agent can also end it so with `routine_quiet`, which for a routine with a flow keeps its answer from going on. `routine_failed` makes the run a failure, with why, and `pause_routine` turns the routine off and tells its person why. A routine that fails five times in a row turns itself off, with a notification that says so. A change to it starts the count again.

### Routine flows

A routine's answer can go on to other agents, step by step, as a flow: the morning brief one agent writes is summed up by another, and a third posts it.

- A flow is a list of steps after the routine, 20 at most. Each has a `name`, an `agent`, a `prompt` and `after`, the steps it waits for: `routine` (the routine itself) or steps before it in the list, so a flow never loops back. Left out, `after` is the step before it.
- A step goes once every step it comes after has finished. It is a quiet turn in its agent's chat, as the routine's own run is: its prompt and answer aren't kept there, and the agent posts with `routine_post` what the person should read. The agent is told, beside the prompt, what each of those steps answered, and which of them failed, and the routine's run keeps what each step answered. A step whose inputs all failed, or were skipped, is skipped. A step whose agent is busy in its chat waits for the turn to end, so two steps for one agent go one after the other.
- The apps hear how a flow stands as each step starts and ends (`flow.updated`), and the person or an agent stops one with `routine.stop`: the step that's running is stopped, and the rest skipped. The person sets a flow up on the **Routines** page, and an agent with `schedule`.
- A flow that's running is kept, so a restart doesn't end it: when the instance starts again, a step the restart cut short goes again, and the flow carries on.

## Notifications and push

- **Desktop.** The app lives in the tray and keeps the WebSocket open, so the core shows system notifications itself. No push service needed. Clicking one brings the window up on what it's about: its server, its agent, and the task, branch or routine thread it came from, or else the chat.
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

How it works ([protocol.md, "Through the push relay"](protocol/protocol.md#through-the-push-relay)):

- **The relay** is `loom-push` ([push/](push/README.md)). It has an X25519 key that phones seal their tokens for, and an APNs key of its own, which `loom-push key` sets. It answers `GET /v1/key` and `POST /v1/push`, nothing else.
- **The owner turns it on** with `instance.relay` and the relay's address. The instance asks the relay for its key once, keeps it, and gives it to every app in `welcome`.
- **The core seals the token.** When the app hands it an APNs token, the core seals it for the relay's key. It sends that to the instance with the phone's own X25519 key, its box, which the credential store keeps.
- **The instance seals the notification** for the phone's box, and signs each request to the relay with its Ed25519 key. The relay checks the signature and the time, counts the request against that key's limit, opens the token, and sends the notification on to APNs.
- **The phone opens it.** The notification service extension calls `loom_push_open` with what came, and shows the notification the instance made. Without the extension, the phone shows "Something new".

## Moving an instance

An instance moves whole, in any direction: desktop to server, server to desktop, server to server.

- **What moves:** everything. Accounts, devices, agents and their revisions, conversations, memory, routines, skills, plugins, ACP agent settings, secrets, MCP setups, agents' folders and usage. ACP agents' own session files come too, where the agent keeps them on disk. Where it doesn't, the conversation carries on from a summary.
- **What doesn't move:** logins that belong to a machine, such as an ACP agent's Claude Code login. The new instance lists which agents need them, and the person logs in again there. Members' sandboxes move as their folders and are built again on the new server. A desktop instance has no sandboxes, so moving there turns them off.
- **How:** the new place runs `loom receive`, which prints a one-time move code. The old instance connects with it, pauses runs and routines, and streams everything over an encrypted connection: a snapshot of the database, and the folders as a zstd tar stream. The new instance checks it all, then takes over.
- **Secrets** travel encrypted with a key from the move code, as everything in the move does, never in plain text. The instance's own `secret.key` goes with them, sealed the same way, so they open at the new place, and the instance keeps its identity key too, so the apps can tell it's the same instance (`server/move.calx`).
- **The apps follow.** The old instance sends every device a move event, signed with its key, with the new address. The apps switch over, keep their device tokens and cache, and catch up from the same event number. A device that was offline finds the old address answering "moved to …" for 30 days, or until the owner shuts it down.
- **Backups are the same export**, written to a file, by hand or on a schedule: `loom export FILE` (a `.tar.zst` with a snapshot of the database, the secret key and the folders) and `loom import FILE` into an empty folder (`server/backup.calx`). `loom serve --backups DIR` makes one each day, outside the instance's folder, and keeps the newest 7.

## Importing from OpenBot and Grok Bot

What another agent app keeps on the instance's computer comes over in one go, from **Import** in the settings, `import.run` (an agent's asks first), or `loom import openbot|grokbot [PATH]`. On a server, PATH is a copy of the app's data folder. Only the owner imports, since it reads the computer's files ([protocol.md](protocol/protocol.md#commands)).

- **From another user, on a server.** OpenBot on a server runs as a user of its own, in a folder only it can read, and the `loom` service can't see it. `loom import openbot --from-user openbot`, as root, copies what an import reads into the instance's `imports/` (`server/stage.calx`): the database and shared tables as snapshots, safe while OpenBot writes, each agent's instructions, the skills, the files the chats have, but not ones that look like secrets, and the folders in the user's home that the agents' instructions, memories and skills name, each the git repo a path is in or the folder it names, up to 2 GB each. Those go in the person's shared folder, and what named them names them there; running it again on a server imported before brings them too. A repo's remote needs a key or token for the loom user, which the import says. The instance imports it for its owner, at once or once the owner has set up, so `loom install --import openbot` brings OpenBot over in the same step as the install.
- **Only reading, never secrets.** `server/importer.calx` reads the other app's files and opens its SQLite read-only, and never opens what holds a secret: API keys, vaults, logins, OAuth and webhook secrets, MCP servers' environment and headers, and files that look like keys. What needs a key gets it again in loom, as a secret.
- **The provider, after one yes.** The one exception: `import.scan` says which endpoint OpenBot's Claude uses (such as a proxy, from Claude Code's settings), never its token, and the app asks "Use the same for Claude Agent?". On yes, `import.run` copies the endpoint and token into the Claude agent program's settings itself, sealed, so no agent reads the token (`--carry-provider` from the command line).
- **Once.** Each thing is kept with the other app's id (`imported`, in the database), so importing again brings only what's new, and never over the person's own skill, MCP server or table of the same name.
- **Said plainly.** The answer counts what came and says, a sentence each, what couldn't or what changed on the way, such as a weekly routine that became daily.
- **OpenBot** (its `openbot.db`, an event log whose `projection_*` tables hold things as they are, and its workspace): agents with their `AGENTS.md` (or their title and description), nearest thread colour, model and pin, on the agent program named for their provider; each agent's chat with its messages, times, pins and the files they had that are still there, and the threads beside it as branches of it, or as their routine's own; memories; routines, on loom's schedules (hourly becomes every 60 minutes; weekly and monthly, daily at their time), their prompts naming loom's tools where they named OpenBot's (`openbot.routine_no_update` is `routine_quiet`, `openbot.routine_report_failure` is `routine_failed`, at each run too, for a routine imported before); skills, from its bundles and `~/.agents/skills`, as their `SKILL.md`; MCP servers, which every agent that comes has, as each had them all in OpenBot (agents an import made before that get them once, when the instance starts); and the shared tables, each its owner agent's, with their rows.
- **Grok Bot** keeps its agents on its own server, and only a cache on the computer: the roster, a recent stretch of each agent's chat, and the pins, as JSON blobs, which are told apart by what they hold, and MCP servers in `~/.grokbot/settings.json`. Those come. Instructions, memory, skills and automations aren't on the computer, and the answer says so; each agent's title and description become its instructions.

## Files and images

- **Both ways.** People send files and images to agents (drag and drop, paste, the share sheet on the iPhone), and agents send files back: screenshots, reports, exports.
- **Kept once.** Files are stored on the instance under their SHA-256, so the same file is never kept twice. Devices download a file when it's shown, and cache it.
- **Thumbnails** are made on the instance with std.image.
- **Pictures show as pictures.** In the composer, a picture waiting to go is a thumbnail. A file's name goes in the words where it was attached, as `[name]`, so the agent reads where it belongs, and the sent message shows it as a link to the file. A picture in the chat opens big inside the app, with the chat's other pictures a step away, and Open hands it to its own program. Two or more pictures together in a message, the person's or an agent's, are one stack of a fixed size however many there are: the first on top, up to two more peeking out behind it, and how many in its corner. It opens the viewer at the first, stepping through the stack's pictures, with each one's name and where it is among them ("1 / 3"). Their names leave the message's words where they only list them, and stay as links among other words.
- **Photos from the iPhone** go up as JPEG, since the models agents use don't take HEIC.
- **Space.** Files count toward each person's storage, which the owner can limit.

## Links and sources

- **Sources under the reply.** An agent that looked things up ends its reply with a Sources list of links. The instance takes the list out of the reply's words and keeps it with the message, and the apps show it as a tab on the bubble's bottom-left corner, hanging a little below it: the first three sites' icons and "5 sources", or the site itself when there's one. The tab is the bubble's colour, a shade lighter, with a faint edge and 8px corners; open, it's a shade darker than the bubble. A click opens the list under it, one row each: the site's icon, the title, and the site on the right, cut short when there's no room. A row opens the page in the browser; Esc, a click outside or the tab again closes it. On the iPhone a tap opens a sheet.
- **Links by their page.** A link in any message, the person's or an agent's, has its site's icon in front of it. A bare address shows as the page's title, underlined, and the whole address shows on hover; a link the agent wrote words for keeps them. Before the page is looked up, a bare address shows as it was written.
- **Icons as they are.** A site's icon is drawn as the site made it, with the small radius loom's chips have and nothing behind it.
- **Looked up by the instance.** The instance reads each page's title (its `og:title`, or else its `<title>`) and its site's icon (one the page names that loom can draw, or else `/favicon.ico`; a PNG, a JPEG or an ICO, kept at 64px as the person's file). It does so on a thread of its own, with a cap on size and time, and only for https pages of public hosts: a redirect never leaves https, so a lookup reaches only a host with a certificate for its name, never the person's router, their other computers or the instance itself. An address on a network of the person's own, or a name only it knows, isn't looked up. A page is looked up once while the instance runs.

## Mentions

- **@ in the composer.** Typing `@`, at the start or after a space, opens a picker over the composer: the person's other agents whose names match what follows, each with its thread and its name in its colour and the first line of its instructions, then the files that match in the agent's folder and the shared one, each with its icon, its name and its folder. Hidden folders, `node_modules`, `target` and `__pycache__` are left out. Up and down choose, Enter or Tab takes one, Esc shuts it, and a click takes one too. What's taken sits in the composer as one piece, which Backspace takes out whole.
- **Kept as links.** A mention goes in the message as a link: `[@Casper](loom:agent/a_2)` for an agent, `[hours.md](loom:file/a_1/notes%2Fhours.md)` for a file, its path as a reply would name it. So the message keeps who and what it meant, whatever is renamed later.
- **Read by the agent as words.** The agent reads `@Casper` and `` `notes/hours.md` ``, with a note telling it who each agent is and how to reach it (`message_agent` with its name), and that each file is in its folder or the shared one. The person can ask it to send another agent a message, hand over a file or read one, in their own words.
- **Shown in the chat.** An agent mentioned shows as its thread and its name in its colour, a little weathered toward the quiet ink, with no @ and nothing behind it. A file shows its icon and its name, underlined, which opens it. A file sent with a message shows the same way where its name is, with a tiny copy of itself in place of the icon when it's a picture.
- **Another agent's message.** A message one agent sends another with `message_agent` sits on the person's side of the chat, as the person's do, its bubble a breath of the sender's colour, with the sender's thread and name in its colour over it.

## Search

Full-text search over every conversation, with FTS5 as memory uses, filtered by agent and date.

## Side questions

Ask an agent about its chat without adding to it: "what did we decide about the deadline?", "what was that link?". The agent's menu has **Ask on the side**, which opens beside the chat. The agent answers in a conversation of its own for side questions, which no chat shows, with the chat's newest messages to go on, and with no tools that ask: any it tries are refused, without asking the person. The question and the answer are shown beside the chat, and neither goes in it.

## Pinned messages

A message worth keeping at hand is pinned in its chat, as in Discord: by the person, from the pin in its corner, or by an agent, with `run_command`. The pin is kept on the message, with when and who, so every device sees it. The chat's header shows how many there are, and opens them beside the chat, the newest pin first, each to jump to or take off.

An agent is pinned too, from its menu: pinned agents sit at the top of the sidebar as tiles, three across, each a rounded square with the agent's thread drawn large in its shape and its name under it, as OpenBot's pinned agents are. A tile shows what the agent's row would: its thread moves as the agent works, a dot in its corner is a reply not seen yet, and what it's on shows under the pointer. On the rail the pinned agents come first, with a short line under them. Otherwise agents stay where they are: nothing the person does with one moves it, and they drag them into the order they want, among the tiles, in the list or on the rail (`agent.order`). The pin and the order are kept on the instance, so every device has them, and neither is a revision of the agent. Every other agent is one line in the sidebar, its thread and its name, with what it's on (thinking, its tool, what it wants to do) quietly after the name while it works, so starting or stopping never moves a row.

## Branching a chat

To try another way from some point without losing the chat, branch it there: by the person, from the branch in a message's corner, or by an agent, with `run_command`. A branch is a conversation of its own with the same agent, marked with the chat and the message it goes on from, and named after that message's first words. The first time the agent answers in it, it's handed the chat up to that message (its 40 newest messages, up to 24,000 bytes), and nothing after it. The chat goes on as it was. The chat shows its branches under the messages they go on from. Opening one shows it in the chat's place with a strip over it, which goes back to the chat. Side questions don't branch.

## Security

- **What a model reads is data, not orders.** Web pages, mail and tool results can carry instructions meant to trick an agent. Nothing in them can approve a risky action or change a permission. Only a person can, in the app.
- **Secrets stay out of prompts.** Loom never puts a secret in a prompt or a transcript, only its name. The keys an ACP agent needs (its API key, its MCP servers' keys) go to that agent's process alone, and only for its own person. The agent's own shell could still read its own environment, which is one more reason a member's agents run in a sandbox that holds only that member's keys.
- **An activity log** of everything agents do: each tool call, what it changed, and the approval that allowed it. People see their own, and the owner sees the whole instance's.
- **Logins** are rate-limited by address and by account, and a revoked device token stops working at once.
- **Plugins** run sandboxed (panels) or as their own processes (tools), with only the permissions they were given.
- **Reporting a reply.** Any reply can be reported, as the App Store asks. The report goes to the instance's owner.

## Updates and versions

- **Desktop apps** update themselves from signed releases (an Ed25519 signature on each release, checked before the swap), with loom's own small updater. The iPhone app updates through TestFlight, and later the App Store.
- **loom's own installer** on Windows, `loom-setup.exe` ([desktop/setup](desktop/setup/README.md)), installs, updates and uninstalls, and looks and moves like loom: a small program drawn by hand in loom's look, not GPUI, so it's about a megabyte and opens at once. A release's `loom-setup.exe` carries `loom.exe` compressed, under a header the release key signs, which it checks before it installs anything. For an update the core downloads the new release's setup, the app hands it over and quits, and the setup swaps `loom.exe`, starts the new one, and puts the old one back if it doesn't start.
- **On a Mac** a release has `loom-mac.dmg`, a disk image holding `loom.app` and a link to Applications to drag it onto, for Apple silicon (`scripts/mac.calx`). The app is signed ad hoc, since loom has no Developer ID yet, so the first time macOS asks the person to open it from System Settings → Privacy & Security. For an update the core downloads the release's disk image (`macos-desktop` in the manifest) and checks it as on Windows. The app copies the new `loom.app` out of it, starts the new loom to put itself in place and quits; the new loom waits for the old one to end, moves the old bundle aside, puts itself where it was and starts from there, and if that loom ends with an error within ten seconds, the old bundle goes back and starts again (`desktop/src/swap.rs`, tested end to end by `tests/setup/mac.calx`). A loom that runs from its disk image, from the copy macOS runs of an app that was never moved out of Downloads (App Translocation), or from a folder it can't change can't update itself, and the update icon says why (`app_why` in the core's config).
- **Servers** update with `loom update`, which checks the release's signature and swaps the program, or from the app: `welcome` says the server's version, the core compares it with the newest release, and a server that's behind shows quietly, as a small ring on its yarn and a line in the server popup ("loom 0.2.0 · 0.2.1 is out"), with **Update** for an owner or admin. **The update icon.** While anything loom runs is behind (this app, or any server it's signed in to), a quiet update icon sits just left of the settings gear in the sidebar's foot. The app keeps looking while it runs, about every ten minutes, give or take a minute, asking with the last manifest's ETag so a release that didn't change costs a 304. A click updates everything this person may update: the servers first, each through its own core, and the app last, once each server's update has gone far enough to finish on its own (downloaded and waiting for a turn, restarting, or failed), since the app restarts through its setup. Hovering shows a small popup of each device that's behind, with its version, the new one and its own **Update**, or how its update stands, or, for one that can't update itself where it runs (in Docker, say), plainly why. The first core decides all of it (`core/updates.calx`); the app only draws it. A core whose server said it's restarting tries again every second, so the app is back on it as soon as it is. The server downloads and checks the release on a thread of its own, waits until nothing runs (or, with **Now**, stops what runs, which carries on after as after any restart), swaps its program and starts again: under systemd it exits and the unit starts the new one, and run by hand it starts the new one itself, on the same port. In Docker the image is pulled instead, and the instance in the desktop app updates with the app. The owner can let it update by itself: it looks every half hour, waits for a few quiet minutes, and tells the owner after with a notification. A release has the server for Linux on x64 and Arm, Windows, and macOS on Apple silicon, and a Docker image on GitHub's registry ([protocol.md](protocol/protocol.md#updating-the-instance)).
- **How a release is signed.** A tag `v…` builds every program, and `scripts/release.calx` writes `manifest.json` with each file's SHA-256 and `manifest.json.sig`, the Ed25519 signature of the manifest's exact bytes, with the release key (the CI secret `LOOM_RELEASE_KEY`; its public half is in `shared/release.calx`). The desktop core and `loom update` read the latest release's manifest, check the signature, read from the release the manifest names rather than through the latest link again, which can move to a new release between the two reads while one is published, and then each file's hash, and only then use it. The desktop app is installed per person (`%LOCALAPPDATA%\Programs\loom`), so its setup swaps `loom.exe` without asking for an administrator. On a Mac it's wherever the person dragged `loom.app`, Applications as a rule.
- **Protocol versions.** `hello` says which protocol versions the app speaks, and the instance answers with the one it will use. An instance supports its newest version and the one before, so an app a release behind still works, and the app says when it has to update.
- **The database** carries its schema version, and the instance upgrades it on start, after a backup. A move between instances on different versions goes through the same upgrade.

## Testing

- **Unit tests** in every Calx module, as `@test` functions run with `calx test`.
- **A fake ACP agent** that plays scripted turns (replies, tool calls, permission requests, crashes), so sessions, approvals and reconnects are tested without a real model or spending money.
- **Protocol tests** drive a real instance through the WebSocket, as Phase 1's test script does, and grow into the main suite.
- **End-to-end tests** run the real app against an instance with the fake ACP agent.
- **Benchmarks** (`tests/bench/`) time the server and the core with the fake agent: a long reply, short turns, catching up, and what comes before an agent's first word (a program starting, a new session, Code Mode's scripts and `loom mcp` itself). The desktop app built with `--features perf` times its own frames, so a change that should make loom faster is measured before and after.
- **CI** builds and tests the server, the library and the app on Linux, Windows and macOS on every push.

## Storage

One SQLite database on the server, with these tables: `users`, `invites`, `link_codes`, `devices` (with each device's public key), `device_grants`, `sandboxes`, `plugins`, `plugin_revisions`, `acp_agents`, `acp_settings`, `agents`, `agent_revisions`, `skills`, `mcp_servers`, `mcp_auth` (each MCP sign-in, sealed), `conversations`, `acp_sessions`, `messages` (content blocks as JSON, with FTS5), `files`, `runs`, `tool_calls`, `approvals`, `activity`, `reports`, `events`, `memories` (with FTS5, and the site each is tied to), `recordings` (the steps of a task the person recorded), `proposals` (what agents propose, until the person keeps or drops it), `routines`, `routine_runs` (each routine's newest runs), `push_tokens`, `secrets`, `usage`, `imported` (what came from another app, by its ids), `schema_version`. Files themselves sit in the data folder, named by their SHA-256.

What only grows is pruned. Events go after 30 days, keeping each person's newest 10,000 ([Sync](#sync-an-event-log-with-a-cursor)). Finished `runs`, the client ids of sent messages (`sent`) and the tokens turns used (`usage`) go with the events' age, but never before 35 days, since a month's budget counts usage, and a run still going never; the instance looks once an hour, 10,000 rows of each at most. The app's cache keeps a tool call as long as the events are kept, 30 days, and one of a run still going for as long as it goes.

The instance has the database open, and so do `loom` commands run beside it, its search and checkpoint threads, and its backups. Each connection waits up to 5 s for another's lock (std.sqlite's busy timeout). A transaction that writes takes the write lock as it begins (`BEGIN IMMEDIATE`), since one that read first and took the lock at its first write would fail at once with "database is locked" when another connection had written meanwhile, and every error path rolls its transaction back, so none is left holding the lock. The app's cache works the same way.

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
  site/       the website, loom.rip, and the user guides
  tests/      the fake ACP agent, protocol and end-to-end tests
  scripts/    Calx: build, release, install
```

**How the Calx code fits together.** The repository's root is the source root of every Calx module, so a file's module is its path: `server/main.calx` is `module server.main`, and imports `shared.protocol` from `shared/protocol.calx`. That needs no packages, so loom has no `calx.toml` until something outside the repository is a dependency (Calx has packages now, CLX-370). Scripts in `scripts/` stand alone. `calx scripts/check.calx` checks the formatting of every `.calx` file, checks each one builds, runs every file's tests and checks the design tokens are up to date. CI runs it on Linux, Windows and macOS on every push and pull request.

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
| An iOS target, for devices and the simulator, as a static library Xcode links | **Done**: `aarch64-ios` and `aarch64-ios-sim`, which the iPhone app links | [CLX-408](https://acta.aaenz.no/aaen-studios/clx#CLX-408) | The iPhone app. Started early, during Phases 3–4 (below) | 5 |
| Cross-compiling: building for another system or processor | **Done**: `calx build --target`, iOS from a Mac | [CLX-409](https://acta.aaenz.no/aaen-studios/clx#CLX-409) | iOS builds from a Mac, Linux arm64 and other release builds | 5 |
| std.net and std.http on iOS: kqueue, and URLSession since iOS has no libcurl | **Done** | [CLX-410](https://acta.aaenz.no/aaen-studios/clx#CLX-410) | The core on iPhone | 5 |
| ES256 (ECDSA P-256) signing | **Done** | [CLX-406](https://acta.aaenz.no/aaen-studios/clx#CLX-406) | APNs tokens | 5 |
| HTTP/2 in std.http | **Done** | [CLX-383](https://acta.aaenz.no/aaen-studios/clx#CLX-383) | APNs, which speaks only HTTP/2 | 5 |
| A library's panic reports to the host instead of ending the app | **Done** | [CLX-407](https://acta.aaenz.no/aaen-studios/clx#CLX-407) | A bug in the core shouldn't close the app | 5 |
| A static library for macOS that links with Rust | **Done**: `calx build --staticlib` on Apple silicon links into the Mac app as it is, checked 2026-11-23 | — | The Mac app | 6 |
| X25519 in std.crypto, and a Noise handshake built on it, Ed25519 and AEAD | **Done**: `X25519Key`, `NoiseHandshake` and `NoiseTransport` | [CLX-411](https://acta.aaenz.no/aaen-studios/clx#CLX-411) | End-to-end encryption to a desktop instance, and the push relay's sealed tokens | 6 |
| UDP sockets with multicast in std.net, and mDNS | **Done**: UDP in std.net, `std.net.mdns`, and `interfaces()` | [CLX-412](https://acta.aaenz.no/aaen-studios/clx#CLX-412) | Finding a desktop instance on the Wi-Fi when its address changes | 6 |
| Choosing the CPU a release build targets, such as x86-64-v2 | **Done**: the `--cpu` level | [CLX-381](https://acta.aaenz.no/aaen-studios/clx#CLX-381) | Installing loom on any VPS | 7 |
| Writing and reading tar as a stream, to and from a file or a connection | **Done**: `StreamWriter` and `StreamReader` over std.zstd | [CLX-413](https://acta.aaenz.no/aaen-studios/clx#CLX-413) | Exports and moves of many gigabytes | 7 |
| Waking a thread that waits on the network, a program or a socket, from another thread | **Done**: `waker()` in std.http.server (`std.net.Waker`), std.process and std.http.websocket | [CLX-437](https://acta.aaenz.no/aaen-studios/clx#CLX-437) | The instance's loop, agent programs and the core act on what comes at once, rather than looking every 10 ms: a turn's first word 31 ms → 1 ms | 1–2 |
| Waking a loop on a WebSocket's messages, a finished HTTP request or a program's output | **Done**: `wake_on(std.net.Waker)` on a WebSocket, a `Request` and a `Child` | [CLX-440](https://acta.aaenz.no/aaen-studios/clx#CLX-440) | The core's loop waits for its socket, requests, commands and the app at once, rather than looking every 10 ms: idle, about 43 turns a second → none | 2 |

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
| 6 | Mac, and phones on desktop instances | The GPUI app on macOS, with an instance in it too. The phone reaching a desktop instance: on the Wi-Fi with mDNS, and from anywhere through Tailscale, encrypted with Noise | — (all done) | The Mac app works, and a phone reaches an instance on a PC both at home and away |
| 7 | Moving and plugins | The panel break-out test first, then export, import and backups, moving an instance with the apps following, the install script, `loom update`, `loom` on the command line with JSON output, panels, tool plugins, agents writing plugins | Streaming tar (the CPU choice is done) | An agent installs loom on a fresh VPS and moves your instance there, and your apps follow without you touching them |
| 8 | Your own integrations | T3 Code threads, Acta, the VPS dashboard and deploys, built in | As they come up | It does things OpenBot can't |
| 9 | iPhone extras | Widgets, Live Activities in the Dynamic Island, Action Button, Siri and Shortcuts, Control Center. The calendar, reminders, contacts, Health and Shortcuts as device tools | As they come up | Whenever you want, one by one |
| 10 | For others | Inviting and managing people (invites, roles, budgets, member sandboxes), release builds for every platform, Docker image, docs, setting up agents and keys, a guide to Tailscale, the opt-in push relay with encrypted notifications, a guide to building the iPhone app with your own Apple account | — | Someone else runs their own instance, on a server or on their PC, and invites someone to it |
| 11 | Agents that do things | Can come before 8–10. The agent's browser, watching it and taking over, secrets filled unseen; queued and steered messages, durable tasks, quieter routines, a backup brain; rules on what's touched, the record before each action, limits on agents messaging agents, sandboxes for anyone; the shared folder, teaching a task ([Agents that do things](#agents-that-do-things)) | — (WebSockets, processes and JSON are there) | An agent signs up for something in its browser, asks you to log in partway, you take over from your phone, and it finishes |

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
- **What a web app shell gave for free is now small crates:** the tray (`tray-icon`), notifications (WinRT toasts on Windows, and `UNUserNotificationCenter` through `objc2` on macOS, which needs the app's bundle and plays the notification's sound with it, so Focus and the person's settings silence both), starting at login (`auto-launch`), screenshots, the clipboard, the updater. `tray-icon` pulls in GTK on Linux, which loom doesn't target.
- **Screen readers.** GPUI now has AccessKit on Windows, and `gpui-component`'s widgets set roles and labels. Loom's own controls are plain elements, which a screen reader doesn't see until each has an id, a `role` and an `aria_label`. A view drawn with `cached` leaves its nodes out of every frame that reuses it, so while a screen reader listens the chat is drawn at each frame. Check each screen with Narrator and VoiceOver as it's built.
- **Licences.** GPUI, `gpui-component` and GPUI Kit are Apache-2.0, which fits loom's MIT. Zed's editor and terminal crates are GPL, so they can't be used. A terminal panel would use `alacritty_terminal` (Apache-2.0).

### What the Windows spike showed (LOO-4)

The throwaway test that opens Phase 2: a stand-in core (since replaced by `core/lib.calx`) built with `calx build --staticlib` by `desktop/build.rs`, linked into a GPUI Kit window, streaming a Markdown reply through `on_event` from the core's own thread, below 3000 older Markdown messages.

- **It works.** The core's 71 deltas arrive over 1.1 s, and GPUI draws each one as it comes, with code blocks, lists and inline code.
- **Release build:** the window is up in about 0.55 s, the process holds about 85 MB, and the exe is 26 MB.
- **The composer** takes typing and Enter, sends to the core, and the list follows the new message. Dropping files gives their paths (`ExternalPaths`). Its + opens a small menu above it: attach files, start the message with one of the person's skills, or run one of the agent's routines now. Its mic dictates: on Windows it puts the focus in the composer and opens the system's voice typing (Windows+H), which types what's said there, so no audio goes through loom. On a Mac it does the same with macOS's own dictation, sending `startDictation:` along the responder chain as Edit → Start Dictation does, and GPUI's view takes what it types through `NSTextInputClient` (marked text, then the words committed). The tray icon and a notification come from the crates above, on GPUI's own message loop.
- **Not yet checked by hand:** typing with a real IME (Japanese, say), and Narrator. Both are on the Phase 2 desktop chat task. What was checked without them, in the app off screen: the UI Automation tree names the sidebar's agents, + and collapse, the server and the gear; the header's agent, brain and permission; each message, as a list item saying who said what, with its Pin and Branch; and the composer's field, +, voice typing and Send. The composer takes what an IME commits (`WM_IME_CHAR`) once and whole, and an Enter the IME keeps mid-word (`VK_PROCESSKEY`) doesn't send. Showing a word inline while it's composed needs a real IME on the focused window, since GPUI reads the composition from its own input context.
- **For the real core:** `loom_core_stop` must join the core's threads before it returns, so no `on_event` call comes after the app frees its side.

GPUI is ready for this on Windows, so the Tauri fallback isn't needed.

### What the Mac showed (LOO-262)

- **The same app.** The core's static library links on Apple silicon as on Windows, and the app is the same code: what's the Mac's own is `desktop/src/mac.rs` (AppKit through `objc2`: showing and hiding the window, where it goes, keeping the Mac awake) and `desktop/src/swap.rs` (the update). The window keeps macOS's traffic lights, in the middle of the title bar's height, with the knot and the wordmark after them. With the sidebar put away, the title bar crosses the window over the rail, as a Mac's does, with the control that opens the sidebar after the lights, and the rail begins under it; full screen, where the lights go, the rail is as on Windows (`app.rs`, `LIGHTS`). The app has a menu bar with loom's, Edit's and Window's usual items and keys, and closing the window keeps loom in the menu bar, as the tray does on Windows.
- **The agents' PATH.** An app opened from the Finder or at login gets launchd's short PATH, without Homebrew's or the person's own folders, so an agent program wouldn't be found. The app reads the PATH the person's login shell sets, once as it starts, and keeps it for everything it runs.
- **Starting at login** is a LaunchAgent (`~/Library/LaunchAgents/no.aaenz.loom.desktop.plist`), with `--background` so loom starts in the menu bar.
- **Notifications** need the app's bundle and its identifier, so a build run with `cargo run` doesn't show them. macOS asks the person once whether loom may notify.
- **Release build:** the disk image is about 17 MB.

## What happens to v0.1

Nothing from v0.1's code carries over: the engine was Rust with its own model loop, and the UI was React. It stays reachable at a tag (`v0.1`) for reference. Some of what it had may come back later in the new stack, each decided on its own:

- The real terminal, the editor and the git panel, as panels or as their own features.
- Computer use on Windows, as a device tool (see [Your devices](#your-devices)).
- Image generation, as an MCP tool rather than a provider feature.
- Semantic search over a coding workspace, if full-text search isn't enough.

**People on v0.1 are moved over by v0.1's own updater.** v0.1 looks at the latest release's `update.json` and takes a payload signed with its minisign key, which every release from v0.2.0 on carries (`scripts/release.calx bridge`, with the CI secret `LOOM_V01_KEY`): a zip whose `loom.exe` is that release's `loom-setup.exe`. v0.1 swaps it in and starts it, and the setup installs the new loom and takes v0.1 off, without running v0.1's own uninstaller, which would end the new loom and delete the folder they share. v0.1's chats and settings in `~/.loom` stay where they are; the new loom doesn't read them. Someone who downloads `loom-setup.exe` instead gets the same: v0.1 is taken off as the new loom goes in.

## Later ideas

- Group conversations, where several people and agents share one chat.
- A model loop of loom's own, as one more kind of agent, if ACP agents ever stop being enough.
- Agents controlling a PC's screen, mouse and keyboard, as a device tool.
- A web client: the core built for `wasm32`, running in a browser tab with nothing to install. It would need a UI of its own.
- Android: a Kotlin app on the same core, once Calx has an Android target.

## Open questions

- **The word for an instance.** "Instance" is a placeholder.
- **The panel break-out test** at the start of Phase 7 ([The panel sandbox](#the-panel-sandbox)).
- **The Calx checks** marked "Not documented" in [What Calx needs](#what-calx-needs).
