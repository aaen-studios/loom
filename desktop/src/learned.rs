//! What an agent kept or proposed from a reply, under it (PLAN.md, "Sharing and
//! learning"): a quiet line for what was kept, which opens to list it with Forget or
//! Undo, and a card for what waits for the person. The model says which goes where
//! (`Message::learned_view`); this only draws it.

use super::*;
use crate::model::{ChangeLine, Learned};

/// The key that opens a quiet line's list, or unticks a note on the card.
fn open_key(m: &Message, what: &str) -> String {
    format!("learned/{}/{what}", m.id)
}

fn untick_key(id: &str) -> String {
    format!("untick/{id}")
}

/// What reply `m` has under it, if anything.
pub(super) fn learned_under(
    p: Palette,
    m: &Message,
    name: &str,
    tint: Hsla,
    open: &HashSet<String>,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Option<Div> {
    if m.learned.is_empty() {
        return None;
    }
    let v = m.learned_view();
    let mut out = div().flex().flex_col().gap(px(space::S));
    let mut any = false;
    if !v.notes.is_empty() {
        any = true;
        let key = open_key(m, "notes");
        let opened = open.contains(&key);
        out = out.child(quiet_line(
            p,
            &key,
            Ico::Brain,
            &v.notes_line,
            opened,
            this.clone(),
        ));
        if opened {
            let mut list = div().flex().flex_col().gap(px(space::XS)).pl(px(space::XL));
            for l in &v.notes {
                list = list.child(note_row(p, l, name, scale, this.clone()));
            }
            out = out.child(list);
        }
    }
    for (l, line) in &v.fixes {
        any = true;
        let key = open_key(m, &l.id);
        let opened = open.contains(&key);
        out = out.child(quiet_line(
            p,
            &key,
            Ico::BookOpen,
            line,
            opened,
            this.clone(),
        ));
        if opened {
            out = out.child(fix_done(p, l, scale, this.clone()));
        }
    }
    if v.later > 0 {
        any = true;
        let words = if v.later == 1 {
            "1 waiting in Proposals".to_string()
        } else {
            format!("{} waiting in Proposals", v.later)
        };
        let opener = this.clone();
        out = out.child(
            div()
                .id(SharedString::from(format!("learned-later-{}", m.id)))
                .role(Role::Button)
                .flex()
                .items_center()
                .gap(px(space::S))
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .cursor_pointer()
                .soft_ink(p.ink_3, p.ink)
                .child(Icon::new(Ico::Lightbulb).xsmall())
                .child(words)
                .on_click(move |_, _, cx| {
                    let _ = opener.update(cx, |loom, cx| loom.open(Screen::Proposals, cx));
                }),
        );
    }
    if !v.asks.is_empty() {
        any = true;
        out = out.child(notes_card(
            p,
            m,
            &v.asks,
            name,
            tint,
            open,
            scale,
            this.clone(),
        ));
    }
    for l in &v.fix_asks {
        any = true;
        out = out.child(fix_card(p, m, l, name, tint, scale, this.clone()));
    }
    any.then_some(out)
}

/// A quiet line under the reply, which opens and shuts what it says.
fn quiet_line(
    p: Palette,
    key: &str,
    icon: Ico,
    words: &str,
    opened: bool,
    this: WeakEntity<Loom>,
) -> Stateful<Div> {
    let k = key.to_string();
    div()
        .id(SharedString::from(format!("line-{key}")))
        .role(Role::Button)
        .aria_label(words.to_string())
        .flex()
        .items_center()
        .gap(px(space::S))
        .text_size(px(size::CAPTION))
        .text_color(p.ink_3)
        .cursor_pointer()
        .soft_ink(p.ink_3, p.ink)
        .child(Icon::new(icon).xsmall())
        .child(words.to_string())
        .child(
            Icon::new(if opened {
                Ico::ChevronUp
            } else {
                Ico::ChevronDown
            })
            .xsmall(),
        )
        .on_click(move |_, _, cx| {
            let _ = this.update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
        })
}

/// A small word that acts, quiet until the pointer is on it: Forget, Undo.
fn text_button(
    p: Palette,
    id: String,
    label: &str,
    act: impl Fn(&mut Loom, &mut Context<Loom>) + 'static,
    this: WeakEntity<Loom>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .role(Role::Button)
        .aria_label(label.to_string())
        .flex_none()
        .text_size(px(size::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.ink_2)
        .cursor_pointer()
        .soft_ink(p.ink_2, p.ink)
        .child(label.to_string())
        .on_click(move |_, _, cx| {
            let _ = this.update(cx, |loom, cx| act(loom, cx));
        })
}

/// A word that says it's done: Forgotten, Undone.
fn done_word(p: Palette, label: &str) -> Div {
    div()
        .flex_none()
        .text_size(px(size::CAPTION))
        .text_color(p.ink_3)
        .child(label.to_string())
}

/// A kept note in the opened line: what it says, who it's for, and Forget.
fn note_row(p: Palette, l: &Learned, name: &str, scale: f32, this: WeakEntity<Loom>) -> Div {
    let undone = l.status == "undone";
    let id = l.id.clone();
    let end = if undone {
        done_word(p, "Forgotten").into_any_element()
    } else {
        text_button(
            p,
            format!("forget-{}", l.id),
            "Forget",
            move |loom, cx| loom.undo_learned(&id, cx),
            this,
        )
        .into_any_element()
    };
    div()
        .flex()
        .items_start()
        .gap(px(space::M))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(size::SMALL * scale))
                        .text_color(if undone { p.ink_3 } else { p.ink_2 })
                        .when(undone, |d| d.line_through())
                        .child(l.text.clone()),
                )
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(l.for_whom(name)),
                ),
        )
        .child(end)
}

