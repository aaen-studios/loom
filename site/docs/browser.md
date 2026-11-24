# The agent's browser

Each agent has a browser of its own, which loom starts the first time the agent uses it. On a server it's Chromium. On a PC it's the Chrome or Edge already there. To use another, set `LOOM_BROWSER` to its path before loom starts. The browser keeps its own profile for each agent, so logins last between runs, and it stops after ten idle minutes.

The agent reads a page as its outline and clicks, types, scrolls and goes back on it, as you would.

## Watching it

While an agent browses, its page shows as a small window at the chat's bottom right, live. Click it to open the browser in the panel beside the chat, bigger. Its × puts it back in the small window. The full screen button beside the × opens it over the whole window, and the same button or Esc puts it back.

**Browser**, over the chat, shows or hides it while the browser is open. Loom sends the page's picture only while someone watches.

## Taking over

Some things only you should do: a login, a 2FA code, a CAPTCHA. The agent can ask you, and then a card over the message box says what it wants, with **Take over**. You can also take over whenever you like: press **Take over** under the page, or click into the page.

Once you have it, the page has the agent's colour round it, and your clicks, scrolling and typing go to the page, Tab and Enter too, and Ctrl+V pastes. The agent's browser tools wait. **Hand back** lets the agent carry on, and it's told what you did.

## Secrets

An agent can type one of your secrets into a page by its name, without seeing it. Or it can ask you for one: a card in the chat says what it needs, and you type it there and choose **Fill it in**, or **Not now**. Either way, the agent is told it's filled, never what it was.

## Teaching a task

Show an agent how to do something once, in its browser, and it writes the steps up as a skill.

1. Take over the browser.
2. Turn on **Record**, beside Hand back.
3. Do the task.
4. Turn Record off, or hand the browser back.

While it records, loom keeps each page you're on, what you click, what you type and where, and the keys you press. What goes into a password or one-time-code field is never kept, only that something secret went there. Any of your secrets typed anywhere else is kept as `[secret NAME]`.

When you stop, the agent gets a task, beside its chat, to write the steps up as a skill. The skill isn't saved: it's proposed, and waits for you on the Proposals page.

## Proposals

Agents propose skills and notes from your past chats, on their own, or when you ask. Each waits on the **Proposals** page, under Agents in Settings, until you keep or drop it. Nothing an agent proposes is used until you say yes, and an agent can't keep its own.

- Open a proposed skill to read it, with what you did beside it when it came from a recording. Change its name, what it's for and how to do it, then **Keep** or **Drop**.
- **Ask**, with the agent's name, asks the agent on screen to look over its chats for more.

### Notes tied to a site

A note can be tied to a site, such as `github.com`. When the agent's browser opens a page there, or under it, the agent is shown its notes for that site, once a visit. The agent's Memory page shows the site each note is for.
