# Running loom

Loom is one program, `loom`, which keeps everything in one folder: the database, files, and `secret.key`, the key that seals secrets. You can run it on a server, in Docker, or inside the desktop app with no server at all.

## On a Linux server

As root:

```sh
curl -fsSL https://github.com/aaen-studios/loom/releases/latest/download/install.sh | sh -s -- --domain loom.example.com
```

The script downloads the newest `loom` for the machine (x64 or Arm) and checks it against the release's signed manifest. Then `loom install`:

- makes a `loom` user, which has no rights beyond its own folder, `/var/lib/loom`;
- runs `loom serve` as that user from a systemd service, `loom.service`;
- sets up rootless Podman, for the sandboxes the agents of everyone but the owner run in ([People](people.md)), unless you give `--no-sandboxes`;
- puts Caddy in front for TLS when you give `--domain`, or, without one, serves it on your tailnet if Tailscale is installed ([Tailscale](tailscale.md));
- prints the address and the one-time **setup code**.

In the desktop app, choose **I have a server** and give its address. Pick a username and password, then choose **New server? Add its setup code** and put the code in. That account is the owner.

Run the script again to update loom, or use `loom update`, which checks the release's signature before it swaps the program.

## In Docker

```sh
docker run -d --name loom -p 127.0.0.1:8790:8790 -v loom-data:/data ghcr.io/aaen-studios/loom
docker logs loom    # the setup code, on the first start
```

The image runs loom as its own unprivileged user, with Node for the agent programs that start with `npx`. Put a proxy with TLS in front of port 8790, such as Caddy:

```
loom.example.com {
    reverse_proxy 127.0.0.1:8790
}
```

Commands go to the same program: `docker exec loom loom user list --data /data`.

In Docker, only the owner's agents run: the sandboxes everyone else's agents run in need Podman or Docker on the machine itself, beside loom rather than around it ([People](people.md)). For an instance with other people, install loom on the server.

## On your PC, with no server

In the desktop app, choose **Let's begin**. The instance runs inside the app, as you, and finds the agent programs already on the computer. Then the app makes your first agent with you: its brain, a name and a colour. The instance listens only on this computer until you let your phone reach it ([Phones](phones.md)).

## Backups and moving

- `loom export FILE` writes the whole instance, secrets and all, to a `.tar.zst` file while it runs. Keep it as safe as the instance. `loom import FILE` makes a new instance from one.
- `loom serve --backups DIR` writes one every day and keeps the newest seven.
- To move an instance to another machine, run `loom receive` there. It prints a move code. Then, in the app, open **Move** in the server popup (click the server's name at the bottom of the sidebar), and give the new place's address and the move code. Everything goes, encrypted, and your apps follow by themselves.
