# In the chat

Each agent has one chat that never ends. Choose an agent in the sidebar to talk to it. Its name over the chat opens its menu: Edit agent, Activity, Memory, Ask on the side, Move to…, Start fresh and Delete agent.

## Cards in replies

An agent can put cards in its reply, drawn in the chat's own look:

- **Choices**: two to six answers to the agent's question, each a button. Click one and it goes as your next message. The buttons answer only while that reply is the newest in the chat.
- **Checklist**: up to 30 items, each done or not.
- **Table**: up to 8 columns and 50 rows.
- **Progress**: a bar with a label.
- **Facts**: up to 12 labels, each with its value.

A card can change while the agent works: a checklist ticks itself off, and a bar fills. The finished reply keeps each card as it was last.

Anything bigger, such as a report or a chart, comes as a page in the chat instead, which opens beside it.

## How an agent uses its tools

Agents use their tools by writing small scripts, in Code Mode. Instead of being given every tool up front, an agent looks up the tools it needs and writes a script that calls them, so its context stays small. Only what the script gives back reaches the agent: it can read a whole document and pass on three lines.

What you see:

- **The same approvals.** Each tool call in a script is put to the agent's permission mode and your rules ([Permissions](permissions.md)), as if the agent had called it itself. A call that needs your yes asks you while the script waits, and a no stops that call.
- **In Asks first,** you also see each script before it runs, as **Run a script**, with the script as the agent wrote it, and say yes or no. In the other modes, scripts run at once and only their calls are checked.
- **The tool calls** show as your Chat settings say ([Settings](settings.md#chat)).

The scripts run with no network, no files and no programs, and their only way out is loom. They never see a key: the MCP servers get their settings and secrets, and the script gets only what they answer. A script stops after 60 seconds of its own time, or 100 tool calls.

The first script waits a moment, while the instance downloads Deno, which runs them.

Code Mode is on for every agent. To give an agent each tool plainly instead, turn off **Code Mode** on its edit page, under **Knows**.

## A message while it answers

A message to an agent that's busy waits its turn, in a queue above the composer, where you can edit, move up or drop it. The lightning bolt steers: it stops the running turn and sends the message now, and the agent carries on with it in mind.

To have every message steer at once, set **A message while it answers** on the agent's edit page, or on the Defaults page for every agent.

## A backup program

An agent runs on an agent program, its brain. When that program runs out of usage or hits a rate limit, or doesn't start, a backup program can take the turn over. The reply says so first, in one line, such as "Claude Code ran out of usage, so Codex answers this chat now."

Pick an agent's **Backup program** on its edit page, under **Brain**: the default, None, or another of the instance's programs. The default backup is on the Defaults page.

The chat stays on the backup from then on, since usage limits last hours and only you know when yours is back. Move it back yourself, as below.

## Moving a chat to another program

**Move to…** in the agent's menu lists the programs on the instance, with the one the chat runs on ticked. Pick another, and the next reply says the chat moved, in one line.

The new program starts with loom's own summary of the chat: what you asked, oldest first, then the newest of the chat word for word. It's made from the chat itself, not by a model, so it costs nothing and works even when the old program can't answer. The agent can search the chat for anything older.

On another program, the agent runs on your defaults for that program's model and effort, not its own picks, which are for its own program.
