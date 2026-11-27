// The draft card in the Shop chat, as the desktop app's works it (desktop/src/draft_view.rs,
// PLAN.md "Drafts"): Send waits five seconds with Undo and then says Sent with the time,
// Send later says when it goes, Edit makes the rows and the body editable where they are,
// and Reject asks for a note and leaves one line. Nothing is sent anywhere.
// Send has Message sent's sound (sounds.ts).
import { cue } from './sounds';

/** How long Undo lasts after Send, in seconds (server/drafts.calx, UNDO_MS). */
const UNDO = 5;

export type Draft = {
	/** The card as the agent wrote it, open, before anything was done with it. */
	reset: () => void;
	shown: (on: boolean) => void;
};

export function draft(card: HTMLElement): Draft {
	const open = card.querySelector<HTMLElement>('.draft-open')!;
	const line = card.querySelector<HTMLElement>('.draft-line')!;
	const answers = card.querySelector<HTMLElement>('.draft-answers')!;
	const fold = card.querySelector<HTMLElement>('[data-draft-fold]')!;
	const later = card.querySelector<HTMLElement>('.draft-later')!;
	const note = card.querySelector<HTMLElement>('.draft-note')!;
	const foot = card.querySelector<HTMLElement>('.draft-foot')!;
	const edit = card.querySelector<HTMLElement>('[data-draft="edit"]')!;
	const reject = card.querySelector<HTMLElement>('[data-draft="reject"]')!;
	const undo = card.querySelector<HTMLElement>('[data-draft="undo"]')!;
	const how = card.querySelector<HTMLElement>('[data-draft-how]')!;
	const fields = [...card.querySelectorAll<HTMLElement>('[data-field]')];
	const field = (name: string) => fields.find((f) => f.dataset.field === name)!;
	const first = new Map(fields.map((f) => [f, f.textContent ?? '']));
	let timer = 0;

	const clock = () => new Date().toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
	const editing = (on: boolean) => {
		card.classList.toggle('editing', on);
		for (const f of fields) f.contentEditable = on ? 'plaintext-only' : 'false';
		edit.textContent = on ? 'Cancel' : 'Edit';
		reject.hidden = on;
		if (on) field('to').focus();
	};
	/** Its one line: the subject, where it went, and how it stands. */
	const gone = (words: string, sent: boolean, canUndo: boolean) => {
		clearInterval(timer);
		later.hidden = true;
		open.hidden = true;
		line.hidden = false;
		card.querySelector('[data-draft-what]')!.textContent = field('subject').textContent;
		card.querySelector('[data-draft-to]')!.textContent = `→ ${field('to').textContent}`;
		how.textContent = words;
		how.classList.toggle('sent', sent);
		undo.hidden = !canUndo;
	};
	const reopen = () => {
		clearInterval(timer);
		line.hidden = true;
		open.hidden = false;
	};
	const send = () => {
		editing(false);
		cue('sent');
		const at = clock();
		let left = UNDO;
		gone(`Sent ${at} ✓`, true, true);
		undo.textContent = `Undo · ${left}`;
		timer = window.setInterval(() => {
			left -= 1;
			if (left > 0) undo.textContent = `Undo · ${left}`;
			else gone(`Sent ${at} ✓`, true, false);
		}, 1000);
	};

	card.addEventListener('click', (e) => {
		const b = (e.target as HTMLElement).closest<HTMLElement>('button');
		if (!b || !card.contains(b)) return;
		if (b === fold) {
			answers.hidden = !answers.hidden;
			fold.textContent = `${answers.hidden ? '›' : '⌄'} The message it answers`;
			return;
		}
		if (b.dataset.later) {
			editing(false);
			gone(`Sends ${b.dataset.later.replace(/^In/, 'in').replace(/^Tomorrow/, 'tomorrow')}`, false, true);
			undo.textContent = 'Undo';
			return;
		}
		switch (b.dataset.draft) {
			case 'send':
				send();
				break;
			case 'later':
				later.hidden = !later.hidden;
				break;
			case 'edit':
				later.hidden = true;
				editing(!card.classList.contains('editing'));
				if (!card.classList.contains('editing')) for (const [f, text] of first) f.textContent = text;
				break;
			case 'reject':
				later.hidden = true;
				foot.hidden = true;
				note.hidden = false;
				note.querySelector('input')!.focus();
				break;
			case 'cancel':
				note.hidden = true;
				foot.hidden = false;
				break;
			case 'rejected':
				note.hidden = true;
				foot.hidden = false;
				gone('Rejected', false, false);
				break;
			case 'undo':
				reopen();
				break;
		}
	});

	return {
		reset: () => {
			reopen();
			editing(false);
			for (const [f, text] of first) f.textContent = text;
			answers.hidden = true;
			fold.textContent = '› The message it answers';
			later.hidden = true;
			note.hidden = true;
			foot.hidden = false;
		},
		shown: (on) => (card.hidden = !on),
	};
}
