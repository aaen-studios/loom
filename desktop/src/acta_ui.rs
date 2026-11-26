//! Acta beside the chat on screen (PLAN.md, "Acta beside the chat"): a card's side
//! preview, the form that makes a card from a message, and a board's columns and list,
//! in the panel on the right; and the pills of the cards made from a message, under it.
//! What each shows, and what a change sends, is the model's (`model::acta`).

use super::*;
use crate::model::acta::{self as panels, BoardTab, Panel};

thread_local! {
    /// The window, for a card's pill and its preview, which draw outside it, to open the
    /// side preview and the board view.
    static OPENER: RefCell<Option<WeakEntity<Loom>>> = const { RefCell::new(None) };
    /// The form's and the list's fields, made the first time they show.
    static FIELDS: RefCell<Option<Fields>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct Fields {
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    search: Entity<InputState>,
}

/// Keeps the window, for the pills to open Acta's panels from.
pub(super) fn keep_opener(this: WeakEntity<Loom>) {
    OPENER.with(|o| {
        let mut o = o.borrow_mut();
        if o.is_none() {
            *o = Some(this);
        }
    });
}

/// Opens a card's side preview from its pill or its preview, or with no window to
/// open it in, the card in Acta.
pub(super) fn open_card(card: &model::ActaCard, cx: &mut App) {
    let opener = OPENER.with(|o| o.borrow().clone());
    let (board, id) = (card.board.clone(), card.id.clone());
    let opened = opener.is_some_and(|o| {
        o.update(cx, |loom, cx| loom.open_card_panel(&board, &id, cx))
            .is_ok()
    });
    if !opened {
        cx.open_url(&card.link());
    }
}

/// Opens a board's view from a card's preview.
pub(super) fn open_board(board: &str, cx: &mut App) {
    let opener = OPENER.with(|o| o.borrow().clone());
    if let Some(o) = opener {
        let board = board.to_string();
        let _ = o.update(cx, |loom, cx| loom.open_board_panel(Some(&board), cx));
    }
}

/// The colour a status is drawn in, as a card's dot is.
fn status_ink(p: Palette, status: &str) -> Hsla {
    card_ink(
        p,
        &model::ActaCard {
            status: status.to_string(),
            ..Default::default()
        },
    )
}

/// A small chip, as a card's preview has them: a status in its colour, an assignee,
/// a size or a label.
fn small_chip(words: String, color: Hsla, edge: Hsla) -> Div {
    div()
        .h(px(22.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(11.0))
        .border_1()
        .border_color(edge)
        .text_size(px(size::CAPTION))
        .text_color(color)
        .whitespace_nowrap()
        .child(words)
}

/// A menu over everything, under what opened it.
fn drop_menu(p: Palette, width: f32) -> Div {
    div()
        .occlude()
        .w(px(width))
        .p(px(space::XS))
        .flex()
        .flex_col()
        .bg(p.raised)
        .border_1()
        .border_color(p.line_strong)
        .rounded(px(radius::CARD))
        .shadow_lg()
}

/// The pills of the cards made from a message, under it, on its side (PLAN.md, "Acta
/// beside the chat"). None when none were.
pub(super) fn made_pills(p: Palette, m: &Message) -> Option<Div> {
    let mut made: Vec<&model::ActaCard> = m.cards.values().filter(|c| c.made.is_some()).collect();
    if made.is_empty() {
        return None;
    }
    made.sort_by_key(|c| c.made);
    let mut row = div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(space::XS))
        .pt(px(space::XS))
        .when(m.role == "person", |d| d.justify_end())
        // A reply's: level with its words, past its thread (`reply`).
        .when(m.role != "person", |d| d.pl(px(14.0 + space::M)));
    for c in made {
        row = row.child(card_pill(
            p,
            SharedString::from(format!("made-{}-{}", m.id, c.id)),
            &c.id,
            Some(c),
        ));
    }
    Some(row)
}

/// A card dragged in a board's columns, to another.
#[derive(Clone)]
pub(super) struct CardDrag {
    id: String,
    title: SharedString,
    p: Palette,
}

impl Render for CardDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        div()
            .w(px(layout::BOARD_COLUMN))
            .px(px(space::M))
            .py(px(space::S))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(radius::ROW))
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .shadow_lg()
            .child(card_id_text(&self.id, p.ink_3))
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink)
                    .line_clamp(2)
                    .child(self.title.clone()),
            )
    }
}

