//! Loom's card under a reply whose chat its agent program moved to another model
//! (PLAN.md, "A model fallback"), as `model/model_fallback.rs` works it out: in the reply
//! column, where a reply's words start. While the chat is there, its title, and Switch
//! back and New chat under it; once one is used, or the person picks a model, one quiet
//! line saying what happened.

use super::*;
use crate::model::model_fallback::{self, Going, Shown};

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
    let (inner, said) =
        match model_fallback::shown(card) {
            Some(Shown::On {
                title,
                from,
                going,
                error,
            }) => {
                let agent = match &card.body {
                    model::CardBody::ModelFallback { agent, .. } => agent.clone(),
                    _ => String::new(),
                };
                let back_label = if going == Some(Going::Back) {
                    "Switching back…".to_string()
                } else {
                    format!("Switch back to {from}")
                };
                let new_label = if going == Some(Going::NewChat) {
                    "Starting a new chat…".to_string()
                } else {
                    format!("New chat on {from}")
                };
                let back_this = this.clone();
                let back = card_button(p, format!("model-back-{}", m.id), &back_label, true)
                    .on_click(move |_, _, cx| {
                        let _ = back_this.update(cx, |loom, cx| loom.switch_model_back(cx));
                    });
                let fresh = card_button(p, format!("model-new-{}", m.id), &new_label, false)
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |loom, cx| loom.new_chat_from(&agent, cx));
                    });
                (on(p, &title, back, fresh, &error, scale), title.clone())
            }
            Some(Shown::Done(line)) => (
                card_done(p, done_lead(p, None, Ico::ArrowRightLeft), &line, "", scale),
                line,
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

/// On the fallback: the title with the warn colour's icon, the two buttons under it, and
/// what the instance said to one it refused.
fn on(
    p: Palette,
    title: &str,
    back: Stateful<Div>,
    fresh: Stateful<Div>,
    error: &str,
    scale: f32,
) -> Div {
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
        .gap(px(space::M))
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
                .gap(px(space::S))
                .text_size(px(size::BODY * scale))
                .child(Icon::new(Ico::ArrowRightLeft).small().text_color(p.warn))
                .child(
                    div()
                        .min_w_0()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.ink)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(space::S))
                .child(back)
                .child(fresh),
        )
        .children(error)
}

impl Loom {
    /// Switch back, on the open chat's fallback card.
    pub(super) fn switch_model_back(&mut self, cx: &mut Context<Self>) {
        let Some(conversation) = self.model.conversation() else {
            return;
        };
        let frame = self.model.switch_model_back(&conversation);
        self.send(frame);
        self.rows_dirty = true;
        cx.notify();
    }

    /// New chat, on the open chat's fallback card.
    pub(super) fn new_chat_from(&mut self, agent: &str, cx: &mut Context<Self>) {
        let Some(conversation) = self.model.conversation() else {
            return;
        };
        let frame = self.model.new_chat_from(&conversation, agent);
        self.send(frame);
        self.rows_dirty = true;
        cx.notify();
    }
}
