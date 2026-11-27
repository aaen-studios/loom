//! A draft card in an agent's reply (PLAN.md, "Drafts"), as `model/draft_card.rs` works
//! it out. Open, the whole of what goes out: a head with the service's mark, "Draft
//! email" and the thread a reply goes in; From, To, Cc and Subject as rows, a label
//! beside each value; the body; then a foot with "The message it answers", folded, and
//! Reject, Edit and Send, with Send later under the arrow. Edit makes the rows and the
//! body editable in place. Once it's sent, one line: the mark, the subject and where it
//! went, "Sent 14:02 ✓", and Undo with its seconds left while it can be taken back.

use super::*;
use crate::model::draft_card::{self, Draft, Shown, Toggle};

/// What's typed on a draft while it's edited or rejected, by its key (`message/card`):
/// each editable row's box, by its field, the body's, and Reject's note.
#[derive(Clone)]
pub(super) struct DraftInputs {
    rows: Vec<(String, Entity<InputState>)>,
    body: Option<Entity<TextareaState>>,
    note: Option<Entity<InputState>>,
}

thread_local! {
    /// The drafts' boxes, made when Edit or Reject opens them.
    static INPUTS: RefCell<HashMap<String, DraftInputs>> = RefCell::new(HashMap::new());
}

fn inputs_of(key: &str) -> Option<DraftInputs> {
    INPUTS.with(|i| i.borrow().get(key).cloned())
}

/// The label column's width, as the design has it.
const LABEL_W: f32 = 64.0;
const MARK: f32 = 14.0;

/// The service's name as the From row says it after the account: "(Gmail)".
fn service_name(service: &str) -> &str {
    match service {
        "gmail" => "Gmail",
        "outlook" => "Outlook",
        "slack" => "Slack",
        "discord" => "Discord",
        "github" => "GitHub",
        "twilio" => "Twilio",
        _ => "",
    }
}

/// The service's mark, one colour in the quiet ink, or its badge where loom has none.
#[inline(never)]
fn mark(p: Palette, d: &Draft, badge: Option<&model::Badge>) -> AnyElement {
    if !d.mark.is_empty() {
        return svg()
            .path(crate::runtime_svg(
                &format!("drafts/{}.svg", d.service),
                &d.mark,
            ))
            .size(px(MARK))
            .flex_none()
            .text_color(p.ink_2)
            .into_any_element();
    }
    match badge {
        Some(b) => badge_view(p, b, BADGE_MINI).into_any_element(),
        None => name_badge(p, &d.service, BADGE_MINI).into_any_element(),
    }
}

/// A button with no outline, quiet: Reject, Undo, Cancel.
#[inline(never)]
fn ghost(p: Palette, id: impl Into<SharedString>, label: &str) -> Stateful<Div> {
    div()
        .id(id.into())
        .role(Role::Button)
        .aria_label(label.to_string())
        .px(px(space::M))
        .h(px(control::BUTTON_SMALL))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(radius::BUTTON))
        .text_size(px(size::SMALL))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.ink_2)
        .cursor_pointer()
        .soft_hover(transparent_black(), p.hover, None)
        .child(label.to_string())
}

/// The draft's card in the reply.
pub(super) fn card(
    p: Palette,
    card: &model::Card,
    key: &str,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Div {
    let model::CardBody::Draft(d) = &card.body else {
        return div();
    };
    let wrap = |inner: Div| div().w_full().max_w(px(CARD_MOST)).child(inner);
    match d.shown(model::now_ms()) {
        Shown::Open => wrap(open(p, card, d, key, scale, this)),
        Shown::Going { what, to, at, left } => {
            let undo = answer_button(
                p,
                ghost(p, format!("draft-undo-{key}"), &format!("Undo · {left}")),
                d,
                &card.id,
                "undo",
                this,
            );
            wrap(line(
                p,
                card,
                d,
                &what,
                &to,
                sent_words(p, at),
                Some(undo),
                scale,
            ))
        }
        Shown::Sent { what, to, at } => {
            let how = if d.state == "sent" {
                sent_words(p, at)
            } else {
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child("Sending…")
            };
            wrap(line(p, card, d, &what, &to, how, None, scale))
        }
        Shown::Later { what, to, at } => {
            let undo = answer_button(
                p,
                ghost(p, format!("draft-undo-{key}"), "Undo"),
                d,
                &card.id,
                "undo",
                this,
            );
            let when = div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_2)
                .child(format!("Sends {}", later_words(at)));
            wrap(line(p, card, d, &what, &to, when, Some(undo), scale))
        }
        Shown::Failed { what, to, why } => {
            let again = answer_button(
                p,
                card_button(p, format!("draft-retry-{key}"), "Try again", false),
                d,
                &card.id,
                "retry",
                this,
            );
            let said = div()
                .min_w_0()
                .truncate()
                .text_size(px(size::CAPTION))
                .text_color(p.bad)
                .child(format!("Not sent: {why}"));
            wrap(line(p, card, d, &what, &to, said, Some(again), scale))
        }
        Shown::Rejected { what } => wrap(card_done(
            p,
            mark(p, d, card.badge.as_ref()),
            "Rejected",
            &what,
            scale,
        )),
    }
}

