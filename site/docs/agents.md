# Agents and keys

Loom doesn't run models itself. Each agent runs on an agent program that speaks the [Agent Client Protocol](https://agentclientprotocol.com) (ACP). The program runs the model and its own tools; loom runs everything around it: conversations, memory, routines, approvals and your devices.

## Adding an agent program

In the app, open **Brains** in Settings. It lists what's installed and the [ACP Registry](https://agentclientprotocol.com/get-started/registry)'s agents. **Install** downloads a build for the instance's machine and checks it against the registry, or runs the agent with `npx` or `uvx` when it has no build. Installing one again updates it. Only the owner installs programs.

On a server you can also add a program by hand:

```sh
sudo -u loom loom acp add claude npx -y @agentclientprotocol/claude-agent-acp --data /var/lib/loom
```

| You want | Install |
|---|---|
| Claude | Claude Agent, with an Anthropic API key |
| OpenAI's models | Codex, with an OpenAI API key |
| OpenRouter, Ollama, LM Studio, DeepSeek, Groq, Mistral and others | OpenCode or goose |
| Gemini | Gemini CLI |

## Keys

A program's settings are the environment variables it starts with: an API key, a model, a provider. Set them on the **Brains** page:

- Claude Agent: `ANTHROPIC_API_KEY`
- Codex: `OPENAI_API_KEY`

Loom keeps them sealed with the instance's `secret.key`, never shows their values again, and gives them only to the program's process. Loom never signs in to anything for an agent: a program uses the key you give it, or whatever sign-in it already has on that machine.

## Making an agent

Choose **+ New** at the end of the sidebar's list, then **New agent**. An agent has a name, the program it runs on, its instructions, its permission mode (how often it asks first), and the MCP servers and skills it gets. Agents can change their own instructions, and every change is a revision you can go back to.

**Edit agent**, in the agent's menu, opens its settings beside the chat. At the top are its colour, under its thread, its name, its instructions, and **What it's for**, one line that group chats read to pick it ([In the chat](chat.md#group-chats)), filled from its instructions to start with. **Save** keeps the name, instructions and what it's for; everything else changes as you pick it. Under them, in groups:

- **Brain**: its model and effort, permission mode, what a message while it answers does, and its backup program.
- **Knows**: its memories, skills, tables and files, each counted, its MCP servers, and Code Mode ([In the chat](chat.md#how-an-agent-uses-its-tools)).
- **Does**: its activity, its routines with what they cost a day together, a line under them for each that spends a lot for nothing ("Inbox watch  5.4M tokens a day, mostly quiet"), which opens it, and the tokens it used today and this month. **Following**, under them, has the Acta boards, cards and T3 threads it follows ([In the chat](chat.md#following-acta-and-t3)).
- **Runs**: whether it's isolated, and its network ([Permissions](permissions.md#isolated-agents)), then the versions of its instructions, to go back to one.
- **This chat**: the program the chat runs on ([Moving a chat to another program](chat.md#moving-a-chat-to-another-program)), and **Start fresh**.

## Agent defaults

The **Defaults** page, under Agents in Settings, sets what every agent of yours follows unless it sets its own:

- **Brain**: the program a new agent runs on.
- **Backup program**: what takes a turn over when an agent's own program runs out of usage or doesn't start ([In the chat](chat.md#a-backup-program)).
- **Permission**: Asks first, Asks when it's risky, or Doesn't ask, for agents without a mode of their own. It's Asks first until you change it.
- **A message while it answers**: waits its turn, or steers at once.
- **Model and effort** for each program, for agents that don't pick their own. A program's models show here once an agent on it has started.
- **Group chats**: the program that picks who answers in a group chat, Claude unless you pick another, and its model, Claude Haiku 5.5 to begin with ([In the chat](chat.md#group-chats)).
- **Instructions**: what every agent follows, told to each beside About you, under their own heading. Choose **Save** to keep them.

The defaults are live: change one and every agent that follows it changes too. On an agent's edit page, each choice that follows the default says **Default:** and what that is. Pick it again to set the agent back to following.

Agents made before the defaults keep the permission mode they were made with until you set them to follow. They're kept on the instance, so all your devices have them. An agent can read them, but changing them asks you first.

## MCP servers and secrets

Set up MCP servers once, in **Spools**, and pick which each agent gets on its edit page. A setting can name one of your secrets, such as `ACTA_KEY=secret:ACTA_KEY`, which loom fills in only for the agent's process. Secrets are yours alone, sealed like program settings, and kept on the **Secrets** page.

**Spools**, in Settings, has Acta and T3 Code first, then MCP servers by category, and its search finds the MCP Registry's servers too, marked Community. Each shows what it can do, its tools marked as reading or changing something, where it runs and how it signs in. **Connect** signs in or asks for its key, then asks which agents get it. **Installed** lists your servers and how each stands, with **Try it** or **Reconnect**, **Disconnect** and **Remove**, and the form that changes how one is reached: its name, its command or address, and its settings, one `NAME=value` a line. **Add by hand…** adds one Spools doesn't have, by its command or its address. **Spools…** in the + menu and **Add from Spools** on an agent's edit page open it too.

In Code Mode, loom connects to the agent's MCP servers for it, and the agent reaches their tools through its scripts. With Code Mode off, the agent connects to them itself.

Every agent already has a browser, which is part of loom ([The agent's browser](browser.md)), so it isn't an MCP server. If you added one with the old **Add a browser** button, loom removes it when it updates, and your agents use loom's own.

An agent can add a server for you too, when you ask it to. A card in its reply, "Add the MCP server acta", shows its address or command and each setting, with a field for any key you don't keep yet. **Add** keeps that key as your secret, adds the server as your own, gives it to the agent, and tries it. A server that needs you to sign in goes on to **Sign in** and **Later** on the same card. **Cancel** adds nothing. Only you answer the card, and it asks whatever the agent's permission mode.

**Test** on a server starts it as an agent would and says the tools it offers, or why it didn't start.

### Servers you sign in to

Hosted servers such as Acta, Cloudflare and Vercel ask you to sign in instead of taking a key. Add one by its address, with **Add by hand…** under **Installed**. When it needs a sign-in, Installed shows **Sign in** for it, which opens the server's sign-in page in your own browser. Once you've signed in, it says whom you're signed in as, with **Disconnect**, and **Reconnect** says it's signed in.

Loom keeps the sign-in on your instance, sealed like your secrets, renews it before it runs out, and sends it to the server for every agent that has it. An agent never signs in for you: when a server needs you to, the agent is told so and you get a card in the chat with **Sign in**.

A few servers don't let apps sign up by themselves. For those, get a client ID (and secret) from the server and put them in the server's **Client ID** and **Client secret** fields. The secret can be one of your keys, as `secret:KEY`.

## Sharing keys with the people you invite

The people you invite run their agents in sandboxes of their own ([People](people.md)), with their own secrets as their agent programs' settings. To let them use yours, open the program on **Brains** and **Share** its settings, with a number of tokens each person may use a day or a month. Loom counts what each turn uses, as the program reports it. Someone past their budget waits for the next day or month, or adds their own key as a secret with the setting's name, which then wins.
