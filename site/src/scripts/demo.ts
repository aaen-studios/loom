// Plays the live copy of the app (src/components/AppDemo.astro) once, as three short scenes,
// each in its own chat: the person's message typed and sent, then the reply written in. Juniper
// answers with an Acta card's pill (app.rs, card_pill); in the Shop group chat the router picks
// who answers, and the line over the reply names it (room_view.rs, route_line); Moss starts a
// T3 thread, whose card works and then lies straight with its pull request (t3_view.rs,
// thread_card). After a few seconds' rest the next chat opens from its row in the sidebar, and
// after the last it stays still. A click in it ends every scene where it would end, and each
// chat's row opens it.
//
// What opens by hand is the app's: a card's pill opens its preview under the pointer and its
// side preview on a click (acta_ui.rs, card_side); the ring in the header opens the context card,
// with the plan's limits under it (app.rs, context_ring); the header's pin opens the chat's pins
// (pins_pop); a message's Reply and ⋯ by its corner, or words selected in it, put a reply bar
// over the composer or open its menu (reply_bar, ctx_menu); the + opens its menu (plus_menu,
// plus_sub); a row drags to a new place (droppable); and the sidebar collapses to the rail, as
// it is anyway in a narrow window (rail.rs), or with no room for the rail, as on a phone, opens
// over the chat from the header. The header gives way in a narrow window as the app's does (model/header.rs).
//
// It plays only while it's on screen, and with reduced motion it shows each chat's end instead.
import { threadOf, threadsIn, yarnsIn, type Mood, type Thread } from './thread';

type Chat = 'juniper' | 'shop' | 'moss';
/** How full each chat's session is, in thousands of tokens of 200k, what its card's title
 * says, what the chat costs (its last turn's tokens and the last 24 hours') and its line, when
 * it was compacted: in the Shop group chat, the member that answered last's. */
type Fill = { used: number; title: string; line: string; turn: string; day: string; who?: string };
/** The Shop group chat's context once Biscuit has answered there. */
const BISCUIT_FILL: Fill = { used: 61, title: "Biscuit's context", line: '', turn: '58k', day: '1.2M', who: 'Biscuit' };
/** The steps of Moss's T3 thread, a minute apart as its card counts them, before it's done. */
const T3_STEPS = ['rg -n Contiune src', 'Edit src/checkout/PayButton.tsx', 'bun test checkout'];

/** Lucide's crossed-out pin, as the app's Unpin has it (Ico::PinOff). */
const UNPIN_ICON =
	'<path d="M12 17v5"></path><path d="M15 9.34V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H7.89"></path><path d="m2 2 20 20"></path><path d="M9 9v1.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h11"></path>';

/** A pause that ends early, with an error, when the demo is stopped. */
class Stopped extends Error {}

/** A reply's words, to write in a word at a time as an agent writes them, with its pieces (a
 * card's pill, a mention, a link) each coming whole where it is. */
function writing(said: HTMLElement) {
	const texts: [Text, string][] = [];
	const pieces: HTMLElement[] = [];
	const steps: (() => void)[] = [];
	const walk = (el: Node) => {
		for (const n of [...el.childNodes]) {
			if (n instanceof Text) {
				const whole = n.data;
				texts.push([n, whole]);
				const words = whole.match(/\s*\S+\s*/g) ?? [whole];
				for (let i = 1; i <= words.length; i++) {
					const upTo = words.slice(0, i).join('');
					steps.push(() => (n.data = upTo));
				}
			} else if (n instanceof HTMLElement && n.matches('.acta-pill, .mention, .link')) {
				pieces.push(n);
				steps.push(() => (n.style.display = ''));
			} else walk(n);
		}
	};
	walk(said);
	return {
		steps,
		clear() {
			for (const [n] of texts) n.data = '';
			for (const p of pieces) p.style.display = 'none';
		},
		all() {
			for (const [n, whole] of texts) n.data = whole;
			for (const p of pieces) p.style.display = '';
		},
	};
}