/// "Sent 14:02 ✓", in the good colour.
#[inline(never)]
fn sent_words(p: Palette, at: i64) -> Div {
    div()
        .flex_none()
        .text_size(px(size::CAPTION))
        .text_color(p.ok)
        .child(format!("Sent {} ✓", time_of(at)))
}

/// When a draft set to go later goes: "14:02", or "tomorrow 08:00", or its date.
fn later_words(at: i64) -> String {
    use chrono::TimeZone;
    let Some(t) = chrono::Local.timestamp_millis_opt(at).single() else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    if t.date_naive() == today {
        t.format("%H:%M").to_string()
    } else if today.succ_opt() == Some(t.date_naive()) {
        t.format("tomorrow %H:%M").to_string()
    } else {
        t.format("%-d %b %H:%M").to_string()
    }
}

/// A button that answers the draft with `answer`, as it is.
#[inline(never)]
fn answer_button(
    p: Palette,
    button: Stateful<Div>,
    d: &Draft,
    card_id: &str,
    answer: &'static str,
    this: WeakEntity<Loom>,
) -> AnyElement {
    let _ = p;
    let (message, card_id) = (d.message.clone(), card_id.to_string());
    button
        .on_click(move |_, _, cx| {
            let _ = this.update(cx, |loom, cx| {
                loom.answer_draft(&message, &card_id, answer, None, cx)
            });
        })
        .into_any_element()
}

/// The one line a draft is once it's gone, going or couldn't go: the mark, the subject
/// and where it went, how it stands, and its one button.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn line(
    p: Palette,
    card: &model::Card,
    d: &Draft,
    what: &str,
    to: &str,
    how: Div,
    button: Option<AnyElement>,
    scale: f32,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::S + 2.0))
        .px(px(14.0))
        .py(px(7.0))
        .rounded(px(radius::BUBBLE))
        .border_1()
        .border_color(p.line)
        .bg(p.bg)
        .child(mark(p, d, card.badge.as_ref()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .gap(px(space::XS + 2.0))
                .text_size(px(size::SMALL * scale))
                .child(
                    div()
                        .min_w_0()
                        .flex_shrink_0()
                        .max_w(relative(0.6))
                        .truncate()
                        .text_color(p.ink)
                        .child(what.to_string()),
                )
                .when(!to.is_empty(), |d| {
                    d.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(p.ink_3)
                            .child(format!("→ {to}")),
                    )
                }),
        )
        .child(how)
        .children(button)
}

/// What `open` hands its parts: the draft, its key, whether it's being edited, what's
/// typed on it, and the text size.
struct Parts<'a> {
    p: Palette,
    card: &'a model::Card,
    d: &'a Draft,
    key: &'a str,
    editing: bool,
    inputs: Option<DraftInputs>,
    scale: f32,
    this: WeakEntity<Loom>,
}

impl Parts<'_> {
    fn text(&self) -> Pixels {
        px(size::SMALL * self.scale)
    }

    /// A click that turns `what` on or off.
    fn toggle(&self, what: Toggle, on: bool) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
        let this = self.this.clone();
        let (message, card_id) = (self.d.message.clone(), self.card.id.clone());
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |loom, cx| {
                loom.toggle_draft(&message, &card_id, what, on, window, cx)
            });
        }
    }

    /// A button that answers the draft with `answer`, at `at` for Send later.
    fn answers(&self, button: Stateful<Div>, answer: &'static str, at: Option<i64>) -> AnyElement {
        let this = self.this.clone();
        let (message, card_id) = (self.d.message.clone(), self.card.id.clone());
        button
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |loom, cx| {
                    loom.answer_draft(&message, &card_id, answer, at, cx)
                });
            })
            .into_any_element()
    }
}

