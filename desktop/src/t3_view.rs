//! T3 Code's threads in the chat (PLAN.md, "T3 threads"): each thread a message names or
//! started, as a card (LOO-435, option C) with a band across its top saying where it is,
//! and the panel beside the chat that starts one. `model::t3` says what each shows and
//! what to send; this draws it, in black and white as Spools and the Acta panel are.

use super::*;
use model::t3::{Act, Body, ThreadCard, ThreadState, Tool};

/// T3 Code's mark, as Spools draws it (`server/catalog/t3.svg`).
const T3_MARK: &str = include_str!("../../server/catalog/t3.svg");
/// The size of the mark's tile at a card's head.
const MARK_TILE: f32 = 22.0;

/// What the cards show of their own, by card (`t3-{message}-{thread}`): the one whose
/// Send box is open, the one whose ⋯ menu is open, those whose steps are shown, and the
/// Send box's field, which `T3Ui` makes.
#[derive(Default)]
struct CardUi {
    sending: Option<String>,
    menu: Option<String>,
    steps: HashSet<String>,
    send_in: Option<Entity<InputState>>,
}

thread_local! {
    /// Where each thread drawn so far opens in T3's app, by its id, for a link to one
    /// in a message's words (`open_card_link`).
    static PAGES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static UI: RefCell<CardUi> = RefCell::new(CardUi::default());
}

fn ui<R>(f: impl FnOnce(&mut CardUi) -> R) -> R {
    UI.with(|u| f(&mut u.borrow_mut()))
}

/// Where a link to a T3 thread opens: its page in T3's app, once its card has been drawn.
pub(super) fn page_of(url: &str) -> Option<String> {
    let id = model::t3::link_thread(url)?;
    PAGES.with(|p| p.borrow().get(id).cloned())
}

/// A message's T3 threads, one card under another, or none.
pub(super) fn thread_cards(
    p: Palette,
    m: &Message,
    scale: f32,
    wide: bool,
    this: &WeakEntity<Loom>,
) -> Option<Div> {
    if m.threads.is_empty() {
        return None;
    }
    let now = model::now_ms();
    let mut out = div()
        .w_full()
        .when(!wide, |d| d.max_w(px(CARD_MOST)))
        .flex()
        .flex_col()
        .gap(px(space::S));
    for c in m.threads.iter() {
        if !c.url.is_empty() {
            PAGES.with(|pages| {
                pages.borrow_mut().insert(c.thread.clone(), c.url.clone());
            });
        }
        out = out.child(thread_card(p, &m.id, c, now, scale, this));
    }
    Some(out)
}

/// T3's mark on a tile in ink, its logo in the card's fill.
fn t3_mark(p: Palette) -> Div {
    div()
        .size(px(MARK_TILE))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::ROW))
        .bg(p.button)
        .child(
            svg()
                .path(crate::runtime_svg("spools/t3.svg", T3_MARK))
                .size(px((MARK_TILE * 0.6).round()))
                .text_color(p.button_ink),
        )
}

/// A small square button at a card's foot with an icon, named for a screen reader.
fn tool_button(p: Palette, id: String, ico: Ico, name: &str, on: bool) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .role(Role::Button)
        .aria_label(name.to_string())
        .size(px(control::BUTTON_SMALL))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(radius::BUTTON))
        .border_1()
        .border_color(if on { p.ink } else { p.line_strong })
        .text_color(p.ink)
        .cursor_pointer()
        .soft_hover(transparent_black(), p.hover, None)
        .child(Icon::new(ico).small())
}