export function play(app: HTMLElement) {
	threadsIn(app);
	yarnsIn(app);
	const head = threadOf(app.querySelector('svg[data-head]'))!;
	const lists: Record<Chat, HTMLElement> = {
		juniper: app.querySelector<HTMLElement>('.msgs.mine')!,
		shop: app.querySelector<HTMLElement>('.msgs.room')!,
		moss: app.querySelector<HTMLElement>('.msgs.moss')!,
	};
	const tpl = (id: string) => (document.getElementById(id) as HTMLTemplateElement).content.firstElementChild!.cloneNode(true) as HTMLElement;
	const chatOf = (row: HTMLElement): Chat => (row.closest('.msgs.room') ? 'shop' : row.closest('.msgs.moss') ? 'moss' : 'juniper');
	let chat: Chat = 'juniper';

	// An Acta card's preview, which the page's CSS opens under its pill, goes over it when
	// there's no room under it in the chat, as the app keeps a popover in the window.
	for (const pill of app.querySelectorAll<HTMLElement>('.acta-pill')) {
		const preview = pill.querySelector<HTMLElement>('.acta-preview');
		if (!preview) continue;
		const place = () => {
			const at = pill.closest<HTMLElement>('.chat');
			if (!at) return;
			preview.classList.remove('up');
			preview.style.display = 'flex';
			const tall = preview.offsetHeight;
			preview.style.display = '';
			if (pill.getBoundingClientRect().bottom + tall + 8 > at.getBoundingClientRect().bottom) preview.classList.add('up');
		};
		pill.addEventListener('pointerenter', place);
		pill.addEventListener('focus', place);
	}
	// A card's pill, or Open in its preview, opens its side preview beside the chat (PLAN.md,
	// "Acta beside the chat"; acta_ui.rs, card_side). Its status chip's menu moves the card, and
	// its pill's dot and preview with it; a tick ticks a subtask, and the counts follow; the tabs
	// show Details, Comments and Activity, and Comment adds what was typed to the comments.
	const actaSide = app.querySelector<HTMLElement>('[data-acta-side]');
	const sidePill = app.querySelector<HTMLElement>('.acta-pill[data-card="SHOP-31"]');
	if (actaSide && sidePill) {
		const show = (open: boolean) => {
			actaSide.hidden = !open;
			if (open) sidePill.blur();
		};
		sidePill.querySelector('.acta-face')!.addEventListener('click', () => show(true));
		sidePill.querySelector('[data-acta-open]')?.addEventListener('click', (e) => {
			e.stopPropagation();
			show(true);
		});
		actaSide.querySelector('[data-acta-close]')!.addEventListener('click', () => show(false));
		// ⋯: Follow, and Following with its tick, which stops it; Copy link, Copy branch and
		// Make a thread close it.
		const more = actaSide.querySelector<HTMLButtonElement>('[data-acta-more]')!;
		const moreMenu = actaSide.querySelector<HTMLElement>('[data-acta-more-menu]')!;
		const openMore = (open: boolean) => {
			moreMenu.hidden = !open;
			more.setAttribute('aria-expanded', String(open));
		};
		more.addEventListener('click', () => openMore(moreMenu.hidden === true));
		for (const item of moreMenu.querySelectorAll<HTMLButtonElement>('button')) item.addEventListener('click', () => openMore(false));
		const follow = actaSide.querySelector<HTMLButtonElement>('[data-acta-follow]')!;
		follow.addEventListener('click', () => {
			const on = follow.getAttribute('aria-pressed') !== 'true';
			follow.setAttribute('aria-pressed', String(on));
			follow.querySelector('[data-follow-words]')!.textContent = on ? 'Following' : 'Follow';
		});
		document.addEventListener('pointerdown', (e) => {
			if (!moreMenu.hidden && !more.parentElement!.contains(e.target as Node)) openMore(false);
		});
		// The tabs, and the box to comment in under Comments.
		const reply = actaSide.querySelector<HTMLFormElement>('[data-acta-reply]')!;
		const tabs = [...actaSide.querySelectorAll<HTMLButtonElement>('[data-acta-tab]')];
		for (const tab of tabs) {
			tab.addEventListener('click', () => {
				for (const t of tabs) t.setAttribute('aria-selected', String(t === tab));
				for (const pane of actaSide.querySelectorAll<HTMLElement>('[data-acta-pane]')) pane.hidden = pane.dataset.actaPane !== tab.dataset.actaTab;
				reply.hidden = tab.dataset.actaTab !== 'comments';
			});
		}
		reply.addEventListener('submit', (e) => {
			e.preventDefault();
			const box = reply.querySelector<HTMLTextAreaElement>('[data-acta-reply-words]')!;
			const words = box.value.trim();
			if (!words) return;
			const comment = document.createElement('div');
			comment.className = 'as-comment';
			comment.innerHTML = '<span class="as-avatar">Y</span><div><div class="as-who"><b>You</b> · now</div><p></p></div>';
			comment.querySelector('p')!.textContent = words;
			const list = actaSide.querySelector<HTMLElement>('[data-acta-comments]')!;
			list.append(comment);
			actaSide.querySelector('[data-acta-comment-count]')!.textContent = String(list.children.length);
			box.value = '';
		});
		const chip = actaSide.querySelector<HTMLButtonElement>('[data-acta-status]')!;
		const menu = actaSide.querySelector<HTMLElement>('[data-acta-menu]')!;
		const openMenu = (open: boolean) => {
			menu.hidden = !open;
			chip.setAttribute('aria-expanded', String(open));
		};
		chip.addEventListener('click', () => openMenu(menu.hidden === true));
		for (const item of menu.querySelectorAll<HTMLButtonElement>('[data-status]')) {
			item.addEventListener('click', () => {
				const words = item.dataset.words!;
				for (const at of [chip, sidePill]) at.style.setProperty('--status', item.dataset.color!);
				chip.querySelector('.as-words')!.textContent = words;
				sidePill.querySelector('.cp-status')!.textContent = words;
				for (const other of menu.querySelectorAll('[data-status]')) other.classList.toggle('on', other === item);
				openMenu(false);
			});
		}
		document.addEventListener('pointerdown', (e) => {
			if (!menu.hidden && !chip.parentElement!.contains(e.target as Node)) openMenu(false);
		});
		const ticks = [...actaSide.querySelectorAll<HTMLButtonElement>('[data-tick]')];
		for (const tick of ticks) {
			tick.addEventListener('click', () => {
				const done = tick.classList.toggle('done');
				tick.setAttribute('aria-checked', String(done));
				const n = ticks.filter((t) => t.classList.contains('done')).length;
				const words = `Subtasks ${n} of ${ticks.length}`;
				actaSide.querySelector('[data-acta-count]')!.textContent = `Subtasks · ${n} of ${ticks.length}`;
				const line = sidePill.querySelector<HTMLElement>('.cp-subtasks');
				if (line) {
					line.firstChild!.textContent = words;
					line.querySelector<HTMLElement>('.cp-bar span')!.style.width = `${(100 * n) / ticks.length}%`;
				}
			});
		}
	}
	const typed = app.querySelector<HTMLElement>('.typed')!;

	// How full the open chat's session context is, after its model, in thousands of tokens of
	// its 200k (app.rs, context_ring; model.rs, ContextFill), with the plan's limits under it in
	// its card. Its card shows under the pointer and stays once the ring is clicked, until a
	// click outside, Esc or the ring again (app.rs, context_hover, toggle_context).
	const ringAt = app.querySelector<HTMLElement>('.context-at')!;
	const ring = ringAt.querySelector<HTMLButtonElement>('[data-context]')!;
	const card = ringAt.querySelector<HTMLElement>('[data-context-card]')!;
	const cardLine = card.querySelector<HTMLElement>('[data-context-line]')!;
	let cardKept = false;
	let cardLeft = 0;
	// Each chat's session: Juniper's, the Shop group chat's member that answered last, Juniper
	// until Biscuit answers ("Juniper's context"), and Moss's (model.rs, context_title,
	// ContextFill::costs, ContextFill::line).
	const fills: Record<Chat, Fill> = {
		juniper: { used: 82, title: 'Context', line: 'Compacted 2 h ago', turn: '410k', day: '6.2M' },
		shop: { used: 23, title: "Juniper's context", line: '', turn: '96k', day: '1.1M', who: 'Juniper' },
		moss: { used: 24, title: 'Context', line: 'Compacted 15 h ago', turn: '64k', day: '380k' },
	};
	const drawContext = () => {
		const { used, title, line, turn, day } = fills[chat];
		const size = 200;
		const percent = Math.floor((used * 100) / size);
		ring.querySelector<SVGElement>('.fill')!.style.setProperty('--fill', String(percent));
		card.querySelector<HTMLElement>('[data-context-bar]')!.style.setProperty('--fill', `${percent}%`);
		card.querySelector<HTMLElement>('[data-context-amount]')!.textContent = `${used}k of ${size}k`;
		card.querySelector<HTMLElement>('.cc-title')!.textContent = title;
		card.setAttribute('aria-label', title);
		cardLine.textContent = line;
		cardLine.hidden = line === '';
		card.querySelector<HTMLElement>('[data-context-turn]')!.textContent = turn;
		card.querySelector<HTMLElement>('[data-context-day]')!.textContent = day;
		ringAt.classList.toggle('warn', percent >= 70 && percent < 90);
		ringAt.classList.toggle('bad', percent >= 90);
		ring.setAttribute('aria-label', `Context ${percent}%, ${used}k of ${size}k`);
	};
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
			fills[chat] = { ...fills[chat], used: after, line: compact ? 'Compacted just now' : '' };
			drawContext();
			const line = tpl('demo-loom-line');
			line.querySelector('span')!.textContent = compact
				? `${who ? `${who}'s context` : 'Context'} compacted · ${before}k → ${after}k`
				: `${who ? `${who}'s fresh session` : 'A fresh session'} starts here`;
			lists[chat].append(line);
			showCard(false);
		});
	}

	// Replying (PLAN.md, "Replying to a message"): a message's Reply, or words selected in it,
	// put a bar over the composer's words (app.rs, reply_bar).
	const replyBar = app.querySelector<HTMLElement>('[data-reply-bar]')!;
	/** A message's words, on one line: an agent's answer, or what the person wrote, with an
	 *  Acta card's pill as its id, as it was written (PLAN.md, "Cards in messages"). */
	const wordsOf = (row: HTMLElement) => {
		const said = row.querySelector('.said, .bubble')?.cloneNode(true) as HTMLElement | undefined;
		for (const pill of said?.querySelectorAll<HTMLElement>('.acta-pill') ?? []) pill.replaceWith(pill.dataset.card ?? '');
		return said?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
	};
	/** Who said a message: its agent's name over it, or in a group chat the line's, or "You". */
	const whoOf = (row: HTMLElement) =>
		row.classList.contains('reply') ? (row.querySelector('.who, .routed-who')?.textContent?.trim() ?? 'Juniper') : 'You';
	/** Replies to a message in the open chat; one in a chat that isn't open is left be. */
	const replyTo = (row: HTMLElement, part?: string, ask = false) => {
		if (chatOf(row) !== chat) return;
		const who = whoOf(row);
		const words = (part ?? wordsOf(row)).replace(/\s+/g, ' ').trim();
		replyBar.querySelector('.rb-head')!.textContent =
			ask && part ? `Asking ${who} about` : row.classList.contains('reply') ? `Replying to ${who}` : 'Replying to yourself';
		replyBar.querySelector('.rb-words')!.textContent = words;
		replyBar.hidden = false;
	};
	const unreply = () => (replyBar.hidden = true);
	app.querySelector('[data-reply-x]')!.addEventListener('click', unreply);
	for (const b of app.querySelectorAll<HTMLButtonElement>('.msgs .reply-btn')) {
		b.addEventListener('click', () => replyTo(b.closest<HTMLElement>('.row')!));
	}
	/** A message lit for a moment, as Jump in the pins leaves it. */
	const flash = (row: HTMLElement) => {
		row.classList.add('lit');
		setTimeout(() => row.classList.remove('lit'), 1800);
	};
	// Words selected in a message have a small bar where the selection ended.
	let picked: HTMLElement | null = null;
	const unpick = () => {
		picked?.remove();
		picked = null;
	};
	app.querySelector<HTMLElement>('.chat')!.addEventListener('mouseup', (e) => {
		setTimeout(() => {
			unpick();
			const sel = getSelection();
			const words = (sel?.toString() ?? '').replace(/\s+/g, ' ').trim();
			const row = sel?.anchorNode?.parentElement?.closest<HTMLElement>('.row[data-msg]');
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
	// A message's ⋯ by its corner opens its menu: Reply, Copy, Pin or Unpin, Branch from here,
	// Make a card… and, after a line, Delete, which goes at once and leaves a quiet mark where it
	// was (PLAN.md, "Deleting a message"; app.rs, ctx_menu, deleted_mark). Branch from here and
	// Make a card… only close it here.
	let menu: HTMLElement | null = null;
	const unmenu = () => {
		menu?.remove();
		menu = null;
	};
	const remove = (row: HTMLElement) => {
		const mark = tpl('demo-gone');
		mark.querySelector('.gone-words')!.textContent = row.classList.contains('reply') ? `${whoOf(row)}'s message was deleted` : 'You deleted a message';
		row.classList.add('deleted');
		row.append(mark);
		// Deleting a message takes its pin.
		if (pinned.delete(row)) drawPins();
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
					else if (item.dataset.act === 'delete') remove(row);
					unmenu();
				});
			}
			app.append(menu);
			// Kept inside the window, as the app's menus are.
			const low = at.height - menu.offsetHeight - 8;
			if (e.clientY - at.top + 6 > low) menu.style.top = `${Math.max(8, low)}px`;
		});
	}
	document.addEventListener('pointerdown', (e) => {
		const on = e.target as Element;
		if (menu && !menu.contains(on) && !on.closest?.('.more-btn')) unmenu();
	});
	// The header in a narrow window gives way as the app's does (desktop/src/model/header.rs),
	// before it reaches the window's buttons: one step at a time, always in this order, until
	// nothing in it runs past its end. Then the name shortens to about three letters, then the
	// mode's words, and then the rest of the name (model::header::fit). Its words cut to
	// nothing, the mode's chip goes, and the name gets back what that gives. Narrower than the
	// app's window ever gets, as on a phone, the thread beside the name goes too, before the
	// name gives any more. What it took from each shows whole under the pointer
	// (model::header::Fit::cut). Widths are measured to the fraction of a pixel, so a name or
	// words that fit are never cut.
	const header = app.querySelector<HTMLElement>('[data-header]')!;
	const steps = ['fit-effort', 'fit-members', 'fit-icon', 'fit-close', 'fit-chevrons'];
	/** How far past its end the header may run and still fit: a rounding error, not a pixel. */
	const SLACK = 0.01;
	/** The least of its name a shortened head keeps while the mode's words can still give
	 * (model::header::NAME_LEAST). */
	const NAME_LEAST = 48;
	const brainChip = header.querySelector<HTMLElement>('.chip.brain')!;
	const modeChip = header.querySelector<HTMLElement>('.chip.mode')!;
	const wordsIn = (el: Element | null) => el?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
	const shortened = (el: Element | null) => !!el && el.scrollWidth > el.clientWidth;
	/** How wide `el`'s words are, to the fraction of a pixel. */
	const widthOf = (el: HTMLElement) => {
		const r = document.createRange();
		r.selectNodeContents(el);
		return r.getBoundingClientRect().width;
	};
	/** At most `most` of words `whole` wide, or all of them when that's room enough. */
	const cap = (el: HTMLElement, whole: number, most: number) => {
		el.style.maxWidth = most >= whole ? '' : `${most}px`;
	};
	const tipIf = (el: HTMLElement, cut: boolean, words: string) => {
		if (cut) el.title = words;
		else el.removeAttribute('title');
	};
	const fitHeader = () => {
		const modeWords = modeChip.querySelector<HTMLElement>('.mode-words')!;
		const headBtn = header.querySelector<HTMLElement>(app.classList.contains('in-room') ? '.room-btn' : '.agent-btn')!;
		const name = headBtn.querySelector<HTMLElement>('.head-name')!;
		header.classList.remove(...steps, 'fit-mode', 'fit-thread');
		for (const el of header.querySelectorAll<HTMLElement>('.head-name, .mode-words')) el.style.maxWidth = '';
		// How far the chips run past the header's end, inside its padding.
		const over = () => {
			const end = header.getBoundingClientRect().right - parseFloat(getComputedStyle(header).paddingRight);
			return header.lastElementChild!.getBoundingClientRect().right - end;
		};
		for (const step of steps) {
			if (over() <= SLACK) break;
			header.classList.add(step);
		}
		if (over() > SLACK) {
			const whole = widthOf(name);
			// The name down to its least, or whole once it fits.
			const shorten = () => {
				name.style.maxWidth = '';
				const most = Math.max(whole - over(), Math.min(whole, NAME_LEAST));
				cap(name, whole, most);
				return most;
			};
			let most = shorten();
			if (over() > SLACK && modeWords.offsetParent) {
				const words = widthOf(modeWords);
				const left = Math.max(words - over(), 0);
				cap(modeWords, words, left);
				if (left === 0) {
					header.classList.add('fit-mode');
					most = shorten();
				}
			}
			if (over() > SLACK) {
				header.classList.add('fit-thread');
				most = shorten();
			}
			if (over() > SLACK) cap(name, whole, Math.max(most - over(), 0));
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

	// Each chat opens from its row in the chat's place, as the app's does (room_view.rs,
	// open_room): Juniper's, the Shop group chat and Moss's. Each has its own head, pins and
	// session, and leaving one closes what's open over it.
	const headName = header.querySelector<HTMLElement>('.agent-btn .head-name')!;
	const phName = app.querySelector<HTMLElement>('[data-ph-name]')!;
	const openChat = (to: Chat) => {
		if (to === chat) return;
		if (!pop.hidden) showPins(false);
		showCard(false);
		openPlus(false);
		unreply();
		chat = to;
		app.classList.toggle('in-room', to === 'shop');
		app.classList.toggle('in-moss', to === 'moss');
		const agent = to === 'moss' ? { name: 'Moss', color: 'mint' } : { name: 'Juniper', color: 'sky' };
		headName.textContent = agent.name;
		phName.textContent = agent.name;
		head.tint(agent.color);
		app.style.setProperty('--tint', `var(--thread-${agent.color})`);
		drawPins();
		drawContext();
		fitHeader();
	};
	app.querySelector('[data-room="Shop"]')?.addEventListener('click', () => openChat('shop'));
	app.querySelector('.agent[data-agent="Moss"]')?.addEventListener('click', () => openChat('moss'));
	app.querySelector('.agent[data-agent="Juniper"]')?.addEventListener('click', () => openChat('juniper'));

	// The chat's pins (PLAN.md, "Pinned messages"): the header's pin opens them in a popover under
	// it, the newest first, each drawn as the chat draws its message under who said it and when,
	// cut at about four lines with Show more. Jump goes to the message and lights it, Copy copies
	// it, and Unpin takes it off with Undo for a few seconds (app.rs, pins_pop, pin_row).
	const pinsBtn = app.querySelector<HTMLButtonElement>('[data-pins]')!;
	const pop = app.querySelector<HTMLElement>('[data-pins-pop]')!;
	const pinList = pop.querySelector<HTMLElement>('[data-pins-list]')!;
	const pinEmpty = pop.querySelector<HTMLElement>('.pp-empty')!;
	const pinNote = pop.querySelector<HTMLElement>('[data-pins-note]')!;
	// Every message that can be pinned, and when each was: Juniper's two to start with, the
	// lower the newer, and each one pinned since newer still.
	const pinRows = [...app.querySelectorAll<HTMLElement>('.msgs .row')].filter((r) => r.querySelector(':scope > .pin-btn'));
	const pinAt = new Map<HTMLElement, number>();
	for (const r of pinRows) if (r.dataset.pin) pinAt.set(r, Number(r.dataset.pin));
	const pinned = new Set(pinAt.keys());
	let newest = 0;
	let unpinned: HTMLElement | null = null;
	let noteTimer = 0;
	let lit: HTMLElement | null = null;
	/** One pin: the message's bubble, its words cut at about four lines (app.rs, pin_cut). */
	const pinOf = (row: HTMLElement) => {
		const pin = tpl('demo-pin');
		const who = pin.querySelector('.pin-who')!;
		who.querySelector('b')!.textContent = whoOf(row);
		who.querySelector('time')!.textContent = row.dataset.when ?? '';
		if (row.classList.contains('reply')) {
			const mark = row.querySelector<SVGElement>('.reply-thread')!.cloneNode(true) as SVGElement;
			mark.removeAttribute('class');
			who.prepend(mark);
			pin.style.setProperty('--from', `var(--thread-${mark.dataset.color ?? 'sky'})`);
		}
		const bubble = row.querySelector('.said, .bubble')!.cloneNode(true) as HTMLElement;
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
			flash(row);
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
	 * message's own pin by its corner, there while it's pinned. A message not sent yet has none. */
	const drawPins = () => {
		for (const r of pinRows) {
			const on = pinned.has(r);
			r.classList.toggle('pinned', on);
			const b = r.querySelector<HTMLButtonElement>(':scope > .pin-btn')!;
			b.setAttribute('aria-label', on ? 'Unpin' : 'Pin');
			b.title = on ? 'Unpin' : 'Pin';
		}
		const shown = pinRows
			.filter((r) => pinned.has(r) && chatOf(r) === chat && !r.classList.contains('unsent'))
			.sort((a, b) => pinAt.get(a)! - pinAt.get(b)!);
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
	/** Pins a message, newest of all, or takes its pin off, from its corner or its menu. */
	const pin = (row: HTMLElement, on: boolean) => {
		if (on) {
			pinned.add(row);
			pinAt.set(row, --newest);
		} else pinned.delete(row);
		drawPins();
	};
	for (const row of pinRows) {
		row.querySelector(':scope > .pin-btn')!.addEventListener('click', () => pin(row, !pinned.has(row)));
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
	const composer = app.querySelector<HTMLElement>('.composer')!;
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

	// A row says what its agent is on after its name; a pinned agent's tile, under the pointer.
	const preview = (agent: string, text: string, mood: Mood) => {
		const row = app.querySelector<HTMLElement>(`[data-agent="${agent}"]`);
		const p = row?.querySelector<HTMLElement>('.agent-preview');
		if (p) p.textContent = text;
		if (row?.classList.contains('tile')) row.title = text ? `${agent} · ${text}` : agent;
		threadOf(row?.querySelector('svg[data-thread]') ?? null)?.set(mood);
	};

	// A row drags to a new place in the list, or into the folder over its name, as in the app
	// (app.rs, droppable): it fades where it was, and a line in the accent shows where it lands.
	const sideList = app.querySelector<HTMLElement>('.side .list')!;
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
		const { el, where } = target;
		// In the folder, last; the folder is the list's last part, so that's over + New.
		if (where === 'in') sideList.querySelector('.new-row')!.before(dragged);
		else if (where === 'before') el.before(dragged);
		else el.after(dragged);
		untarget();
	});

	// The sidebar: collapsed by its control, and a rail anyway in a narrow window. With no room
	// for the rail, as on a phone, the header's control opens it whole over the chat, and a row
	// that opens a chat, its own control, the shade over the chat or Esc closes it again.
	const bar = app.querySelector<HTMLButtonElement>('[data-bar]')!;
	const headBar = app.querySelector<HTMLButtonElement>('[data-head-bar]')!;
	const narrow = matchMedia('(max-width: 860px)');
	const phone = matchMedia('(max-width: 560px)');
	let collapsed = false;
	let overChat = false;
	const rail = () => {
		const railed = !overChat && (collapsed || narrow.matches);
		app.classList.toggle('railed', railed);
		bar.setAttribute('aria-label', railed ? 'Open the sidebar' : 'Collapse the sidebar');
	};
	const drawer = (on: boolean) => {
		overChat = on && phone.matches;
		app.classList.toggle('drawer', overChat);
		headBar.setAttribute('aria-expanded', String(overChat));
		rail();
	};
	headBar.addEventListener('click', () => drawer(true));
	app.querySelector('[data-shade]')!.addEventListener('click', () => drawer(false));
	for (const row of app.querySelectorAll('[data-room="Shop"], .agent[data-agent="Juniper"], .agent[data-agent="Moss"]')) {
		row.addEventListener('click', () => drawer(false));
	}
	document.addEventListener('keydown', (e) => {
		if (overChat && e.key === 'Escape') drawer(false);
	});
	phone.addEventListener('change', () => drawer(false));
	bar.addEventListener('click', () => {
		if (overChat) return drawer(false);
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

	// The scenes: in each chat, the person's message and the reply, who answers it, and the
	// sidebar's thread that works while it does; in Moss's, its T3 thread's card.
	type Scene = { chat: Chat; by: string; ask: HTMLElement; reply: HTMLElement; words: ReturnType<typeof writing>; thread?: Thread; t3: HTMLElement | null };
	const scenes: Scene[] = (['juniper', 'shop', 'moss'] as const).map((c) => {
		const ask = lists[c].querySelector<HTMLElement>('[data-scene="ask"]')!;
		const reply = lists[c].querySelector<HTMLElement>('[data-scene="reply"]')!;
		return {
			chat: c,
			by: reply.dataset.by!,
			ask,
			reply,
			words: writing(reply.querySelector<HTMLElement>('.said')!),
			thread: threadOf(reply.querySelector('.reply-thread')),
			t3: reply.querySelector<HTMLElement>('.t3'),
		};
	});
	/** Moss's T3 thread as its card shows it: working, its strand waving in sky, the minutes it
	 * has run at its title's end and its latest step under it; or, with no step, done, straight
	 * in the ok colour with its pull request. */
	const t3 = (at: HTMLElement, minutes: number, step?: string) => {
		at.classList.toggle('done', !step);
		at.querySelector('[data-t3-time]')!.textContent = `${minutes} m`;
		if (step) at.querySelector('[data-t3-step]')!.textContent = step;
		threadOf(at.querySelector('svg[data-thread]'))?.set(step ? 'writing' : 'idle');
	};
	/** A message comes into its chat, and its pin with it. */
	const send = (row: HTMLElement) => {
		row.classList.remove('unsent');
		row.classList.add('fresh');
		drawPins();
	};
	/** A scene as it ends: both messages there, the reply whole, and its T3 thread done. */
	const end = (s: Scene) => {
		for (const row of [s.ask, s.reply]) row.classList.remove('unsent');
		s.words.all();
		s.thread?.set('idle');
		preview(s.by, '', 'idle');
		if (s.chat === 'shop') fills.shop = BISCUIT_FILL;
		if (s.t3) {
			s.t3.hidden = false;
			t3(s.t3, T3_STEPS.length + 1);
		}
	};

	let run = 0;
	let onScreen = false;
	let playing = false;
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

	/** One scene: its chat opened, the person's message typed and sent, and the agent's reply
	 * written in once it has thought; in the group chat, once the router has picked it. */
	async function scene(s: Scene, id: number) {
		openChat(s.chat);
		await wait(1000, id);
		const said = s.ask.querySelector('.bubble')!.textContent!.trim();
		const words = document.createTextNode('');
		typed.append(words);
		for (let i = 1; i <= said.length; i++) {
			words.data = said.slice(0, i);
			await wait(said[i - 1] === ' ' ? 70 : 38 + Math.random() * 40, id);
		}
		await wait(400, id);
		typed.replaceChildren();
		send(s.ask);
		app.classList.add('busy');
		// About a second for the router's pick, on its warm session.
		if (s.chat === 'shop') await wait(900, id);
		const work = (mood: Mood, text: string) => {
			preview(s.by, text, mood);
			if (s.chat !== 'shop') head.set(mood);
		};
		work('thinking', 'Thinking');
		await wait(1300, id);
		work('writing', '');
		send(s.reply);
		s.thread?.set('writing');
		for (const step of s.words.steps) {
			step();
			await wait(45 + Math.random() * 70, id);
		}
		s.thread?.set('idle');
		work('idle', '');
		app.classList.remove('busy');
		// The header's ring is the member's that answered last.
		if (s.chat === 'shop') {
			fills.shop = BISCUIT_FILL;
			drawContext();
		}
		// Its T3 thread's card comes under it, works a step a minute, and lies straight once done.
		if (s.t3) {
			await wait(600, id);
			t3(s.t3, 1, T3_STEPS[0]);
			s.t3.hidden = false;
			for (let m = 1; m < T3_STEPS.length; m++) {
				await wait(1500, id);
				t3(s.t3, m + 1, T3_STEPS[m]);
			}
			await wait(1500, id);
			t3(s.t3, T3_STEPS.length + 1);
		}
	}

	/** The scenes, once, each chat after a few seconds' rest; then it stays still. */
	async function playAll() {
		const id = ++run;
		playing = true;
		for (const s of scenes) {
			for (const row of [s.ask, s.reply]) row.classList.add('unsent');
			s.words.clear();
			if (s.t3) s.t3.hidden = true;
		}
		drawPins();
		try {
			for (const [i, s] of scenes.entries()) {
				if (i > 0) await wait(3500, id);
				await scene(s, id);
			}
			playing = false;
		} catch (e) {
			if (!(e instanceof Stopped)) throw e;
		}
	}

	/** Every chat as its scene ends, and nothing playing: with reduced motion, or once the
	 * visitor clicks in it. */
	function still() {
		run++;
		playing = false;
		typed.replaceChildren();
		app.classList.remove('busy');
		head.set('idle');
		for (const s of scenes) end(s);
		drawPins();
		drawContext();
	}
	app.addEventListener(
		'pointerdown',
		() => {
			if (playing) still();
		},
		{ capture: true },
	);

	const reduced = matchMedia('(prefers-reduced-motion: reduce)');
	reduced.addEventListener('change', () => {
		if (reduced.matches && playing) still();
	});
	// The newest entry: a scroll before the first callback brings both in one batch.
	new IntersectionObserver((entries) => (onScreen = entries[entries.length - 1].isIntersecting)).observe(app);
	if (reduced.matches) still();
	else playAll();
}