/// The open draft: head, rows, body, the message it answers, and the foot. Each part is
/// drawn in a function of its own, so no one frame on the stack holds them all.
fn open(
    p: Palette,
    card: &model::Card,
    d: &Draft,
    key: &str,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Div {
    let inputs = if d.editing || d.rejecting {
        inputs_of(&draft_card::key(&d.message, &card.id))
    } else {
        None
    };
    let editing = d.editing && inputs.is_some();
    let parts = Parts {
        p,
        card,
        d,
        key,
        editing,
        inputs,
        scale,
        this,
    };
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(radius::BUBBLE))
        .border_1()
        .border_color(p.line)
        .bg(p.bg)
        .overflow_hidden()
        .child(open_head(&parts))
        .child(open_rows(&parts))
        .child(open_body(&parts))
        .children(open_answers(&parts))
        .child(open_foot(&parts))
        .children(open_later(&parts))
        .children(open_error(&parts))
}

/// The head: the mark, what it is, and the thread a reply goes in.
#[inline(never)]
fn open_head(x: &Parts) -> AnyElement {
    let (p, d) = (x.p, x.d);
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(14.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(p.line)
        .child(mark(p, d, x.card.badge.as_ref()))
        .child(
            div()
                .flex_none()
                .text_size(x.text())
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink)
                .child(x.card.title.clone()),
        )
        .when(!d.thread.is_empty(), |h| {
            h.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(format!("· replying in “{}”", d.thread)),
            )
        })
        .into_any_element()
}

/// The rows, a label beside each value; while it's edited, a box for each Edit changes.
#[inline(never)]
fn open_rows(x: &Parts) -> AnyElement {
    let mut rows = div().flex().flex_col().px(px(14.0)).py(px(6.0));
    for r in x.d.rows_shown() {
        rows = rows.child(open_row(x, &r));
    }
    rows.into_any_element()
}

#[inline(never)]
fn open_row(x: &Parts, r: &draft_card::DraftRow) -> AnyElement {
    let p = x.p;
    let boxed = x
        .editing
        .then_some(x.inputs.as_ref())
        .flatten()
        .and_then(|i| i.rows.iter().find(|(k, _)| *k == r.key))
        .map(|(_, input)| input.clone());
    let value: AnyElement = match boxed {
        Some(input) => div()
            .flex_1()
            .min_w_0()
            .child(Input::new(&input).small())
            .into_any_element(),
        None => {
            let named = (r.key == "from")
                .then(|| service_name(&x.d.service))
                .filter(|n| !n.is_empty() && !r.value.eq_ignore_ascii_case(n));
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .gap(px(space::XS))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(p.ink)
                        .child(r.value.clone()),
                )
                .children(named.map(|n| {
                    div()
                        .flex_none()
                        .text_color(p.ink_3)
                        .child(format!("({n})"))
                }))
                .into_any_element()
        }
    };
    div()
        .flex()
        .items_center()
        .min_h(px(30.0))
        .text_size(x.text())
        .child(
            div()
                .w(px(LABEL_W))
                .flex_none()
                .truncate()
                .text_color(p.ink_3)
                .child(r.label.clone()),
        )
        .child(value)
        .into_any_element()
}

/// The body, as it goes out, or its box.
#[inline(never)]
fn open_body(x: &Parts) -> AnyElement {
    let p = x.p;
    let body_box = x
        .editing
        .then(|| x.inputs.as_ref().and_then(|i| i.body.clone()))
        .flatten();
    div()
        .px(px(14.0))
        .py(px(10.0))
        .border_t_1()
        .border_color(p.line)
        .text_size(x.text())
        .line_height(relative(1.6))
        .text_color(p.ink)
        .child(match body_box {
            Some(b) => div().w_full().child(Textarea::new(&b)).into_any_element(),
            None => div().child(x.d.body.clone()).into_any_element(),
        })
        .into_any_element()
}

