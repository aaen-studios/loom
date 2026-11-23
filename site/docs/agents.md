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
| Claude | Claude, with an Anthropic API key |
| OpenAI's models | Codex, with an OpenAI API key |
| OpenRouter, Ollama, LM Studio, DeepSeek, Groq, Mistral and others | OpenCode or Goose |
| Gemini | Gemini CLI |

## Keys

A program's settings are the environment variables it starts with: an API key, a model, a provider. Set them on the **Brains** page:

- Claude: `ANTHROPIC_API_KEY`
- Codex: `OPENAI_API_KEY`

Loom keeps them sealed with the instance's `secret.key`, never shows their values again, and gives them only to the program's process. Loom never signs in to anything for an agent: a program uses the key you give it, or whatever sign-in it already has on that machine.

## Making an agent

Choose **+** at the top of the sidebar. An agent has a name, the program it runs on, its instructions, its permission mode (how often it asks first), and the MCP servers and skills it gets. Agents can change their own instructions, and every change is a revision you can go back to.

**Edit agent**, in the agent's menu, opens its settings beside the chat, in groups:

- **Brain**: its name, model and effort, permission mode, what a message while it answers does, its backup program, its colour, and its instructions. **Save** keeps the name and instructions; the rest changes as you pick it.
- **Knows**: its memories, skills, tables and files, each counted, its MCP servers, and Code Mode ([In the chat](chat.md#how-an-agent-uses-its-tools)).
- **Does**: its routines, and the tokens it used today and this month.
- **Runs**: whether it's isolated, and its network ([Permissions](permissions.md#isolated-agents)).

## Agent defaults

The **Defaults** page, under Agents in Settings, sets what every agent of yours follows unless it sets its own:

- **Brain**: the program a new agent runs on.
- **Backup program**: what takes a turn over when an agent's own program runs out of usage or doesn't start ([In the chat](chat.md#a-backup-program)).
- **Permission**: Asks first, Asks when it's risky, or Doesn't ask, for agents without a mode of their own. It's Asks first until you change it.
- **A message while it answers**: waits its turn, or steers at once.
- **Model and effort** for each program, for agents that don't pick their own. A program's models show here once an agent on it has started.
- **Instructions every agent follows**, told to each agent beside About you, under their own heading. Choose **Save** to keep them.

The defaults are live: change one and every agent that follows it changes too. On an agent's edit page, each choice that follows the default says **Default:** and what that is. Pick it again to set the agent back to following.

Agents made before the defaults keep the permission mode they were made with until you set them to follow. They're kept on the instance, so all your devices have them. An agent can read them, but changing them asks you first.

## MCP servers and secrets

Set up MCP servers once, on the **MCP servers** page, and pick which each agent gets on its edit page. A setting can name one of your secrets, such as `ACTA_KEY=secret:ACTA_KEY`, which loom fills in only for the agent's process. Secrets are yours alone, sealed like program settings.

In Code Mode, loom connects to the agent's MCP servers for it, and the agent reaches their tools through its scripts. With Code Mode off, the agent connects to them itself.

**Test** on a server starts it as an agent would and says the tools it offers, or why it didn't start.

### Servers you sign in to

Hosted servers such as Acta, Cloudflare and Vercel ask you to sign in instead of taking a key. Add one by its address. When it needs a sign-in, its row on the MCP servers page shows **Sign in**, which opens the server's sign-in page in your own browser. Once you've signed in, the row says whom you're signed in as, with **Sign out**, and Test says it's signed in.

Loom keeps the sign-in on your instance, sealed like your secrets, renews it before it runs out, and sends it to the server for every agent that has it. An agent never signs in for you: when a server needs you to, the agent is told so and you get a card in the chat with **Sign in**.

A few servers don't let apps sign up by themselves. For those, get a client ID (and secret) from the server and put them in the server's **Client ID** and **Client secret** fields. The secret can be one of your keys, as `secret:KEY`.

## Sharing keys with the people you invite

The people you invite run their agents in sandboxes of their own ([People](people.md)), with their own secrets as their agent programs' settings. To let them use yours, open the program on **Brains** and **Share** its settings, with a number of tokens each person may use a day or a month. Loom counts what each turn uses, as the program reports it. Someone past their budget waits for the next day or month, or adds their own key as a secret with the setting's name, which then wins.