impl Loom {
    /// The form's and the list's fields, made the first time they're needed.
    fn acta_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Fields {
        if let Some(f) = FIELDS.with(|f| f.borrow().clone()) {
            return f;
        }
        let fields = Fields {
            title: cx.new(|cx| InputState::new(window, cx).placeholder("Title")),
            description: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(4, 14)
                    .placeholder("Description")
            }),
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search")),
        };
        FIELDS.with(|f| *f.borrow_mut() = Some(fields.clone()));
        fields
    }

    /// Opens a card's side preview beside the chat, and looks it up.
    pub(super) fn open_card_panel(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        let f = self.model.open_card(board, id);
        self.send(f);
        self.show_side(Screen::Acta, cx);
    }

    /// Opens a board's view beside the chat: `board`, or the one last used.
    pub(super) fn open_board_panel(&mut self, board: Option<&str>, cx: &mut Context<Self>) {
        for f in self.model.open_board(board) {
            self.send(f);
        }
        self.show_side(Screen::Acta, cx);
    }

    /// Opens the form that makes a card from message `id`, filled from its words.
    pub(super) fn start_card_panel(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.model.messages.iter().find(|m| m.id == id).cloned() else {
            return;
        };
        let text = crate::emoji::as_names(&model::copy_text(&m, &self.model.tools));
        if let Some(f) = self.model.start_card(id, &text) {
            self.send(f);
        }
        let fields = self.acta_fields(window, cx);
        if let Some(Panel::Make(form)) = &self.model.acta.panel {
            let (title, description) = (form.title.clone(), form.description.clone());
            fields
                .title
                .update(cx, |s, cx| s.set_value(title, window, cx));
            fields
                .description
                .update(cx, |s, cx| s.set_value(description, window, cx));
        }
        self.show_side(Screen::Acta, cx);
    }

    /// Whether Acta's panel is the wide one: a board's view.
    pub(super) fn acta_wide(&self) -> bool {
        matches!(self.model.acta.panel, Some(Panel::Board(_)))
    }

    fn close_acta(&mut self, cx: &mut Context<Self>) {
        self.model.close_acta();
        self.close_side(cx);
    }

    /// Acta's panel beside the chat: its head, what it shows, and its foot.
    pub(super) fn acta_side(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let w = window.viewport_size().width.as_f32();
        let width = side_width(self.acta_wide(), w, self.bar_width(w), self.side_w);
        let (head, body, foot) = match self.model.acta.panel.clone() {
            Some(Panel::Card(c)) => self.card_side(*c, cx),
            Some(Panel::Make(f)) => self.make_side(f, window, cx),
            Some(Panel::Board(b)) => self.board_side(b, window, cx),
            // The card was made, and its form went: the side closes with the next frame.
            None => {
                self.side = None;
                cx.notify();
                (div(), div().flex_1(), None)
            }
        };
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .relative()
            .border_l_1()
            .border_color(p.line)
            .child(self.side_edge(width, cx))
            .child(
                div()
                    .h(px(SIDE_HEAD))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .child(head.flex_1().min_w_0())
                    .child(
                        self.close_button("acta-close")
                            .on_click(cx.listener(|this, _, _, cx| this.close_acta(cx))),
                    ),
            )
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .children(foot)
    }

    // --- A card's side preview ---

    fn card_side(
        &mut self,
        c: panels::CardPanel,
        cx: &mut Context<Self>,
    ) -> (Div, Div, Option<Div>) {
        let p = self.p;
        let board_name = c
            .detail
            .as_ref()
            .map(|d| d.card.board_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.model.board_name(&c.board));
        let head = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(board_icon(16.0))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(font::MONO)
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .child(format!("{board_name} · {}", c.id)),
            );
        let mut page = div().flex().flex_col().gap(px(space::L));
        match &c.detail {
            None => {
                if let Some(e) = &c.error {
                    page = page.child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.bad)
                            .child(e.clone()),
                    );
                }
            }
            Some(d) => {
                page = page
                    .child(
                        div()
                            .text_size(px(size::HEADING))
                            .line_height(px(size::HEADING * 1.3))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if d.card.struck() { p.ink_3 } else { p.ink })
                            .when(d.card.struck(), |t| t.line_through())
                            .child(d.card.title.clone()),
                    )
                    .child(self.card_chips(&c, d, cx));
                if let Some(e) = &c.error {
                    page = page.child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.bad)
                            .child(e.clone()),
                    );
                }
                if !d.description.trim().is_empty() {
                    let key = SharedString::from(format!("acta-desc-{}-{}", c.board, c.id));
                    page = page.child(div().text_size(px(size::BODY)).text_color(p.ink).child(
                        KeptText::new(key, d.description.clone(), |v| v.selectable(true)),
                    ));
                }
                if !d.checklist.is_empty() {
                    page = page.child(self.card_subtasks(d, cx));
                }
                if !d.comments.is_empty() {
                    page = page.child(self.card_comments(&c, d));
                }
            }
        }
        let body = div().flex_1().min_h_0().child(
            div()
                .id("acta-card")
                .size_full()
                .overflow_y_scroll()
                .px(px(space::L))
                .pt(px(space::S))
                .pb(px(space::XL))
                .child(page),
        );
        let link = c
            .detail
            .as_ref()
            .map(|d| d.card.link())
            .unwrap_or_else(|| format!("https://{}/{}#{}", model::ACTA, c.board, c.id));
        let foot = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::M))
            .border_t_1()
            .border_color(p.line)
            .child(
                self.button("acta-open", "Open in Acta", false)
                    .on_click(move |_, _, cx| cx.open_url(&link)),
            )
            .children(self.follow_button("acta-follow-card", &c.board, Some(&c.id), cx));
        (head, body, Some(foot))
    }

    /// Follow, or Following with a tick, for a board or one card on it in the chat
    /// shown: a watch whose lines come here. None until the person's watches are known.
    fn follow_button(
        &self,
        id: &'static str,
        board: &str,
        card: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let following = self.model.following(board, card)?;
        let p = self.p;
        let label = if following { "Following" } else { "Follow" };
        let (board, card) = (board.to_string(), card.map(str::to_string));
        Some(
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .px(px(space::L))
                .h(px(control::BUTTON))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded(px(radius::BUTTON))
                .border_1()
                .border_color(p.line_strong)
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink)
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .when(following, |d| d.child(Icon::new(Ico::Check).xsmall()))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(f) = this.model.toggle_follow(&board, card.as_deref()) {
                        this.send(f);
                    }
                    cx.notify();
                })),
        )
    }

    /// The side preview's chips: its status, which opens a menu to move it, its
    /// assignee, size and labels.
    fn card_chips(
        &self,
        c: &panels::CardPanel,
        d: &panels::CardDetail,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let ink = card_ink(p, &d.card);
        let status = small_chip(d.card.status_words(), ink, ink.opacity(0.6))
            .child(Icon::new(Ico::ChevronDown).xsmall());
        let status = div()
            .id("acta-status")
            .role(Role::Button)
            .aria_label(format!("Status: {}", d.card.status_words()))
            .cursor_pointer()
            .child(status)
            .on_click(cx.listener(|this, _, _, cx| {
                this.model.toggle_status_menu();
                cx.notify();
            }));
        let menu = c.status_menu.then(|| {
            let mut menu = drop_menu(p, 180.0).on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.model.toggle_status_menu();
                cx.notify();
            }));
            for s in panels::STATUSES {
                let on = d.card.status == s;
                menu = menu.child(
                    div()
                        .id(SharedString::from(format!("acta-move-{s}")))
                        .role(Role::MenuItem)
                        .aria_label(panels::status_name(s))
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .px(px(space::M))
                        .py(px(6.0))
                        .rounded(px(radius::ROW))
                        .cursor_pointer()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink)
                        .soft_hover(transparent_black(), p.hover, None)
                        .child(
                            div()
                                .flex_none()
                                .size(px(7.0))
                                .rounded_full()
                                .bg(status_ink(p, s)),
                        )
                        .child(div().flex_1().child(panels::status_name(s)))
                        .when(on, |d| {
                            d.child(
                                div()
                                    .text_color(p.ink_2)
                                    .child(Icon::new(Ico::Check).xsmall()),
                            )
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(f) = this.model.move_card(s) {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                );
            }
            menu
        });
        let mut chips =
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .child(div().relative().child(status).when_some(menu, |d, menu| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(26.0))
                            .left_0()
                            .child(deferred(anchored().child(menu))),
                    )
                }));
        for chip in d.card.chips() {
            chips = chips.child(small_chip(chip, p.ink_2, p.line_strong));
        }
        if !d.priority.is_empty() && d.priority != "none" {
            chips = chips.child(small_chip(
                panels::priority_name(&d.priority).to_string(),
                p.ink_2,
                p.line_strong,
            ));
        }
        chips
    }

    /// Its subtasks, each with a tick that toggles it.
    fn card_subtasks(&self, d: &panels::CardDetail, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(2.0)).child(
            div()
                .pb(px(space::XS))
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink_2)
                .child(d.card.subtasks_line().unwrap_or_else(|| "Subtasks".into())),
        );
        for t in &d.checklist {
            let sid = t.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("acta-sub-{}", t.id)))
                    .role(Role::CheckBox)
                    .aria_label(t.title.clone())
                    .flex()
                    .items_start()
                    .gap(px(space::S))
                    .px(px(space::XS))
                    .py(px(5.0))
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, None)
                    .child(
                        div()
                            .mt(px(1.0))
                            .flex_none()
                            .size(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(if t.done { p.ok } else { p.line_strong })
                            .when(t.done, |d| {
                                d.bg(p.ok)
                                    .text_color(p.bg)
                                    .child(Icon::new(Ico::Check).xsmall())
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(size::SMALL))
                            .line_height(px(size::SMALL * line::BODY))
                            .text_color(if t.done { p.ink_3 } else { p.ink })
                            .child(t.title.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.model.toggle_subtask(&sid) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        list
    }

    /// Its last three comments, oldest first: who and when, then what they said.
    fn card_comments(&self, c: &panels::CardPanel, d: &panels::CardDetail) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(space::M)).child(
            div()
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink_2)
                .child("Comments"),
        );
        for (n, m) in d.comments.iter().enumerate() {
            let day = panels::comment_day(&m.at);
            let who = if day.is_empty() {
                m.by.clone()
            } else {
                format!("{} · {day}", m.by)
            };
            let key = SharedString::from(format!("acta-comment-{}-{}-{n}-{}", c.board, c.id, m.at));
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(who),
                    )
                    .child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink)
                            .child(KeptText::new(key, m.body.clone(), |v| v.selectable(true))),
                    ),
            );
        }
        list
    }

    // --- Making a card ---

    /// The board picker of the form and the board view: the board's tile, its name and
    /// a chevron, with the person's boards under it, the last used first.
    fn board_picker(&self, board: &str, open: bool, for_form: bool, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let button = div()
            .id(if for_form {
                "acta-form-board"
            } else {
                "acta-view-board"
            })
            .role(Role::Button)
            .aria_label(format!("Board: {}", self.model.board_name(board)))
            .h(px(control::BUTTON_SMALL))
            .px(px(space::S))
            .flex()
            .items_center()
            .gap(px(space::S))
            .rounded(px(radius::BUTTON))
            .when(for_form, |d| d.border_1().border_color(p.line_strong))
            .cursor_pointer()
            .text_size(px(size::SMALL))
            .text_color(p.ink)
            .font_weight(FontWeight::MEDIUM)
            .soft_hover(transparent_black(), p.hover, None)
            .child(board_icon(16.0))
            .child(self.model.board_name(board))
            .child(
                div()
                    .text_color(p.ink_3)
                    .child(Icon::new(Ico::ChevronDown).xsmall()),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.model.toggle_board_menu();
                cx.notify();
            }));
        let menu = open.then(|| {
            let mut menu =
                drop_menu(p, layout::MENU).on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.model.toggle_board_menu();
                    cx.notify();
                }));
            for (n, (b, name)) in self.model.board_choices().into_iter().enumerate() {
                let on = b == board;
                menu = menu.child(
                    self.menu_item(
                        SharedString::from(format!("acta-board-{n}")),
                        Ico::Layers,
                        &name,
                        false,
                    )
                    .when(on, |d| {
                        d.child(
                            div()
                                .text_color(p.ink_2)
                                .child(Icon::new(Ico::Check).xsmall()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if for_form {
                            if let Some(f) = this.model.form_board(&b) {
                                this.send(f);
                            }
                        } else {
                            this.open_board_panel(Some(&b), cx);
                        }
                        cx.notify();
                    })),
                );
            }
            menu
        });
        div().relative().child(button).when_some(menu, |d, menu| {
            d.child(
                div()
                    .absolute()
                    .top(px(control::BUTTON_SMALL + 4.0))
                    .left_0()
                    .child(deferred(anchored().child(menu))),
            )
        })
    }

    fn make_side(
        &mut self,
        f: panels::MakeForm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Div, Div, Option<Div>) {
        let p = self.p;
        let fields = self.acta_fields(window, cx);
        let head = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .text_size(px(size::SMALL))
            .text_color(p.ink_2)
            .child(Icon::new(Ico::LayersPlus).small())
            .child("Make a card");
        let title = fields.title.read(cx).value().to_string();
        let mut page = div().w_full().flex().flex_col().gap(px(space::L));
        page = page
            .child(self.field("Board", self.board_picker(&f.board, f.board_menu, true, cx)))
            .child(self.field("Title", Input::new(&fields.title)))
            .child(self.field("Description", Textarea::new(&fields.description)));
        let labels = self
            .model
            .acta
            .boards
            .get(&f.board)
            .map(|b| b.labels.clone())
            .unwrap_or_default();
        if !labels.is_empty() {
            let mut chips = div().flex().flex_wrap().gap(px(space::S));
            for (n, l) in labels.into_iter().enumerate() {
                let on = f.labels.contains(&l);
                chips = chips.child(self.chip(("acta-label", n), &l, on, false).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.model.toggle_label(&l);
                        cx.notify();
                    }),
                ));
            }
            page = page.child(self.field("Labels", chips));
        }
        let sizes = panels::SIZES
            .iter()
            .map(|s| {
                let s: &'static str = s;
                self.segment(
                    format!("acta-size-{s}"),
                    &s.to_uppercase(),
                    f.size.as_deref() == Some(s),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.form_size(s);
                    cx.notify();
                }))
            })
            .collect();
        let priorities = panels::PRIORITIES
            .iter()
            .map(|s| {
                let s: &'static str = s;
                self.segment(
                    format!("acta-priority-{s}"),
                    panels::priority_name(s),
                    f.priority.as_deref() == Some(s),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.form_priority(s);
                    cx.notify();
                }))
            })
            .collect();
        page = page
            .child(self.field("Size", div().flex().child(self.segments(sizes))))
            .child(self.field("Priority", div().flex().child(self.segments(priorities))));
        let make = self
            .button("acta-make", "Make", true)
            .when(!self.model.can_make(&title), |d| d.opacity(0.5))
            .on_click(cx.listener(|this, _, window, cx| {
                let fields = this.acta_fields(window, cx);
                let title = fields.title.read(cx).value().to_string();
                let description = fields.description.read(cx).value().to_string();
                if let Some(f) = this.model.make_card(&title, &description) {
                    this.send(f);
                }
                cx.notify();
            }));
        let cancel = self
            .button("acta-cancel", "Cancel", false)
            .on_click(cx.listener(|this, _, _, cx| this.close_acta(cx)));
        page = page.child(div().flex().gap(px(space::S)).child(make).child(cancel));
        if let Some(e) = &f.error {
            page = page.child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.bad)
                    .child(e.clone()),
            );
        }
        let body = div().flex_1().min_h_0().child(
            div()
                .id("acta-make")
                .size_full()
                .overflow_y_scroll()
                .px(px(space::L))
                .pt(px(space::S))
                .pb(px(space::XXL))
                .child(page),
        );
        (head, body, None)
    }

    // --- A board ---

    fn board_side(
        &mut self,
        b: panels::BoardView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Div, Div, Option<Div>) {
        let p = self.p;
        let tabs = self.segments(vec![
            self.segment("acta-tab-columns", "Columns", b.tab == BoardTab::Columns)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model.board_tab(BoardTab::Columns);
                    cx.notify();
                })),
            self.segment("acta-tab-list", "List", b.tab == BoardTab::List)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model.board_tab(BoardTab::List);
                    cx.notify();
                })),
        ]);
        let head = div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .child(self.board_picker(&b.board, b.board_menu, false, cx))
            .child(div().flex_1())
            .children(self.follow_button("acta-follow-board", &b.board, None, cx))
            .child(tabs);
        let content = match b.tab {
            BoardTab::Columns => self.board_columns(&b, cx),
            BoardTab::List => self.board_list(&b, window, cx),
        };
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(e) = &b.error {
            body = body.child(
                div()
                    .px(px(space::L))
                    .pb(px(space::S))
                    .text_size(px(size::SMALL))
                    .text_color(p.bad)
                    .child(e.clone()),
            );
        }
        (head, body.child(content), None)
    }

    /// A card's row in a board: its id in mono, its assignee's initial and its size on
    /// the right, and its title under them. A click opens its side preview.
    fn board_row(
        &self,
        board: &str,
        t: &panels::BoardTask,
        show_status: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = self.p;
        let (board, id) = (board.to_string(), t.id.clone());
        let initial = t.initial();
        let right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!t.size.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(t.size.to_uppercase()),
                )
            })
            .when(!initial.is_empty(), |d| {
                d.child(
                    div()
                        .size(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(p.raised_2)
                        .text_size(px(10.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_2)
                        .child(initial),
                )
            });
        div()
            .id(SharedString::from(format!("acta-row-{}", t.id)))
            .role(Role::Button)
            .aria_label(format!("{} {}", t.id, t.title))
            .px(px(space::S))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, None)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .when(show_status, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .size(px(7.0))
                                .rounded_full()
                                .bg(status_ink(p, &t.status)),
                        )
                    })
                    .child(card_id_text(&t.id, p.ink_3))
                    .child(div().flex_1())
                    .child(right),
            )
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .line_height(px(size::SMALL * 1.4))
                    .text_color(if t.status == "canceled" {
                        p.ink_3
                    } else {
                        p.ink
                    })
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(t.title.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_card_panel(&board, &id, cx)))
    }

    /// The board's columns side by side, Todo, In progress, Review and Done, each a
    /// compact list. A card dragged to another column moves there.
    fn board_columns(&self, b: &panels::BoardView, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let mut row = div()
            .flex()
            .items_start()
            .gap(px(space::S))
            .px(px(space::M))
            .pb(px(space::XL));
        for (status, tasks) in panels::columns_of(&b.tasks) {
            let mut list = div().flex().flex_col().gap(px(2.0));
            for t in &tasks {
                list = list.child(self.board_row(&b.board, t, false, cx).on_drag(
                    CardDrag {
                        id: t.id.clone(),
                        title: t.title.clone().into(),
                        p,
                    },
                    |d: &CardDrag, _, _, cx| {
                        let d = d.clone();
                        cx.new(|_| d)
                    },
                ));
            }
            row = row.child(
                div()
                    .id(SharedString::from(format!("acta-col-{status}")))
                    .flex_1()
                    .min_w(px(layout::BOARD_COLUMN))
                    .min_h(px(120.0))
                    .p(px(space::XS))
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .rounded(px(radius::CARD))
                    .bg(p.side)
                    .drag_over::<CardDrag>(move |s, _, _, _| s.bg(p.hover))
                    .on_drop(cx.listener(move |this, d: &CardDrag, _, cx| {
                        if let Some(f) = this.model.drop_task(&d.id, status) {
                            this.send(f);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .px(px(space::S))
                            .pt(px(space::XS))
                            .pb(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(size::CAPTION))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.ink_2)
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(7.0))
                                    .rounded_full()
                                    .bg(status_ink(p, status)),
                            )
                            .child(panels::status_name(status))
                            .child(
                                div()
                                    .text_color(p.ink_3)
                                    .font_weight(FontWeight::NORMAL)
                                    .child(tasks.len().to_string()),
                            ),
                    )
                    .child(list),
            );
        }
        div()
            .id("acta-columns")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .child(row)
    }

    /// The board's cards as one list, with search and its labels and people to filter by.
    fn board_list(
        &mut self,
        b: &panels::BoardView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = self.p;
        let fields = self.acta_fields(window, cx);
        let q = fields.search.read(cx).value().to_string();
        if q != b.search {
            self.model.board_search(&q);
        }
        let (labels, people) = self.model.board_filters();
        let mut filters = div().flex().flex_wrap().gap(px(space::XS));
        for (n, l) in labels.into_iter().enumerate() {
            let on = b.label.as_deref() == Some(l.as_str());
            filters = filters.child(self.chip(("acta-filter-label", n), &l, on, false).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.model.board_label(&l);
                    cx.notify();
                }),
            ));
        }
        let mut who = div().flex().flex_wrap().gap(px(space::XS));
        for (n, a) in people.into_iter().enumerate() {
            let on = b.assignee.as_deref() == Some(a.as_str());
            who = who.child(self.chip(("acta-filter-who", n), &a, on, false).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.model.board_assignee(&a);
                    cx.notify();
                }),
            ));
        }
        let rows = panels::filtered(&b.tasks, &q, b.label.as_deref(), b.assignee.as_deref());
        let mut list = div().flex().flex_col().gap(px(2.0));
        for t in &rows {
            list = list.child(self.board_row(&b.board, t, true, cx));
        }
        if rows.is_empty() && !b.loading() {
            list = list.child(
                div()
                    .px(px(space::S))
                    .py(px(space::M))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("No cards"),
            );
        }
        div()
            .id("acta-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(space::L))
            .pb(px(space::XL))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(Input::new(&fields.search))
                    .child(filters)
                    .child(who)
                    .child(list),
            )
    }
}
