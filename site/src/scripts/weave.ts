// The agents' threads woven through the wordmark, as the app opens: a port of desktop/src/thread.rs
// (weave and weave_over). Each thread is its own slow wave across a band, so they cross over and
// under one another; over the word, each goes over one letter and under the next, neighbours taking
// turns. They come in from the left when the page opens, and as the page scrolls on they gather on
// to the first thread's line and calm, until there's only that one.
import { now } from './thread';

const NS = 'http://www.w3.org/2000/svg';
const TAU = Math.PI * 2;
const STROKE = 2.2;
/** How long the threads take to come in, in seconds. */
const IN = 1.4;

const ease = (x: number) => 1 - Math.pow(1 - Math.min(1, Math.max(0, x)), 3);
const smooth = (x: number) => {
	const c = Math.min(1, Math.max(0, x));
	return c * c * (3 - 2 * c);
};

/** Whether thread k is over the word at u, 0 to 1 across it: four stretches, one a letter of "loom". */
const overWord = (k: number, u: number) => u >= 0 && u < 1 && (Math.floor(u * 4) + k) % 2 === 0;

/**
 * Weaves threads of these colours through word: under is the band drawn behind it, and over the
 * same band in front, which draws only the stretches that pass over it.
 */
export function weave(under: SVGSVGElement, over: SVGSVGElement, word: HTMLElement, colors: string[]) {
	const band = under;
	const n = colors.length;
	const make = (svg: SVGSVGElement) => {
		const paths = colors.map((c) => {
			const p = document.createElementNS(NS, 'path');
			p.setAttribute('fill', 'none');
			p.setAttribute('stroke-width', String(STROKE));
			p.setAttribute('stroke-linejoin', 'round');
			p.style.stroke = `var(--thread-${c})`;
			return p;
		});
		// Drawn back to front, so the first colour is the one on top.
		for (const p of [...paths].reverse()) svg.append(p);
		return paths;
	};
	const underPaths = make(under);
	const overPaths = make(over);

	const reduced = matchMedia('(prefers-reduced-motion: reduce)');
	const start = now();
	let gather = 0;
	let visible = true;
	let raf = 0;

	function draw() {
		raf = 0;
		const still = reduced.matches;
		const t = still ? 3 : now();
		const reveal = still ? 1 : ease((t - start) / IN);
		const b = band.getBoundingClientRect();
		const { width, height } = b;
		if (!width || !height) return;
		for (const svg of [under, over]) svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
		const mid = height / 2;
		const half = height / 2 - STROKE;
		const w = word.getBoundingClientRect();
		const wordLeft = w.left - b.left;
		const wordWidth = w.width;
		const steps = Math.max(16, Math.round(width / 3));
		const g = smooth(gather);
		// Where thread k is at u, loose, before any gathering.
		const loose = (k: number, u: number) => {
			const lane = ((k - (n - 1) / 2) / n) * 0.9;
			const amp = 0.55 + 0.25 * Math.sin(k * 1.7);
			const wave = 0.9 + 0.35 * Math.cos(k * 0.9);
			const speed = 0.1 + 0.04 * Math.abs(Math.sin(k * 2.3));
			// Calmer at the edges, so the threads come in from nowhere.
			const edge = Math.pow(Math.max(0, Math.sin(Math.PI * u)), 0.6);
			return lane + amp * edge * Math.sin(TAU * (u * wave - t * speed) + k * 1.37);
		};
		const firstLane = ((0 - (n - 1) / 2) / n) * 0.9;
		for (let i = 0; i < n; i++) {
			// Each comes in a little after the one before.
			const shown = ease(Math.min(1, Math.max(0, reveal * 1.5 - (i / n) * 0.5)));
			const last = Math.floor(steps * shown);
			let du = '';
			let dv = '';
			let pen = false;
			if (shown > 0) {
				for (let s = 0; s <= Math.max(last, 1); s++) {
					const u = s / steps;
					const first = loose(0, u) - firstLane;
					const own = loose(i, u);
					const y = (own + (first - own) * g) * (1 - g * 0.85);
					const x = u * width;
					const py = mid - Math.tanh(y) * half;
					const pt = `${x.toFixed(1)} ${py.toFixed(1)}`;
					du += (s ? 'L' : 'M') + pt;
					if (overWord(i, (x - wordLeft) / wordWidth)) {
						dv += (pen ? 'L' : 'M') + pt;
						pen = true;
					} else pen = false;
				}
			}
			underPaths[i].setAttribute('d', du);
			overPaths[i].setAttribute('d', dv);
		}
		if (!still && visible && !document.hidden) raf = requestAnimationFrame(draw);
	}
	const kick = () => {
		if (!raf) raf = requestAnimationFrame(draw);
	};

	// Gathered as the hero scrolls away.
	const onScroll = () => {
		gather = Math.min(1, Math.max(0, scrollY / 420));
		if (reduced.matches) draw();
	};
	addEventListener('scroll', onScroll, { passive: true });
	addEventListener('resize', kick);
	new IntersectionObserver(([e]) => {
		visible = e.isIntersecting;
		kick();
	}).observe(band);
	document.addEventListener('visibilitychange', kick);
	reduced.addEventListener('change', kick);
	// The word's font may still be loading; draw again once it has.
	document.fonts?.ready.then(kick);
	onScroll();
	draw();
}