/// A kept fix, opened: why the skill failed, its change, and Undo.
fn fix_done(p: Palette, l: &Learned, scale: f32, this: WeakEntity<Loom>) -> Div {
    let id = l.id.clone();
    let end = if l.status == "undone" {
        done_word(p, "Undone").into_any_element()
    } else {
        text_button(
            p,
            format!("undo-{}", l.id),
            "Undo",
            move |loom, cx| loom.undo_learned(&id, cx),
            this,
        )
        .into_any_element()
    };
    let failed = l.failed_line();
    div()
        .pl(px(space::XL))
        .flex()
        .flex_col()
        .gap(px(space::S))
        .when(!failed.is_empty(), |d| {
            d.child(
                div()
                    .text_size(px(size::SMALL * scale))
                    .text_color(p.ink_2)
                    .child(failed),
            )
        })
        .child(change_view(p, &l.lines, scale))
        .child(div().flex().child(end))
}

/// A fix's change as its lines: removed ones in red, added in green, the ones around
/// them quiet, in mono.
pub(super) fn change_view(p: Palette, lines: &[ChangeLine], scale: f32) -> Div {
    let mut out = div()
        .w_full()
        .py(px(space::XS))
        .rounded(px(radius::INPUT))
        .bg(card_inset(p))
        .flex()
        .flex_col()
        .font_family(crate::prefs::mono_font())
        .text_size(px(size::CAPTION * scale))
        .line_height(relative(1.5));
    for line in lines {
        let (mark, ink, fill) = match line.op.as_str() {
            "-" => ("−", p.bad, Some(p.bad.opacity(0.10))),
            "+" => ("+", p.ok, Some(p.ok.opacity(0.10))),
            "…" => ("", p.ink_3, None),
            _ => (" ", p.ink_3, None),
        };
        let text = if line.op == "…" {
            "…".to_string()
        } else {
            line.text.clone()
        };
        out = out.child(
            div()
                .flex()
                .gap(px(space::S))
                .px(px(space::M))
                .when_some(fill, |d, f| d.bg(f))
                .child(div().w(px(10.0)).flex_none().text_color(ink).child(mark))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(if line.op == "-" || line.op == "+" {
                            p.ink
                        } else {
                            p.ink_3
                        })
                        .child(text),
                ),
        );
    }
    out
}

