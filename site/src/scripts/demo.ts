// Plays the live copy of the app (src/components/AppDemo.astro) in a loop: the earlier answer's
// Reply, by its corner on hover, puts it over the composer's words, and the message that goes
// has its quote over its bubble, which goes back to the answer and lights it (desktop/src/app.rs,
// reply_bar, quote_view, jump_to), while the reply comes: the round button at the bottom of the
// chat counts it in the agent's colour and takes the chat back down (app.rs, jump_button); a
// message's Reply, or words selected in one with the bar
// that comes over them (Copy, Quote, Ask about this), does the same by hand. A message is typed,
// with a link pasted and a file attached among its words, each a piece in them, the file with its
// pill over them, and sent, showing them the same way; the agent thinks, uses a tool, writes,
// asks before it deploys and waits for the yes, then finishes, starting a T3 thread for the
// release notes whose card works and then lies straight with its pull request (desktop/src/
// t3_view.rs, thread_card). Sam then moves the notes' card to Done, which the watch on the shop
// board says in a quiet line, its card's pill and "moved to Done · Sam", and the reply of
// Juniper, which it woke, follows it (watch_view.rs, update_line). Once it's shipped, `:tada` typed
// opens the emoji row over the composer, which puts 🎉 in the words as Twemoji's picture, and a
// message of only it is big (emoji_ui.rs, emoji_row_view; PLAN.md, "Emoji"). While the turn runs, one quiet
// line under the reply says the step it's on; once it's done, its last words are the answer and
// the steps before them fold under one line that opens (desktop/src/model.rs, parts), and a
// subagent's call in the earlier reply opens to show what its subagent did inside it (model.rs,
// inside). The earlier reply's sources tab opens their list (app.rs, sources_tab), and the Acta
// card it names is a pill whose preview opens under it under the pointer, which the page's CSS
// does (app.rs, card_pill, card_preview). The moods,
// what the sidebar says after an agent's name on its one line ("Thinking", the tool's title,
// "Wants to …"), or under the pointer on a pinned agent's tile, and the wording are the app's.
// Clicking Allow, Always allow (under the arrow beside Allow) or Deny answers the approval,
// which says how in one quiet line before it goes; left alone, it's allowed after a moment.
//
// The file goes in from the + menu's Recent files, the list it opens beside it (app.rs,
// plus_menu, plus_sub); the + opens the menu by hand too. Under the finished reply, what Juniper
// kept, "Fixed the skill deploy-shop", opens to say why and with Undo (learned.rs). The session
// is past 80% by then, so the next message starts a fresh one, with loom's line over it, and
// Compact now and Start a fresh session in the ring's card leave theirs (PLAN.md, "One endless
// chat").
//
// Then the Shop group chat opens from its row, and a message written there goes to the agent the
// router picks: the line over the reply, "→ Biscuit · invoices", in Biscuit's shape and colour,
// comes with it, before its words (room_view.rs, route_line). Its row opens it by hand too. Its
// header has its own pins and the context of the member that answered last.
//
// Then Moss's chat opens from its row, with "3 new" over the replies that came after the person
// left, a table whose arrow copies it as Markdown, and the draft it wrote yesterday cut after ten
// lines; Ctrl+F finds "Northwind" in it, newest first, stepping back to the draft, which opens
// while the find is on it and folds again once it's closed (app.rs, find_box, cut_long,
// table_line; model::folds). Ctrl+F over the demo opens the find by hand.
//
// The sidebar's control collapses it to the rail and opens it again, and a narrow window has the
// rail, as the app's does (desktop/src/rail.rs). "From Biscuit" under Juniper opens Biscuit's
// thread with it in the chat's place, to read, with Continue in your own chat for a composer. The
// header's pin opens the chat's pins in a popover under it (app.rs, pins_pop), with Jump, Copy
// and Unpin on each, as Ctrl+Shift+P does. A message's pin by its corner, or Pin in its ⋯
// menu, pins it or takes it off. Delete in that menu leaves a quiet mark where it was.
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
const SHIPPED = 'Shipped. 2.4.1 is live, and its health check answers. I started a T3 thread for its release notes.';
/** How long the T3 thread it starts has run, as its card says it, while it works and once it's done. */
const T3_MINUTES = 6;
/** Juniper's reply once the watch on the shop board wakes it: Sam moved the notes' card to Done. */
const WOKEN = 'SHOP-31 is done, so the 2.4.1 notes are in the changelog and the store listing.';
const HELD = "Okay, I won't deploy. Main stays as it is.";
/** What Moss's chat finds with Ctrl+F. */
const FIND = 'Northwind';
/** A long message is cut after this many lines, and only when it runs a line and a half past
 * them (app.rs, FOLD_LINES, cut_long). */
const FOLD_LINES = 10;
/** How long a notice stays at the window's foot (app.rs, NOTICE_SECONDS). */
const NOTICE_MS = 8000;
type Chat = 'juniper' | 'shop' | 'moss';
/** How full each chat's session is, in thousands of tokens of 200k, what its card's title
 * says, and its line: in the Shop group chat, the member that answered last's. */
type Fill = { used: number; title: string; line: string; who?: string };
const FRESH_AT = 'a fresh session starts at 80%';
/** What the person writes in the Shop group chat, which the router gives Biscuit, and its answer. */
const ROOM_ASK = 'Did the October invoice go out?';
const ROOM_ANSWER = 'It did, on 1 November. Northwind paid it on the 14th.';

