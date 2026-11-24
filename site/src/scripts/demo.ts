// Plays the live copy of the app (src/components/AppDemo.astro) in a loop: a message is typed and
// sent, the agent thinks, uses a tool, writes, asks before it deploys and waits for the yes, then
// finishes. While the turn runs, one quiet line under the reply says the step it's on; once it's
// done, its last words are the answer and the steps before them fold under one line that opens
// (desktop/src/model.rs, parts), and a subagent's call in the earlier reply opens to show what
// its subagent did inside it (model.rs, inside). The earlier reply's sources tab opens their
// list (app.rs, sources_tab). The moods, what the sidebar says after an agent's name on its
// one line ("Thinking", the tool's title, "Wants to …"), or under the pointer on a pinned agent's
// tile, and the wording are the app's. Clicking
// Allow, Always allow (under the arrow beside Allow) or Deny answers the approval, which says how
// in one quiet line before it goes; left alone, it's allowed after a moment.
//
// The sidebar's control collapses it to the rail and opens it again, and a narrow window has the
// rail, as the app's does (desktop/src/rail.rs).
//
// It plays only while it's on screen, and with reduced motion it shows the finished chat instead.
import { Thread, threadsIn, yarnsIn, type Mood } from './thread';

const SEND = 'Is main green? If it is, ship it.';
const TOOL = 'Fetch github.com/acme/shop/actions';
const ANSWER = 'Main is green. The last run passed at 09:12, with all four jobs.';
const ASK = 'Run ./deploy.sh production';
/** What loom knows the call does, as the core says it for a command (core/asks.calx). */
const EFFECT = 'Runs a command';
const SHIPPED = 'Shipped. 2.4.1 is live, and its health check answers.';
const HELD = "Okay, I won't deploy. Main stays as it is.";

/** A pause that ends early, with an error, when the demo is stopped. */
class Stopped extends Error {}

/** A reply's fold: its quiet line, and the steps under it, which open on a click. */
function foldOf(reply: HTMLElement) {
	const fold = reply.querySelector<HTMLElement>('[data-fold]')!;
	const line = fold.querySelector<HTMLButtonElement>('.fold-line')!;
	const words = fold.querySelector<HTMLElement>('.words')!;
	const steps = fold.querySelector<HTMLElement>('.steps')!;
	line.addEventListener('click', () => {
		if (fold.classList.contains('bare')) return;
		steps.hidden = !steps.hidden;
		line.setAttribute('aria-expanded', String(!steps.hidden));
		line.querySelector('.chevron')!.textContent = steps.hidden ? '›' : '⌄';
	});
	/** Sets the line's words; with nothing under it to open, it has no chevron. */
	const say = (text: string) => {
		words.textContent = text;
		fold.classList.toggle('bare', steps.childElementCount === 0);
	};
	return { steps, say };
}