/// The card of notes that wait for the person: "Juniper would remember", a ticked list
/// with who each is for, and Not now and Keep N.
#[allow(clippy::too_many_arguments)]
fn notes_card(
    p: Palette,
    m: &Message,
    asks: &[Learned],
    name: &str,
    tint: Hsla,
    open: &HashSet<String>,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Div {
    let key = open_key(m, "card");
    let ticked = asks
        .iter()
        .filter(|l| !open.contains(&untick_key(&l.id)))
        .count();
    let mut list = div().flex().flex_col().gap(px(space::S));
    for l in asks {
        let on = !open.contains(&untick_key(&l.id));
        let k = untick_key(&l.id);
        let toggle = this.clone();
        let mark = div()
            .size(px(16.0))
            .mt(px(2.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border_1();
        list = list.child(
            div()
                .id(SharedString::from(format!("tick-{}", l.id)))
                .role(Role::CheckBox)
                .aria_label(l.text.clone())
                .flex()
                .items_start()
                .gap(px(space::S + 2.0))
                .cursor_pointer()
                .child(if on {
                    mark.border_color(tint)
                        .bg(tint)
                        .text_color(motion::ink_on(tint, p.button_ink, p.button))
                        .child(Icon::new(Ico::Check).xsmall())
                } else {
                    mark.border_color(p.line_strong)
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(size::SMALL * scale))
                                .text_color(if on { p.ink } else { p.ink_3 })
                                .child(l.text.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(l.for_whom(name)),
                        ),
                )
                .on_click(move |_, _, cx| {
                    let _ = toggle.update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
                }),
        );
    }
    let keep_label = if ticked == 0 {
        "Drop them".to_string()
    } else {
        format!("Keep {ticked}")
    };
    let (keeper, later) = (this.clone(), this.clone());
    let (kept_m, later_m) = (m.clone(), m.clone());
    CardFrame::new()
        .lead(card_icon(p, Ico::Brain, tint))
        .title(format!("{name} would remember"))
        .mark(card_mark(&key, true, tint))
        .row(list)
        .action(
            card_button(p, format!("keep-learned-{}", m.id), &keep_label, true).on_click(
                move |_, _, cx| {
                    let _ = keeper.update(cx, |loom, cx| loom.answer_learned(&kept_m, true, cx));
                },
            ),
        )
        .action(
            card_button(p, format!("later-learned-{}", m.id), "Not now", false).on_click(
                move |_, _, cx| {
                    let _ = later.update(cx, |loom, cx| loom.answer_learned(&later_m, false, cx));
                },
            ),
        )
        .waits(tint)
        .draw(p, scale)
}

/// The card of a fix that waits for the person: why the skill failed, its change, and
/// Keep the fix, Change it and Drop.
fn fix_card(
    p: Palette,
    m: &Message,
    l: &Learned,
    name: &str,
    tint: Hsla,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Div {
    let key = format!("{}/{}", open_key(m, "fix"), l.id);
    let failed = l.failed_line();
    let (keeper, changer, dropper) = (this.clone(), this.clone(), this);
    let (keep_l, change_l, drop_id) = (l.clone(), l.clone(), l.id.clone());
    CardFrame::new()
        .lead(card_icon(p, Ico::BookOpen, tint))
        .title(format!("{name} would fix {}", l.name))
        .mark(card_mark(&key, true, tint))
        .when_some_row((!failed.is_empty()).then(|| card_reason(p, &failed, tint, scale)))
        .row(change_view(p, &l.lines, scale))
        .action(
            card_button(p, format!("keep-fix-{}", l.id), "Keep the fix", true).on_click(
                move |_, _, cx| {
                    let _ = keeper.update(cx, |loom, cx| {
                        let f = loom.model.keep_proposal(&keep_l.proposal());
                        loom.send(f);
                        cx.notify();
                    });
                },
            ),
        )
        .action(
            card_button(p, format!("change-fix-{}", l.id), "Change it", false).on_click(
                move |_, window, cx| {
                    let _ = changer.update(cx, |loom, cx| loom.change_fix(&change_l, window, cx));
                },
            ),
        )
        .action(
            card_button(p, format!("drop-fix-{}", l.id), "Drop", false).on_click(
                move |_, _, cx| {
                    let _ = dropper.update(cx, |loom, cx| {
                        let f = loom.model.drop_proposal(&drop_id);
                        loom.send(f);
                        cx.notify();
                    });
                },
            ),
        )
        .waits(tint)
        .draw(p, scale)
}

impl CardFrame {
    /// A row, when there is one.
    fn when_some_row(self, row: Option<Div>) -> Self {
        match row {
            Some(r) => self.row(r),
            None => self,
        }
    }
}

impl Loom {
    /// Forgets a kept note, or puts back the skill a kept fix changed.
    pub(super) fn undo_learned(&mut self, id: &str, cx: &mut Context<Self>) {
        let f = self.model.undo_proposal(id);
        self.send(f);
        cx.notify();
    }

    /// Answers the card of notes under reply `m`: Keep, the ticked ones, or Not now.
    fn answer_learned(&mut self, m: &Message, keep: bool, cx: &mut Context<Self>) {
        let unticked: HashSet<String> = m
            .learned
            .iter()
            .map(|l| l.id.clone())
            .filter(|id| self.open_thoughts.contains(&untick_key(id)))
            .collect();
        for f in self.model.answer_learned(m, keep, &unticked) {
            self.send(f);
        }
        for id in unticked {
            self.open_thoughts.remove(&untick_key(&id));
        }
        cx.notify();
    }

    /// Opens a fix on the Proposals page, to change it before keeping it, as the person
    /// left it if they changed it before.
    fn change_fix(&mut self, l: &Learned, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Screen::Proposals, cx);
        let fix = l.proposal();
        self.proposal_open = Some(fix.clone());
        self.fill_proposal(&fix, window, cx);
        cx.notify();
    }
}