/** Lucide's crossed-out pin, as the app's Unpin has it (Ico::PinOff). */
const UNPIN_ICON =
	'<path d="M12 17v5"></path><path d="M15 9.34V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H7.89"></path><path d="m2 2 20 20"></path><path d="M9 9v1.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h11"></path>';

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
	// An Acta card's preview, which the page's CSS opens under its pill, goes over it when
	// there's no room under it in the chat, as the app keeps a popover in the window.
	function placed(pill: HTMLElement) {
		const preview = pill.querySelector<HTMLElement>('.acta-preview');
		if (!preview) return;
		const place = () => {
			const chat = pill.closest<HTMLElement>('.chat');
			if (!chat) return;
			preview.classList.remove('up');
			preview.style.display = 'flex';
			const tall = preview.offsetHeight;
			preview.style.display = '';
			if (pill.getBoundingClientRect().bottom + tall + 8 > chat.getBoundingClientRect().bottom) preview.classList.add('up');
		};
		pill.addEventListener('pointerenter', place);
		pill.addEventListener('focus', place);
	}
	for (const pill of app.querySelectorAll<HTMLElement>('.acta-pill')) placed(pill);
	const typed = app.querySelector<HTMLElement>('.typed')!;
	const attached = app.querySelector<HTMLElement>('[data-attached]')!;
	const composer = app.querySelector<HTMLElement>('.composer')!;

	/** A message of only 🎉, which the chat shows big (PLAN.md, "Emoji"). */
	function cheer(fresh: boolean) {
		const you = tpl('demo-you');
		you.classList.toggle('fresh', fresh);
		const tada = tpl('demo-tada');
		tada.classList.add('jumbo');
		you.querySelector('.bubble')!.append(tada);
		return you;
	}

	// How full Juniper's session context is, after its model, in thousands of tokens of its
	// 200k: it fills a little with each turn (app.rs, context_ring; model.rs, ContextFill).
	// Its card shows under the pointer and stays once the ring is clicked, until a click
	// outside, Esc or the ring again (app.rs, context_hover, toggle_context).
	const ringAt = app.querySelector<HTMLElement>('.context-at')!;
	const ring = ringAt.querySelector<HTMLButtonElement>('[data-context]')!;
	const card = ringAt.querySelector<HTMLElement>('[data-context-card]')!;
	const cardLine = card.querySelector<HTMLElement>('[data-context-line]')!;
	let cardKept = false;
	let cardLeft = 0;
	// The chat that's open, and each one's session: Juniper's, the Shop group chat's member
	// that answered last ("Juniper's context"), and Moss's, which its program compacted
	// yesterday (model.rs, context_title, ContextFill::line).
	let chat: Chat = 'juniper';
	const fills: Record<Chat, Fill> = {
		juniper: { used: 82, title: 'Context', line: `Compacted 2 h ago · ${FRESH_AT}` },
		shop: { used: 23, title: "Juniper's context", line: `A ${FRESH_AT}`, who: 'Juniper' },
		moss: { used: 24, title: 'Context', line: `Compacted 15 h ago · ${FRESH_AT}` },
	};
	const drawContext = () => {
		const { used, title, line } = fills[chat];
		const size = 200;
		const percent = Math.floor((used * 100) / size);
		ring.querySelector<SVGElement>('.fill')!.style.setProperty('--fill', String(percent));
		card.querySelector<HTMLElement>('[data-context-bar]')!.style.setProperty('--fill', `${percent}%`);
		card.querySelector<HTMLElement>('[data-context-amount]')!.textContent = `${used}k of ${size}k`;
		card.querySelector<HTMLElement>('.cc-title')!.textContent = title;
		card.setAttribute('aria-label', title);
		cardLine.textContent = line;
		ringAt.classList.toggle('warn', percent >= 70 && percent < 90);
		ringAt.classList.toggle('bad', percent >= 90);
		ring.setAttribute('aria-label', `Context ${percent}%, ${used}k of ${size}k`);
	};
	/** Sets how full a chat's session is, and its card's line, and draws it if it's open. */
	function context(used: number, of: Chat = 'juniper', line?: string) {
		fills[of].used = used;
		if (line !== undefined) fills[of].line = line;
		if (of === chat) drawContext();
	}
	/** Where the card hangs: under the ring, centred on it, and kept 8px inside the window
	 * (app.rs, context_ring, snap_to_window_with_margin). */
	const placeCard = () => {
		const frame = app.getBoundingClientRect();
		const at = ring.getBoundingClientRect();
		const room = app.clientWidth;
		const width = Math.min(300, room - 16);
		const middle = (at.left + at.right) / 2 - frame.left - app.clientLeft;
		const left = Math.min(Math.max(middle - width / 2, 8), room - 8 - width);
		card.style.setProperty('--card-left', `${left}px`);
		card.style.setProperty('--card-width', `${width}px`);
		card.style.setProperty('--card-top', `${at.bottom - frame.top - app.clientTop + 4}px`);
	};
	const showCard = (open: boolean, kept = false) => {
		if (open) placeCard();
		card.hidden = !open;
		cardKept = open && kept;
		ring.setAttribute('aria-expanded', String(cardKept));
	};
	ringAt.addEventListener('pointerenter', () => {
		cardLeft++;
		if (card.hidden) showCard(true);
	});
	ringAt.addEventListener('pointerleave', () => {
		const left = ++cardLeft;
		setTimeout(() => {
			if (!cardKept && left === cardLeft) showCard(false);
		}, 160);
	});
	ring.addEventListener('click', () => showCard(!cardKept, !cardKept));
	document.addEventListener('pointerdown', (e) => {
		if (cardKept && !ringAt.contains(e.target as Node)) showCard(false);
	});
	document.addEventListener('keydown', (e) => {
		if (e.key === 'Escape' && !card.hidden) showCard(false);
	});
	// Compact now leaves a tenth, as a compaction does, and loom's line where it happened; a
	// fresh session starts from nothing, with its line where it starts. In the group chat both
	// name the member (model.rs, mark_text).
	for (const b of card.querySelectorAll<HTMLButtonElement>('[data-context-act]')) {
		b.addEventListener('click', () => {
			const compact = b.dataset.contextAct === 'compact';
			const before = fills[chat].used;
			const after = compact ? Math.max(1, Math.round(before / 10)) : 0;
			const who = fills[chat].who;
			context(after, chat, compact ? `Compacted just now · ${FRESH_AT}` : `A ${FRESH_AT}`);
			if (compact) loomLine(who ? `${who}'s context compacted · ${before}k → ${after}k` : `Context compacted · ${before}k → ${after}k`);
			else loomLine(who ? `${who}'s fresh session starts here` : 'A fresh session starts here');
			showCard(false);
		});
	}
	/** A line from loom across the open chat, at its end (app.rs, chat_row). */
	function loomLine(words: string, into?: HTMLElement) {
		const line = tpl('demo-loom-line');
		line.querySelector('span')!.textContent = words;
		(into ?? chatMsgs()).append(line);
		return line;
	}

	// Replying (PLAN.md, "Replying to a message"): a message's Reply, or words selected in it,
	// put a bar over the composer's words; the message goes with a quote over its bubble, which
	// goes back to the original and lights it (app.rs, reply_bar, quote_view, jump_to).
	const replyBar = app.querySelector<HTMLElement>('[data-reply-bar]')!;
	type Replying = { row: HTMLElement; who: string; words: string; part: boolean; ask: boolean };
	let replying: Replying | null = null;
	/** A message's words, on one line: an agent's answer, or what the person wrote, with an
	 *  Acta card's pill as its id, as it was written (PLAN.md, "Cards in messages"). */
	const wordsOf = (row: HTMLElement) => {
		const said = (row.querySelector('.said:not(.on-the-way)') ?? row.querySelector('.bubble'))?.cloneNode(true) as HTMLElement | undefined;
		for (const pill of said?.querySelectorAll<HTMLElement>('.acta-pill') ?? []) pill.replaceWith(pill.dataset.card ?? '');
		return said?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
	};
	/** Who said a message: its agent's name over it, or in a group chat the line's, or "You". */
	const whoOf = (row: HTMLElement) =>
		row.classList.contains('reply') ? (row.querySelector('.who, .routed-who')?.textContent?.trim() ?? 'Juniper') : 'You';
	/** The chat's messages: the open chat's, or Biscuit's thread with Juniper while it's open. */
	function chatMsgs() {
		if (chat === 'shop') return roomMsgs;
		if (chat === 'moss') return mossMsgs;
		return app.classList.contains('in-thread') ? app.querySelector<HTMLElement>('.msgs.asked')! : msgs;
	}
	const replyTo = (row: HTMLElement, part?: string, ask = false) => {
		const agent = row.classList.contains('reply');
		const who = whoOf(row);
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
	for (const b of app.querySelectorAll<HTMLButtonElement>('.msgs .reply-btn')) {
		b.addEventListener('click', () => replyTo(b.closest<HTMLElement>('.row')!));
	}
	// While the chat isn't at its bottom, a button goes down to the newest, with how many
	// messages came meanwhile, not the person's, at its corner (app.rs, jump_button).
	const jumpBtn = app.querySelector<HTMLButtonElement>('[data-jump]')!;
	const jumpCount = jumpBtn.querySelector<HTMLElement>('.jump-count')!;
	let came = 0;
	let back: ReturnType<typeof setTimeout> | undefined;
	const counted = () => {
		jumpCount.textContent = String(came);
		jumpCount.hidden = came === 0;
	};
	new MutationObserver((changes) => {
		if (jumpBtn.hidden) return;
		for (const c of changes) for (const n of c.addedNodes) if (n instanceof HTMLElement && n.classList.contains('reply')) came++;
		counted();
	}).observe(msgs, { childList: true });
	/** How far a chat's messages are moved down, to show what's above its bottom. */
	const movedBy = (list: HTMLElement) => Number(list.dataset.by ?? 0);
	const moveTo = (list: HTMLElement, by: number) => {
		list.dataset.by = String(by);
		list.style.transform = by ? `translateY(${by}px)` : '';
	};
	/** Back to the bottom: the chat as it was, and the button gone. */
	const toBottom = () => {
		clearTimeout(back);
		for (const list of app.querySelectorAll<HTMLElement>('.msgs')) {
			moveTo(list, 0);
			setTimeout(() => list.classList.remove('jumping'), 400);
		}
		jumpBtn.hidden = true;
	};
	jumpBtn.addEventListener('click', toBottom);
	/** Moves the chat so `el` shows `room` px under its top, if it's above it or under its
	 * bottom; the jump button takes it back down. */
	const reveal = (el: Element, room = 24) => {
		const list = el.closest<HTMLElement>('.msgs')!;
		const view = app.querySelector<HTMLElement>('.chat')!.getBoundingClientRect();
		const at = el.getBoundingClientRect();
		let by = movedBy(list);
		if (at.top < view.top + room) by += view.top + room - at.top;
		else if (at.bottom > view.bottom - 56) by = Math.max(0, by - (at.bottom - view.bottom + 56));
		else return false;
		list.classList.add('jumping');
		moveTo(list, by);
		jumpBtn.hidden = by === 0;
		if (by > 0) {
			came = 0;
			counted();
		}
		return by > 0;
	};
	/** Goes to the original a quote names: the chat moves to show it, lit, and stays there
	 * until the jump button takes it back down, or a few seconds pass. */
	const jump = (row: HTMLElement) => {
		row.classList.add('lit');
		setTimeout(() => row.classList.remove('lit'), 1800);
		if (!reveal(row)) return;
		clearTimeout(back);
		back = setTimeout(toBottom, 6000);
	};
	// Words selected in a message have a small bar where the selection ended.
	let picked: HTMLElement | null = null;
	const unpick = () => {
		picked?.remove();
		picked = null;
	};
	// What a selection takes in, an emoji's picture as the emoji itself, as the app copies
	// and quotes it (emoji.rs, drawn; PLAN.md, "Emoji").
	const selectedWords = (sel: Selection | null): string => {
		if (!sel || sel.rangeCount === 0) return '';
		const part = sel.getRangeAt(0).cloneContents();
		const pictures = part.querySelectorAll('img[alt]');
		if (pictures.length === 0) return sel.toString();
		for (const img of pictures) img.replaceWith(img.getAttribute('alt') ?? '');
		// Laid out off screen, so its lines break where the message's do.
		const box = document.createElement('div');
		box.style.cssText = 'position: fixed; left: -10000px; top: 0';
		box.append(part);
		document.body.append(box);
		const words = box.innerText;
		box.remove();
		return words;
	};
	app.querySelector<HTMLElement>('.chat')!.addEventListener('mouseup', (e) => {
		setTimeout(() => {
			unpick();
			const sel = getSelection();
			const words = selectedWords(sel).replace(/\s+/g, ' ').trim();
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
	// A message's ⋯ by its corner opens its menu: Reply, Copy and, after a line, Delete,
	// which goes at once and leaves a quiet mark where it was; a quote of it says the
	// original was deleted (PLAN.md, "Deleting a message"; app.rs, ctx_menu, deleted_mark).
	let menu: HTMLElement | null = null;
	const unmenu = () => {
		menu?.remove();
		menu = null;
	};
	const quotes = new Map<HTMLElement, HTMLElement>();
	const remove = (row: HTMLElement) => {
		const agent = row.classList.contains('reply');
		const mark = tpl('demo-gone');
		mark.querySelector('.gone-words')!.textContent = agent ? `${whoOf(row)}'s message was deleted` : 'You deleted a message';
		row.classList.add('deleted');
		row.append(mark);
		// Deleting a message takes its pin.
		if (pinned.delete(row)) drawPins();
		for (const [quote, to] of quotes) {
			if (to !== row) continue;
			quote.classList.add('original-gone');
			quote.querySelector('.q-words')!.textContent = 'The original was deleted';
		}
	};
	/** The chat's first messages back as they were, as the scene starts again. */
	const undelete = () => {
		let repinned = false;
		for (const row of app.querySelectorAll<HTMLElement>('.msgs .row.deleted')) {
			row.classList.remove('deleted');
			row.querySelector('.gone')?.remove();
			if (row.dataset.pin) {
				pinned.add(row);
				repinned = true;
			}
		}
		if (repinned) drawPins();
	};
	for (const b of app.querySelectorAll<HTMLButtonElement>('.msgs .more-btn')) {
		b.addEventListener('click', (e) => {
			unmenu();
			const row = b.closest<HTMLElement>('.row')!;
			menu = tpl('demo-more');
			const at = app.getBoundingClientRect();
			menu.style.left = `${Math.min(e.clientX - at.left, at.width - 210)}px`;
			menu.style.top = `${e.clientY - at.top + 6}px`;
			// Pin, or Unpin with its crossed-out pin while it's pinned.
			if (pinned.has(row)) {
				const item = menu.querySelector('[data-act="pin"]')!;
				item.querySelector('span')!.textContent = 'Unpin';
				item.querySelector('svg')!.innerHTML = UNPIN_ICON;
			}
			for (const item of menu.querySelectorAll<HTMLButtonElement>('[data-act]')) {
				item.addEventListener('click', () => {
					if (item.dataset.act === 'reply') replyTo(row);
					else if (item.dataset.act === 'copy') navigator.clipboard?.writeText(wordsOf(row)).catch(() => {});
					else if (item.dataset.act === 'pin') pin(row, !pinned.has(row));
					else remove(row);
					unmenu();
				});
			}
			app.append(menu);
		});
	}
	document.addEventListener('pointerdown', (e) => {
		const on = e.target as Element;
		if (menu && !menu.contains(on) && !on.closest?.('.more-btn')) unmenu();
	});
	// The header in a narrow window gives way as the app's does (desktop/src/model/header.rs),
	// before it reaches the window's buttons: one step at a time, always in this order, until
	// nothing in it runs past its end. Then the name shortens to about three letters, then the
	// mode's words, and then the rest of the name (model::header::fit). What it took from each
	// shows whole under the pointer (model::header::Fit::cut).
	const header = app.querySelector<HTMLElement>('[data-header]')!;
	const steps = ['fit-effort', 'fit-members', 'fit-icon', 'fit-close', 'fit-chevrons'];
	/** The least of its name a shortened head keeps while the mode's words can still give
	 * (model::header::NAME_LEAST). */
	const NAME_LEAST = 48;
	const brainChip = header.querySelector<HTMLElement>('.chip.brain')!;
	const modeChip = header.querySelector<HTMLElement>('.chip.mode')!;
	const wordsIn = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
	const shortened = (el: Element | null) => !!el && el.scrollWidth > el.clientWidth;
	const tipIf = (el: HTMLElement, cut: boolean, words: string) => {
		if (cut) el.title = words;
		else el.removeAttribute('title');
	};
	const fitHeader = () => {
		const modeWords = modeChip.querySelector<HTMLElement>('.mode-words')!;
		const headBtn = header.querySelector<HTMLElement>(app.classList.contains('in-room') ? '.room-btn' : '.agent-btn')!;
		const name = headBtn.querySelector<HTMLElement>('.head-name')!;
		header.classList.remove(...steps);
		for (const el of header.querySelectorAll<HTMLElement>('.head-name, .mode-words')) el.style.maxWidth = '';
		const over = () => header.scrollWidth - header.clientWidth;
		for (const step of steps) {
			if (over() <= 0) break;
			header.classList.add(step);
		}
		if (over() > 0) {
			const whole = name.scrollWidth;
			let most = Math.max(whole - over(), Math.min(whole, NAME_LEAST));
			name.style.maxWidth = `${most}px`;
			if (over() > 0 && modeWords.offsetParent) {
				modeWords.style.maxWidth = `${Math.max(modeWords.scrollWidth - over(), 0)}px`;
			}
			if (over() > 0) {
				most = Math.max(most - over(), 0);
				name.style.maxWidth = `${most}px`;
			}
		}
		const brainWords = brainChip.querySelector('.brain-words');
		tipIf(brainChip, header.classList.contains('fit-effort'), wordsIn(brainWords));
		tipIf(modeChip, shortened(modeWords), wordsIn(modeWords));
		const count = headBtn.querySelector('.room-count');
		const cut = shortened(name) || (!!count && header.classList.contains('fit-members'));
		tipIf(headBtn, cut, count ? `${wordsIn(name)} · ${wordsIn(count)}` : wordsIn(name));
	};
	new ResizeObserver(fitHeader).observe(header);
	document.fonts.ready.then(fitHeader);
	// And once the sidebar has finished sliding, at the width it ends at.
	const side = app.querySelector<HTMLElement>('.side')!;
	side.addEventListener('transitionend', (e) => {
		if (e.target === side) fitHeader();
	});
	// Biscuit's thread with Juniper opens from its row under Juniper, in the chat's place,
	// read only; the strip's arrow, Continue in your own chat in the composer's place, or
	// Juniper's own row, goes back to the person's chat (app.rs, open_thread, branch_strip,
	// continue_button). The chat goes on meanwhile.
	const inThread = (on: boolean) => {
		if (on) openChat('juniper');
		app.classList.toggle('in-thread', on);
	};
	// The Shop group chat opens from its row in the chat's place, as the app's does
	// (room_view.rs, open_room), and Moss's chat from Moss's; Juniper's row goes back to its
	// chat. Each has its own head, pins and session, and leaving one closes what's open over it.
	const roomMsgs = app.querySelector<HTMLElement>('.msgs.room')!;
	const roomHistory = [...roomMsgs.children];
	const mossMsgs = app.querySelector<HTMLElement>('.msgs.moss')!;
	const mossHistory = [...mossMsgs.children];
	const newLine = mossMsgs.querySelector<HTMLElement>('[data-new-line]')!;
	const headName = header.querySelector<HTMLElement>('.agent-btn .head-name')!;
	const phName = app.querySelector<HTMLElement>('[data-ph-name]')!;
	const openChat = (to: Chat) => {
		if (to === chat) return;
		if (!pop.hidden) showPins(false);
		showCard(false);
		closeFind();
		openPlus(false);
		toBottom();
		// "3 new" stays until the chat is left; the draft folds again.
		if (chat === 'moss') {
			newLine.hidden = true;
			for (const cut of mossMsgs.querySelectorAll<HTMLElement>('[data-cut]')) cut.dataset.open = '';
		}
		if (to !== 'juniper') app.classList.remove('in-thread');
		chat = to;
		app.classList.toggle('in-room', to === 'shop');
		app.classList.toggle('in-moss', to === 'moss');
		const agent = to === 'moss' ? { name: 'Moss', color: 'mint' } : { name: 'Juniper', color: 'sky' };
		headName.textContent = agent.name;
		phName.textContent = agent.name;
		head.tint(agent.color);
		app.style.setProperty('--tint', `var(--thread-${agent.color})`);
		if (to === 'moss') cutLong(mossMsgs);
		drawPins();
		drawContext();
		fitHeader();
	};
	const inRoom = (on: boolean) => openChat(on ? 'shop' : 'juniper');
	app.querySelector('[data-room="Shop"]')?.addEventListener('click', () => openChat('shop'));
	app.querySelector('.agent[data-agent="Moss"]')?.addEventListener('click', () => openChat('moss'));
	app.querySelector('[data-ask]')?.addEventListener('click', () => inThread(true));
	app.querySelector('[data-back]')?.addEventListener('click', () => inThread(false));
	app.querySelector('[data-continue]')?.addEventListener('click', () => inThread(false));
	app.querySelector('.agent.on')?.addEventListener('click', () => {
		inThread(false);
		openChat('juniper');
	});

	// A long message is cut after ten lines, fading into its bubble, with a quiet Show more
	// under it that opens it in place and Show less that folds it again; nothing is cut that
	// would hide only a line or so. It opens while the find box is on it (app.rs, cut_long;
	// model::folds). Measured as it's laid out, so the cut fits the width.
	function cutLong(root: ParentNode) {
		for (const said of root.querySelectorAll<HTMLElement>('[data-cut]')) {
			let body = said.querySelector<HTMLElement>(':scope > .cut-body');
			let more = said.querySelector<HTMLButtonElement>(':scope > .show-more');
			if (!body || !more) {
				body = document.createElement('div');
				body.className = 'cut-body';
				body.append(...said.childNodes);
				more = document.createElement('button');
				more.type = 'button';
				more.className = 'show-more';
				said.append(body, more);
				more.addEventListener('click', () => {
					said.dataset.open = said.dataset.open === 'hand' ? '' : 'hand';
					cutLong(said.parentElement!);
				});
			}
			const line = parseFloat(getComputedStyle(said).lineHeight) || 23;
			said.style.setProperty('--cut-at', `${line * FOLD_LINES}px`);
			said.style.setProperty('--cut-fade', `${line * 2}px`);
			const long = body.scrollHeight > line * (FOLD_LINES + 1.5);
			const open = !!said.dataset.open;
			said.classList.toggle('cut', long && !open);
			more.hidden = !long;
			more.textContent = open ? 'Show less' : 'Show more';
		}
	}
	addEventListener('resize', () => {
		if (chat === 'moss') cutLong(mossMsgs);
	});
	const tpl =(id: string) => (document.getElementById(id) as HTMLTemplateElement).content.firstElementChild!.cloneNode(true) as HTMLElement;

	// The chat's pins (PLAN.md, "Pinned messages"): the header's pin opens them in a popover under
	// it, the newest first, each drawn as the chat draws its message under who said it and when,
	// cut at about four lines with Show more. Jump goes to the message and lights it, Copy copies
	// it, and Unpin takes it off with Undo for a few seconds (app.rs, pins_pop, pin_row).
	const pinsBtn = app.querySelector<HTMLButtonElement>('[data-pins]')!;
	const pop = app.querySelector<HTMLElement>('[data-pins-pop]')!;
	const pinList = pop.querySelector<HTMLElement>('[data-pins-list]')!;
	const pinEmpty = pop.querySelector<HTMLElement>('.pp-empty')!;
	const pinNote = pop.querySelector<HTMLElement>('[data-pins-note]')!;
	// Each chat's own: Juniper's two, and the Shop group chat's one.
	const pinRows = [...app.querySelectorAll<HTMLElement>('.msgs .row[data-pin]')].sort((a, b) => Number(a.dataset.pin) - Number(b.dataset.pin));
	const pinned = new Set(pinRows);
	const chatOf = (row: HTMLElement): Chat => (row.closest('.msgs.room') ? 'shop' : row.closest('.msgs.moss') ? 'moss' : 'juniper');
	let unpinned: HTMLElement | null = null;
	let noteTimer = 0;
	let lit: HTMLElement | null = null;
	/** One pin: the message's bubble, its words cut at about four lines (app.rs, pin_cut). */
	const pinOf = (row: HTMLElement) => {
		const pin = tpl('demo-pin');
		const agent = row.classList.contains('reply');
		const who = pin.querySelector('.pin-who')!;
		who.querySelector('b')!.textContent = whoOf(row);
		who.querySelector('time')!.textContent = row.dataset.when ?? '';
		if (agent) {
			const mark = row.querySelector<SVGElement>('.reply-thread')!.cloneNode(true) as SVGElement;
			mark.removeAttribute('class');
			who.prepend(mark);
			pin.style.setProperty('--from', `var(--thread-${mark.dataset.color ?? 'sky'})`);
		}
		const bubble = (row.querySelector('.said:not(.on-the-way)') ?? row.querySelector('.bubble'))!.cloneNode(true) as HTMLElement;
		const clip = document.createElement('div');
		clip.style.overflow = 'hidden';
		clip.append(...bubble.childNodes);
		bubble.append(clip);
		pin.querySelector('.pin-msg')!.append(bubble);
		const more = pin.querySelector<HTMLButtonElement>('.pin-more')!;
		const cut = (open: boolean) => {
			clip.style.maxHeight = open ? '' : '6.2em';
			const long = open || clip.scrollHeight > clip.clientHeight + 1;
			clip.style.maskImage = long && !open ? 'linear-gradient(#000 45%, transparent)' : '';
			more.hidden = !long;
			more.textContent = open ? 'Show less' : 'Show more';
		};
		more.addEventListener('click', () => cut(!!clip.style.maxHeight));
		pin.querySelector('[data-act="jump"]')!.addEventListener('click', () => {
			showPins(false);
			jump(row);
		});
		pin.querySelector('[data-act="copy"]')!.addEventListener('click', () => {
			navigator.clipboard?.writeText(wordsOf(row)).catch(() => {});
			note('Copied', 1600);
		});
		pin.querySelector('[data-act="unpin"]')!.addEventListener('click', () => {
			pinned.delete(row);
			unpinned = row;
			drawPins();
			note('Unpinned', 6000, true);
		});
		pin.addEventListener('mouseenter', () => light(null));
		return { pin, cut };
	};
	/** The popover's list, with how many there are on it and on the header's pin, and each
	 * message's own pin by its corner, there while it's pinned. */
	const drawPins = () => {
		for (const r of pinRows) {
			const on = pinned.has(r);
			r.classList.toggle('pinned', on);
			const b = r.querySelector<HTMLButtonElement>(':scope > .pin-btn');
			b?.setAttribute('aria-label', on ? 'Unpin' : 'Pin');
			if (b) b.title = on ? 'Unpin' : 'Pin';
		}
		const shown = pinRows.filter((r) => pinned.has(r) && chatOf(r) === chat);
		const drawn = shown.map(pinOf);
		pinList.replaceChildren(...drawn.map((d) => d.pin));
		pinList.hidden = shown.length === 0;
		pinEmpty.hidden = shown.length > 0;
		for (const c of app.querySelectorAll('[data-pins-count]')) c.textContent = shown.length ? String(shown.length) : '';
		// The header has the pin while the chat has some, or while they're open (app.rs, header).
		const at = pinsBtn.closest<HTMLElement>('.pins-at')!;
		const had = at.hidden;
		at.hidden = shown.length === 0 && pop.hidden;
		if (had !== at.hidden) fitHeader();
		if (!pop.hidden) for (const d of drawn) d.cut(false);
		return drawn;
	};
	const note = (words: string, ms: number, undo = false) => {
		clearTimeout(noteTimer);
		pinNote.textContent = words;
		pinNote.classList.toggle('copied', !undo);
		if (undo) {
			const b = document.createElement('button');
			b.type = 'button';
			b.textContent = 'Undo';
			b.addEventListener('click', () => {
				if (unpinned) pinned.add(unpinned);
				unpinned = null;
				pinNote.hidden = true;
				drawPins();
			});
			pinNote.append(b);
		}
		pinNote.hidden = false;
		noteTimer = window.setTimeout(() => {
			pinNote.hidden = true;
			unpinned = null;
		}, ms);
	};
	/** The arrow keys' ring, on a pin or none. */
	const light = (pin: HTMLElement | null) => {
		lit?.classList.remove('lit');
		lit = pin;
		lit?.classList.add('lit');
		lit?.scrollIntoView({ block: 'nearest' });
	};
	/** Where the popover goes across the window, as the app works it out (model::pop_under):
	 * centred under the pin, 12px inside the window's edges and no wider than the window less
	 * those, with its nub on the pin but clear of its rounded corners. */
	const placePins = () => {
		const frame = app.getBoundingClientRect();
		const pin = pinsBtn.getBoundingClientRect();
		const room = app.clientWidth;
		const margin = 12;
		const inset = 12 + 11 / Math.SQRT2;
		const middle = (pin.left + pin.right) / 2 - frame.left - app.clientLeft;
		const width = Math.max(0, Math.min(460, room - 2 * margin));
		const left = Math.min(Math.max(middle - width / 2, margin), Math.max(margin, room - margin - width));
		const nub = width > 2 * inset ? Math.min(Math.max(middle - left, inset), width - inset) : width / 2;
		const at = pop.parentElement!.style;
		at.setProperty('--pop-left', `${left}px`);
		at.setProperty('--pop-width', `${width}px`);
		at.setProperty('--nub-in', `${nub}px`);
		at.setProperty('--nub-at', `${left + nub}px`);
	};
	addEventListener('resize', () => {
		if (!pop.hidden) placePins();
	});
	const showPins = (open: boolean) => {
		pop.hidden = !open;
		if (open) {
			drawPins();
			placePins();
		}
		pinsBtn.setAttribute('aria-expanded', String(open));
		lit = null;
		if (open) {
			pinList.scrollTop = 0;
			pop.classList.remove('scrolled');
			pinList.focus({ preventScroll: true });
		} else {
			pinNote.hidden = true;
			unpinned = null;
			drawPins();
		}
	};
	/** Pins a message, or takes its pin off, from its corner or its menu. */
	const pin = (row: HTMLElement, on: boolean) => {
		if (on) pinned.add(row);
		else pinned.delete(row);
		drawPins();
	};
	for (const b of app.querySelectorAll<HTMLButtonElement>('.msgs .pin-btn')) {
		const row = b.closest<HTMLElement>('.row')!;
		b.addEventListener('click', () => pin(row, !pinned.has(row)));
	}
	drawPins();
	pinsBtn.addEventListener('click', () => showPins(pop.hidden === true));
	pinList.addEventListener('scroll', () => pop.classList.toggle('scrolled', pinList.scrollTop > 0));
	document.addEventListener('pointerdown', (e) => {
		if (!pop.hidden && !pop.parentElement!.contains(e.target as Node)) showPins(false);
	});
	document.addEventListener('keydown', (e) => {
		const key = e.key.toLowerCase();
		if (key === 'p' && e.shiftKey && (e.ctrlKey || e.metaKey) && app.matches(':hover, :focus-within')) {
			e.preventDefault();
			showPins(pop.hidden === true);
			return;
		}
		if (pop.hidden) return;
		const pins = [...pinList.children] as HTMLElement[];
		const at = lit ? pins.indexOf(lit) : -1;
		if (e.key === 'Escape') showPins(false);
		else if (e.key === 'ArrowDown' && pins.length) light(pins[Math.min(at + 1, pins.length - 1)]);
		else if (e.key === 'ArrowUp' && pins.length) light(pins[at < 0 ? pins.length - 1 : Math.max(at - 1, 0)]);
		else if (e.key === 'Enter' && lit) (lit.querySelector('[data-act="jump"]') as HTMLButtonElement).click();
		else return;
		e.preventDefault();
	});
	// The composer's + opens its menu over the composer (app.rs, plus_menu): the row under the
	// pointer lit in a breath of the agent's colour, and a row with › opening its list beside
	// it, to its right, or its left where the window has no room (plus_sub). A row is taken
	// with a click, which shuts the menu; Esc, a click outside or the + again shuts it too.
	const plusBtn = app.querySelector<HTMLButtonElement>('[data-plus-btn]')!;
	const plus = app.querySelector<HTMLElement>('[data-plus]')!;
	const subs = [...app.querySelectorAll<HTMLElement>('[data-plus-sub]')];
	const lightRow = (row: Element | null, among: Element) => {
		for (const r of among.querySelectorAll('.plus-row')) r.classList.toggle('lit', r === row);
	};
	/** Where the list beside the menu goes: beside its row, growing up from the row's foot,
	 * or down from its head where there's no room above, kept inside the window. */
	const placeSub = (sub: HTMLElement, row: HTMLElement) => {
		const box = composer.getBoundingClientRect();
		const frame = app.getBoundingClientRect();
		const at = row.getBoundingClientRect();
		const menu = plus.getBoundingClientRect();
		const w = sub.offsetWidth;
		const h = sub.offsetHeight;
		let left = at.right + 4;
		if (left + w > frame.right - 8) left = menu.left - 4 - w;
		if (left < frame.left + 8) left = frame.right - 8 - w;
		let top = at.bottom + 5 - h;
		if (top < frame.top + 8) top = at.top;
		sub.style.setProperty('--sub-left', `${left - box.left}px`);
		sub.style.setProperty('--sub-top', `${top - box.top}px`);
	};
	const openSub = (which: string | null) => {
		for (const sub of subs) {
			const on = sub.dataset.plusSub === which;
			if (on === !sub.hidden) continue;
			sub.hidden = !on;
			if (on) {
				lightRow(null, sub);
				placeSub(sub, plus.querySelector<HTMLElement>(`[data-opens="${which}"]`)!);
			}
		}
	};
	function openPlus(open: boolean) {
		plus.hidden = !open;
		plusBtn.setAttribute('aria-expanded', String(open));
		if (open) lightRow(plus.querySelector('.plus-row'), plus);
		openSub(null);
	}
	// On a Mac its keys are ⌘'s.
	if (document.documentElement.dataset.os === 'mac') {
		for (const k of plus.querySelectorAll<HTMLElement>('[data-keys]')) k.textContent = `⌘${k.dataset.keys}`;
	}
	plusBtn.addEventListener('click', () => openPlus(plus.hidden === true));
	for (const row of plus.querySelectorAll<HTMLElement>('.plus-row')) {
		row.addEventListener('pointerenter', () => {
			lightRow(row, plus);
			openSub(row.dataset.opens ?? null);
		});
		row.addEventListener('click', () => {
			if (row.dataset.opens) openSub(row.dataset.opens);
			else openPlus(false);
		});
	}
	for (const sub of subs) {
		for (const row of sub.querySelectorAll<HTMLElement>('.plus-row')) {
			row.addEventListener('pointerenter', () => lightRow(row, sub));
			row.addEventListener('click', () => openPlus(false));
		}
	}
	document.addEventListener('pointerdown', (e) => {
		const on = e.target as Node;
		if (!plus.hidden && !plus.contains(on) && !plusBtn.contains(on) && !subs.some((s) => s.contains(on))) openPlus(false);
	});

	// Ctrl+F finds words in the open chat (app.rs, find_box; model.rs, find): the box at the
	// chat's top right, every match marked and the one it's on brighter, starting on the newest,
	// with where it is ("3 of 7"). Enter and ↑ go back to older ones, Shift+Enter and ↓ on to
	// newer, round at the ends; Esc or × closes it and the marks go. A cut message opens while
	// the find is on it, and the chat moves to show the match.
	const findBox = app.querySelector<HTMLElement>('[data-find]')!;
	const findIn = findBox.querySelector<HTMLInputElement>('[data-find-in]')!;
	const findCount = findBox.querySelector<HTMLElement>('[data-find-count]')!;
	let found: HTMLElement[] = [];
	let foundAt = -1;
	/** The words of the open chat's messages, without what only shows under the pointer. */
	const findable = (n: Node) => {
		const el = n.parentElement;
		if (!el || el.closest('.table-line, .acta-preview, .link-tip, .show-more, .sources, button, .pin-btn')) return false;
		return !!el.closest('.said, .bubble, .from-bubble');
	};
	const unmark = () => {
		for (const m of app.querySelectorAll('mark.found')) {
			const parent = m.parentNode!;
			m.replaceWith(...m.childNodes);
			parent.normalize();
		}
		found = [];
		foundAt = -1;
	};
	const markAll = (query: string) => {
		unmark();
		const q = query.trim().toLowerCase();
		if (q) {
			const walk = document.createTreeWalker(chatMsgs(), NodeFilter.SHOW_TEXT);
			const nodes: Text[] = [];
			for (let n = walk.nextNode(); n; n = walk.nextNode()) if (findable(n)) nodes.push(n as Text);
			for (const node of nodes) {
				let rest = node;
				for (let i = rest.data.toLowerCase().indexOf(q); i >= 0; i = rest.data.toLowerCase().indexOf(q)) {
					const hit = rest.splitText(i);
					rest = hit.splitText(q.length);
					const mark = document.createElement('mark');
					mark.className = 'found';
					hit.replaceWith(mark);
					mark.append(hit);
					found.push(mark);
				}
			}
		}
		foundAt = found.length - 1;
		showFound();
	};
	/** The match the find is on: lit, its cut message open, and in view. */
	function showFound() {
		findCount.textContent = !findIn.value.trim() ? '' : found.length ? `${foundAt + 1} of ${found.length}` : 'No matches';
		findBox.classList.toggle('some', found.length > 1);
		for (const [i, m] of found.entries()) m.classList.toggle('on', i === foundAt);
		const on = found[foundAt];
		let refold = false;
		for (const said of chatMsgs().querySelectorAll<HTMLElement>('[data-cut]')) {
			const inside = !!on && said.contains(on);
			if (inside && !said.dataset.open) said.dataset.open = 'find';
			else if (!inside && said.dataset.open === 'find') said.dataset.open = '';
			else continue;
			refold = true;
		}
		if (refold) cutLong(chatMsgs());
		if (on) reveal(on, 72);
	}
	const findStep = (by: number) => {
		if (!found.length) return;
		foundAt = (foundAt + by + found.length) % found.length;
		showFound();
	};
	const openFind = () => {
		findBox.hidden = false;
		if (findIn.value) markAll(findIn.value);
	};
	/** Closes the find: its marks go, and a message it opened folds again where it is. */
	function closeFind() {
		if (findBox.hidden) return;
		findBox.hidden = true;
		findIn.value = '';
		findCount.textContent = '';
		unmark();
		for (const said of app.querySelectorAll<HTMLElement>('[data-cut]')) {
			if (said.dataset.open !== 'find') continue;
			const list = said.closest<HTMLElement>('.msgs')!;
			const was = said.getBoundingClientRect().top;
			said.dataset.open = '';
			cutLong(list);
			const by = Math.max(0, movedBy(list) + was - said.getBoundingClientRect().top);
			moveTo(list, by);
			jumpBtn.hidden = by === 0;
		}
	}
	findIn.addEventListener('input', () => markAll(findIn.value));
	findIn.addEventListener('keydown', (e) => {
		if (e.key === 'Enter') findStep(e.shiftKey ? 1 : -1);
		else if (e.key === 'ArrowUp') findStep(-1);
		else if (e.key === 'ArrowDown') findStep(1);
		else if (e.key === 'Escape') closeFind();
		else return;
		e.preventDefault();
		e.stopPropagation();
	});
	for (const b of findBox.querySelectorAll<HTMLButtonElement>('[data-find-step]')) {
		b.addEventListener('click', () => findStep(Number(b.dataset.findStep)));
	}
	findBox.querySelector('[data-find-close]')!.addEventListener('click', closeFind);
	document.addEventListener('keydown', (e) => {
		if (e.key.toLowerCase() === 'f' && !e.shiftKey && !e.altKey && (e.ctrlKey || e.metaKey) && app.matches(':hover, :focus-within')) {
			e.preventDefault();
			openFind();
			findIn.focus({ preventScroll: true });
			findIn.select();
		} else if (e.key === 'Escape' && !findBox.hidden && app.matches(':hover, :focus-within')) closeFind();
	});

	// The notice at the window's foot, after a copy: it stays a few seconds, and a click puts
	// it away (app.rs, toast).
	const toast = app.querySelector<HTMLElement>('[data-toast]')!;
	let toastTimer = 0;
	const notice = (words: string) => {
		toast.querySelector('[data-toast-words]')!.textContent = words;
		toast.hidden = false;
		clearTimeout(toastTimer);
		toastTimer = window.setTimeout(() => (toast.hidden = true), NOTICE_MS);
	};
	toast.addEventListener('click', () => (toast.hidden = true));

	// Under a table in a reply: how many rows, and Copy, which copies it as CSV; its arrow has
	// Copy as CSV, as Markdown and as text (app.rs, table_line; model.rs, table_copy).
	const tableRows = (table: HTMLTableElement) => [...table.rows].map((r) => [...r.cells].map((c) => c.textContent?.trim() ?? ''));
	const tableAs = (rows: string[][], how: string) => {
		if (how === 'csv') return rows.map((r) => r.map((c) => (/[",\n]/.test(c) ? `"${c.replace(/"/g, '""')}"` : c)).join(',')).join('\n');
		if (how === 'md') return [rows[0], rows[0].map(() => '---'), ...rows.slice(1)].map((r) => `| ${r.join(' | ')} |`).join('\n');
		return rows.map((r) => r.join('\t')).join('\n');
	};
	let tableMenu: HTMLElement | null = null;
	const untable = () => {
		tableMenu?.remove();
		tableMenu = null;
		for (const b of app.querySelectorAll('[data-table-more]')) b.setAttribute('aria-expanded', 'false');
	};
	/** Copies it as `how` says, for a click; the demo playing by itself only shows the notice. */
	const copyTable = (table: HTMLTableElement, how: string, clicked = true) => {
		if (clicked) navigator.clipboard?.writeText(tableAs(tableRows(table), how)).catch(() => {});
		notice('Copied.');
	};
	/** Opens the arrow's menu under it. */
	const tableMore = (more: HTMLElement) => {
		untable();
		const table = more.closest('.said')!.querySelector('table')!;
		const at = app.getBoundingClientRect();
		const by = more.getBoundingClientRect();
		tableMenu = tpl('demo-table-menu');
		tableMenu.style.left = `${Math.min(by.left - at.left, at.width - 210)}px`;
		tableMenu.style.top = `${by.bottom - at.top + 4}px`;
		for (const item of tableMenu.querySelectorAll<HTMLButtonElement>('[data-copy-as]')) {
			item.addEventListener('click', () => {
				copyTable(table, item.dataset.copyAs!);
				untable();
			});
		}
		more.setAttribute('aria-expanded', 'true');
		app.append(tableMenu);
		return tableMenu;
	};
	for (const b of app.querySelectorAll<HTMLElement>('[data-table-copy]')) {
		b.addEventListener('click', () => copyTable(b.closest('.said')!.querySelector('table')!, 'csv'));
	}
	for (const b of app.querySelectorAll<HTMLElement>('[data-table-more]')) {
		b.addEventListener('click', () => (tableMenu ? untable() : tableMore(b)));
	}
	document.addEventListener('pointerdown', (e) => {
		const on = e.target as Element;
		if (tableMenu && !tableMenu.contains(on) && !on.closest?.('[data-table-more]')) untable();
	});

	/** What Juniper kept from its reply, under it: the quiet line opens and shuts what it says,
	 * and Undo puts the skill back (learned.rs, quiet_line, fix_done). */
	function kept() {
		const under = tpl('demo-kept');
		const line = under.querySelector<HTMLButtonElement>('.kept-line')!;
		const words = line.querySelector<HTMLElement>('.kept-words')!;
		const opened = under.querySelector<HTMLElement>('.kept-open')!;
		const toggle = (open: boolean) => {
			opened.hidden = !open;
			line.setAttribute('aria-expanded', String(open));
			line.querySelector('.kept-chev svg')!.innerHTML = `<path d="${open ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6'}"></path>`;
		};
		line.addEventListener('click', () => toggle(opened.hidden === true));
		const undo = under.querySelector<HTMLButtonElement>('.kept-undo')!;
		undo.addEventListener('click', () => {
			undo.disabled = true;
			undo.textContent = 'Undone';
			words.textContent = 'Undid the fix to the skill deploy-shop';
		});
		return { under, toggle };
	}

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

	// A row drags to a new place in the list, or into the folder over its name, as in the app
	// (app.rs, droppable): it fades where it was, and a line in the accent shows where it lands.
	// Other agents' threads with it go along under it.
	const sideList = app.querySelector<HTMLElement>('.side .list')!;
	const withAsks = (row: Element) => {
		const rows: Element[] = [row];
		for (let n = row.nextElementSibling; n?.classList.contains('ask'); n = n.nextElementSibling) rows.push(n);
		return rows;
	};
	let dragged: HTMLElement | null = null;
	let target: { el: HTMLElement; where: 'before' | 'after' | 'in' } | null = null;
	const untarget = () => {
		target?.el.classList.remove('drop-before', 'drop-after', 'drop-in');
		target = null;
	};
	for (const row of sideList.querySelectorAll<HTMLElement>(':scope > .agent')) {
		row.draggable = true;
		row.addEventListener('dragstart', (e) => {
			dragged = row;
			const ghost = row.cloneNode(true) as HTMLElement;
			ghost.classList.add('ghost');
			ghost.style.width = `${row.offsetWidth}px`;
			app.append(ghost);
			e.dataTransfer?.setDragImage(ghost, 24, 16);
			e.dataTransfer?.setData('text/plain', '');
			if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
			setTimeout(() => {
				ghost.remove();
				row.classList.add('dragging');
			});
		});
		row.addEventListener('dragend', () => {
			row.classList.remove('dragging');
			dragged = null;
			untarget();
		});
	}
	sideList.addEventListener('dragover', (e) => {
		if (!dragged) return;
		const over = (e.target as Element).closest<HTMLElement>('.list > .agent, .list > .group-head.folder');
		if (!over || over === dragged) return untarget();
		e.preventDefault();
		const box = over.getBoundingClientRect();
		const where = over.classList.contains('folder') ? 'in' : e.clientY < box.top + box.height / 2 ? 'before' : 'after';
		if (target?.el === over && target.where === where) return;
		untarget();
		target = { el: over, where };
		over.classList.add(`drop-${where}`);
	});
	sideList.addEventListener('dragleave', (e) => {
		if (!sideList.contains(e.relatedTarget as Node | null)) untarget();
	});
	sideList.addEventListener('drop', (e) => {
		if (!dragged || !target) return;
		e.preventDefault();
		const rows = withAsks(dragged);
		const { el, where } = target;
		// In the folder, last; the folder is the list's last part, so that's over + New.
		if (where === 'in') sideList.querySelector('.new-row')!.before(...rows);
		else if (where === 'before') el.before(...rows);
		else withAsks(el).at(-1)!.after(...rows);
		untarget();
	});

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
			// The file comes from the + menu's Recent files: sent there before, it goes again
			// without uploading.
			if (part.piece === 'demo-file') await fromRecent(id);
			typed.append(tpl(part.piece));
			over(attached, part.over);
			attached.hidden = attached.childElementCount === 0;
			await wait(450, id);
		}
	}

	/** The + opened, Recent files lit, its list beside it, and notes-2.4.md taken from it. */
	async function fromRecent(id: number) {
		plusBtn.classList.add('pressed');
		openPlus(true);
		await wait(650, id);
		plusBtn.classList.remove('pressed');
		const row = plus.querySelector<HTMLElement>('[data-opens="recent"]')!;
		lightRow(row, plus);
		openSub('recent');
		await wait(700, id);
		const sub = subs.find((s) => s.dataset.plusSub === 'recent')!;
		lightRow(sub.querySelector('[data-plus-item="notes-2.4.md"]'), sub);
		await wait(600, id);
		openPlus(false);
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
			quotes.set(quote, to.row);
			if (to.row.classList.contains('deleted')) {
				quote.classList.add('original-gone');
				quote.querySelector('.q-words')!.textContent = 'The original was deleted';
			}
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

	/** The T3 thread the reply started, as its card under the reply (PLAN.md, "T3 threads"):
	 * working, its strand waving in sky, or done, straight in the ok colour with its pull request. */
	function t3Card(reply: HTMLElement, done: boolean) {
		const card = tpl('demo-t3');
		reply.querySelector(':scope > .body')!.append(card);
		const strand = threadsIn(card)[0];
		const side = card.querySelector<HTMLElement>('.t3-side')!;
		const at = (minutes: number, finished: boolean) => {
			side.textContent = `${minutes} m`;
			card.classList.toggle('done', finished);
			strand.set(finished ? 'idle' : 'writing');
		};
		at(done ? T3_MINUTES : 1, done);
		return at;
	}

	/** The earlier answer's SHOP-31 as its pill shows the card now: in progress, or done once
	 * Sam has moved it. */
	const notesPill = msgs.querySelector<HTMLElement>('.acta-pill[data-card="SHOP-31"]')!;
	function notesDone(done: boolean) {
		notesPill.style.setProperty('--status', done ? 'var(--ok)' : 'var(--thread-sky)');
		notesPill.querySelector('.cp-status')!.textContent = done ? 'Done' : 'In progress';
		notesPill.querySelector('.cp-subtasks')!.firstChild!.textContent = done ? 'Subtasks 4 of 4' : 'Subtasks 2 of 4';
		notesPill.querySelector<HTMLElement>('.cp-bar span')!.style.width = done ? '100%' : '50%';
	}

	/** The watch on the shop board's line once Sam moves the notes' card to Done, quiet and
	 * on the left (PLAN.md, "Watching outside events"), and the reply of Juniper, which it
	 * woke, after it; `p` the reply's paragraph. */
	function watched(fresh: boolean) {
		const line = tpl('demo-watch-line');
		const reply = tpl('demo-woken');
		for (const row of [line, reply]) row.classList.toggle('fresh', fresh);
		placed(line.querySelector<HTMLElement>('.acta-pill')!);
		return { line, reply, p: reply.querySelector<HTMLElement>('.said p')! };
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

	/** Biscuit's reply in the group chat, under the line that says the router gave it the
	 * message and why; `p` its answer's paragraph. */
	function routedReply(fresh: boolean) {
		const reply = tpl('demo-routed');
		reply.classList.toggle('fresh', fresh);
		roomMsgs.append(reply);
		return { reply, p: reply.querySelector<HTMLElement>('.said p')!, thread: threadsIn(reply)[0] };
	}

	/** The person's message in the group chat. */
	function roomSent(fresh: boolean) {
		const you = tpl('demo-you');
		you.classList.toggle('fresh', fresh);
		you.querySelector('.bubble')!.textContent = ROOM_ASK;
		roomMsgs.append(you);
	}

	/** The Shop group chat opened, a message written in it, and the router's pick answering:
	 * the line over the reply comes with it, then the reply is written (PLAN.md, "Group chats"). */
	async function roomScene(id: number) {
		inRoom(true);
		await wait(1100, id);
		await type(ROOM_ASK, id);
		await wait(450, id);
		cleared();
		roomSent(true);
		// About a second for the router's pick, on its warm session.
		await wait(900, id);
		const { p, thread } = routedReply(true);
		thread.set('thinking');
		app.classList.add('busy');
		await wait(1200, id);
		thread.set('writing');
		const words = ROOM_ANSWER.split(' ');
		for (let i = 1; i <= words.length; i++) {
			p.textContent = words.slice(0, i).join(' ');
			await wait(45 + Math.random() * 70, id);
		}
		thread.set('idle');
		app.classList.remove('busy');
		// The header's ring is Biscuit's now, the member that answered last.
		fills.shop = { used: 61, title: "Biscuit's context", line: `A ${FRESH_AT}`, who: 'Biscuit' };
		drawContext();
		await wait(5000, id);
	}

	/** Moss's chat opened from its row: "3 new" over what came after the person left, the
	 * table copied as Markdown from its arrow, then Ctrl+F for "Northwind", newest first, back
	 * to yesterday's draft, which opens while the find is on it and folds again once it's closed;
	 * then down to the newest. */
	async function mossScene(id: number) {
		openChat('moss');
		await wait(2600, id);
		const more = mossMsgs.querySelector<HTMLElement>('[data-table-more]')!;
		const menu = tableMore(more);
		await wait(1000, id);
		const item = menu.querySelector<HTMLElement>('[data-copy-as="md"]')!;
		item.classList.add('lit');
		await wait(500, id);
		copyTable(more.closest('.said')!.querySelector('table')!, 'md', false);
		untable();
		await wait(1700, id);
		openFind();
		await wait(500, id);
		for (let i = 1; i <= FIND.length; i++) {
			findIn.value = FIND.slice(0, i);
			markAll(findIn.value);
			await wait(70 + Math.random() * 50, id);
		}
		await wait(1200, id);
		findStep(-1);
		await wait(1200, id);
		findStep(-1);
		await wait(2800, id);
		closeFind();
		await wait(2400, id);
		toBottom();
		await wait(1500, id);
	}

	/** Each chat as the scene starts: Juniper's open, the group chat's ring Juniper's, and
	 * Moss's replies new again. */
	function fresh() {
		openChat('juniper');
		waitingOn('Juniper', false);
		cleared();
		context(82, 'juniper', `Compacted 2 h ago · ${FRESH_AT}`);
		fills.shop = { used: 23, title: "Juniper's context", line: `A ${FRESH_AT}`, who: 'Juniper' };
		fills.moss = { used: 24, title: 'Context', line: `Compacted 15 h ago · ${FRESH_AT}` };
		for (const el of [...mossMsgs.children]) if (!mossHistory.includes(el)) el.remove();
		newLine.hidden = false;
		toast.hidden = true;
		notesDone(false);
	}

	async function scene(id: number) {
		fresh();
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
		await wait(450, id);
		// Its quote goes back to the answer, and the reply comes while the chat is there: the
		// jump button counts it, and takes the chat back down.
		if (to) jump(to.row);
		await wait(500, id);

		const reply = tpl('demo-reply');
		msgs.append(reply);
		const replyThread = threadsIn(reply)[0];
		const { steps, say } = foldOf(reply);

		mood('thinking', replyThread);
		preview('Juniper', 'Thinking', 'thinking');
		say('Thinking…');
		await wait(1700, id);
		toBottom();

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
			// Its card follows the thread as it works, and lies straight once it's done.
			const t3 = t3Card(reply, false);
			toBottom();
			for (let m = 2; m < T3_MINUTES; m++) {
				await wait(650, id);
				t3(m, false);
			}
			await wait(650, id);
			t3(T3_MINUTES, true);
			// Sam moves the notes' card to Done: the watch on the board says so in a quiet
			// line, and wakes Juniper, whose reply follows it.
			await wait(1400, id);
			const woke = watched(true);
			msgs.append(woke.line);
			notesDone(true);
			toBottom();
			await wait(900, id);
			msgs.append(woke.reply);
			const wokeThread = threadsIn(woke.reply)[0];
			mood('writing', wokeThread);
			preview('Juniper', '', 'writing');
			toBottom();
			const said = WOKEN.split(' ');
			for (let i = 1; i <= said.length; i++) {
				woke.p.textContent = said.slice(0, i).join(' ');
				await wait(45 + Math.random() * 70, id);
			}
			mood('idle', wokeThread);
		} else {
			deploy('denied');
			mood('writing', replyThread);
			preview('Juniper', '', 'writing');
			await stream(say, HELD, id);
			finish(reply, say, steps, HELD);
		}
		context(92);
		await wait(200, id);
		mood('idle', replyThread);
		preview('Juniper', '', 'idle');

		// A name typed after a colon opens the row of emoji over the composer; Tab takes the
		// lit one, which goes in the words as its picture, and a message of only it is big.
		if (answer === 'allow') {
			// Once the turn is over, what Juniper kept from it: the fix to its deploy skill,
			// opened and shut again.
			await wait(700, id);
			const fix = kept();
			reply.querySelector(':scope > .body')!.append(fix.under);
			await wait(1300, id);
			fix.toggle(true);
			await wait(3000, id);
			fix.toggle(false);
			await wait(1200, id);
			await type(':tada', id);
			const row = tpl('demo-emoji-row');
			composer.append(row);
			await wait(1100, id);
			row.remove();
			typed.lastChild?.remove();
			typed.append(tpl('demo-tada'));
			await wait(700, id);
			cleared();
			// The session is past 80%, so this message goes to a fresh one, given the newest of
			// the chat, with loom's line where it starts (PLAN.md, "One endless chat").
			loomLine('A fresh session starts here', msgs);
			context(9, 'juniper', `A ${FRESH_AT}`);
			msgs.append(cheer(true));
		}
		await wait(4000, id);
		// Then the group chat, where the router picks who answers, and Moss's chat.
		await roomScene(id);
		await mossScene(id);
		openChat('juniper');
		await wait(1500, id);

		// Back to the start, quietly, and again.
		for (const el of [...msgs.children]) if (!history.includes(el)) el.classList.add('leaving');
		await wait(400, id);
		for (const el of [...msgs.children]) if (!history.includes(el)) el.remove();
		for (const el of [...roomMsgs.children]) if (!roomHistory.includes(el)) el.remove();
		quotes.clear();
		undelete();
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
		toBottom();
		for (const el of [...msgs.children]) if (!history.includes(el)) el.remove();
		for (const el of [...roomMsgs.children]) if (!roomHistory.includes(el)) el.remove();
		fresh();
		quotes.clear();
		undelete();
		// The group chat as it ends, for its row to open.
		roomSent(false);
		routedReply(false).p.textContent = ROOM_ANSWER;
		const earlier = history.find((el) => el.classList.contains('reply')) as HTMLElement;
		const you = sent(false, { row: earlier, who: 'Juniper', words: wordsOf(earlier), part: false, ask: false });
		const reply = tpl('demo-reply');
		reply.classList.remove('fresh');
		const { steps, say } = foldOf(reply);
		tool(steps, TOOL)('done');
		words(steps, ANSWER);
		tool(steps, ASK)('done');
		finish(reply, say, steps, SHIPPED);
		const woke = watched(false);
		woke.p.textContent = WOKEN;
		notesDone(true);
		msgs.append(you, reply, woke.line, woke.reply);
		loomLine('A fresh session starts here', msgs).classList.remove('fresh-in');
		msgs.append(cheer(false));
		context(9, 'juniper', `A ${FRESH_AT}`);
		t3Card(reply, true);
		reply.querySelector(':scope > .body')!.append(kept().under);
		threadsIn(woke.reply);
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
