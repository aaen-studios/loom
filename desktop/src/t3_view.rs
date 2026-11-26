//! T3 Code's threads in the chat (PLAN.md, "T3 threads"): each thread a message names or
//! started, as a card with its strand beside the title, and the panel beside the chat
//! that starts one. `model::t3` says what each shows and what to send; this draws it.

use super::*;
use model::t3::{Act, Quiet, ThreadCard, Tone};

thread_local! {
    /// Where each thread drawn so far opens in T3's app, by its id, for a link to one
    /// in a message's words (`open_card_link`).
    static PAGES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Where a link to a T3 thread opens: its page in T3's app, once its card has been drawn.
pub(super) fn page_of(url: &str) -> Option<String> {
    let id = model::t3::link_thread(url)?;
    PAGES.with(|p| p.borrow().get(id).cloned())
}

/// A tone's colour: the sky thread's while working, and the palette's warn, ok and bad.
fn tone(p: Palette, t: Tone) -> Hsla {
    match t {
        Tone::Sky => thread::color("sky", p.dark),
        Tone::Warn => p.warn,
        Tone::Ok => p.ok,
        Tone::Bad => p.bad,
    }
}

/// A message's T3 threads, one card under another, or none.
pub(super) fn thread_cards(
    p: Palette,
    m: &Message,
    scale: f32,
    this: &WeakEntity<Loom>,
) -> Option<Div> {
    if m.threads.is_empty() {
        return None;
    }
    let now = model::now_ms();
    let mut out = div()
        .w_full()
        .max_w(px(CARD_MOST))
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

/// A quiet box in a card: its latest step, or why it failed.
fn quiet_box(p: Palette, scale: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::M))
        .py(px(space::S))
        .rounded(px(radius::ROW))
        .border_1()
        .border_color(p.line)
        .bg(card_inset(p))
        .text_size(px(size::SMALL * scale))
}

/// One thread's card (option C): the strand beside its title in the colour of where it
/// is, the time or what it waits on at the title's end, and under the title
/// `T3 · project · model · effort`. Then its step, its question, its pull requests or
/// why it failed, and its buttons. The card opens the thread in T3's app.
fn thread_card(
    p: Palette,
    message: &str,
    c: &ThreadCard,
    now: i64,
    scale: f32,
    this: &WeakEntity<Loom>,
) -> Stateful<Div> {
    let (mood, t) = c.strand();
    let color = tone(p, t);
    let key = format!("t3-{message}-{}", c.thread);
    // Level with the title's first line.
    let strand = div()
        .flex_none()
        .h(px(size::BODY * scale * 1.35))
        .flex()
        .items_center()
        .child(thread::thread(
            SharedString::from(format!("{key}/strand")),
            mood,
            color,
            MARK_W,
            MARK_H,
            None,
        ));
    let mut frame = CardFrame::new()
        .lead(strand)
        .title(c.title.clone())
        .sub(c.sub());
    let side = c.side(now);
    if !side.is_empty() {
        frame = frame.side(
            div()
                .text_size(px(size::SMALL * scale))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(side),
        );
    }
    match c.quiet() {
        Some(Quiet::Step(step)) => {
            frame = frame.row(
                quiet_box(p, scale)
                    .child(
                        div()
                            .flex_none()
                            .font_family(crate::prefs::mono_font())
                            .text_color(p.ink_3)
                            .child("›_"),
                    )
                    .child(div().min_w_0().truncate().text_color(p.ink_2).child(step)),
            );
        }
        Some(Quiet::Reason(why)) => {
            frame =
                frame.row(quiet_box(p, scale).child(div().min_w_0().text_color(p.bad).child(why)));
        }
        None => {}
    }
    if let Some(q) = c.asks() {
        frame = frame.row(
            div()
                .border_l_2()
                .border_color(p.warn)
                .pl(px(space::M))
                .text_size(px(size::BODY * scale))
                .line_height(relative(1.45))
                .text_color(p.ink)
                .child(q.to_string()),
        );
    }
    let chips = c.chips();
    if !chips.is_empty() {
        let mut row = div().flex().flex_wrap().gap(px(space::S));
        for (i, (label, url)) in chips.into_iter().enumerate() {
            row = row.child(
                div()
                    .id(SharedString::from(format!("{key}/chip-{i}")))
                    .role(Role::Link)
                    .aria_label(label.clone())
                    .px(px(space::S))
                    .py(px(2.0))
                    .rounded(px(radius::ROW))
                    .border_1()
                    .border_color(p.line)
                    .bg(card_inset(p))
                    .font_family(crate::prefs::mono_font())
                    .text_size(px(size::CAPTION * scale))
                    .text_color(p.ink_2)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(p.ink).border_color(p.line_strong))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    })
                    .child(label),
            );
        }
        frame = frame.row(row);
    }
    if c.in_t3() {
        frame = frame.action(
            div()
                .text_size(px(size::SMALL * scale))
                .text_color(p.ink_2)
                .child("Approve in T3"),
        );
    }
    for (i, b) in c.buttons().into_iter().enumerate() {
        let this = this.clone();
        let card = c.clone();
        let button = card_button(p, format!("{key}/b{i}"), &b.label, b.main);
        frame = frame.action(button.on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = this.update(cx, |loom, cx| loom.t3_press(&card, &b.act, &b.label, cx));
        }));
    }
    frame.edge = c.edge().map(|(t, a)| tone(p, t).opacity(a));
    let url = c.url.clone();
    frame
        .draw(p, scale)
        .id(SharedString::from(key))
        .role(Role::Group)
        .aria_label(c.title.clone())
        .when(!url.is_empty(), |d| {
            d.cursor_pointer()
                .on_click(move |_, _, cx| cx.open_url(&url))
        })
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
