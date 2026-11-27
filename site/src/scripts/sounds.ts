// loom's sounds in the demo (PLAN.md, "Sounds"): the app's own files from design/sounds, played
// where the app plays them. A browser lets a page play sound only after the visitor has clicked
// in it, so nothing sounds before the first click anywhere on the page.
import reply from '../../../design/sounds/loom-reply.wav?url';
import waiting from '../../../design/sounds/loom-waiting.wav?url';
import routine from '../../../design/sounds/loom-routine.wav?url';
import failed from '../../../design/sounds/loom-failed.wav?url';
import sent from '../../../design/sounds/loom-sent.wav?url';
import dictationOn from '../../../design/sounds/loom-dictation-on.wav?url';
import dictationOff from '../../../design/sounds/loom-dictation-off.wav?url';

/** Each cue's file. Another agent answering in a group chat has the Reply cue's. */
const FILES = {
	reply,
	waiting,
	other: reply,
	routine,
	failed,
	sent,
	'dictation-on': dictationOn,
	'dictation-off': dictationOff,
} as const;

export type Cue = keyof typeof FILES;

/** The app's volume as it starts, 80%. */
const VOLUME = 0.8;

let audio: AudioContext | null = null;
const decoded = new Map<string, Promise<AudioBuffer | null>>();

function buffer(url: string): Promise<AudioBuffer | null> {
	let b = decoded.get(url);
	if (!b) {
		b = fetch(url)
			.then((r) => r.arrayBuffer())
			.then((bytes) => audio!.decodeAudioData(bytes))
			.catch(() => null);
		decoded.set(url, b);
	}
	return b;
}

/** From the visitor's first click on the page on, the demo may sound. */
export function listen() {
	document.addEventListener(
		'pointerdown',
		() => {
			if (audio || typeof AudioContext === 'undefined') return;
			audio = new AudioContext();
			for (const url of new Set(Object.values(FILES))) void buffer(url);
		},
		{ capture: true, once: true },
	);
}

/** Plays a cue, once the page may sound. */
export function cue(c: Cue) {
	if (!audio) return;
	const ctx = audio;
	void buffer(FILES[c]).then((b) => {
		if (!b) return;
		const source = ctx.createBufferSource();
		const gain = ctx.createGain();
		gain.gain.value = VOLUME;
		source.buffer = b;
		source.connect(gain).connect(ctx.destination);
		source.start();
	});
}
