// loom.rip for AI assistants and search (the llms.txt convention): what loom is, and where the rest is.
import { latest } from '../lib/releases.mjs';
import { GITHUB, GUIDES, INSTALL, ORIGIN } from '../lib/site.mjs';

export async function GET() {
	const r = await latest();
	const text = `# loom

> loom is a self-hostable agent app. You chat with your own AI agents from the desktop app; they work on your server, or on your PC if you don't want one. Each agent runs on an agent program, such as Claude, Codex, OpenCode, Gemini CLI or goose, or any program that speaks the Agent Client Protocol. It's open source, under the MIT license.

${r ? `The newest release is loom ${r.version}.` : ''}

- Each agent has a name, a colour, its instructions and a folder to work in, and asks before what matters: everything, what's risky, or nothing, with rules that allow, ask or deny by site, file, command or tool.
- It runs on a PC with no server, or on a Linux server (\`${INSTALL}\`) or in Docker, and one app signs in to as many servers as you like. People invited to a server get agents of their own, each in a sandbox.
- Routines and webhooks, long tasks, a browser of its own for each agent, notes, skills and shared tables, a backup agent program, and plugins that agents write themselves.
- The server and the app's core are written in Calx; the desktop app is native.

## Pages

- [Download](${ORIGIN}/download/): the Windows app, and the server for Linux, Docker, macOS and Windows
- [Changelog](${ORIGIN}/changelog/): every release, newest first
- [Guides](${GUIDES}): how to use and run loom
- [Source](${GITHUB}): the code, the plan and the wire protocol
`;
	return new Response(text, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}
