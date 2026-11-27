# Settings

The gear at the bottom of the sidebar opens Settings, as a popup over the chat. Its pages are down the left, under three headings:

- **You**: About you, Appearance, Chat, Emoji, App & notifications, and Voice.
- **Agents**: Defaults, Rules, Brains, Skills, Proposals, Spools, MCP servers, Secrets and Plugins.
- **Work**: Routines and Tasks.

Import sits under them. Click outside the popup, or press Esc, to close it. Drag its bottom-right corner to make it bigger or smaller, and it opens at that size next time.

Brains, Skills, Proposals, MCP servers and Routines list theirs down the left, with the one you pick beside them. What you change in one stays, with a dot on its row, until you **Save** or **Cancel**, even if you pick another or close Settings. A new name in an MCP server's or a skill's form renames it.

The search at the top finds a page by its name or by what's on it: "dark" finds Appearance, and "steer" or "group chats" finds Defaults.

What's about the server rather than you (Add a device, People, Move, Add a server) is in the server popup instead. Click the server's name at the bottom of the sidebar to open it.

## This device's settings

Appearance, Chat, App & notifications, the options under your emoji in Emoji, and Voice's Shortcut and Hold to talk belong to the device you set them on, not to your account. They're kept in `prefs.json` in the app's data folder, so your laptop and your desktop can each be set their own way.

Everything else in Settings is kept on the instance, so every device you sign in on has it.

## Appearance

- **Theme**: System (it follows Windows or macOS), Light or Dark.
- **Chat text**: Small, Default or Large.
- **Fonts**: the words in loom's own font, Geist, or the system's, and code in Geist Mono or the system's monospace font.

## Chat

Under **Messages**:

