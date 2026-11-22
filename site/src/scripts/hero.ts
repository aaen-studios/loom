// The agents' threads behind the wordmark, flat, as the app draws them (desktop/src/thread.rs, weave):
// each its own slow wave across the page, so they cross over and under one another, and the wordmark
// on top, with the threads showing through its holes.
//
// When the page opens they come in from the left together, as one cord, which comes apart behind the
// word into its seven colours, and then they keep drifting slowly.
import { now } from './thread';

const TAU = Math.PI * 2;
const COLORS = ['periwinkle', 'mint', 'apricot', 'pink', 'sky', 'gold', 'lavender'];

const clamp = (x: number, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const easeOut = (x: number) => 1 - Math.pow(1 - clamp(x), 3);
/** Eases out past the end a little and comes back, as a cord does when it springs apart. */
const easeOutBack = (x: number) => {
	const c = clamp(x) - 1;
	return 1 + 2.2 * c * c * c + 1.2 * c * c;
};

/** Draws the threads in canvas, behind mark's wordmark. */
export function startHero(mark: HTMLElement, canvas: HTMLCanvasElement) {
	const word = mark.querySelector<HTMLElement>('.word');
	const base = mark.querySelector<HTMLElement>('.word .base');
	const ctx = canvas.getContext('2d');
	if (!word || !base || !ctx) return;

	// Colours from the page's tokens, so they follow the theme.
	let colors: string[] = [];
	const paint = () => {
		const css = getComputedStyle(document.documentElement);
		colors = COLORS.map((c) => css.getPropertyValue(`--thread-${c}`).trim() || '#888');
	};

	let W = 0;
	let H = 0;
	let dpr = 1;
	let width = 2.2;
	let mid = 0;
	let half = 0;
	const layout = () => {
		const box = canvas.getBoundingClientRect();
		W = Math.round(box.width);
		H = Math.round(box.height);
		if (!W || !H) return false;
		dpr = Math.min(devicePixelRatio || 1, 2);
		canvas.width = Math.round(W * dpr);
		canvas.height = Math.round(H * dpr);
		const fs = parseFloat(getComputedStyle(word).fontSize) || 100;
		width = Math.max(1.6, fs * 0.024);
		// The band the threads run in: the letters' bodies, a little above the line.
		mid = base.getBoundingClientRect().top - box.top - 0.3 * fs;
		half = 0.72 * fs;
		return true;
	};

	const reduced = matchMedia('(prefers-reduced-motion: reduce)');
	const start = now();
	const n = COLORS.length;

	const frame = () => {
		const t = reduced.matches ? 30 : now() - start;
		// In together as one cord, then apart into each thread's own line.
		const apart = easeOutBack((t - 0.85) / 1.5);
		const g = 1 - apart;
		const step = 3;
		const count = Math.ceil((W + 40) / step);
		const firstLane = ((0 - (n - 1) / 2) / n) * 0.9;
		const loose = (k: number, u: number) => {
			const lane = ((k - (n - 1) / 2) / n) * 0.9;
			const amp = 0.55 + 0.25 * Math.sin(k * 1.7);
			const wave = 0.9 + 0.35 * Math.cos(k * 0.9);
			// Slow, so seven of them moving at once never pull the eye from the page.
			const speed = 0.045 + 0.02 * Math.abs(Math.sin(k * 2.3));
			// Calmer at the edges, so the threads come in from nowhere.
			const edge = Math.pow(Math.max(0, Math.sin(Math.PI * u)), 0.6);
			return lane + amp * edge * Math.sin(TAU * (u * wave - t * speed) + k * 1.37);
		};
		ctx.setTransform(1, 0, 0, 1, 0, 0);
		ctx.clearRect(0, 0, canvas.width, canvas.height);
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.lineWidth = width;
		ctx.lineCap = 'round';
		ctx.lineJoin = 'round';
		// The first thread on top, as the app has it.
		for (let k = n - 1; k >= 0; k--) {
			const shown = easeOut((t - 0.1 - k * 0.03) / 1.1);
			if (shown <= 0) continue;
			const last = Math.round(count * shown);
			ctx.beginPath();
			for (let i = 0; i <= last; i++) {
				const u = i / count;
				const own = loose(k, u);
				const first = loose(0, u) - firstLane;
				const v = (own + (first - own) * g) * (1 - g * 0.85);
				const x = -20 + i * step;
				const y = mid - Math.tanh(v) * half;
				if (i) ctx.lineTo(x, y);
				else ctx.moveTo(x, y);
			}
			ctx.strokeStyle = colors[k];
			ctx.stroke();
		}
	};

	// Draw while it's on screen and the page is showing; once with reduced motion.
	let visible = true;
	let raf = 0;
	const tick = () => {
		raf = 0;
		frame();
		if (!reduced.matches && visible && !document.hidden) raf = requestAnimationFrame(tick);
	};
	const kick = () => {
		if (!raf) raf = requestAnimationFrame(tick);
	};

	paint();
	if (!layout()) return;
	kick();
	new ResizeObserver(() => {
		if (layout()) kick();
	}).observe(mark);
	new IntersectionObserver(([e]) => {
		visible = e.isIntersecting;
		kick();
	}).observe(canvas);
	document.addEventListener('visibilitychange', kick);
	reduced.addEventListener('change', kick);
	// The theme: the footer's switch sets data-theme, and the system's can change under it.
	const repaint = () => {
		paint();
		kick();
	};
	new MutationObserver(repaint).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
	matchMedia('(prefers-color-scheme: dark)').addEventListener('change', repaint);
}
