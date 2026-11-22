// Agents' threads and servers' yarn, drawn as the desktop app draws them: a port of
// desktop/src/thread.rs, with the numbers from design/tokens.toml ([thread.motion.*]). A thread is
// one line whose movement says what its agent is doing. It only bends, twists and curls; nothing
// is ever added to it. It never jumps either: a new mood eases in over 0.7 seconds.
//
// Every thread on a page shares one animation loop, which runs only while a thread on screen is
// moving and the tab is visible. With reduced motion, each keeps its shape and holds still.

export type Mood = 'idle' | 'thinking' | 'writing' | 'tool' | 'waiting' | 'failed';

const NS = 'http://www.w3.org/2000/svg';
const TAU = Math.PI * 2;
const PI = Math.PI;
const EASE = 0.7;
const SNAP = 1.2;

const motion = {
	idle: { amplitude: 0.035, speed: 0.12 },
	thinking: { loopSpeed: 0.22, amplitude: 0.06 },
	writing: { wavelength: 15, speed: 1.9, amplitude: 0.28 },
	tool: { slideSpeed: 1.4, amplitude: 0.31 },
	waiting: { bobSpeed: 2.6, tugEvery: 3.9 },
	snap: { amplitude: 0.34, damping: 5, frequency: 28 },
	pluck: { amplitude: 0.32, damping: 3.2, frequency: 34 },
};
const PLUCK = 2;
const stroke = { small: 1.6, large: 2.2 };

const reduced = typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : undefined;
const t0 = performance.now();
/** One clock for every thread, so they move together. */
export const now = () => (performance.now() - t0) / 1000;

const ease = (x: number) => {
	const c = Math.min(1, Math.max(0, x));
	return c * c * (3 - 2 * c);
};
const fract = (x: number) => x - Math.floor(x);

/** The loop a thread curls into, r pixels round, centred at c along it. */
function curl(u: number, c: number, r: number, width: number): [number, number] {
	if (r <= 0 || width <= 0) return [0, 0];
	const span = Math.min((r * 2.2) / width, 0.9);
	const s = (u - c) / span + 0.5;
	if (s < 0 || s > 1) return [0, 0];
	const e = Math.sin(PI * s);
	return [r * Math.sin(TAU * s) * e, r * (1 - Math.cos(TAU * s)) * e * 0.5];
}

/** Where a thread in mood is at u (0 to 1 along it): pushed along it, and up, in pixels. */
function bend(mood: Mood, u: number, width: number, room: number, t: number, sinceSnap: number): [number, number] {
	const x = u * width;
	const loopR = Math.min(room * 0.9, width * 0.18);
	let up = 0;
	switch (mood) {
		case 'idle': {
			const base = Math.sin(TAU * (u * 1.3 + t * motion.idle.speed)) * motion.idle.amplitude * 2;
			const snap =
				sinceSnap < SNAP
					? motion.snap.amplitude * 2 * Math.exp(-motion.snap.damping * sinceSnap) * Math.sin(TAU * (motion.snap.frequency / 10) * sinceSnap) * Math.sin(PI * u)
					: 0;
			up = base + snap;
			break;
		}
		case 'thinking': {
			const head = 0.15 + 0.7 * fract(t * motion.thinking.loopSpeed);
			const [back, lift] = curl(u, head, loopR * 0.8, width);
			const wave = Math.sin(TAU * (u * 2 - t * 0.35)) * motion.thinking.amplitude * 4;
			return [back, wave * room + lift];
		}
		case 'writing':
			up = Math.sin(TAU * (x / motion.writing.wavelength - t * motion.writing.speed)) * motion.writing.amplitude * 2.4;
			break;
		case 'tool': {
			const swell = Math.sin(PI * fract(u - t * motion.tool.slideSpeed * 0.5));
			const wavelength = Math.max(room * 1.2, 7);
			up = Math.sin(TAU * (x / wavelength - t * motion.tool.slideSpeed * 2)) * swell * motion.tool.amplitude * 1.6;
			break;
		}
		case 'waiting': {
			const tug = t % motion.waiting.tugEvery < 0.25;
			const breath = 0.85 + 0.15 * Math.sin((TAU * t * motion.waiting.bobSpeed) / 4);
			return curl(u, 0.5, loopR * breath * (tug ? 0.8 : 1), width);
		}
		case 'failed':
			up = -0.55 * u * u;
			break;
	}
	return [0, up * room];
}

export interface ThreadOptions {
	w: number;
	h: number;
	/** The thread's colour: sky, mint, apricot, … from the tokens. */
	color: string;
	mood?: Mood;
}

