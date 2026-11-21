# People

An instance on a server can have other people. Each person's conversations, memory, routines, agents and secrets are their own. (An instance in the desktop app is its owner's alone.)

## Roles

- **Owner**: the account made with the setup code. Manages everything, and is the only one who makes admins and installs agent programs.
- **Admin**: invites members, resets their passwords, and removes them.
- **Member**: uses the instance.

## Inviting someone

In the app, open **People** and choose **Make an invite**, for a member or an admin. Pass the link on the way you'd send a password: it shows only once. Whoever opens it in the app picks a username and password and is in. An invite works once, for a week; open ones are listed, to delete.

## Managing people

**People** lists everyone, to:

- make someone an admin, or a member again (the owner only);
- make a **password reset link** for someone who forgot theirs. It works once, for a day, and signs them out on every other device;
- **remove** someone, with everything of theirs.

Anyone but the owner can delete their own account, with everything of theirs: **Delete my account**, on the **Add a phone** page.

The owner's own password is reset on the server:

```sh
sudo -u loom loom user password sam --data /var/lib/loom
```

which reads the new password from standard input and signs the owner out everywhere.

## Where their agents run

Only the owner's agents run as the instance's own user, which can read everything on it. Everyone else's run in a sandbox of their own: a container that sees only that person's folder, with the internet but none of the server's own services but loom's tools. Their agents get their own secrets, such as `ANTHROPIC_API_KEY`, as their keys, never the owner's.

`loom install` sets sandboxes up on the server: it installs Podman and pasta and runs `loom serve --sandboxes podman` (unless you give it `--no-sandboxes`). Elsewhere, start `loom serve` with `--sandboxes podman` on a machine with rootless Podman and pasta. Without sandboxes, only the owner's agents run.

On a server that has Docker instead, `--sandboxes docker` runs them in Docker. The user loom runs as needs to be in the `docker` group. Each sandbox runs as that user, not root, on a network of loom's own, whose bridge is `loom0`. Loom says at start where sandboxes reach its tools, such as `172.18.0.1:41234`. A sandbox could also reach anything else the server listens on at that address, so let only that port through:

```sh
sudo iptables -I INPUT -i loom0 -j DROP
sudo iptables -I INPUT -i loom0 -p tcp --dport 41234 -j ACCEPT
```

Each sandbox has 2 GB of memory, two CPUs and 10 GB of disk. A person's folder over 10 GB stops their agents' runs until they delete something.
