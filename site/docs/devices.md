# This PC and your servers

When loom runs on a server, your agents can also use the computer you chat from, and the servers you own, if you let them. Nothing is on until you turn it on, and you choose for each thing whether an agent may do it, must ask you each time, or may not.

Both pages are in the server's settings: click the server at the foot of the sidebar. They only show for a server somewhere else, since in the desktop app's own instance the agents already run on this computer.

## This PC

**This PC** lists what agents may do on this computer: run commands, read files, list folders, open files and links, take screenshots, and use the clipboard. Each one is **Off**, **Ask** or **Allow**.

With **Ask**, the agent's chat shows the request and waits for your yes. Your yes has to come from this computer, which signs it and checks the signature before it does anything, so the server can't say yes for you. Each request is done once, exactly as you saw it, and it goes in the agent's activity log.

## Your servers

**Hosts** is where your own servers go: a VPS, a box at home, anything you reach with `ssh` from this computer. Agents then run commands on them through loom, asking you first, instead of finding your ssh config and logging in on their own.

- **Adding one.** When the page opens, loom looks in your `~/.ssh/config` and, if you use [Tailscale](tailscale.md), at the machines on your tailnet, and lists what it finds under **Found**. Choose **Add** beside one. Or type what you'd give `ssh`, such as `vps` or `root@vps.example.com`, and choose **Add**.
- **What agents may do.** A host starts at **Ask**: each command shows in the chat and runs only once you say yes on this computer. **Allow** lets agents run any command there without asking. **Off** stops them.
- **Where it runs.** The command runs on this computer, as `ssh <host> -- <command>`, with your keys and your ssh config, so loom never holds them. It never waits for a password or asks about a new host key, so connect once yourself first, or use a key.
- **While the app runs.** Like This PC, a host is only reachable while loom runs on this computer, in the tray counts.

Agents see each host in their list of devices, beside the computer that reaches it. Each command and its output are in the agent's activity log.