export class Thread {
	readonly svg: SVGSVGElement;
	w: number;
	h: number;
	color: string;
	private from: Mood;
	private to: Mood;
	private at = -99;
	private snapAt = -99;
	private pluckAt = -99;
	private pluckAmp = 1;
	private path: SVGPathElement;
	visible = true;

	constructor(svg: SVGSVGElement, o: ThreadOptions) {
		this.svg = svg;
		this.w = o.w;
		this.h = o.h;
		this.color = o.color;
		this.from = this.to = o.mood ?? 'idle';
		svg.setAttribute('viewBox', `0 0 ${o.w} ${o.h}`);
		svg.setAttribute('fill', 'none');
		this.path = document.createElementNS(NS, 'path');
		this.path.setAttribute('stroke-linejoin', 'round');
		svg.append(this.path);
		// The colour is the token's variable, so it follows the theme by itself.
		this.path.style.stroke = `var(--thread-${o.color})`;
		loop.add(this);
	}

	get mood() {
		return this.to;
	}

	/** A new mood, eased into. A run that ends snaps taut once on its way back to idle. */
	set(mood: Mood) {
		if (mood === this.to) return;
		const t = now();
		const k = (t - this.at) / EASE;
		this.from = k < 0.5 ? this.from : this.to;
		if (mood === 'idle' && this.to !== 'failed') this.snapAt = t;
		this.to = mood;
		this.at = t;
		loop.kick();
	}

	/** A one-time snap, as when a run finishes or a chat opens. */
	snap() {
		this.snapAt = now();
		loop.kick();
	}

	/** A new colour (one of the tokens' thread colours). */
	tint(color: string) {
		this.color = color;
		this.path.style.stroke = `var(--thread-${color})`;
	}

	/** Hovering a thread plucks it: a standing wave that dies away ([thread.motion.pluck]). */
	pluck(strength = 1) {
		this.pluckAt = now();
		this.pluckAmp = strength;
		loop.kick();
	}

	/** Whether the thread needs frames just now. */
	get moving() {
		const t = now();
		return (this.to !== 'idle' && this.to !== 'failed') || t - this.snapAt < SNAP || t - this.at < EASE || t - this.pluckAt < PLUCK;
	}

	draw(clock: number) {
		const still = reduced?.matches ?? false;
		const t = still ? 1.7 : clock;
		const { w, h } = this;
		const width = h > 12 ? stroke.large : stroke.small;
		const k = still ? 1 : ease((clock - this.at) / EASE);
		const sinceSnap = still ? 99 : clock - this.snapAt;
		const room = Math.max(h / 2 - width, 1);
		const steps = Math.max(48, Math.round(w / 0.75));
		const failed = (this.from === 'failed' ? 1 - k : 0) + (this.to === 'failed' ? k : 0);
		const end = 1 - 0.2 * failed;
		const sincePluck = still ? 99 : clock - this.pluckAt;
		const pluck =
			sincePluck < PLUCK ? room * 2 * motion.pluck.amplitude * this.pluckAmp * Math.exp(-motion.pluck.damping * sincePluck) * Math.sin(sincePluck * motion.pluck.frequency) : 0;
		let d = '';
		for (let i = 0; i <= steps; i++) {
			const u = (end * i) / steps;
			const [ax, ay] = bend(this.from, u, w, room, t, sinceSnap);
			const [bx, by] = bend(this.to, u, w, room, t, sinceSnap);
			const dx = ax + (bx - ax) * k;
			const up = ay + (by - ay) * k + pluck * Math.sin(PI * u);
			d += `${i ? 'L' : 'M'}${(u * w + dx).toFixed(2)} ${(h / 2 - up).toFixed(2)}`;
		}
		this.path.setAttribute('d', d);
		this.path.setAttribute('stroke-width', String(width));
	}
}

/** The one animation loop. */
const loop = (() => {
	const threads = new Set<Thread>();
	let raf = 0;
	const seen =
		typeof IntersectionObserver === 'function'
			? new IntersectionObserver((entries) => {
					for (const e of entries) for (const th of threads) if (th.svg === e.target) th.visible = e.isIntersecting;
					kick();
				})
			: undefined;
	function frame() {
		raf = 0;
		const c = now();
		let more = false;
		for (const th of threads) {
			if (!th.svg.isConnected) {
				threads.delete(th);
				seen?.unobserve(th.svg);
				continue;
			}
			if (!th.visible) continue;
			th.draw(c);
			if (th.moving) more = true;
		}
		if (more && !reduced?.matches && !document.hidden) raf = requestAnimationFrame(frame);
	}
	function kick() {
		if (!raf) raf = requestAnimationFrame(frame);
	}
	reduced?.addEventListener('change', kick);
	document.addEventListener('visibilitychange', kick);
	return {
		add(th: Thread) {
			threads.add(th);
			seen?.observe(th.svg);
			th.draw(now());
			kick();
		},
		kick,
	};
})();