/// The message it answers, under the body once it's unfolded.
#[inline(never)]
fn open_answers(x: &Parts) -> Option<AnyElement> {
    let (p, d) = (x.p, x.d);
    (d.answers_open && !d.answers.is_empty()).then(|| {
        div()
            .px(px(14.0))
            .pb(px(10.0))
            .child(
                div()
                    .pl(px(space::M))
                    .border_l_2()
                    .border_color(p.line_strong)
                    .text_size(px(size::CAPTION * x.scale))
                    .line_height(relative(1.55))
                    .text_color(p.ink_2)
                    .child(d.answers.clone()),
            )
            .into_any_element()
    })
}

/// The foot: the message it answers, folded, then Reject, Edit, Send and the arrow; or
/// while it's edited, Cancel, Send and the arrow; or Reject's note, Cancel and Reject.
#[inline(never)]
fn open_foot(x: &Parts) -> AnyElement {
    let (p, d, key) = (x.p, x.d, x.key);
    let foot = div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(14.0))
        .py(px(10.0))
        .border_t_1()
        .border_color(p.line);
    if d.rejecting {
        let note = x.inputs.as_ref().and_then(|i| i.note.clone());
        return foot
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .children(note.map(|n| Input::new(&n).small())),
            )
            .child(
                ghost(p, format!("draft-reject-cancel-{key}"), "Cancel")
                    .on_click(x.toggle(Toggle::Reject, false)),
            )
            .child(x.answers(
                card_button(p, format!("draft-reject-go-{key}"), "Reject", true),
                "reject",
                None,
            ))
            .into_any_element();
    }
    let send = x.answers(
        card_button(
            p,
            format!("draft-send-{key}"),
            if d.sending { "Sending…" } else { "Send" },
            true,
        ),
        "send",
        None,
    );
    let arrow = card_button(p, format!("draft-later-{key}"), "▾", false)
        .px(px(space::S + 1.0))
        .aria_label("Send later")
        .on_click(x.toggle(Toggle::Later, !d.later_open));
    let foot = foot.child(div().flex_1().min_w_0().children(open_folded(x)));
    let foot = if x.editing {
        foot.child(
            ghost(p, format!("draft-edit-cancel-{key}"), "Cancel")
                .on_click(x.toggle(Toggle::Edit, false)),
        )
    } else {
        foot.child(
            ghost(p, format!("draft-reject-{key}"), "Reject")
                .on_click(x.toggle(Toggle::Reject, true)),
        )
        .child(
            card_button(p, format!("draft-edit-{key}"), "Edit", false)
                .on_click(x.toggle(Toggle::Edit, true)),
        )
    };
    foot.child(send).child(arrow).into_any_element()
}

/// "› The message it answers", which unfolds it.
#[inline(never)]
fn open_folded(x: &Parts) -> Option<AnyElement> {
    let (p, d) = (x.p, x.d);
    (!d.answers.is_empty()).then(|| {
        div()
            .id(SharedString::from(format!("draft-answers-{}", x.key)))
            .role(Role::Button)
            .aria_label("The message it answers")
            .min_w_0()
            .truncate()
            .text_size(px(size::CAPTION))
            .text_color(p.ink_3)
            .cursor_pointer()
            .soft_ink(p.ink_3, p.ink)
            .child(format!(
                "{} The message it answers",
                if d.answers_open { "⌄" } else { "›" }
            ))
            .on_click(x.toggle(Toggle::Answers, !d.answers_open))
            .into_any_element()
    })
}

/// Send later's times, under the foot while the arrow's open.
#[inline(never)]
fn open_later(x: &Parts) -> Option<AnyElement> {
    if !x.d.later_open {
        return None;
    }
    let p = x.p;
    let mut times = div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_end()
        .gap(px(space::S))
        .px(px(14.0))
        .pb(px(10.0))
        .child(
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Send later"),
        );
    for (i, (words, at)) in draft_card::later_times(chrono::Local::now())
        .into_iter()
        .enumerate()
    {
        times = times.child(x.answers(
            card_button(p, format!("draft-at-{}-{i}", x.key), &words, false),
            "later",
            Some(at),
        ));
    }
    Some(times.into_any_element())
}

/// What the instance said to an answer it refused.
#[inline(never)]
fn open_error(x: &Parts) -> Option<AnyElement> {
    (!x.d.error.is_empty()).then(|| {
        div()
            .px(px(14.0))
            .pb(px(10.0))
            .text_size(px(size::CAPTION))
            .text_color(x.p.bad)
            .child(x.d.error.clone())
            .into_any_element()
    })
}

