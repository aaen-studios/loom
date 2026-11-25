# The agent's browser

Each agent has a browser of its own, which loom starts the first time the agent uses it. You don't install anything for it. On Windows it's the Chrome or Edge already there, and every Windows PC has Edge. On a Mac it's Chrome or Edge if you have one, and otherwise loom downloads a headless Chrome of its own the first time an agent browses, about 100 MB, which takes a moment once. On a server it's Chromium, which `loom install` sets up. To use another browser, set `LOOM_BROWSER` to its path before loom starts. The browser keeps its own profile for each agent, so logins last between runs, and it stops after five idle minutes.

The agent reads a page as its outline and clicks, types, scrolls and goes back on it, as you would.

## Watching it

While an agent browses, its page shows as a small window at the chat's bottom right, live. Click it to open the browser in the panel beside the chat, bigger. Its × puts it back in the small window. The full screen button beside the × opens it over the whole window, and the same button or Esc puts it back.

**Browser**, over the chat, shows or hides it while the browser is open. Loom sends the page's picture only while someone watches.

## Taking over

Some things only you should do: a login, a 2FA code, a CAPTCHA. The agent can ask you, and then a card over the message box says what it wants, with **Take over**. You can also take over whenever you like: press **Take over** under the page, or click into the page.

Once you have it, the page has the agent's colour round it, and your clicks, scrolling and typing go to the page, with the keys held with them, such as Shift+Tab and Shift with the arrows. Ctrl+A, C, X, Z and Y (⌘ on a Mac) select, copy, cut, undo and redo on the page, and Ctrl+V (⌘V) pastes. The agent's browser tools wait. **Hand back** lets the agent carry on, and it's told what you did.

## Secrets

An agent never asks for a password or a code in the chat. It types one of your secrets into a page by its name, without seeing it, and the first time a secret goes to a site, you're asked. When it needs one you don't keep, a card in its reply says what for and on which site: type it in (the eye shows what you typed), and leave **Save as** ticked, with the name it's given (such as `ntnu-password`) or one of your own, to keep it for that site. **Fill** puts it into the page, and next time the agent fills it there without asking. **Not now** tells the agent no. An email and a password can come on one card, and a one-time code is never kept. Either way, the agent is told it's in, and what it was kept as, never what it was.

Your secrets are on the **Secrets** page in Settings, each with the sites it's filled in on. Take a site off, remove a secret, or add one yourself.

## Teaching a task

Show an agent how to do something once, in its browser, and it writes the steps up as a skill.

1. Take over the browser.
2. Turn on **Record**, beside Hand back.
3. Do the task.
4. Turn Record off, or hand the browser back.

While it records, loom keeps each page you're on, what you click, what you type and where, and the keys you press. What goes into a password or one-time-code field is never kept, only that something secret went there. Any of your secrets typed anywhere else is kept as `[secret NAME]`.

When you stop, the agent gets a task, beside its chat, to write the steps up as a skill. The skill isn't saved: it's proposed, and waits for you on the Proposals page.

## Proposals

Four minutes after your last message gets its answer, the chat has paused, and the agent looks back over it for what's worth remembering. You don't see it working. A message from you stops it. A skill the agent said failed partway is fixed in the same look back.

What happens next depends on the agent's [permission mode](permissions.md#permission-modes):

- **Doesn't ask**: the agent keeps its notes and fixes on its own. A line under its reply says so, such as "Remembered 2 things" or "Fixed the skill post-release-notes". Open it to see each one, with **Forget** or **Undo**.
- **Asks first** or **Asks when it's risky**: a card under the reply asks. For notes it says what the agent would remember and who each is for: untick any you don't want, then **Keep** (it says how many, "Keep 2", or **Drop them** with none ticked) or **Not now**. A fix says why the skill failed and shows the lines it removes and adds, with **Keep the fix**, **Change it** and **Drop**.

A note for all your agents always asks, whatever the mode.

What you don't answer, or put off with **Not now**, waits on the **Proposals** page, under Agents in Settings, with skills written from a recording. Open one to read and change it before you **Keep** or **Drop** it. **Ask**, with the agent's name, asks the agent on screen to look over its chats for more.

Every note and fix that was kept is in the agent's activity log, with **Undo**. An agent can't keep, drop or undo its own.

### Notes tied to a site

A note can be tied to a site, such as `github.com`. When the agent's browser opens a page there, or under it, the agent is shown its notes for that site, once a visit. The agent's Memory page shows the site each note is for.
