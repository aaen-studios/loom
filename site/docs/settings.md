# Settings

The gear at the bottom of the sidebar opens Settings, as a popup over the chat. Its pages are down the left, under three headings:

- **You**: About you, Appearance, Chat, and App & notifications.
- **Agents**: Defaults, Rules, Brains, Skills, Proposals, MCP servers, Secrets and Plugins.
- **Work**: Routines and Tasks.

Import sits under them. Click outside the popup, or press Esc, to close it.

The search at the top finds a page by its name or by what's on it: "dark" finds Appearance, and "steer" finds Defaults.

What's about the server rather than you (Add a device, People, Move, Add a server) is in the server popup instead. Click the server's name at the bottom of the sidebar to open it.

## This device's settings

Appearance, Chat, and App & notifications belong to the device you set them on, not to your account. They're kept in `prefs.json` in the app's data folder, so your laptop and your desktop can each be set their own way.

Everything else in Settings is kept on the instance, so every device you sign in on has it.

## Appearance

- **Theme**: System (it follows Windows or macOS), Light or Dark.
- **Chat text**: Small, Default or Large.
- **Fonts**: the words in loom's own font, Geist, or the system's, and code in Geist Mono or the system's monospace font.

## Chat

Under **Messages**:

- **Chat width**: Normal, Comfortable or Full width, how wide the chat's column grows. Normal is the default.
- **Agent replies**: **Bubbles**, where an agent's answer sits in a grey bubble, or **Plain**, as text in the column. Bubbles are the default.
- **Vibrant bubbles**: your messages sit in the agent's own colour, light in the light theme and deep in the dark, rather than a touch of it. On by default.
- **Tool calls**: how much of an agent's work a reply shows. **Hidden** shows none, not even one that failed: the agent tells you itself when a failure matters. **Folded** puts each run of calls on one line, which opens to show each. **Every call** shows each on its own line. Hidden is the default.
- **Thoughts**: whether a reply shows an agent's thinking. **Hidden** shows none, **Folded** puts each under Thought, to open, and **Open** shows each open. Hidden is the default.

However much of its work a reply shows, a finished turn reads as one answer: the agent's last words, with what it said and did on the way folded under one line, such as "3 steps", that opens with a click. While it works, one quiet line says what it's doing. A turn you stopped, or one that failed, keeps everything it had in view.

Under **Composer**:

- **Send with Ctrl+Enter**: Enter makes a new line, and Ctrl+Enter sends. Off by default, so Enter sends and Shift+Enter makes a new line.

Under **Sidebar**:

- **Search in the sidebar**: a search box over the agents. Off by default.

## App & notifications

Under **This computer**:

- **Start at login**: loom starts when you log in, in the tray.
- **Keep running when closed**: closing the window keeps loom in the tray, so routines and replies go on. On by default. Turn it off and closing the window quits loom.
- **Keep the computer awake while an agent works**: the computer doesn't sleep while one is working.

Under **Notifications**:

- **Notifications**: loom tells you when an agent says something while you're away.
- **Show what was said**: a notification shows the message. Turn it off and it says only who it's from.
- **Play a sound**.

Under **Updates**:

- Loom's version, and **Check now**. Loom also looks for a new version every few hours.
- **Update on its own**: a new version goes in by itself, while loom is in the tray and no agent is working. Otherwise the sidebar offers it, to restart into.
