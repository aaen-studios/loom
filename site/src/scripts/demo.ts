// Plays the live copy of the app (src/components/AppDemo.astro) in a loop: the earlier answer's
// Reply, by its corner on hover, puts it over the composer's words, and the message that goes
// has its quote over its bubble, which goes back to the answer and lights it (desktop/src/app.rs,
// reply_bar, quote_view, jump_to); a message's Reply, or words selected in one with the bar
// that comes over them (Copy, Quote, Ask about this), does the same by hand. A message is typed,
// with a link pasted and a file attached among its words, each a piece in them, the file with its
// pill over them, and sent, showing them the same way; the agent thinks, uses a tool, writes,
// asks before it deploys and waits for the yes, then finishes. While the turn runs, one quiet
// line under the reply says the step it's on; once it's done, its last words are the answer and
// the steps before them fold under one line that opens (desktop/src/model.rs, parts), and a
// subagent's call in the earlier reply opens to show what its subagent did inside it (model.rs,
// inside). The earlier reply's sources tab opens their list (app.rs, sources_tab). The moods,
// what the sidebar says after an agent's name on its one line ("Thinking", the tool's title,
// "Wants to …"), or under the pointer on a pinned agent's tile, and the wording are the app's.
// Clicking Allow, Always allow (under the arrow beside Allow) or Deny answers the approval,
// which says how in one quiet line before it goes; left alone, it's allowed after a moment.
//
// The sidebar's control collapses it to the rail and opens it again, and a narrow window has the
// rail, as the app's does (desktop/src/rail.rs). "From Biscuit" under Juniper opens Biscuit's
// thread with it in the chat's place.
//
// It plays only while it's on screen, and with reduced motion it shows the finished chat instead.
import { Thread, threadsIn, yarnsIn, type Mood } from './thread';

/** What the person writes: words typed, and pieces put in among them, a link pasted on its own
 * and a file attached, each by the template that draws it (app.rs, composer_token), and the
 * file's pill over the words. A link is only its piece. */