/// The band across a card's top: where it is in words, beside an icon of it, filled with
/// ink while it waits on the person; its run and steps, which show its last steps when
/// pressed; and its time.
fn band_row(
    p: Palette,
    key: &str,
    c: &ThreadCard,
    now: i64,
    scale: f32,
    this: &WeakEntity<Loom>,
) -> Div {
    let band = c.band(now);
    let shown = ui(|u| u.steps.contains(key));
    let (bg, ink, quiet) = if band.ink {
        (p.ink, p.bg, p.bg.opacity(0.75))
    } else {
        (transparent_black(), p.ink, p.ink_2)
    };
    let icon = match c.state {
        ThreadState::Working => Ico::Loader,
        ThreadState::Waiting => Ico::Hand,
        ThreadState::Done => Ico::Check,
        ThreadState::Failed => Ico::X,
    };
    let can = !c.steps.is_empty();
    let toggle = key.to_string();
    let this = this.clone();
    let run = div()
        .id(SharedString::from(format!("{key}/run")))
        .when(can, |d| {
            d.cursor_pointer()
                .role(Role::Button)
                .aria_label(if shown { "Hide steps" } else { "Show steps" })
                .hover(move |s| s.text_color(ink))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    ui(|u| {
                        if !u.steps.remove(&toggle) {
                            u.steps.insert(toggle.clone());
                        }
                    });
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
        })
        .child(band.run.clone());
    div()
        .flex()
        .items_center()
        .gap(px(space::S + 2.0))
        .h(px(control::BUTTON_SMALL))
        .px(px(space::L))
        .bg(bg)
        // The card's clip is square, so the band rounds its own top inside the
        // card's border.
        .rounded_t(px(radius::BUBBLE - 1.0))
        .border_b_1()
        .border_color(if band.ink { p.ink } else { p.line })
        .text_size(px(size::SMALL * scale))
        .text_color(quiet)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::XS + 2.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(ink)
                .child(Icon::new(icon).xsmall())
                .child(band.state),
        )
        .when(!band.run.is_empty(), |d| d.child(run))
        .child(div().ml_auto().child(band.time))
}

/// What a card says under its head, as where it is has it.
fn body_view(p: Palette, body: Body, scale: f32) -> Div {
    match body {
        Body::Doing { words, raw } => div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(size::BODY * scale))
                    .text_color(p.ink)
                    .child(words),
            )
            .when(!raw.is_empty(), |d| {
                d.child(
                    div()
                        .truncate()
                        .font_family(crate::prefs::mono_font())
                        .text_size(px(size::CAPTION * scale))
                        .text_color(p.ink_3)
                        .child(raw),
                )
            }),
        Body::Asks(q) => div()
            .border_l_2()
            .border_color(p.ink)
            .pl(px(space::M))
            .text_size(px(size::BODY * scale))
            .line_height(relative(1.45))
            .text_color(p.ink)
            .child(q),
        Body::Summary(words) => div()
            .line_clamp(2)
            .text_size(px(size::BODY * scale))
            .line_height(relative(1.45))
            .text_color(p.ink_2)
            .child(words),
        Body::Failed(why) => div()
            .text_size(px(size::BODY * scale))
            .text_color(p.ink)
            .child(why),
    }
}

