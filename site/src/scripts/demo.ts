// Plays the live copy of the app (src/components/AppDemo.astro) in a loop: a message is typed and
// sent, the agent thinks, uses a tool, answers, asks before it deploys and waits for the yes, then
// finishes. The moods, the sidebar's previews ("Thinking", the tool's title, "Wants to …") and the
// wording are the app's (desktop/src/model.rs). Clicking Allow once or Deny answers the approval;
// left alone, it's allowed after a moment.
//
// It plays only while it's on screen, and with reduced motion it shows the finished chat instead.
import { Thread, threadsIn, yarnsIn, type Mood } from './thread';

const SEND = 'Is main green? If it is, ship it.';
const TOOL = 'Fetch github.com/acme/shop/actions';
const ANSWER = 'Main is green. The last run passed at 09:12, with all four jobs.';
const ASK = 'Run ./deploy.sh production';
const COMMAND = './deploy.sh production';
const SHIPPED = 'Shipped. 2.4.1 is live, and its health check answers.';
const HELD = "Okay, I won't deploy. Main stays as it is.";

/** A pause that ends early, with an error, when the demo is stopped. */
class Stopped extends Error {}

export function play(app: HTMLElement) {
	const threads = threadsIn(app);
	yarnsIn(app);
	const byAgent = new Map<string, Thread>();
	for (const row of app.querySelectorAll<HTMLElement>('.agent')) {
		const svg = row.querySelector<SVGSVGElement>('svg[data-thread]')!;
		const th = threads.find((t) => t.svg === svg);
		if (th) byAgent.set(row.dataset.agent!, th);
	}
	const head = threads.find((t) => t.svg.hasAttribute('data-head'))!;
	const msgs = app.querySelector<HTMLElement>('.msgs')!;
	const history = [...msgs.children];
	const typed = app.querySelector<HTMLElement>('.typed')!;
	const tpl = (id: string) => (document.getElementById(id) as HTMLTemplateElement).content.firstElementChild!.cloneNode(true) as HTMLElement;
	const preview = (agent: string, text: string, mood: Mood) => {
		const row = app.querySelector<HTMLElement>(`.agent[data-agent="${agent}"]`);
		const p = row?.querySelector<HTMLElement>('.agent-preview');
		if (p) p.textContent = text;
		byAgent.get(agent)?.set(mood);
	};

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

	async function stream(into: HTMLElement, text: string, id: number) {
		const p = document.createElement('p');
		into.append(p);
		const words = text.split(' ');
		for (let i = 1; i <= words.length; i++) {
			p.textContent = words.slice(0, i).join(' ');
			await wait(45 + Math.random() * 70, id);
		}
	}

	/** The approval, answered by a click or, after a while, by itself. */
	function approve(id: number) {
		const row = tpl('demo-approval');
		row.querySelector('.card-title')!.textContent = `${ASK}?`;
		row.querySelector('.card-cmd')!.textContent = COMMAND;
		msgs.append(row);
		return new Promise<'allow' | 'deny'>((done, fail) => {
			let answered = false;
			const answer = (a: 'allow' | 'deny', button: HTMLElement) => {
				if (answered) return;
				answered = true;
				button.classList.add('pressed');
				setTimeout(() => {
					row.classList.add('leaving');
					setTimeout(() => row.remove(), 350);
					done(a);
				}, 220);
			};
			for (const b of row.querySelectorAll<HTMLButtonElement>('button[data-answer]')) {
				b.addEventListener('click', () => answer(b.dataset.answer as 'allow' | 'deny', b));
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
		const words = reply.querySelector<HTMLElement>('.waiting-words')!;
		const said = reply.querySelector<HTMLElement>('.said')!;
		const time = reply.querySelector<HTMLElement>('.who span')!;

		mood('thinking', replyThread);
		preview('Juniper', 'Thinking', 'thinking');
		words.textContent = 'Thinking…';
		await wait(1700, id);

		mood('tool', replyThread);
		preview('Juniper', TOOL, 'tool');
		words.textContent = `${TOOL}…`;
		await wait(1600, id);

		words.textContent = '';
		mood('writing', replyThread);
		preview('Juniper', '', 'writing');
		await stream(said, ANSWER, id);
		await wait(300, id);

		mood('waiting', replyThread);
		preview('Juniper', `Wants to ${ASK[0].toLowerCase()}${ASK.slice(1)}`, 'waiting');
		const answer = await approve(id);

		if (answer === 'allow') {
			mood('tool', replyThread);
			preview('Juniper', ASK, 'tool');
			await wait(1900, id);
			mood('writing', replyThread);
			preview('Juniper', '', 'writing');
			await stream(said, SHIPPED, id);
		} else {
			mood('writing', replyThread);
			preview('Juniper', '', 'writing');
			await stream(said, HELD, id);
		}
		await wait(200, id);
		mood('idle', replyThread);
		preview('Juniper', '', 'idle');
		time.textContent = '09:14';
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
		reply.querySelector('.who span')!.textContent = '09:14';
		for (const text of [ANSWER, SHIPPED]) {
			const p = document.createElement('p');
			p.textContent = text;
			reply.querySelector('.said')!.append(p);
		}
		msgs.append(you, reply);
		threadsIn(reply);
		mood('idle');
		for (const a of byAgent.keys()) preview(a, '', 'idle');
	}

	const reduced = matchMedia('(prefers-reduced-motion: reduce)');
	const start = () => (reduced.matches ? still() : loop());
	reduced.addEventListener('change', start);
	new IntersectionObserver(([e]) => (onScreen = e.isIntersecting)).observe(app);
	start();
}
