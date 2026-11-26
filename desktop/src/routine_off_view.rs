//! The card a routine leaves in the chat where it posts when it turns off on its own
//! (PLAN.md, "Routines"), as `model/routine_off.rs` works it out: in the reply column,
//! where a reply's words start. While it's off, one compact card: its name and "is off",
//! why under it, and Turn on at its end. Once it's on again, or deleted, one quiet line,
//! as an answered card's.

use super::*;
use crate::model::routine_off::{self, Shown};

/// The width of a reply's thread, so the card starts where a reply's words do.
const MARK_W: f32 = 14.0;

/// The card's row in the chat.
pub(super) fn row(
    p: Palette,
    m: &Message,
    card: &model::Card,
    column: f32,
    time_line: Option<Div>,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Stateful<Div> {
    let name = card.title.clone();
    let (inner, said) = match routine_off::shown(card) {
        Some(Shown::Off {
            why,
            sending,
            error,
        }) => {
            let routine = match &card.body {
                model::CardBody::RoutineOff { routine, .. } => routine.clone(),
                _ => String::new(),
            };
            let label = if sending { "Turning on…" } else { "Turn on" };
            let turn_on = card_button(p, format!("routine-on-{}", m.id), label, true).on_click(
                move |_, _, cx| {
                    let _ = this.update(cx, |loom, cx| loom.turn_routine_on(&routine, cx));
                },
            );
            (
                off(p, &name, &why, &error, turn_on, scale),
                format!("{name} is off. {why}"),
            )
        }
        Some(Shown::On) => (
            card_done(p, done_lead(p, None, Ico::Check), "On again", &name, scale),
            format!("{name} is on again"),
        ),
        Some(Shown::Deleted) => (
            card_done(p, done_lead(p, None, Ico::X), "Deleted", &name, scale),
            format!("{name} was deleted"),
        ),
        None => (div(), String::new()),
    };
    div()
        .id(SharedString::from(format!("msg-{}", m.id)))
        .role(Role::ListItem)
        .aria_label(said)
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .px(px(space::XL))
        .children(time_line)
        .child(
            div()
                .w_full()
                .max_w(px(column + 2.0 * HOVER_REACH))
                .mx(px(-HOVER_REACH))
                .px(px(HOVER_REACH))
                .flex()
                .gap(px(space::M))
                .py(px(space::S))
                .child(div().w(px(MARK_W)).flex_none())
                .child(div().flex_1().min_w_0().max_w(px(CARD_MOST)).child(inner)),
        )
}

/// Off: its name and "is off", why under it, and Turn on at its end; what the instance
/// said to a Turn on it refused under them.
fn off(p: Palette, name: &str, why: &str, error: &str, turn_on: Stateful<Div>, scale: f32) -> Div {
    let error = (!error.is_empty()).then(|| {
        div()
            .text_size(px(size::SMALL * scale))
            .text_color(p.bad)
            .child(error.to_string())
    });
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .px(px(space::L))
        .py(px(space::M))
        .rounded(px(radius::BUBBLE))
        .border_1()
        .border_color(if p.dark { p.line } else { p.line_strong })
        .bg(card_fill(p))
        .shadow(card_shadow(p))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .gap(px(space::XS + 1.0))
                                .text_size(px(size::BODY * scale))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(p.ink)
                                        .child(name.to_string()),
                                )
                                .child(div().flex_none().text_color(p.ink_3).child("is off")),
                        )
                        .child(
                            div()
                                .text_size(px(size::SMALL * scale))
                                .text_color(p.ink_3)
                                .child(why.to_string()),
                        ),
                )
                .child(turn_on),
        )
        .children(error)
}

impl Loom {
    /// Turn on, on a routine's card.
    pub(super) fn turn_routine_on(&mut self, routine: &str, cx: &mut Context<Self>) {
        let frame = self.model.turn_routine_on(routine);
        self.send(frame);
        // The card says it's on its way.
        self.rows_dirty = true;
        cx.notify();
    }
}