impl Loom {
    /// Edit, Reject, the message it answers, or Send later, on or off on a draft. Edit
    /// makes a box for each row it changes and the body, with what they say now;
    /// Reject, one for its note.
    pub(super) fn toggle_draft(
        &mut self,
        message: &str,
        card: &str,
        what: Toggle,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = draft_card::key(message, card);
        if on && (what == Toggle::Edit || what == Toggle::Reject) {
            let d = self
                .model
                .messages
                .iter()
                .find(|m| m.id == message)
                .and_then(|m| {
                    m.blocks.iter().find_map(|b| match b.card.as_ref() {
                        Some(c) if c.id == card => match &c.body {
                            model::CardBody::Draft(d) => Some((**d).clone()),
                            _ => None,
                        },
                        _ => None,
                    })
                });
            let mut inputs = inputs_of(&key).unwrap_or(DraftInputs {
                rows: Vec::new(),
                body: None,
                note: None,
            });
            if what == Toggle::Edit {
                let mut shown = d.unwrap_or_default();
                shown.editing = true;
                inputs.rows = shown
                    .rows_shown()
                    .into_iter()
                    .filter(|r| r.edit)
                    .map(|r| {
                        let input = cx.new(|cx| InputState::new(window, cx));
                        let value = r.value.clone();
                        input.update(cx, |s, cx| s.set_value(value, window, cx));
                        (r.key, input)
                    })
                    .collect();
                let body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 16));
                let text = shown.body.clone();
                body.update(cx, |s, cx| s.set_value(text, window, cx));
                inputs.body = Some(body);
                self.model
                    .toggle_draft(message, card, Toggle::Reject, false);
            } else {
                inputs.note = Some(
                    cx.new(|cx| InputState::new(window, cx).placeholder("A note for the agent")),
                );
                self.model.toggle_draft(message, card, Toggle::Edit, false);
                self.model.toggle_draft(message, card, Toggle::Later, false);
            }
            INPUTS.with(|i| i.borrow_mut().insert(key.clone(), inputs));
        }
        if !on && what == Toggle::Edit {
            INPUTS.with(|i| {
                if let Some(x) = i.borrow_mut().get_mut(&key) {
                    x.rows.clear();
                    x.body = None;
                }
            });
        }
        self.model.toggle_draft(message, card, what, on);
        self.rows_dirty = true;
        cx.notify();
    }

    /// The person's answer on a draft: Send (with what Edit changed), Send later at
    /// `at`, Reject (with its note), Undo or Try again.
    pub(super) fn answer_draft(
        &mut self,
        message: &str,
        card: &str,
        answer: &str,
        at: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        let key = draft_card::key(message, card);
        let inputs = inputs_of(&key);
        let mut edits = None;
        let mut note = String::new();
        if let Some(i) = &inputs {
            if (answer == "send" || answer == "later") && i.body.is_some() {
                let mut e = serde_json::Map::new();
                for (k, input) in &i.rows {
                    e.insert(k.clone(), input.read(cx).value().to_string().into());
                }
                if let Some(b) = &i.body {
                    e.insert("body".into(), b.read(cx).value().to_string().into());
                }
                edits = Some(serde_json::Value::Object(e));
            }
            if answer == "reject" {
                if let Some(n) = &i.note {
                    note = n.read(cx).value().to_string();
                }
            }
        }
        let f = self
            .model
            .answer_draft(message, card, answer, edits, &note, at.unwrap_or(0));
        self.send(f);
        if answer == "send" {
            self.draft_tick(cx);
        }
        self.rows_dirty = true;
        cx.notify();
    }

    /// While a draft can be taken back, its Undo counts down: the chat is drawn again
    /// each second until none can.
    pub(super) fn draft_tick(&mut self, cx: &mut Context<Self>) {
        if self.draft_ticking {
            return;
        }
        self.draft_ticking = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let going = this
                .update(cx, |loom, cx| {
                    let now = model::now_ms();
                    let going = loom.model.messages.iter().any(|m| {
                        m.blocks
                            .iter()
                            .any(|b| match b.card.as_ref().map(|c| &c.body) {
                                Some(model::CardBody::Draft(d)) => {
                                    d.state == "sending" && d.until + 1000 > now
                                }
                                _ => false,
                            })
                    });
                    loom.rows_dirty = true;
                    cx.notify();
                    if !going {
                        loom.draft_ticking = false;
                    }
                    going
                })
                .unwrap_or(false);
            if !going {
                break;
            }
        })
        .detach();
    }
}