/**
 * Starts a thread in every `<svg data-thread>` under root: its colour from data-color, its mood
 * from data-mood, and its size from its width and height attributes.
 */
export function threadsIn(root: ParentNode = document) {
	const made: Thread[] = [];
	for (const svg of root.querySelectorAll<SVGSVGElement>('svg[data-thread]')) {
		if (threads.has(svg)) continue;
		const th = new Thread(svg, {
			w: Number(svg.getAttribute('width')) || 26,
			h: Number(svg.getAttribute('height')) || 10,
			color: svg.dataset.color ?? 'grey',
			mood: (svg.dataset.mood as Mood) ?? 'idle',
		});
		threads.set(svg, th);
		// Hovering what the thread belongs to (its row, its button) plucks it.
		svg.parentElement?.addEventListener('pointerenter', () => th.pluck(0.8));
		made.push(th);
	}
	return made;
}

const threads = new WeakMap<SVGSVGElement, Thread>();

/** The thread drawn in svg, once threadsIn has started it. */
export const threadOf = (svg: Element | null) => (svg ? threads.get(svg as SVGSVGElement) : undefined);

// ---------------------------------------------------------------------------------------------
// A server's ball of yarn (thread.rs, yarn), at rest as a connected one is: a soft ball in a pale
// tint with four strands of the tint wound round it, each tilted its own way by a number from the
// server's name.

const MASK = (1n << 64n) - 1n;
const YARN_COLORS = ['periwinkle', 'apricot', 'mint', 'pink', 'sky', 'gold', 'lavender'];

/** FNV-1a of the name, as thread.rs's seed_of. */
function seedOf(name: string) {
	let h = 0xcbf29ce484222325n;
	for (const b of new TextEncoder().encode(name)) h = ((h ^ BigInt(b)) * 0x100000001b3n) & MASK;
	return h;
}

/** Draws the yarn of the server named name in every `<svg data-yarn="name">` under root. */
export function yarnsIn(root: ParentNode = document) {
	for (const svg of root.querySelectorAll<SVGSVGElement>('svg[data-yarn]')) {
		if (svg.dataset.ready) continue;
		svg.dataset.ready = '1';
		const name = svg.dataset.yarn ?? '';
		const seed = seedOf(name);
		const color = YARN_COLORS[Number(seed % 7n)];
		const s = Number(svg.getAttribute('width')) || 32;
		svg.setAttribute('viewBox', `0 0 ${s} ${s}`);
		const c = s / 2;
		const r = s * 0.39;
		const line = Math.max(s * 0.06, 1.2);
		// The strands: the front half of a turn round the ball, tilted and squashed.
		let rng = seed | 1n;
		const next = () => {
			rng ^= (rng << 13n) & MASK;
			rng ^= rng >> 7n;
			rng ^= (rng << 17n) & MASK;
			return Number(rng >> 40n) / 2 ** 24;
		};
		let strands = '';
		for (let n = 0; n < 4; n++) {
			const tilt = next() * TAU;
			const squash = 0.2 + next() * 0.6;
			const [co, sn] = [Math.cos(tilt), Math.sin(tilt)];
			for (let i = 0; i <= 24; i++) {
				const u = (PI * i) / 24;
				const lx = r * 0.92 * Math.cos(u);
				const ly = r * 0.92 * squash * Math.sin(u);
				strands += `${i ? 'L' : 'M'}${(c + lx * co - ly * sn).toFixed(2)} ${(c + lx * sn + ly * co).toFixed(2)}`;
			}
		}
		const ball = document.createElementNS(NS, 'circle');
		ball.setAttribute('cx', String(c));
		ball.setAttribute('cy', String(c));
		ball.setAttribute('r', String(r * 1.04));
		const wound = document.createElementNS(NS, 'path');
		wound.setAttribute('d', strands);
		wound.setAttribute('fill', 'none');
		wound.setAttribute('stroke-width', String(line));
		// Pale tint on the sidebar's colour, as thread.rs's mix(tint, side, 0.78).
		ball.style.fill = `color-mix(in srgb, var(--thread-${color}) 22%, var(--side))`;
		wound.style.stroke = `var(--thread-${color})`;
		svg.append(ball, wound);
	}
}
