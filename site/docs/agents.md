# Agents and keys

Loom doesn't run models itself. Each agent runs on an agent program that speaks the [Agent Client Protocol](https://agentclientprotocol.com) (ACP). The program runs the model and its own tools; loom runs everything around it: conversations, memory, routines, approvals and your devices.

## Adding an agent program

In the app, open **Agent programs**. It lists what's installed and the [ACP Registry](https://agentclientprotocol.com/get-started/registry)'s agents. **Install** downloads a build for the instance's machine and checks it against the registry, or runs the agent with `npx` or `uvx` when it has no build. Installing one again updates it. Only the owner installs programs.

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

A program's settings are the environment variables it starts with: an API key, a model, a provider. Set them on the **Agent programs** page:

- Claude: `ANTHROPIC_API_KEY`
- Codex: `OPENAI_API_KEY`

Loom keeps them sealed with the instance's `secret.key`, never shows their values again, and gives them only to the program's process. Loom never signs in to anything for an agent: a program uses the key you give it, or whatever sign-in it already has on that machine.

## Making an agent

Choose **+** in the sidebar. An agent has a name, the program it runs on, its instructions, its permission mode (how often it asks first), and the MCP servers and skills it gets. Agents can change their own instructions, and every change is a revision you can go back to.

## MCP servers and secrets

Set up MCP servers once, on the **MCP servers** page, and pick which each agent gets on its edit page. A setting can name one of your secrets, such as `ACTA_KEY=secret:ACTA_KEY`, which loom fills in only for the agent's process. Secrets are yours alone, sealed like program settings.

## Sharing keys with the people you invite

The people you invite run their agents in sandboxes of their own ([People](people.md)), with their own secrets as their agent programs' settings. To let them use yours, open the program on **Agent programs** and **Share** its settings, with a number of tokens each person may use a day or a month. Loom counts what each turn uses, as the program reports it. Someone past their budget waits for the next day or month, or adds their own key as a secret with the setting's name, which then wins.
