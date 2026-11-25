# In the chat

Each agent has one chat that never ends. Choose an agent in the sidebar to talk to it, and on the desktop start typing: the message box has the keys as a chat opens, and when you come back to the window. Its name over the chat opens its menu: Edit agent, Activity, Memory, Ask on the side, Move to…, Start fresh and Delete agent. Start fresh and Delete agent can't be undone, so each asks for a second click.

## The sidebar

The sidebar lists your group chats first ([Group chats](#group-chats)), then your agents in groups:

- **Waiting on you**, at the top, while an agent waits on a question or a permission. It goes back to its place once you've answered. A pinned agent keeps its tile instead.
- **Pinned agents**, as tiles, three across. **Pin to top** is in the agent's menu.
- **The rest**, a line each, with what an agent is on after its name while it works.
- **Folders** you make: **New folder** in the space under the agents, or **Move to folder** in an agent's menu. Click a folder's name to open or shut it, and right-click it to rename or delete it. Drag an agent onto a folder's name to put it in, or onto an agent outside to take it out. Deleting a folder keeps its agents.
- **Hidden**, at the bottom and shut until you open it: agents you hid with **Hide** in their menu. **Show** brings one back.

Your folders and hidden agents are kept on the server, so every device shows the same. On the iPhone, long-press an agent for Move to folder and Hide, and long-press a folder's name to rename or delete it.

When one agent messages another, they talk in a thread of their own, never in your chat with either. While the agent it went to is open, the thread is a row under it, "From Biscuit", which opens it in the chat's place, and the arrow at its top takes you back to your chat. On the iPhone, the chat's **•••** menu lists these threads. The thread is only read: where you'd write, **Continue in your own chat** opens your chat with the agent, ready to write.

## Back to the newest

When you scroll up in a chat, a round button with a down arrow shows at the bottom of the chat, over the message box. Click it to go down to the newest message. If messages came while you were up there, its corner says how many.

A chat you open with replies you haven't seen has a line over the first of them, "3 new". It stays until you leave the chat.

## Group chats

A group chat is a chat with several of your agents in it, where loom picks which one answers each message. **Everyone** has all your agents in it, and is there once you have two. To make another, choose **+** by **Group chats** in the sidebar, name it, pick two or more agents, and a colour if you want one. On the iPhone, the **+** is by **Group chats** in the agent list.

You just write. One agent answers each message: the one your message is for, or the one that answered last while you stay on the same subject. A line over each answer says who answered and why, "→ Ledger · money question", in that agent's shape and colour. To pick yourself, mention an agent: "@Hermes any new mail?" goes to Hermes, and the line says "you asked".

If the wrong agent answered, click the line and pick another. Your message goes to that one, and loom keeps it as an example: the next message like it goes to the agent you picked. An agent can also hand a message to another member when it's that one's to answer, "→ Hermes · handed off by Ledger".

Each answer is the agent's own, with its memory, tools and permissions. It sees what was said in the group chat since it last answered, by whom. Your chat with each agent stays as it was: nothing from a group chat goes into it, and a message pinned in a group chat is pinned there only.

Click the group chat's name over the chat to change its name, members, colour, and who answers when loom can't tell, the agent you pick or the one that answered last. Its **Corrections** are the examples you made, newest first; the × by one forgets it. **Usage** says how many tokens its agents' answers there used today and this month, and on its own line, how many loom's picking used. Everyone's members are always all your agents. Right-click a group chat in the sidebar to delete it; Everyone can't be deleted. On the iPhone, the people icon at the chat's top opens these, and swiping a correction left forgets it.

The picking is done by a small, fast model: Claude Haiku 5.5, unless you pick another in [Agent defaults](agents.md#agent-defaults). It's ready when you open the app or a group chat, so a message is picked for in about a second. What it reads to pick an agent is the agent's **What it's for** line ([Making an agent](agents.md#making-an-agent)), its skills and its MCP servers.

## Replying to a message

Point at a message, yours or an agent's, and choose **Reply** by its corner, or right-click it and choose **Reply**. With the message box empty, ↑ replies to the newest message. On the iPhone, swipe right on a message, or long-press it and choose **Reply**.

Select words in a message for a small bar with **Copy**, **Quote** and, in an agent's reply, **Ask about this**. These are in the right-click menu too. Quote and Ask about this quote only the words you selected. On the iPhone, long-press a message and choose **Select text**; the selection has Quote and Ask about this.

What you reply to shows over the message box, "Replying to Juniper" with its first line, until you send or take it away with its × or Esc. Once sent, your message shows it on one line over the bubble: who said it and the start of what. Click it to go back to the original, even far back in the chat. The agent gets what you quoted with your message, so it knows what you're answering.

## Finding in a chat

Press Ctrl+F (⌘F on a Mac) to find words in the open chat. A small box opens at the top right of the chat, and every match is marked in yellow, the one you're on brighter, with where it is ("3 of 7"). It finds in the whole chat, however far back, and starts on the newest match. Enter goes back to the one before, Shift+Enter on to the next, and so do the arrows in the box. Esc or × closes it and the marks go.

To search every chat at once, press Ctrl+Shift+F (⌘⇧F) for the search in the sidebar.

## Cards in replies

An agent can put cards in its reply, drawn in the chat's own look:

- **Choices**: two to six answers to the agent's question, as tiles. Click one, or on the desktop press Ctrl (⌘ on a Mac) and its number, and it goes as your next message. The tiles answer only while that reply is the newest in the chat.
- **Form**: several questions at once, each a box to type in, a choice, a switch or a number. Fill them in and choose **Send**, and your answers go as one message. **Send** says which question still needs an answer.
- **Checklist**: up to 30 items, each done or not.
- **Table**: up to 8 columns and 50 rows.
- **Progress**: a bar with a label.
- **Facts**: up to 12 labels, each with its value.

Choices and forms come at the end of the reply, once the agent has finished it, so you read the whole reply before you answer. An agent can put a form where it is in the reply instead, which says "Opens once the reply is done" until the reply is finished. A card can change while the agent works: a checklist ticks itself off, and a bar fills. The finished reply keeps each card as it was last.

When one of the agent's MCP servers needs you to sign in, loom puts its own card in the reply, "Sign in to acta", with **Sign in** ([MCP servers and secrets](agents.md#mcp-servers-and-secrets)). The agent is told to wait for you rather than try to sign in itself. **Sign in** works from the iPhone app too.

Loom puts its own card in the reply, too, when an agent wants to add an MCP server for you ([MCP servers and secrets](agents.md#mcp-servers-and-secrets)), or needs a password for a site it's browsing ([Secrets](browser.md#secrets)).

Anything bigger, such as a report or a chart, comes as a page in the chat instead, which opens beside it.

## Pictures

Two or more pictures in one message, yours or an agent's, are one fan: the first in front, and up to four more tilted behind it, which spread out when you point at them. Its corner says how many there are, all of them ("7"). Click it to open the viewer at the first, and step through them with the arrows. A single picture shows as it is.

In the viewer, the wheel zooms toward the pointer and dragging moves the picture around. Double-click to go between fitting the window and the picture's own size; + and − zoom, and 0 fits it again. On a trackpad, pinch to zoom, and once zoomed in, swipe with two fingers to move it. On the iPhone, pinch to zoom, drag to move it, and double-tap to zoom in or out; zoomed in, a swipe moves the picture rather than going to the next one.

## Deleting a message

Point at a message, yours or an agent's, and choose **Delete** from the **⋯** by its corner, or right-click it and choose **Delete**. On the iPhone, long-press it and choose **Delete**. It goes at once, for every device, and leaves a quiet line where it was: "You deleted a message", or "Juniper's message was deleted", so replies to it still make sense. A reply that quoted it says the original was deleted. With your next message, the agent is told the message is gone, and doesn't see it again.

## Long messages and tables

A message longer than about ten lines is cut, with **Show more** under it to read the rest in place and **Show less** to fold it again.

Under a table in an agent's reply, a quiet line says how many rows it has, with **Copy**, which copies it as CSV for a spreadsheet. Its arrow has **Copy as CSV**, **Copy as Markdown** and **Copy as text**.

## Files in the message box

What you attach shows in the message box as it will in the chat: over your words, a picture as itself, two or more as their fan, any other file as its name and size, each with an ×; in your words, each file where you put it, as its tiny picture and its name, or only the tiny picture when it's next to another file. Backspace takes a file out whole, and Ctrl+Z (⌘Z) brings it back. A long name is cut in the middle, keeping its start and its type; point at it to see all of it.

## Files from agents

An agent puts the files it makes in its reply: pictures, PDFs, documents, spreadsheets, sound, video and zips, and screenshots it took in its browser or on your computer. A file it made in that reply and names by its path goes in by itself. Each is kept as your file, and counts toward your space.

A picture a reply names that was in the agent's folder or the shared one already shows in the reply as a picture too, without being copied into your files until you open it in the viewer.

## Links and sources

A link in any message, yours or an agent's, has its site's icon before it, and a bare address shows as its page's title. Hover over it on the desktop to see the whole address.

When an agent ends a reply with a list of its sources, the list comes off the reply and onto a tab under it, with the sites' icons and how many there are. Click the tab for the list, a line each with the page's title and its site, and click a line to open the page. On the iPhone, the list opens as a sheet.

Paste a link on its own into the message box and it shows as it will in the chat: its site's icon and its page's title in your words, never as an attachment. On a computer, point at it to see its preview, with the page's picture, and click it to open it. A YouTube link shows the video's title; a page with no title of its own shows its address. Ctrl+Z right after makes it plain text again, and Ctrl+Shift+V (⌘⇧V on a Mac) pastes it as plain text to begin with. A plain link, or one you type, stays as you wrote it. The agent gets the address either way.

Loom looks the pages up itself, for their titles and icons, and a pasted link's picture. It looks up only https pages on the internet, never one on your own network, and at most twelve a message.

## Mentions

Type `@` in the message box to pick another of your agents, or a file in this agent's folder or the shared one. Keep typing to narrow the list, use the arrows and Enter or Tab to take one, or click it. Esc shuts the list.

In the chat and in the message box, an agent you mention shows as its thread and its name in its colour, which opens its chat, and a file as its icon and its name, which opens it. The agent knows who and what you mean, so you can ask it to message another agent for you, hand over a file, or read one. A file you send shows the same way, with a tiny copy of itself when it's a picture.

In their thread, a message one agent sends another sits on your side, tinted with the sender's colour, with its thread and name over it.

## The + menu

The **+** before the message box, or `/` in an empty one, opens a menu with a search at the top:

- **Attach files** (Ctrl+U, or Cmd+U on a Mac), **Paste image** when you've copied one, and **Recent files**, the files sent or received lately in any of your chats, which go again without uploading.
- **Skills**: pick one and it sits in your message, as a mention does. The agent reads it before it answers.
- **Routines**: pick one to run it now, after **Run**.

Type to find a file, skill or routine in any of them. The arrows move, → opens a list and ← goes back, Enter takes a row, and Esc shuts the menu. Each list ends with a link to its settings page.

On an iPhone, **+** opens a sheet with the same lists, and your newest photos in a row at the top.

## Emoji

Type a colon and the start of a name, such as `:so`, and a row of matching emoji opens over the message box, your own first. The arrows move through it, Tab or Enter takes one, a click takes one, and Esc shuts it. Typing the whole name with its closing colon, `:sob:`, turns it into 😭 straight away. Nothing is turned inside `code`.

The smiley beside the microphone opens the picker: search at the top, a tab for each group (Recently used, Your emoji, then Unicode's), and the emoji under the pointer big at the bottom, with its names and the skin-tone hand. Shift+click takes one and keeps the picker open. Right-click the smiley for **Hide emoji button**. Names and **Emoji** in the + menu still work without it.

A message of only one to three emoji shows them big. Under the pointer an emoji shows its name. Right-click one of your own for **Copy name**.

Your own emoji are pictures you add in **Settings → Emoji**. They're kept on the instance, so all your devices have them. You send one as `:its_name:`, and agents read it that way. When an agent writes an emoji's name, it shows as the emoji. Built-in emoji go as the emoji itself. loom draws them with [Twemoji](https://github.com/jdecked/twemoji), the set Discord uses, so they look the same on Windows, a Mac and an iPhone.

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

Each routine's **History**, on its page under **Routines**, says how every run ended (posted, quiet, failed, or skipped and why) and what the agent posted or answered ([Settings](settings.md#routines)).

## A backup program

An agent runs on an agent program, its brain. When that program runs out of usage or hits a rate limit, or doesn't start, a backup program can take the turn over. The reply says so first, in one line, such as "Claude Code ran out of usage, so Codex answers this chat now."

Pick an agent's **Backup program** on its edit page, under **Brain**: the default, None, or another of the instance's programs. The default backup is on the Defaults page.

The chat stays on the backup from then on, since usage limits last hours and only you know when yours is back. Move it back yourself, as below.

## Moving a chat to another program

**Move to…** in the agent's menu lists the programs on the instance, with the one the chat runs on ticked. Pick another, and the next reply says the chat moved, in one line.

The new program starts with loom's own summary of the chat: what you asked, oldest first, then the newest of the chat word for word. It's made from the chat itself, not by a model, so it costs nothing and works even when the old program can't answer. The agent can search the chat for anything older.

On another program, the agent runs on your defaults for that program's model and effort, not its own picks, which are for its own program.