const SEND: (string | { piece: string; over?: string })[] = [
	'Is main green on ',
	{ piece: 'demo-link' },
	'? If it is, ship it with ',
	{ piece: 'demo-file', over: 'demo-file-pill' },
];
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
	const msgs = app.querySelector<HTMLElement>('.msgs.mine')!;
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
		tab.addEventListener('click', () => show(list.hidden !== false));
		document.addEventListener('pointerdown', (e) => {
			if (!list.hidden && !tab.parentElement!.contains(e.target as Node)) show(false);
		});
	}
	const typed = app.querySelector<HTMLElement>('.typed')!;
	const attached = app.querySelector<HTMLElement>('[data-attached]')!;

	// Replying (PLAN.md, "Replying to a message"): a message's Reply, or words selected in it,
	// put a bar over the composer's words; the message goes with a quote over its bubble, which
	// goes back to the original and lights it (app.rs, reply_bar, quote_view, jump_to).
	const replyBar = app.querySelector<HTMLElement>('[data-reply-bar]')!;
	type Replying = { row: HTMLElement; who: string; words: string; part: boolean; ask: boolean };
	let replying: Replying | null = null;
	/** A message's words, on one line: an agent's answer, or what the person wrote. */
	const wordsOf = (row: HTMLElement) =>
		(row.querySelector('.said:not(.on-the-way)') ?? row.querySelector('.bubble'))?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
	const replyTo = (row: HTMLElement, part?: string, ask = false) => {
		const agent = row.classList.contains('reply');
		const who = agent ? (row.querySelector('.who')?.textContent ?? 'Juniper') : 'You';
		replying = { row, who, words: (part ?? wordsOf(row)).replace(/\s+/g, ' ').trim(), part: !!part, ask: ask && !!part };
		replyBar.querySelector('.rb-head')!.textContent = replying.ask ? `Asking ${who} about` : agent ? `Replying to ${who}` : 'Replying to yourself';
		replyBar.querySelector('.rb-words')!.textContent = replying.words;
		replyBar.hidden = false;
	};
	const unreply = () => {
		replying = null;
		replyBar.hidden = true;
	};
	app.querySelector('[data-reply-x]')!.addEventListener('click', unreply);
	for (const b of msgs.querySelectorAll<HTMLButtonElement>('.reply-btn')) {
		b.addEventListener('click', () => replyTo(b.closest<HTMLElement>('.row')!));
	}
	/** Goes to the original a quote names: the chat moves to show it, lit, then back. */
	const jump = (row: HTMLElement) => {
		const chat = app.querySelector<HTMLElement>('.chat')!;
		const by = chat.getBoundingClientRect().top + 24 - row.getBoundingClientRect().top;
		msgs.classList.add('jumping');
		if (by > 0) msgs.style.transform = `translateY(${by}px)`;
		row.classList.add('lit');
		setTimeout(() => {
			row.classList.remove('lit');
			msgs.style.transform = '';
			setTimeout(() => msgs.classList.remove('jumping'), 400);
		}, 1800);
	};
	// Words selected in a message have a small bar where the selection ended.
	let picked: HTMLElement | null = null;
	const unpick = () => {
		picked?.remove();
		picked = null;
	};
	msgs.addEventListener('mouseup', (e) => {
		setTimeout(() => {
			unpick();
			const sel = getSelection();
			const words = sel?.toString().replace(/\s+/g, ' ').trim() ?? '';
			const row = (sel?.anchorNode?.parentElement)?.closest<HTMLElement>('.row[data-msg]');
			if (!words || !row || !row.contains(sel!.focusNode)) return;
			picked = tpl('demo-picked');
			if (!row.classList.contains('reply')) picked.querySelector('[data-pick="ask"]')!.remove();
			const at = app.getBoundingClientRect();
			picked.style.left = `${Math.min(e.clientX - at.left, at.width - 280)}px`;
			picked.style.top = `${e.clientY - at.top + 10}px`;
			for (const b of picked.querySelectorAll<HTMLButtonElement>('[data-pick]')) {
				b.addEventListener('click', () => {
					if (b.dataset.pick === 'copy') navigator.clipboard?.writeText(words).catch(() => {});
					else replyTo(row, words, b.dataset.pick === 'ask');
					sel?.removeAllRanges();
					unpick();
				});
			}
			app.append(picked);
		});
	});
	document.addEventListener('pointerdown', (e) => {
		if (picked && !picked.contains(e.target as Node)) unpick();
	});
	// Biscuit's thread with Juniper opens from its row under Juniper, in the chat's place;
	// the strip's arrow, or Juniper's own row, goes back to the person's chat (app.rs,
	// open_thread, branch_strip). The chat goes on meanwhile.
	const inThread = (on: boolean) => app.classList.toggle('in-thread', on);
	app.querySelector('[data-ask]')?.addEventListener('click', () => inThread(true));
	app.querySelector('[data-back]')?.addEventListener('click', () => inThread(false));
	app.querySelector('.agent.on')?.addEventListener('click', () => inThread(false));
	const tpl =(id: string) => (document.getElementById(id) as HTMLTemplateElement).content.firstElementChild!.cloneNode(true) as HTMLElement;
	// A row says what its agent is on after its name; a pinned agent's tile, under the pointer.
	const preview = (agent: string, text: string, mood: Mood) => {
		const row = app.querySelector<HTMLElement>(`[data-agent="${agent}"]`);
		const p = row?.querySelector<HTMLElement>('.agent-preview');
		if (p) p.textContent = text;
		if (row?.classList.contains('tile')) row.title = text ? `${agent} · ${text}` : agent;
		byAgent.get(agent)?.set(mood);
	};
	// An agent waiting on the person goes to the top of the sidebar, under Waiting on you, and
	// back to its place once it's answered (core/sidebar.calx).
	const waitingGroup = app.querySelector<HTMLElement>('[data-waiting]')!;
	const places = new Map<string, { parent: Node; before: Node | null }>();
	const waitingOn = (agent: string, on: boolean) => {
		const row = app.querySelector<HTMLElement>(`.agent[data-agent="${agent}"]`);
		if (!row) return;
		// Its row, and other agents' threads with it under it.
		const rows: Element[] = [row];
		for (let n = row.nextElementSibling; n?.classList.contains('ask'); n = n.nextElementSibling) rows.push(n);
		if (on && !places.has(agent)) {
			places.set(agent, { parent: row.parentNode!, before: rows[rows.length - 1].nextSibling });
			waitingGroup.append(...rows);
		} else if (!on && places.has(agent)) {
			const { parent, before } = places.get(agent)!;
			for (const r of rows) parent.insertBefore(r, before);
			places.delete(agent);
		}
		waitingGroup.hidden = places.size === 0;
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

	/** Words typed into the composer, after what's in it. */
	async function type(text: string, id: number) {
		const words = document.createTextNode('');
		typed.append(words);
		for (let i = 1; i <= text.length; i++) {
			words.data = text.slice(0, i);
			await wait(text[i - 1] === ' ' ? 70 : 38 + Math.random() * 40, id);
		}
	}

	/** Puts what a piece brings over the words, if it brings anything. */
	function over(row: HTMLElement, template?: string) {
		if (template) row.append(tpl(template));
	}

	/** The person's message written: its words typed, and its link pasted and its file
	 * attached where they go, each a piece in the words, the file with its pill over them. */
	async function compose(id: number) {
		for (const part of SEND) {
			if (typeof part === 'string') {
				await type(part, id);
				continue;
			}
			await wait(300, id);
			typed.append(tpl(part.piece));
			over(attached, part.over);
			attached.hidden = attached.childElementCount === 0;
			await wait(450, id);
		}
	}

	/** The composer empty again, once its message has gone. */
	function cleared() {
		typed.replaceChildren();
		attached.replaceChildren();
		attached.hidden = true;
		unreply();
	}

	/** The person's message in the chat, as the composer showed it: what it replies to and its
	 * file over its words, and the pieces in them (app.rs, person). */
	function sent(fresh: boolean, to: Replying | null) {
		const you = tpl('demo-you');
		you.classList.toggle('fresh', fresh);
		const bubble = you.querySelector<HTMLElement>('.bubble')!;
		const overs = document.createElement('div');
		for (const part of SEND) {
			if (typeof part === 'string') bubble.append(part);
			else {
				bubble.append(tpl(part.piece));
				over(overs, part.over);
			}
		}
		bubble.before(...overs.children);
		// What it replies to, over everything else, on one line: the agent's thread, who
		// ("You" quiet, with none), and the words, picked ones in curly quotes.
		if (to) {
			const quoted = tpl('demo-quote');
			const quote = quoted.querySelector<HTMLElement>('.quote')!;
			const mine = to.who === 'You';
			quote.classList.toggle('mine', mine);
			if (mine) quote.querySelector('.mention-thread')!.remove();
			quote.querySelector('.q-who')!.textContent = to.who;
			quote.querySelector('.q-words')!.textContent = to.part ? `“${to.words}”` : to.words;
			quote.setAttribute('aria-label', `Replying to ${to.who}: ${to.words}`);
			quote.addEventListener('click', () => jump(to.row));
			you.prepend(quoted);
		}
		return you;
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
		waitingOn('Juniper', false);
		cleared();
		await wait(900, id);
		// It replies to the earlier answer, from its Reply on hover.
		const earlier = history.find((el) => el.classList.contains('reply')) as HTMLElement;
		earlier.classList.add('hovered');
		await wait(700, id);
		earlier.classList.remove('hovered');
		replyTo(earlier);
		await wait(500, id);
		await compose(id);
		await wait(500, id);
		const to = replying;
		cleared();
		msgs.append(sent(true, to));
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
		waitingOn('Juniper', true);
		const deploy = tool(steps, ASK);
		say(`${ASK}…`);
		const answer = await approve(id);
		waitingOn('Juniper', false);

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
		cleared();
		const earlier = history.find((el) => el.classList.contains('reply')) as HTMLElement;
		const you = sent(false, { row: earlier, who: 'Juniper', words: wordsOf(earlier), part: false, ask: false });
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
		waitingOn('Juniper', false);
	}

	const reduced = matchMedia('(prefers-reduced-motion: reduce)');
	const start = () => (reduced.matches ? still() : loop());
	reduced.addEventListener('change', start);
	// The newest entry: a scroll before the first callback brings both in one batch.
	new IntersectionObserver((entries) => (onScreen = entries[entries.length - 1].isIntersecting)).observe(app);
	start();
}
