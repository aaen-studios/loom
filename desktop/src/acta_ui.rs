//! Acta beside the chat on screen (PLAN.md, "Acta beside the chat"): a card's side
//! preview, the form that makes a card from a message, and a board's columns and list,
//! in the panel on the right; and the pills of the cards made from a message, under it.
//! What each shows, and what a change sends, is the model's (`model::acta`).

use super::*;
use crate::model::acta::{self as panels, BoardTab, CardTab, Panel};

thread_local! {
    /// The window, for a card's pill and its preview, which draw outside it, to open the
    /// side preview and the board view.
    static OPENER: RefCell<Option<WeakEntity<Loom>>> = const { RefCell::new(None) };
    /// The form's and the list's fields, made the first time they show.
    static FIELDS: RefCell<Option<Fields>> = const { RefCell::new(None) };
    /// The card the comment box was last shown for, `board#id`.
    static COMMENT_ON: RefCell<String> = const { RefCell::new(String::new()) };
}

#[derive(Clone)]
struct Fields {
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    search: Entity<InputState>,
    comment: Entity<TextareaState>,
}

/// Acta's words for what it refused, under the chips.
fn error_line(p: Palette, e: &str) -> Div {
    div()
        .text_size(px(size::SMALL))
        .text_color(p.bad)
        .child(e.to_string())
}