/// One thread's card: its band; T3's mark and the title; where it runs, its project,
/// branch, model and effort; what it's doing in plain words with the step as it came,
/// what it asks, its summary or why it failed; its last steps while the band's count is
/// pressed; its pull requests with how their checks stand; the buttons that answer it;
/// and Open in T3, Send, Stop and ⋯ (Copy link). The card opens the thread in T3's app.
fn thread_card(
    p: Palette,
    message: &str,
    c: &ThreadCard,
    now: i64,
    scale: f32,
    this: &WeakEntity<Loom>,
) -> Stateful<Div> {
    let key = format!("t3-{message}-{}", c.thread);
    let (sending, menu, show_steps) = ui(|u| {
        (
            u.sending.as_deref() == Some(key.as_str()),
            u.menu.as_deref() == Some(key.as_str()),
            u.steps.contains(&key),
        )
    });
    let waits = c.state == ThreadState::Waiting;
    let mut inner = div()
        .flex()
        .flex_col()
        .gap(px(space::M))
        .p(px(space::L))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::S + 2.0))
                .child(t3_mark(p))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(size::BODY * scale))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.ink)
                        .child(c.title.clone()),
                ),
        );
    let meta = c.meta();
    if !meta.is_empty() {
        inner = inner.child(
            div()
                .truncate()
                .text_size(px(size::CAPTION * scale))
                .text_color(p.ink_3)
                .child(meta.join("  ·  ")),
        );
    }
    if let Some(body) = c.body() {
        inner = inner.child(body_view(p, body, scale));
    }
    if show_steps && !c.steps.is_empty() {
        let mut list = div().flex().flex_col().gap(px(2.0));
        for s in &c.steps {
            list = list.child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .text_size(px(size::SMALL * scale))
                    .text_color(p.ink_2)
                    .child(div().text_color(p.ink_3).child("·"))
                    .child(div().min_w_0().truncate().child(s.clone())),
            );
        }
        inner = inner.child(list);
    }
    for (i, pr) in c.prs.iter().enumerate() {
        let url = pr.url.clone();
        let label = if pr.title.is_empty() {
            format!("Pull request #{}", pr.number)
        } else {
            pr.title.clone()
        };
        inner = inner.child(
            div()
                .id(SharedString::from(format!("{key}/pr-{i}")))
                .role(Role::Link)
                .aria_label(format!("#{} {label}", pr.number))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::M))
                .h(px(control::BUTTON_SMALL + 4.0))
                .rounded(px(radius::ROW))
                .border_1()
                .border_color(p.line)
                .bg(card_inset(p))
                .text_size(px(size::SMALL * scale))
                .cursor_pointer()
                .hover(move |s| s.border_color(p.line_strong))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&url);
                })
                .child(
                    div()
                        .flex_none()
                        .text_color(p.ink_3)
                        .child(format!("#{}", pr.number)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(p.ink)
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(size::CAPTION * scale))
                        .text_color(p.ink_2)
                        .child(pr.standing()),
                ),
        );
    }
    let buttons = c.buttons();
    if !buttons.is_empty() || c.in_t3() {
        let mut row = div().flex().flex_wrap().items_center().gap(px(space::S));
        for (i, b) in buttons.into_iter().enumerate() {
            let this = this.clone();
            let card = c.clone();
            row = row.child(
                card_button(p, format!("{key}/b{i}"), &b.label, b.main).on_click(
                    move |_, _, cx| {
                        cx.stop_propagation();
                        let _ =
                            this.update(cx, |loom, cx| loom.t3_press(&card, &b.act, &b.label, cx));
                    },
                ),
            );
        }
        if c.in_t3() {
            row = row.child(
                div()
                    .text_size(px(size::SMALL * scale))
                    .text_color(p.ink_2)
                    .child("Approve in T3"),
            );
        }
        inner = inner.child(row);
    }
    inner = inner.child(foot(p, &key, c, sending, menu, this));
    if sending {
        if let Some(row) = send_box(p, &key, c, this) {
            inner = inner.child(row);
        }
    }
    if menu {
        inner = inner.child(more_menu(p, &key, c, this));
    }

    let url = c.url.clone();
    div()
        .id(SharedString::from(key.clone()))
        .role(Role::Group)
        .aria_label(format!("{}, {}", c.title, c.band(now).state))
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(radius::BUBBLE))
        .border_1()
        .border_color(if waits {
            p.ink
        } else if p.dark {
            p.line
        } else {
            p.line_strong
        })
        .bg(card_fill(p))
        .shadow(card_shadow(p))
        .child(band_row(p, &key, c, now, scale, this))
        .child(inner)
        .when(!url.is_empty(), |d| {
            d.cursor_pointer()
                .on_click(move |_, _, cx| cx.open_url(&url))
        })
}

