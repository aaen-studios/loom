//! Watches on screen (PLAN.md, "Watching outside events"): a watch's quiet line in the
//! chat, and Following in an agent's or a group chat's settings, as `model/watch.rs`
//! works them out.

use super::*;
use crate::model::watch::{FollowPill, Followed, Follower, Lie, LineMark, WatchLine};

/// The width of a line's mark, the reply's thread's, so the pill after it starts where a
/// reply's words do.
const MARK_W: f32 = 14.0;

/// T3's strand, as wide as a reply's thread: lying straight in the ok colour once the
/// run finished, sagging in the bad colour once it failed, and slack and quiet once it
/// was stopped.
pub(super) fn strand(p: Palette, lie: Lie, key: SharedString) -> AnyElement {
    match lie {
        Lie::Straight => straight(p.ok),
        Lie::Sags => thread(key, Mood::Failed, p.bad, MARK_W, 10.0, None).into_any_element(),
        Lie::Slack => thread(key, Mood::Failed, p.ink_3, MARK_W, 10.0, None).into_any_element(),
    }
}

/// The strand lying straight, in `color`.
fn straight(color: Hsla) -> AnyElement {
    div()
        .w(px(MARK_W))
        .h(px(10.0))
        .flex_none()
        .flex()
        .items_center()
        .child(
            div()
                .w_full()
                .h(px(thread_stroke::SMALL))
                .rounded_full()
                .bg(color),
        )
        .into_any_element()
}

/// A pill's frame, as a card's has it: the bubble's fill with a line around it.
fn pill_frame(p: Palette) -> Div {
    div()
        .h(px(control::PILL))
        .min_w_0()
        .max_w(px(layout::PILL_TITLE + 110.0))
        .pl(px(4.0))
        .pr(px(7.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .rounded(px(radius::ROW))
        .border_1()
        .border_color(p.line)
        .bg(p.bubble)
}

/// A pill's words, cut to fit.
fn pill_words(p: Palette, words: String) -> Div {
    div()
        .min_w_0()
        .max_w(px(layout::PILL_TITLE))
        .truncate()
        .text_size(px(size::SMALL))
        .text_color(p.ink)
        .child(words)
}

/// A watch's line in the chat, left-aligned and quiet, as a routine's line is: its mark
/// in the thread's place (the board's tile, or T3's strand), the card's pill or the
/// thread's title, and what happened and who in quiet ink. Lines one after another sit
/// tight (`joined`); the reply of an agent it woke follows it as any reply does.
pub(super) fn update_line(
    p: Palette,
    m: &Message,
    line: &WatchLine,
    joined: model::Joined,
    column: f32,
    time_line: Option<Div>,
) -> Stateful<Div> {
    let key = SharedString::from(format!("line-{}", m.id));
    let mark = match line.mark() {
        LineMark::Board => board_icon(MARK_W),
        LineMark::Strand(lie) => strand(p, lie, key.clone()),
        LineMark::None => div().w(px(MARK_W)).flex_none().into_any_element(),
    };
    let subject = match line.mark() {
        // The pill without its tile, the mark already being it, and its dot the status
        // the line is about; its preview is the card now.
        LineMark::Board => {
            let card = line.card_shown(m.cards.get(&line.card));
            let dot = ink_color(p, line.dot(&card));
            Some(card_pill_in(p, key, &line.card, Some(&card), Some(dot)))
        }
        LineMark::Strand(_) => Some(
            div()
                .min_w_0()
                .max_w(px(layout::PILL_TITLE))
                .truncate()
                .text_size(px(size::SMALL))
                .text_color(p.ink_2)
                .child(line.thread_title())
                .into_any_element(),
        ),
        LineMark::None => None,
    };
    let tight = 2.0;
    div()
        .id(SharedString::from(format!("msg-{}", m.id)))
        .role(Role::ListItem)
        .aria_label(line.spoken())
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .px(px(space::XL))
        .children(time_line)
        .child(
            // As wide as a message's row, its hover's reach either side, so the mark is
            // where a reply's thread is.
            div()
                .w_full()
                .max_w(px(column + 2.0 * HOVER_REACH))
                .mx(px(-HOVER_REACH))
                .px(px(HOVER_REACH))
                .flex()
                .items_center()
                .gap(px(space::M))
                .pt(px(if joined.above { tight } else { space::XS }))
                .pb(px(if joined.below { tight } else { space::S }))
                .child(mark)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .children(subject)
                        // What happened keeps its words; the pill's title gives way.
                        .child(
                            div()
                                .flex_none()
                                .max_w(px(layout::PILL_TITLE))
                                .truncate()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(line.words()),
                        ),
                ),
        )
}

impl Loom {
    /// Following, in an agent's or a group chat's settings: each watch as its pill, with
    /// a dot and why in one line when it couldn't look, and an × that takes it away.
    /// None when there are none.
    pub(super) fn following_list(&self, who: Follower, cx: &mut Context<Self>) -> Option<Div> {
        let items = self.model.following(who);
        if items.is_empty() {
            return None;
        }
        let p = self.p;
        let mut list = div().w_full().flex().flex_col().gap(px(space::XS));
        for f in items {
            let id = f.watch.clone();
            // Why takes what the pill leaves, and pushes the × to the end.
            let failed = f.error.is_some();
            let why = f.error.clone().map(|why| {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().flex_none().size(px(7.0)).rounded_full().bg(p.bad))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(why),
                    )
            });
            list = list.child(
                div()
                    .w_full()
                    .min_h(px(control::PILL + 4.0))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(self.follow_pill(&f))
                    .children(why)
                    .when(!failed, |d| d.child(div().flex_1()))
                    .child(
                        div()
                            .id(SharedString::from(format!("unfollow-{id}")))
                            .role(Role::Button)
                            .aria_label("Stop following")
                            .flex_none()
                            .size(px(control::ICON))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .cursor_pointer()
                            .soft_ink(p.ink_3, p.ink)
                            .child(Icon::new(Ico::X).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.remove_watch(&id);
                                this.send(f);
                                cx.notify();
                            })),
                    ),
            );
        }
        Some(list)
    }

    /// What a watch in Following is on, as its pill: a card's, as in a message, a
    /// board's tile and name, or T3's strand and the thread's title.
    fn follow_pill(&self, f: &Followed) -> AnyElement {
        let p = self.p;
        let key = SharedString::from(format!("follow-{}", f.watch));
        match &f.pill {
            FollowPill::Card { id, link } => {
                let card = match self.model.card_previews.get(link) {
                    Some(model::CardPreview::Ready(c)) => Some((**c).clone()),
                    _ => None,
                };
                card_pill(p, key, id, card.as_ref())
            }
            FollowPill::Board(name) => pill_frame(p)
                .child(board_icon(14.0))
                .child(pill_words(p, name.clone()))
                .into_any_element(),
            // Its strand quiet: a watch doesn't know how the thread is going.
            FollowPill::Thread(title) => pill_frame(p)
                .pl(px(6.0))
                .child(straight(p.ink_3))
                .child(pill_words(p, title.clone()))
                .into_any_element(),
        }
    }
}