/// One of a card's properties as a chip under its title: its status, priority,
/// assignee or size.
fn prop_chip(p: Palette) -> Div {
    div()
        .h(px(22.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(p.line)
        .bg(p.bg)
        .text_size(px(size::CAPTION))
        .text_color(p.ink_2)
        .whitespace_nowrap()
}

/// A priority as three bars, rising, `filled` of them in ink.
fn priority_icon(p: Palette, filled: u8) -> Div {
    let mut icon = div()
        .flex_none()
        .h(px(10.0))
        .flex()
        .items_end()
        .gap(px(1.5));
    for (k, h) in [4.0, 7.0, 10.0].into_iter().enumerate() {
        icon = icon.child(
            div()
                .w(px(2.5))
                .h(px(h))
                .rounded(px(0.5))
                .bg(if (k as u8) < filled {
                    p.ink_2
                } else {
                    p.line_strong
                }),
        );
    }
    icon
}

/// A person as a round avatar with their initial, `d` across.
fn avatar(p: Palette, initial: &str, d: f32) -> Div {
    div()
        .flex_none()
        .size(px(d))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.active)
        .text_color(p.ink)
        .text_size(px((d * 0.55).max(8.0)))
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial.to_string())
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
            None,
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
            comment: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(2, 8)
                    .placeholder("Comment…")
            }),
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
        let text = model::copy_text(&m, &self.model.tools);
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
            Some(Panel::Card(c)) => self.card_side(*c, window, cx),
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Div, Div, Option<Div>) {
        let p = self.p;
        let fields = self.acta_fields(window, cx);
        // A comment that went through, or another card: the box starts empty.
        let other = COMMENT_ON.with(|on| {
            let mut on = on.borrow_mut();
            let key = format!("{}#{}", c.board, c.id);
            let other = *on != key;
            *on = key;
            other
        });
        if self.model.take_commented() || other {
            fields
                .comment
                .update(cx, |s, cx| s.set_value("", window, cx));
        }
        let board_name = c
            .detail
            .as_ref()
            .map(|d| d.card.board_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.model.board_name(&c.board));
        let link = c
            .detail
            .as_ref()
            .map(|d| d.card.link())
            .unwrap_or_else(|| format!("https://{}/{}#{}", model::ACTA, c.board, c.id));
        let head = self.card_head(&c, &board_name, &link, cx);
        let mut page = div().flex().flex_col();
        let mut foot = None;
        match &c.detail {
            None => {
                if let Some(e) = &c.error {
                    page = page.child(div().px(px(space::L)).child(error_line(p, e)));
                }
            }
            Some(d) => {
                page = page
                    .child(
                        div()
                            .px(px(space::L))
                            .pt(px(2.0))
                            .pb(px(space::S))
                            .text_size(px(size::HEADING))
                            .line_height(px(size::HEADING * 1.3))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if d.card.struck() { p.ink_3 } else { p.ink })
                            .when(d.card.struck(), |t| t.line_through())
                            .child(d.card.title.clone()),
                    )
                    .child(
                        div()
                            .px(px(space::L))
                            .pb(px(space::M))
                            .child(self.card_chips(&c, d, cx)),
                    );
                if let Some(e) = &c.error {
                    page = page.child(
                        div()
                            .px(px(space::L))
                            .pb(px(space::M))
                            .child(error_line(p, e)),
                    );
                }
                page = page.child(self.card_tabs(&c, d, cx));
                let tab = match c.tab {
                    CardTab::Details => self.card_details(&c, d, cx),
                    CardTab::Comments => self.card_comments(&c, d),
                    CardTab::Activity => self.card_activity(d),
                };
                page = page.child(tab.px(px(space::L)).pt(px(space::M)));
                if c.tab == CardTab::Comments && d.can_comment {
                    foot = Some(self.comment_box(&c, &fields, cx));
                }
            }
        }
        let body = div().flex_1().min_h_0().child(
            div()
                .id("acta-card")
                .size_full()
                .overflow_y_scroll()
                .pb(px(space::XL))
                .child(page),
        );
        (head, body, foot)
    }

    /// The side preview's head: Acta's mark, the board's name and the card's id, which a
    /// click copies, then Open in Acta and ⋯.
    fn card_head(
        &self,
        c: &panels::CardPanel,
        board_name: &str,
        link: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let id = c.id.clone();
        let open = link.to_string();
        let more = div()
            .id("acta-card-more")
            .role(Role::Button)
            .aria_label("More")
            .size(px(control::ICON))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .text_color(p.ink_3)
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
            .tooltip(tip("More"))
            .child(Icon::new(Ico::Ellipsis).small())
            .on_click(cx.listener(|this, _, _, cx| {
                this.model.toggle_card_menu();
                cx.notify();
            }));
        let menu = c.more_menu.then(|| self.card_menu(c, link, cx));
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(board_icon(p, 15.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .child(div().min_w_0().truncate().child(format!("{board_name} ·")))
                    .child(
                        div()
                            .id("acta-card-id")
                            .role(Role::Button)
                            .aria_label(format!("Copy {}", c.id))
                            .flex_none()
                            .px(px(2.0))
                            .rounded(px(4.0))
                            .font_family(font::MONO)
                            .cursor_pointer()
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
                            .tooltip(tip("Copy"))
                            .child(c.id.clone())
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                            }),
                    ),
            )
            .child(
                div()
                    .id("acta-open")
                    .role(Role::Link)
                    .aria_label("Open in Acta")
                    .flex_none()
                    .h(px(24.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .rounded(px(radius::ROW))
                    .text_size(px(size::SMALL))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, None)
                    .child("Open in Acta")
                    .on_click(move |_, _, cx| cx.open_url(&open)),
            )
            .child(div().relative().child(more).when_some(menu, |d, menu| {
                d.child(
                    div()
                        .absolute()
                        .top(px(control::ICON + 2.0))
                        .right_0()
                        .child(deferred(anchored().anchor(Anchor::TopRight).child(menu))),
                )
            }))
    }

    /// The head's ⋯ menu: Follow, Copy link, Copy branch where Acta gives one, and Make a
    /// thread with a T3 server connected.
    fn card_menu(&self, c: &panels::CardPanel, link: &str, cx: &mut Context<Self>) -> Div {
        let mut menu = drop_menu(self.p, 200.0).on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.model.toggle_card_menu();
            cx.notify();
        }));
        if let Some(following) = self.model.follows(&c.board, Some(&c.id)) {
            let (board, id) = (c.board.clone(), c.id.clone());
            let (icon, label) = if following {
                (Ico::Check, "Following")
            } else {
                (Ico::Bell, "Follow")
            };
            menu = menu.child(
                self.menu_item("acta-menu-follow", icon, label, false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.toggle_card_menu();
                        if let Some(f) = this.model.toggle_follow(&board, Some(&id)) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        let link = link.to_string();
        menu = menu.child(
            self.menu_item("acta-menu-link", Ico::Copy, "Copy link", false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.toggle_card_menu();
                    cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                    cx.notify();
                })),
        );
        let Some(d) = c.detail.as_ref() else {
            return menu;
        };
        if !d.branch.is_empty() {
            let branch = d.branch.clone();
            menu = menu.child(
                self.menu_item("acta-menu-branch", Ico::GitBranch, "Copy branch", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.toggle_card_menu();
                        cx.write_to_clipboard(ClipboardItem::new_string(branch.clone()));
                        cx.notify();
                    })),
            );
        }
        if !self.model.t3.servers.is_empty() {
            let task = d.thread_task();
            menu = menu.child(
                self.menu_item(
                    "acta-menu-thread",
                    Ico::SquareTerminal,
                    "Make a thread",
                    false,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.model.toggle_card_menu();
                    this.open_t3_start(window, cx);
                    let task = task.clone();
                    this.t3_ui
                        .task_in
                        .update(cx, |s, cx| s.set_value(task, window, cx));
                    cx.notify();
                })),
            );
        }
        menu
    }

    /// Follow with a bell, or Following with a tick, for a board or one card on it in the chat
    /// shown: a watch whose lines come here. None until the person's watches are known.
    fn follow_button(
        &self,
        id: &'static str,
        board: &str,
        card: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let following = self.model.follows(board, card)?;
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
                .child(Icon::new(if following { Ico::Check } else { Ico::Bell }).xsmall())
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
    /// priority, its assignee and its size, each its own look.
    fn card_chips(
        &self,
        c: &panels::CardPanel,
        d: &panels::CardDetail,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let ink = card_ink(p, &d.card);
        let status = div()
            .id("acta-status")
            .role(Role::Button)
            .aria_label(format!("Status: {}", d.card.status_words()))
            .cursor_pointer()
            .child(
                prop_chip(p)
                    .child(div().flex_none().size(px(7.0)).rounded_full().bg(ink))
                    .child(d.card.status_words())
                    .child(
                        div()
                            .text_color(p.ink_3)
                            .child(Icon::new(Ico::ChevronDown).xsmall()),
                    ),
            )
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
        let bars = panels::priority_bars(&d.priority);
        if bars > 0 {
            chips = chips.child(
                prop_chip(p)
                    .child(priority_icon(p, bars))
                    .child(panels::priority_name(&d.priority)),
            );
        }
        if !d.card.assignee.is_empty() {
            chips = chips.child(
                prop_chip(p)
                    .child(avatar(p, &panels::initial(&d.card.assignee), 14.0))
                    .child(d.card.assignee.clone()),
            );
        }
        if !d.card.size.is_empty() {
            chips = chips.child(prop_chip(p).child(d.card.size.to_uppercase()));
        }
        chips
    }

    /// Details, Comments · N and Activity, the one shown underlined.
    fn card_tabs(
        &self,
        c: &panels::CardPanel,
        d: &panels::CardDetail,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let mut row = div()
            .flex()
            .gap(px(space::L))
            .px(px(space::L))
            .border_b_1()
            .border_color(p.line);
        for (tab, label, id) in [
            (CardTab::Details, "Details".to_string(), "acta-tab-details"),
            (CardTab::Comments, d.comments_tab(), "acta-tab-comments"),
            (
                CardTab::Activity,
                "Activity".to_string(),
                "acta-tab-activity",
            ),
        ] {
            let on = c.tab == tab;
            row = row.child(
                div()
                    .id(id)
                    .role(Role::Tab)
                    .aria_label(label.clone())
                    .py(px(8.0))
                    .border_b_2()
                    .border_color(if on { p.ink } else { transparent_black() })
                    .text_size(px(size::SMALL))
                    .text_color(if on { p.ink } else { p.ink_3 })
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.card_tab(tab);
                        cx.notify();
                    })),
            );
        }
        row
    }

    /// Details: its description, its labels, its subtasks and its pull request.
    fn card_details(
        &self,
        c: &panels::CardPanel,
        d: &panels::CardDetail,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let mut page = div().flex().flex_col().gap(px(space::M));
        if !d.description.trim().is_empty() {
            let key = SharedString::from(format!("acta-desc-{}-{}", c.board, c.id));
            page = page.child(div().text_size(px(size::BODY)).text_color(p.ink).child(
                KeptText::new(key, d.description.clone(), |v| v.selectable(true)),
            ));
        }
        let labels: Vec<&String> = d.card.labels.iter().filter(|l| !l.is_empty()).collect();
        if !labels.is_empty() {
            let mut row = div().flex().flex_wrap().gap(px(6.0));
            for l in labels {
                row = row.child(
                    div()
                        .h(px(20.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .rounded(px(5.0))
                        .border_1()
                        .border_color(p.line)
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_2)
                        .whitespace_nowrap()
                        .child(l.clone()),
                );
            }
            page = page.child(row);
        }
        if !d.checklist.is_empty() {
            page = page.child(self.card_subtasks(d, cx));
        }
        if let Some(pr) = &d.pull_request {
            let (words, tone) = pr.checks_words();
            let url = pr.url.clone();
            page = page.child(
                div()
                    .id("acta-pr")
                    .role(Role::Link)
                    .aria_label(format!("Pull request #{}", pr.number))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .mx(px(-space::XS))
                    .px(px(space::XS))
                    .py(px(4.0))
                    .rounded(px(radius::ROW))
                    .text_size(px(size::SMALL))
                    .when(!url.is_empty(), |d| {
                        d.cursor_pointer()
                            .soft_hover(transparent_black(), p.hover, None)
                            .on_click(move |_, _, cx| cx.open_url(&url))
                    })
                    .child(
                        div()
                            .flex_none()
                            .font_family(font::MONO)
                            .text_color(p.ink_2)
                            .child(format!("#{}", pr.number)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(p.ink)
                            .child(pr.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(size::CAPTION))
                            .text_color(match tone {
                                panels::Tone::Ok => p.ok,
                                panels::Tone::Bad => p.bad,
                                panels::Tone::Quiet => p.ink_3,
                            })
                            .child(words),
                    ),
            );
        }
        page
    }

    /// Its subtasks, "Subtasks · 1 of 3", each with a tick that toggles it.
    fn card_subtasks(&self, d: &panels::CardDetail, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().child(
            div()
                .pb(px(space::XS))
                .text_size(px(size::CAPTION))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink_3)
                .child(d.subtasks_line()),
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
                    .mx(px(-space::XS))
                    .px(px(space::XS))
                    .py(px(4.0))
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, None)
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex_none()
                            .size(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(if t.done { p.ink } else { p.line_strong })
                            .when(t.done, |d| {
                                d.bg(p.ink)
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
                            .text_color(p.ink)
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

    /// Its comments, oldest first: who's initial, who and the day, then what they said.
    fn card_comments(&self, c: &panels::CardPanel, d: &panels::CardDetail) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(space::M));
        for (n, m) in d.comments.iter().enumerate() {
            let day = panels::comment_day(&m.at);
            let key = SharedString::from(format!(
                "acta-comment-{}-{}-{n}-{}-{}",
                c.board, c.id, m.id, m.at
            ));
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(space::S))
                    .child(avatar(p, &m.initial(), 18.0))
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
                                    .gap(px(4.0))
                                    .text_size(px(size::CAPTION))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(p.ink)
                                            .child(m.by.clone()),
                                    )
                                    .when(!day.is_empty(), |d| {
                                        d.child(div().text_color(p.ink_3).child(format!("· {day}")))
                                    }),
                            )
                            .child(
                                div().text_size(px(size::SMALL)).text_color(p.ink).child(
                                    KeptText::new(key, m.body.clone(), |v| v.selectable(true)),
                                ),
                            ),
                    ),
            );
        }
        list
    }

    /// Its activity, newest first: who, what in Acta's words, and the day.
    fn card_activity(&self, d: &panels::CardDetail) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(6.0));
        for a in d.activity.iter().rev() {
            let day = panels::comment_day(&a.at);
            list = list.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .text_size(px(size::SMALL))
                    .line_height(px(size::SMALL * line::BODY))
                    .child(div().text_color(p.ink).child(a.by.clone()))
                    .child(div().text_color(p.ink_2).child(a.text.clone()))
                    .when(!day.is_empty(), |d| {
                        d.child(div().text_color(p.ink_3).child(format!("· {day}")))
                    }),
            );
        }
        list
    }

    /// The box to comment in, under the comments, with Comment.
    fn comment_box(&self, c: &panels::CardPanel, fields: &Fields, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::M))
            .border_t_1()
            .border_color(p.line)
            .child(Textarea::new(&fields.comment))
            .child(
                div().flex().justify_end().child(
                    self.button(
                        "acta-comment",
                        if c.commenting {
                            "Commenting…"
                        } else {
                            "Comment"
                        },
                        true,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        let fields = this.acta_fields(window, cx);
                        let body = fields.comment.read(cx).value().to_string();
                        if let Some(f) = this.model.comment_card(&body) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
                ),
            )
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
            .child(board_icon(p, 16.0))
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
                    // The board it's made on comes first next time, after a restart too.
                    this.prefs.card_boards = this.model.acta.used.clone();
                    this.prefs.save(&data_dir());
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