/// A card's foot: Open in T3, Send, Stop, and ⋯ at its end.
fn foot(
    p: Palette,
    key: &str,
    c: &ThreadCard,
    sending: bool,
    menu: bool,
    this: &WeakEntity<Loom>,
) -> Div {
    let mut row = div().flex().items_center().gap(px(space::S));
    for t in c.tools() {
        let (ico, name, id) = match t {
            Tool::Open => (Ico::ExternalLink, "Open in T3", "open"),
            Tool::Send => (Ico::Pencil, "Send a message", "send"),
            Tool::Stop => (Ico::Square, "Stop", "stop"),
        };
        let this = this.clone();
        let card = c.clone();
        let k = key.to_string();
        row = row.child(
            tool_button(
                p,
                format!("{key}/{id}"),
                ico,
                name,
                t == Tool::Send && sending,
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match t {
                    Tool::Open => cx.open_url(&card.url),
                    Tool::Send => {
                        let open = ui(|u| {
                            u.menu = None;
                            if u.sending.as_deref() == Some(k.as_str()) {
                                u.sending = None;
                                false
                            } else {
                                u.sending = Some(k.clone());
                                true
                            }
                        });
                        if open {
                            if let Some(input) = ui(|u| u.send_in.clone()) {
                                input.update(cx, |s, cx| {
                                    s.set_value("", window, cx);
                                    s.focus(window, cx);
                                });
                            }
                        }
                        let _ = this.update(cx, |_, cx| cx.notify());
                    }
                    Tool::Stop => {
                        let _ = this
                            .update(cx, |loom, cx| loom.t3_press(&card, &Act::Stop, "Stop", cx));
                    }
                }
            }),
        );
    }
    let this = this.clone();
    let k = key.to_string();
    row.child(div().ml_auto().child(
        tool_button(p, format!("{key}/more"), Ico::Ellipsis, "More", menu).on_click(
            move |_, _, cx| {
                cx.stop_propagation();
                ui(|u| {
                    u.sending = None;
                    u.menu = if u.menu.as_deref() == Some(k.as_str()) {
                        None
                    } else {
                        Some(k.clone())
                    };
                });
                let _ = this.update(cx, |_, cx| cx.notify());
            },
        ),
    ))
}

/// The Send box under a card's foot while it's open: what to send the thread, and Send.
fn send_box(p: Palette, key: &str, c: &ThreadCard, this: &WeakEntity<Loom>) -> Option<Div> {
    let input = ui(|u| u.send_in.clone())?;
    let this = this.clone();
    let card = c.clone();
    let field = input.clone();
    Some(
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(div().flex_1().min_w_0().child(Input::new(&input)))
            .child(
                card_button(p, format!("{key}/send-now"), "Send", true).on_click(
                    move |_, window, cx| {
                        cx.stop_propagation();
                        let text = field.read(cx).value().to_string();
                        if text.trim().is_empty() {
                            return;
                        }
                        field.update(cx, |s, cx| s.set_value("", window, cx));
                        ui(|u| u.sending = None);
                        let _ = this.update(cx, |loom, cx| {
                            loom.t3_press(&card, &Act::Send(text), "Send", cx)
                        });
                    },
                ),
            ),
    )
}

/// The ⋯ menu under a card's foot while it's open: Copy link.
fn more_menu(p: Palette, key: &str, c: &ThreadCard, this: &WeakEntity<Loom>) -> Div {
    let url = c.url.clone();
    let this = this.clone();
    div()
        .flex()
        .flex_col()
        .p(px(space::XS))
        .rounded(px(radius::INPUT))
        .border_1()
        .border_color(p.line)
        .bg(p.raised)
        .child(
            div()
                .id(SharedString::from(format!("{key}/copy")))
                .role(Role::MenuItem)
                .aria_label("Copy link")
                .h(px(control::BUTTON_SMALL))
                .px(px(space::S))
                .flex()
                .items_center()
                .gap(px(space::S))
                .rounded(px(radius::ROW))
                .text_size(px(size::SMALL))
                .text_color(if url.is_empty() { p.ink_3 } else { p.ink })
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .child(Icon::new(Ico::Copy).small())
                .child("Copy link")
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if !url.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                    }
                    ui(|u| u.menu = None);
                    let _ = this.update(cx, |_, cx| cx.notify());
                }),
        )
}

/// The start form's lists, each open in place under its field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Pick {
    Server,
    Project,
    Model,
}

