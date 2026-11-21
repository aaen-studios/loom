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

The owner's own password is reset on the server:

```sh
sudo -u loom loom user password sam --data /var/lib/loom
```

which reads the new password from standard input and signs the owner out everywhere.
