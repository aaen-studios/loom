# Permissions, rules and sandboxes

Three things decide what an agent may do: its permission mode, your rules, and where it runs.

## Permission modes

Each agent has a permission mode, set on its edit page under **Brain**, or on the Defaults page for every agent that doesn't set its own ([Agents and keys](agents.md#agent-defaults)):

- **Asks first** (`ask`): every request comes to you. New agents start here.
- **Asks when it's risky** (`auto`): loom allows what only looks (reading, searching, thinking and fetching) and asks about the rest.
- **Doesn't ask** (`allow`): everything goes.

Whatever the mode, some things always ask: sending, paying, deleting, letting people in, changing permissions and moving the instance. An agent can't loosen its own mode, delete itself or answer its own approval.

## Rules

Rules say what happens when an agent touches something particular, whatever its mode. They're on the **Rules** page, under Agents in Settings.

**New rule** makes one. A rule is:

- **Then**: Allow, Ask or Deny.
- **On** what it touches:
  - a **command**, by how it starts (`git push`) or as a pattern (`rm -rf *`);
  - a **path**, as a pattern (`**/.env`), or a folder, which covers everything in it;
  - a **site**, which covers the sites under it too (`github.com`), or a pattern (`*.bank.no`);
  - a **tool** of an MCP server, as `server.tool` (`github.create_issue`), a pattern (`github.delete_*`), or a server's name for all its tools. Loom's own tools are `loom.<name>`.
- **What it does**: only reads, or changes something.
- **Started by**: who started the turn: you, another agent, a routine, a webhook or a script.
- **For**: every agent, or one.

A rule can have several of these, and all of them must hold. Add a note to say why it's there.

How they're checked:

- Deny wins, then ask, then allow. With no rule about it, the agent's permission mode decides.
- A rule that can't be read denies. So does an action whose command, path, site or starter a rule is about but loom can't make out.
- Where a command line chains several commands, each one is checked. An allow holds only when every command and every path on the line matches it, and a deny or an ask holds when any does.
- An allow answers an agent program's own requests, such as running a command. Loom's own tools keep their own rules, so for those a rule can only ask or deny.
- `*` and `**` match hidden files and folders too, unlike in the shell: a rule on `/srv/secrets` covers `/srv/secrets/.key`, and a command rule of `*` covers `./deploy.sh`.

Rules aren't part of an agent's revisions, so rolling an agent back never takes one away. An agent can read your rules and try a what-if, but adding, changing or removing a rule asks you first.

### What if

Under the rules, **What if** asks the instance what an action would get, without doing it. Pick what it's on, type the command, path, site or tool, pick who started it and which agent, and choose **Check**. It answers allow, ask or deny, and which rule or mode decided.

## Isolated agents

The owner's agents run as the instance's own user, which can read everything on the instance. Members' agents always run in a sandbox of their own ([People](people.md#where-their-agents-run)). You can put any of your own agents in one too.

On the agent's edit page, under **Runs**, turn on **Isolated**. The agent then runs in a sandbox, apart from the instance's files and your other agents, on your own program settings.

An isolated agent's **Network** is one of:

- **Everything**: the internet, as a member's sandbox has.
- **Only these hosts**: type a name, such as `github.com`, or `*.npmjs.org` for every name under one, and choose **Add**. Nothing else is reachable, and never the instance's own computer.
- **Nothing**: no network at all, but loom's own tools.

With a list or nothing, the agent gets a sandbox of its own with no network, and its only way out is through loom: loom's tools, and a proxy that lets through only the hosts on the list. Programs that follow `HTTP_PROXY` and `HTTPS_PROXY` reach the list, and the rest reach nothing. The agent's browser goes through the same proxy. A change to the list holds from the agent's next connection.

An isolated agent can't make an agent that isn't isolated, or give one more network than it has. No agent can change where it runs itself.

### Isolation needs sandboxes

Sandboxes need Linux with Podman or Docker, and an instance started with `--sandboxes podman` or `--sandboxes docker`. `loom install` sets that up ([Running loom](running.md)).

Elsewhere, such as in the desktop app on Windows, or in Docker, loom refuses to isolate an agent rather than pretend: the switch won't turn on, and says why.

A member's agents are always isolated, so for them the switch stays on. Members can still pick each agent's network.