/// The start form's own state in the app: the task as it's typed, and the list open.
pub(super) struct T3Ui {
    pub task_in: Entity<TextareaState>,
    pub open: Option<Pick>,
}

impl T3Ui {
    pub fn new(window: &mut Window, cx: &mut Context<Loom>) -> T3Ui {
        let task_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 14)
                .placeholder("What the thread is to do")
        });
        // Start turns on as the task is typed.
        cx.observe(&task_in, |_: &mut Loom, _, cx| cx.notify())
            .detach();
        // The field a card's Send box shows, one for every card, since one is open at once.
        let send_in = cx.new(|cx| InputState::new(window, cx).placeholder("Message the thread"));
        ui(|u| u.send_in = Some(send_in));
        T3Ui {
            task_in,
            open: None,
        }
    }
}

impl Loom {
    /// Opens the start form beside the chat, and asks for what it offers.
    pub(super) fn open_t3_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ctx = None;
        let f = self.model.open_t3_form(None);
        self.send(f);
        self.t3_ui.open = None;
        self.t3_ui
            .task_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.show_side(Screen::T3Start, cx);
        self.t3_ui.task_in.update(cx, |s, cx| s.focus(window, cx));
    }

    /// A card's button: Open opens the thread in T3's app; the rest answer it or ask it
    /// to try again.
    pub(super) fn t3_press(
        &mut self,
        card: &ThreadCard,
        act: &Act,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        if *act == Act::Open {
            if !card.url.is_empty() {
                cx.open_url(&card.url);
            }
            return;
        }
        if let Some(f) = self.model.t3_act(card, act, label) {
            self.send(f);
        }
        cx.notify();
    }

    fn t3_pick(&mut self, pick: Pick, value: (String, String), cx: &mut Context<Self>) {
        self.t3_ui.open = None;
        let Some(form) = self.model.t3.form.as_mut() else {
            return;
        };
        match pick {
            Pick::Server => {
                if form.server != value.1 {
                    form.server = value.1.clone();
                    let f = self.model.open_t3_form(Some(&value.1));
                    self.send(f);
                }
            }
            Pick::Project => form.project = value.1,
            Pick::Model => form.pick_model(&value.0, &value.1),
        }
        cx.notify();
    }

    fn t3_start_now(&mut self, cx: &mut Context<Self>) {
        let task = self.t3_ui.task_in.read(cx).value().to_string();
        if let Some(f) = self.model.t3_start(&task) {
            self.send(f);
        }
        cx.notify();
    }

    /// A field that opens its list under it: what's picked, and a chevron.
    fn t3_select(&self, pick: Pick, shown: String, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let open = self.t3_ui.open == Some(pick);
        div()
            .id(SharedString::from(format!("t3-pick-{pick:?}")))
            .role(Role::ComboBox)
            .aria_label(shown.clone())
            .h(px(control::BUTTON))
            .px(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::S))
            .rounded(px(radius::INPUT))
            .border_1()
            .border_color(if open { p.line_strong } else { p.line })
            .bg(p.bg)
            .text_size(px(size::BODY))
            .cursor_pointer()
            .hover(move |s| s.border_color(p.line_strong))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.ink)
                    .child(shown),
            )
            .child(
                div().text_color(p.ink_3).child(
                    Icon::new(if open {
                        Ico::ChevronUp
                    } else {
                        Ico::ChevronDown
                    })
                    .small(),
                ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.t3_ui.open = if this.t3_ui.open == Some(pick) {
                    None
                } else {
                    Some(pick)
                };
                cx.notify();
            }))
    }

    /// A field's list, under it while it's open: each choice, the picked one ticked.
    fn t3_list(
        &self,
        pick: Pick,
        items: Vec<(String, String, String)>,
        picked: (String, String),
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let mut list = div()
            .flex()
            .flex_col()
            .p(px(space::XS))
            .rounded(px(radius::INPUT))
            .border_1()
            .border_color(p.line)
            .bg(p.raised);
        for (i, (group, id, label)) in items.into_iter().enumerate() {
            let on = group == picked.0 && id == picked.1;
            list = list.child(
                div()
                    .id(SharedString::from(format!("t3-{pick:?}-{i}")))
                    .role(Role::RadioButton)
                    .aria_label(label.clone())
                    .h(px(control::BUTTON_SMALL))
                    .px(px(space::S))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .rounded(px(radius::ROW))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink)
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, None)
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when(on, |d| {
                        d.child(Icon::new(Ico::Check).small().text_color(p.ink_2))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.t3_pick(pick, (group.clone(), id.clone()), cx);
                    })),
            );
        }
        list
    }

    /// The start form beside the chat: Server when the person has several, Project,
    /// Model, Effort when the model has efforts, Task, and Start.
    pub(super) fn t3_start_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        // Started: the form's done, and its card is in the chat.
        let Some(form) = self.model.t3.form.clone() else {
            self.side = None;
            cx.notify();
            return div().flex_1();
        };
        let mut page = div()
            .w_full()
            .max_w(px(layout::PAGE))
            .flex()
            .flex_col()
            .gap(px(space::L));
        let open = self.t3_ui.open;
        if form.shows_server() {
            let mut field = div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(self.t3_select(Pick::Server, form.server.clone(), cx));
            if open == Some(Pick::Server) {
                let items = form
                    .servers
                    .iter()
                    .map(|s| (String::new(), s.clone(), s.clone()))
                    .collect();
                field = field.child(self.t3_list(
                    Pick::Server,
                    items,
                    (String::new(), form.server.clone()),
                    cx,
                ));
            }
            page = page.child(self.field("Server", field));
        }
        let project = if form.loading && form.projects.is_empty() {
            "Loading…".to_string()
        } else {
            form.project_label()
        };
        let mut field = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(self.t3_select(Pick::Project, project, cx));
        if open == Some(Pick::Project) && !form.projects.is_empty() {
            let items = form
                .projects
                .iter()
                .map(|(id, title)| (String::new(), id.clone(), title.clone()))
                .collect();
            field = field.child(self.t3_list(
                Pick::Project,
                items,
                (String::new(), form.project.clone()),
                cx,
            ));
        }
        page = page.child(self.field("Project", field));
        let mut field = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(self.t3_select(Pick::Model, form.model_label(), cx));
        if open == Some(Pick::Model) {
            let picked = (form.provider.clone(), form.model.clone());
            field = field.child(self.t3_list(Pick::Model, form.model_list(), picked, cx));
        }
        page = page.child(self.field("Model", field));
        let efforts = form.efforts();
        if !efforts.is_empty() {
            let items = efforts
                .into_iter()
                .map(|(id, label)| {
                    let on = id == form.effort;
                    self.segment(format!("t3-effort-{id}"), &label, on)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(f) = this.model.t3.form.as_mut() {
                                f.effort = id.clone();
                            }
                            cx.notify();
                        }))
                })
                .collect();
            page = page.child(self.field("Effort", div().flex().child(self.segments(items))));
        }
        page = page.child(self.field("Task", Textarea::new(&self.t3_ui.task_in)));
        let task = self.t3_ui.task_in.read(cx).value().to_string();
        let can = form.can_start(&task);
        let start = self
            .button(
                "t3-start",
                if form.starting {
                    "Starting…"
                } else {
                    "Start"
                },
                true,
            )
            .when(!can, |d| d.opacity(0.5))
            .on_click(cx.listener(|this, _, _, cx| this.t3_start_now(cx)));
        page = page.child(div().flex().child(start));
        if !form.error.is_empty() {
            page = page.child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.bad)
                    .child(form.error.clone()),
            );
        }
        div().flex_1().min_h_0().child(
            div()
                .id("t3-start-panel")
                .size_full()
                .overflow_y_scroll()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(space::L))
                .px(px(space::XL))
                .pb(px(space::XXL))
                .child(page),
        )
    }
}