export function play(app: HTMLElement) {
	const threads = threadsIn(app);
	yarnsIn(app);
	const byAgent = new Map<string, Thread>();
	for (const row of app.querySelectorAll<HTMLElement>('[data-agent]')) {
		const svg = row.querySelector<SVGSVGElement>('svg[data-thread]')!;
		const th = threads.find((t) => t.svg === svg);
		if (th) byAgent.set(row.dataset.agent!, th);
	}
	const head = threads.find((t) => t.svg.hasAttribute('data-head'))!;
	const msgs = app.querySelector<HTMLElement>('.msgs')!;
	const history = [...msgs.children];
	for (const reply of msgs.querySelectorAll<HTMLElement>('.reply')) foldOf(reply);
	// A subagent's call opens to show what its subagent did inside it.
	for (const line of msgs.querySelectorAll<HTMLButtonElement>('.sub-line')) {
		const inside = line.nextElementSibling as HTMLElement;
		line.addEventListener('click', () => {
			inside.hidden = !inside.hidden;
			line.setAttribute('aria-expanded', String(!inside.hidden));
			line.querySelector('.chevron')!.textContent = inside.hidden ? '›' : '⌄';
		});
	}
	// A reply's sources tab opens its list, and a click anywhere else shuts it.
	for (const tab of msgs.querySelectorAll<HTMLButtonElement>('[data-sources]')) {
		const list = tab.nextElementSibling as HTMLElement;
		const chat = app.querySelector<HTMLElement>('.chat')!;
		const show = (open: boolean) => {
			list.hidden = !open;
			// Kept inside the chat, as the app keeps it in the window: above the tab when
			// there's no room under it.
			list.classList.remove('up');
			if (open && list.getBoundingClientRect().bottom > chat.getBoundingClientRect().bottom) list.classList.add('up');
			tab.setAttribute('aria-expanded', String(open));
			tab.querySelector('.chev path')!.setAttribute('d', open ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6');
		};
		tab.addEventListener('click', () => show(list.hidden));
		document.addEventListener('pointerdown', (e) => {
			if (!list.hidden && !tab.parentElement!.contains(e.target as Node)) show(false);
		});
	}
	const typed = app.querySelector<HTMLElement>('.typed')!;
	const tpl = (id: string) => (document.getElementById(id) as HTMLTemplateElement).content.firstElementChild!.cloneNode(true) as HTMLElement;
	// A row says what its agent is on after its name; a pinned agent's tile, under the pointer.
	const preview = (agent: string, text: string, mood: Mood) => {
		const row = app.querySelector<HTMLElement>(`[data-agent="${agent}"]`);
		const p = row?.querySelector<HTMLElement>('.agent-preview');
		if (p) p.textContent = text;
		if (row?.classList.contains('tile')) row.title = text ? `${agent} · ${text}` : agent;
		byAgent.get(agent)?.set(mood);
	};

	// The sidebar: collapsed by its control, and a rail anyway in a narrow window.
	const bar = app.querySelector<HTMLButtonElement>('[data-bar]')!;
	const narrow = matchMedia('(max-width: 860px)');
	let collapsed = false;
	const rail = () => {
		const railed = collapsed || narrow.matches;
		app.classList.toggle('railed', railed);
		bar.setAttribute('aria-label', railed ? 'Open the sidebar' : 'Collapse the sidebar');
	};
	bar.addEventListener('click', () => {
		collapsed = !app.classList.contains('railed');
		// Opened in a narrow window, it stays open until it's collapsed again.
		if (!collapsed && narrow.matches) {
			app.classList.remove('railed');
			bar.setAttribute('aria-label', 'Collapse the sidebar');
			return;
		}
		rail();
	});
	narrow.addEventListener('change', rail);
	rail();

	let run = 0;
	let onScreen = false;
	const wait = (ms: number, id: number) =>
		new Promise<void>((done, fail) => {
			const step = () => {
				if (id !== run) return fail(new Stopped());
				if (!onScreen || document.hidden) {
					// Hold the moment until it's seen again.
					setTimeout(step, 250);
					return;
				}
				done();
			};
			setTimeout(step, ms);
		});

	// The other agents keep their own work going beside the chat.
	const others = async (id: number) => {
		for (;;) {
			preview('Biscuit', 'Sort the inbox', 'tool');
			await wait(5200, id);
			preview('Biscuit', '', 'idle');
			await wait(6500, id);
			preview('Pip', 'Thinking', 'thinking');
			await wait(3800, id);
			preview('Pip', '', 'idle');
			await wait(4000, id);
		}
	};

	const mood = (m: Mood, reply?: Thread) => {
		head.set(m);
		reply?.set(m);
		app.classList.toggle('busy', m !== 'idle');
	};

	async function type(text: string, id: number) {
		for (let i = 1; i <= text.length; i++) {
			typed.textContent = text.slice(0, i);
			await wait(text[i - 1] === ' ' ? 70 : 38 + Math.random() * 40, id);
		}
	}

	/** Words as they come, on the fold's quiet line, the step the turn is on. */
	async function stream(say: (text: string) => void, text: string, id: number) {
		const words = text.split(' ');
		for (let i = 1; i <= words.length; i++) {
			say(`${words.slice(0, i).join(' ')}…`);
			await wait(45 + Math.random() * 70, id);
		}
	}

	/** A tool call among the reply's steps, running until it's marked. */
	function tool(steps: HTMLElement, title: string) {
		const line = tpl('demo-tool');
		line.querySelector('.title')!.textContent = title;
		steps.append(line);
		return (how: 'done' | 'denied') => {
			line.classList.add(how);
			line.querySelector('.mark')!.textContent = how === 'done' ? '✓' : 'denied';
		};
	}

	/** Words that came before the answer, among the steps, smaller and quieter than it. */
	function words(steps: HTMLElement, text: string) {
		const said = tpl('demo-said');
		const p = document.createElement('p');
		p.textContent = text;
		said.append(p);
		steps.append(said);
	}

	/** A finished reply: its steps under one line, and its answer. */
	function finish(reply: HTMLElement, say: (text: string) => void, steps: HTMLElement, answer: string) {
		const p = document.createElement('p');
		p.textContent = answer;
		reply.querySelector(':scope > .body > .said')!.append(p);
		const n = steps.childElementCount;
		say(n === 1 ? '1 step' : `${n} steps`);
	}

	/** The approval, answered by a click or, after a while, by itself. */
	function approve(id: number) {
		const row = tpl('demo-approval');
		// The title says the command whole, so the card doesn't say it again (core/asks.calx).
		row.querySelector('.card-title .words')!.textContent = ASK;
		row.querySelector('.pill')!.textContent = EFFECT;
		msgs.append(row);
		threadsIn(row);
		// The arrow beside Allow opens Always allow.
		const more = row.querySelector<HTMLButtonElement>('[data-more]')!;
		const menu = row.querySelector<HTMLElement>('.menu')!;
		more.addEventListener('click', () => {
			menu.hidden = !menu.hidden;
			more.setAttribute('aria-expanded', String(!menu.hidden));
		});
		return new Promise<'allow' | 'deny'>((done, fail) => {
			let answered = false;
			const answer = (a: string, button: HTMLElement) => {
				if (answered) return;
				answered = true;
				button.classList.add('pressed');
				menu.hidden = true;
				// One quiet line says how, until the question goes, as in the app.
				setTimeout(() => {
					const how = a === 'deny' ? 'Denied' : a === 'always' ? 'Always allowed' : 'Allowed once';
					row.querySelector('.card-done b')!.textContent = how;
					row.querySelector('.card-done .what')!.textContent = ` · ${ASK}`;
					if (a === 'deny') row.querySelector('.card-done path')!.setAttribute('d', 'M18 6 6 18M6 6l12 12');
					row.classList.add('answered');
					setTimeout(() => {
						row.classList.add('leaving');
						setTimeout(() => row.remove(), 350);
						done(a === 'deny' ? 'deny' : 'allow');
					}, 700);
				}, 220);
			};
			for (const b of row.querySelectorAll<HTMLButtonElement>('button[data-answer]')) {
				b.addEventListener('click', () => answer(b.dataset.answer!, b));
			}
			wait(4200, id).then(
				() => answer('allow', row.querySelector<HTMLElement>('.primary')!),
				(e) => fail(e),
			);
		});
	}

	async function scene(id: number) {
		await wait(900, id);
		await type(SEND, id);
		await wait(350, id);
		typed.textContent = '';
		const you = tpl('demo-you');
		you.querySelector('.bubble')!.textContent = SEND;
		msgs.append(you);
		await wait(250, id);

		const reply = tpl('demo-reply');
		msgs.append(reply);
		const replyThread = threadsIn(reply)[0];
		const { steps, say } = foldOf(reply);

		mood('thinking', replyThread);
		preview('Juniper', 'Thinking', 'thinking');
		say('Thinking…');
		await wait(1700, id);

		mood('tool', replyThread);
		preview('Juniper', TOOL, 'tool');
		const fetched = tool(steps, TOOL);
		say(`${TOOL}…`);
		await wait(1600, id);
		fetched('done');

		mood('writing', replyThread);
		preview('Juniper', '', 'writing');
		await stream(say, ANSWER, id);
		words(steps, ANSWER);
		await wait(300, id);

		mood('waiting', replyThread);
		preview('Juniper', `Wants to ${ASK[0].toLowerCase()}${ASK.slice(1)}`, 'waiting');
		const deploy = tool(steps, ASK);
		say(`${ASK}…`);
		const answer = await approve(id);

		if (answer === 'allow') {
			mood('tool', replyThread);
			preview('Juniper', ASK, 'tool');
			await wait(1900, id);
			deploy('done');
			mood('writing', replyThread);
			preview('Juniper', '', 'writing');
			await stream(say, SHIPPED, id);
			finish(reply, say, steps, SHIPPED);
		} else {
			deploy('denied');
			mood('writing', replyThread);
			preview('Juniper', '', 'writing');
			await stream(say, HELD, id);
			finish(reply, say, steps, HELD);
		}
		await wait(200, id);
		mood('idle', replyThread);
		preview('Juniper', '', 'idle');
		await wait(6000, id);

		// Back to the start, quietly, and again.
		for (const el of [...msgs.children]) if (!history.includes(el)) el.classList.add('leaving');
		await wait(400, id);
		for (const el of [...msgs.children]) if (!history.includes(el)) el.remove();
	}

	async function loop() {
		const id = ++run;
		others(id).catch(() => {});
		try {
			for (;;) await scene(id);
		} catch (e) {
			if (!(e instanceof Stopped)) throw e;
		}
	}

	/** The finished chat, still, for reduced motion. */
	function still() {
		run++;
		for (const el of [...msgs.children]) if (!history.includes(el)) el.remove();
		typed.textContent = '';
		const you = tpl('demo-you');
		you.classList.remove('fresh');
		you.querySelector('.bubble')!.textContent = SEND;
		const reply = tpl('demo-reply');
		reply.classList.remove('fresh');
		const { steps, say } = foldOf(reply);
		tool(steps, TOOL)('done');
		words(steps, ANSWER);
		tool(steps, ASK)('done');
		finish(reply, say, steps, SHIPPED);
		msgs.append(you, reply);
		threadsIn(reply);
		mood('idle');
		for (const a of byAgent.keys()) preview(a, '', 'idle');
	}

	const reduced = matchMedia('(prefers-reduced-motion: reduce)');
	const start = () => (reduced.matches ? still() : loop());
	reduced.addEventListener('change', start);
	// The newest entry: a scroll before the first callback brings both in one batch.
	new IntersectionObserver((entries) => (onScreen = entries[entries.length - 1].isIntersecting)).observe(app);
	start();
}
