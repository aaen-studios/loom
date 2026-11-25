# In the chat

Each agent has one chat that never ends. Choose an agent in the sidebar to talk to it, and on the desktop start typing: the message box has the keys as a chat opens, and when you come back to the window. Its name over the chat opens its menu: Edit agent, Activity, Memory, Ask on the side, Move to…, Start fresh and Delete agent. Start fresh and Delete agent can't be undone, so each asks for a second click.

## Cards in replies

An agent can put cards in its reply, drawn in the chat's own look:

- **Choices**: two to six answers to the agent's question, as tiles. Click one, or on the desktop press Ctrl (⌘ on a Mac) and its number, and it goes as your next message. The tiles answer only while that reply is the newest in the chat.
- **Form**: several questions at once, each a box to type in, a choice, a switch or a number. Fill them in and choose **Send**, and your answers go as one message. **Send** says which question still needs an answer.
- **Checklist**: up to 30 items, each done or not.
- **Table**: up to 8 columns and 50 rows.
- **Progress**: a bar with a label.
- **Facts**: up to 12 labels, each with its value.

Choices and forms always come at the end of the reply, once the agent has finished it, so you read the whole reply before you answer. A card can change while the agent works: a checklist ticks itself off, and a bar fills. The finished reply keeps each card as it was last.

When one of the agent's MCP servers needs you to sign in, loom puts its own card in the reply, "Sign in to acta", with **Sign in** ([MCP servers and secrets](agents.md#mcp-servers-and-secrets)). The agent is told to wait for you rather than try to sign in itself. **Sign in** works from the iPhone app too.

Loom puts its own card in the reply, too, when an agent wants to add an MCP server for you ([MCP servers and secrets](agents.md#mcp-servers-and-secrets)), or needs a password for a site it's browsing ([Secrets](browser.md#secrets)).

Anything bigger, such as a report or a chart, comes as a page in the chat instead, which opens beside it.

## Pictures

Two or more pictures in one message, yours or an agent's, are one fan: the first in front, and up to four more tilted behind it, which spread out when you point at them. Its corner says how many there are, or past five, how many more ("+2"). Click it to open the viewer at the first, and step through them with the arrows. A single picture shows as it is.

## Files from agents

An agent puts the files it makes in its reply: pictures, PDFs, documents, spreadsheets, sound, video and zips, and screenshots it took in its browser or on your computer. A file it made in that reply and names by its path goes in by itself. Each is kept as your file, and counts toward your space.

A picture a reply names that was in the agent's folder or the shared one already shows in the reply as a picture too, without being copied into your files until you open it in the viewer.

## Links and sources

A link in any message, yours or an agent's, has its site's icon before it, and a bare address shows as its page's title. Hover over it on the desktop to see the whole address.

When an agent ends a reply with a list of its sources, the list comes off the reply and onto a tab under it, with the sites' icons and how many there are. Click the tab for the list, a line each with the page's title and its site, and click a line to open the page. On the iPhone, the list opens as a sheet.

Loom looks the pages up itself, for their titles and icons. It looks up only https pages on the internet, never one on your own network, and at most twelve a message.

## Mentions

Type `@` in the message box to pick another of your agents, or a file in this agent's folder or the shared one. Keep typing to narrow the list, use the arrows and Enter or Tab to take one, or click it. Esc shuts the list.

In the chat, an agent you mention shows as its thread and its name in its colour, and a file as its icon and its name, which opens it. The agent knows who and what you mean, so you can ask it to message another agent for you, hand over a file, or read one. A file you send shows the same way, with a tiny copy of itself when it's a picture.

A message one agent sends another sits on your side of the chat, tinted with the sender's colour, with its thread and name over it.

## The + menu

The **+** before the message box, or `/` in an empty one, opens a menu with a search at the top:

- **Attach files** (Ctrl+U, or Cmd+U on a Mac), **Paste image** when you've copied one, and **Recent files**, the files sent or received lately in any of your chats, which go again without uploading.
- **Skills**: pick one and it sits in your message, as a mention does. The agent reads it before it answers.
- **Routines**: pick one to run it now, after **Run**.

Type to find a file, skill or routine in any of them. The arrows move, → opens a list and ← goes back, Enter takes a row, and Esc shuts the menu. Each list ends with a link to its settings page.

On an iPhone, **+** opens a sheet with the same lists, and your newest photos in a row at the top.

## How an agent uses its tools

Agents use their tools by writing small scripts, in Code Mode. Instead of being given every tool up front, an agent looks up the tools it needs and writes a script that calls them, so its context stays small. Only what the script gives back reaches the agent: it can read a whole document and pass on three lines.

What you see:

- **The same approvals.** Each tool call in a script is put to the agent's permission mode and your rules ([Permissions](permissions.md)), as if the agent had called it itself. A call that needs your yes asks you while the script waits, and a no stops that call.
- **In Asks first,** you also see each script before it runs, as **Run a script**, with the script as the agent wrote it, and say yes or no. In the other modes, scripts run at once and only their calls are checked.
- **The tool calls** show as your Chat settings say ([Settings](settings.md#chat)).

The scripts run with no network, no files and no programs, and their only way out is loom. They never see a key: the MCP servers get their settings and secrets, and the script gets only what they answer. A script stops after 60 seconds of its own time, or 100 tool calls.

The first script waits a moment, while the instance downloads Deno, which runs them.

The tools that reach your app and devices are given to the agent directly as well: adding a server, the command that installs one, your devices, cards in replies, and searching your chats. An agent setting loom up for you doesn't have to look them up first.

Code Mode is on for every agent. To give an agent each tool plainly instead, turn off **Code Mode** on its edit page, under **Knows**. The change starts with the agent's next message, in a fresh session, so its tools never change in the middle of one.

## A message while it answers

A message to an agent that's busy waits its turn, in a queue above the composer, where you can edit, move up or drop it. The lightning bolt steers: it stops the running turn and sends the message now, and the agent carries on with it in mind.

To have every message steer at once, set **A message while it answers** on the agent's edit page, or on the Defaults page for every agent.

## Routines in the chat

A routine runs out of sight: its prompt and the agent's answer stay out of the chat. The agent posts in the chat only when there's something to tell you, with "From the routine" and the routine's name over the post, and with **Notify me** on, the post comes as a notification too. A run that fails shows the agent's answer, so you see why.

A routine due while the agent is answering you waits until it's done, and until the messages you queued have gone. If the chat is still busy after 10 minutes, or the routine's next time comes first, that run is skipped.

Each routine's **Runs**, on its page under **Routines**, say how every run ended (posted, nothing new, failed, or skipped and why) and what the agent posted or answered.

## A backup program

An agent runs on an agent program, its brain. When that program runs out of usage or hits a rate limit, or doesn't start, a backup program can take the turn over. The reply says so first, in one line, such as "Claude Code ran out of usage, so Codex answers this chat now."

Pick an agent's **Backup program** on its edit page, under **Brain**: the default, None, or another of the instance's programs. The default backup is on the Defaults page.

The chat stays on the backup from then on, since usage limits last hours and only you know when yours is back. Move it back yourself, as below.

## Moving a chat to another program

**Move to…** in the agent's menu lists the programs on the instance, with the one the chat runs on ticked. Pick another, and the next reply says the chat moved, in one line.

The new program starts with loom's own summary of the chat: what you asked, oldest first, then the newest of the chat word for word. It's made from the chat itself, not by a model, so it costs nothing and works even when the old program can't answer. The agent can search the chat for anything older.

On another program, the agent runs on your defaults for that program's model and effort, not its own picks, which are for its own program.