- **Chat width**: Normal, Comfortable or Full width, how wide the chat's column grows. Normal is the default.
- **Agent replies**: **Bubbles**, where an agent's answer sits in a grey bubble, or **Plain**, as text in the column. Bubbles are the default.
- **Vibrant bubbles**: your messages sit in the agent's own colour, light in the light theme and deep in the dark, rather than a touch of it. Off by default.
- **Tool calls**: how much of an agent's work a reply shows. **Hidden** shows none, not even one that failed: the agent tells you itself when a failure matters. **Folded** puts each run of calls on one line, which opens to show each. **Every call** shows each on its own line. Hidden is the default.
- **Thoughts**: whether a reply shows an agent's thinking. **Hidden** shows none, **Folded** puts each under Thought, to open, and **Open** shows each open. Hidden is the default.
- **Show context**: the ring in the chat's header that fills with the agent session's context, its card, and the lines across the chat where a session was compacted or started fresh. Off by default ([How full the chat is](chat.md#how-full-the-chat-is)).

However much of its work a reply shows, a finished turn reads as one answer: the agent's last words, with what it said and did on the way folded under one line, such as "3 steps", that opens with a click. While it works, one quiet line says what it's doing. A turn you stopped, or one that failed, keeps everything it had in view.

Under **Composer**:

- **Send with Ctrl+Enter**: Enter makes a new line, and Ctrl+Enter sends. Off by default, so Enter sends and Shift+Enter makes a new line.

Under **Sidebar**:

- **Search in the sidebar**: a search box over the agents that finds words in every chat. Off by default; Ctrl+Shift+F (⌘⇧F on a Mac) opens it either way.

## Emoji

**Your emoji** are pictures you add to use as emoji, up to 200. **Add** takes a PNG or a JPEG of up to 256 KB. It's made 128 × 128 on the instance and named after its file. Click a name to change it: 2 to 32 letters a to z, digits and `_`. Point at one and click × to delete it, and **Undo** brings it back. Moving GIFs, WebP and animated PNGs can't be added yet.

The rest belong to this device:

- **Play animated emoji**: Always, or On hover. Neither plays while the system's reduce motion is on.
- **Skin tone**: the tone emoji that have one come in, from six hands.
- **Turn :) into 🙂**: an emoticon followed by a space becomes its emoji. Off by default.
- **Show the emoji button in the composer**. On by default.

## App & notifications

Under **This computer**:

- **Start at login**: loom starts when you log in, in the tray.
- **Keep running when closed**: closing the window keeps loom in the tray, so routines and replies go on. On by default. Turn it off and closing the window quits loom.
- **Keep the computer awake while an agent works**: the computer doesn't sleep while one is working.

Under **Notifications**:

- **Notifications**: loom tells you when an agent says something while you're away.
- **Show what was said**: a notification shows the message. Turn it off and it says only who it's from.
Under **Sounds**:

- **Sounds**: loom, System (the system's notification sound) or Off.
- **Volume**.
- A row for each sound, with a play button to hear it and a switch:
  - **Reply arrived**: an agent's reply is done. On.
  - **Waiting on you**: an agent asks first, or needs you in its browser. On.
  - **Another agent answered**: a reply in a group chat, with the same sound as Reply arrived. On.
  - **Routine posted**: a routine posted in its chat. Off.
  - **Routine failed**: a routine failed, or turned itself off. On.
  - **Message sent**. Off.
  - **Dictation on and off**: as dictation from the composer's mic starts and stops ([Voice](#voice)). On.
- **Silent while the chat is in front**: no sound for the chat you're looking at. On.
- **Quiet hours**: from and to, 22:00 to 07:30 to start with, and their switch. In quiet hours only your own Message sent and dictation sound. Off.

On Windows, loom's sounds stay silent while Focus Assist or Do Not Disturb is on. On a Mac and an iPhone, Focus silences them as it does any app's. To quiet one agent everywhere, its ⋯ menu has **Mute sounds**; a muted agent has a bell with a line through it in the sidebar, and its notifications still come.

Under **Updates**:

- Loom's version, and **Check now**. Loom also looks for a new version every few hours.
- **Update on its own**: a new version goes in by itself, while loom is in the tray and no agent is working. Otherwise the sidebar offers it, to restart into. A version that didn't start when it went in isn't put in again by itself: the update icon still offers it, saying why it didn't start, and a newer version goes in by itself as usual.

Under **Credits**: the emoji are [Twemoji](https://github.com/jdecked/twemoji), © Twitter, Jason Sofonia and Justine De Caires, under [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/).

## Voice

The composer's mic is loom's own dictation: click it and your words come into the message box as you say them, and click it again to stop. Replies can be read aloud too. Both use a service you have a key for, and **Dictation** and **Speech**, beside the page's title, each list theirs as tiles. The one in use is outlined, with a check and **In use**. **Use** picks another, and **Add key** asks for its key, masked, with an eye to show it; **Change key** replaces it. A key is kept with your secrets on the instance, sealed, so it works on every device of yours. With no service set up, the mic opens this page.

Under **Dictation**:

- **ElevenLabs Scribe**, **Deepgram Nova-3** and **OpenAI** (gpt-4o-transcribe) write the words as you say them. **Groq** (Whisper large v3) writes them once you stop.
- **Language**: Detect, or one of twenty.
- **Shortcut**: Ctrl+Shift+Space starts and stops dictation, or Off.
- **Hold to talk**: the mic records while you hold it, and stops when you let go. Off.

Under **Speech**:

- **ElevenLabs** (Flash v2.5, your own voices and the library's), **Fish Audio** (S2.1 Pro, the voice library) and **OpenAI** (gpt-4o-mini-tts, 13 voices).
- **Voice**: one of the service's voices, and its play button to hear it.
- **Speed**: 0.8× to 1.2×. 1× to start with.
- **Read replies aloud**: Never, Only in live, or Always, which reads a reply aloud as it comes in the chat in front of you. Never to start with.

An agent can set these up for you: its card in the reply has the same tile and rows, and asks for a key there when the service needs one it doesn't have. Agents never see a key.

## Routines

A routine sends an agent a prompt on a schedule, when something posts to its webhook, or when something happens on an Acta board or a T3 thread. The page shows the week over your routines: days across, hours down, each routine a small block in its agent's colour at its times, and one that runs every few hours as thin ticks. Today stands out, and the arrows go to other weeks. Click a block, or a routine, to open it.

On a routine's page, **Repeat** sets when it runs:

- **Every few hours**: "Every 2 hours, from 08:00 to 20:00", on the days you pick. A routine an agent set every 30 minutes shows minutes here.
- **On days**: the days, M T W T F S S, and its times, one or more. **+ Time** adds one; × takes one off. "Weekdays" or "Every day" shows beside the days when that's what's picked.
- **Webhook**: only when something posts to its webhook. Its address and secret show under **Options** once it's saved. A routine on a schedule can have one too, with **Webhook too**.
- **On an event**: when something happens on your Acta or T3, which shows once you have an Acta board or a T3 server connected ([MCP servers and secrets](agents.md#mcp-servers-and-secrets)). **On** picks Acta or T3 when you have both, and **Server** which one when you have several. For Acta, pick the **Board**, then the **Event**: **Moves to** a **Status** (Todo, In progress, Review, Done or Canceled), **New card**, or **Comment**. For T3, type the **Thread**'s id; the routine runs when its run finishes. The list says it in words: "When a LOO card moves to Done". What happened comes after the routine's prompt.

Times are in your time zone, summer time included. Under them, **Next** says when it runs the next three times.

Under the prompt, **Gate** is a command that runs first, with the seconds it may take: when it prints something, the agent wakes with that after the prompt, and when it prints nothing, the run ends quiet without waking the agent. **Model** and **Effort** pick what its runs use, from what its agent's program offers, the agent's own marked default. Under its name, what it costs, from the last 7 days: "1.4M tokens a run · 5.4M a day · mostly quiet", in orange when it spends a lot for nothing, as its row in the list says too.

Its **History** has a strip of its last 30 runs, each coloured by how it ended: green posted, grey quiet, red failed, orange skipped. Under it is every run: when, how it ended, the first line of what it posted or why it failed or was skipped, and how long it took. Click a run, or its mark, to see what the agent posted and answered, or why it failed and what to do about it.
