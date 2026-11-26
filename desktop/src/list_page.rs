//! The settings popup's size and shape on screen (PLAN.md, "The settings popup"), as
//! `model::popup` works them out: the corner that resizes it, and list pages, each a
//! list down the left and the one picked from it beside it.

use super::*;
use crate::model::popup::{self, Kind, Pane, Shows};

/// How the popup's open page is laid out, as it's drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Shape {
    pub kind: Kind,
    /// How wide a list page's list is beside the one picked, or none where they show
    /// one at a time.
    pub list: Option<f32>,
    /// How wide what's beside the list is, inside its padding.
    pub pane: f32,
}

/// The popup's own state: the open page's shape, whether a grid page is being drawn,
/// its corner while it's dragged (where the pointer and the size were as it went down),
/// and a list page's find field, with the page it's for. The item whose form is filled
/// in is the model's (`model::drafts`).
pub(super) struct ListUi {
    pub shape: Option<Shape>,
    pub wide: bool,
    pub drag: Option<((f32, f32), (f32, f32))>,
    pub find_in: Entity<InputState>,
    pub find_for: Option<Screen>,
}

impl ListUi {
    pub fn new(window: &mut Window, cx: &mut Context<Loom>) -> ListUi {
        let find_in = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe_in(
            &find_in,
            window,
            |_: &mut Loom, _, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            },
        )
        .detach();
        ListUi {
            shape: None,
            wide: false,
            drag: None,
            find_in,
            find_for: None,
        }
    }
}

/// A list page as its page draws it: its title and action, what it shows (`Shows`),
/// the items the find field found, by their place in the page's list, how to draw
/// each, a row over the list and one under it, such as Routines' week and Brains' ACP
/// Registry, and what's beside the list: its key, so it opens at its top, and its head
/// and groups.
pub(super) struct ListPage {
    pub id: &'static str,
    pub title: &'static str,
    pub action: Option<AnyElement>,
    pub shows: Shows,
    pub found: Vec<usize>,
    pub row: fn(&mut Loom, usize, bool, &mut Context<Loom>) -> AnyElement,
    pub head: Option<AnyElement>,
    pub foot: Option<AnyElement>,
    pub pane_key: String,
    pub pane: Vec<AnyElement>,
}

/// What the find field over a list page's list says, by its page.
fn find_words(screen: Screen) -> &'static str {
    match screen {
        Screen::Mcp => "Find a server",
        Screen::Programs => "Find a brain",
        Screen::Skills => "Find a skill",
        Screen::Proposals => "Find a proposal",
        Screen::Routines => "Find a routine",
        Screen::Installed => "Find a server",
        _ => "Find",
    }
}

impl Loom {
    /// How the popup's page `open` is laid out in a popup `w` wide.
    pub(super) fn popup_shape(&self, open: Screen, w: f32) -> Shape {
        let kind = settings::page_kind(open);
        // The page's room: the popup less its nav and border, less the page's padding
        // and the box its list and pane sit in.
        let page = w - layout::NAV - 2.0 - 2.0 * space::XL;
        let list = (kind == Kind::List)
            .then(|| popup::list_width(page - 2.0))
            .flatten();
        let pane = match list {
            Some(l) => page - 2.0 - l - 1.0 - 2.0 * space::XL,
            None => page - 2.0 - 2.0 * space::XL,
        };
        Shape { kind, list, pane }
    }

    /// Whether the open list page has its list and the one picked side by side.
    pub(super) fn list_split(&self) -> bool {
        self.list_ui.shape.is_some_and(|s| s.list.is_some())
    }

    /// What the find field over the open list page holds.
    pub(super) fn list_query(&self, cx: &App) -> String {
        self.list_ui.find_in.read(cx).value().trim().to_string()
    }

    /// Readies the open list page before it's drawn: an empty find field asking for
    /// what the page lists, each time a page opens, what its form holds kept as its
    /// item's draft while it's changed, and the form of what shows beside the list
    /// without being picked (the first, side by side) filled in as a click fills it.
    pub(super) fn ready_list_page(
        &mut self,
        open: Screen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.list_ui.find_for != Some(open) {
            self.list_ui.find_for = Some(open);
            self.list_ui.find_in.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.set_placeholder(find_words(open), window, cx);
            });
        }
        self.keep_draft(cx);
        let split = self.list_split();
        let filled = |this: &Self, key: &str| {
            form_drafts::draft_page(open).is_some_and(|p| this.model.drafts.is_open(p, Some(key)))
        };
        match open {
            Screen::Mcp => {
                let servers = self.model.mcp_servers.clone();
                let keys: Vec<&str> = servers.iter().map(|m| m.name.as_str()).collect();
                // One saved under a new name: the page follows it there.
                for (was, now) in self.model.drafts.moved_in(DraftPage::Mcp, &keys) {
                    if self.mcp_open.as_ref() == Some(&was) {
                        self.mcp_open = Some(Some(now));
                    }
                }
                let open_at = self.mcp_open.as_ref().map(|o| o.as_deref());
                if let Pane::Item(i) = popup::shows(split, open_at, &keys, false).pane {
                    if !filled(self, &servers[i].name) {
                        self.fill_mcp(&servers[i], window, cx);
                    }
                }
            }
            Screen::Skills => {
                let skills = self.model.skills.clone();
                let keys: Vec<&str> = skills.iter().map(|k| k.name.as_str()).collect();
                for (was, now) in self.model.drafts.moved_in(DraftPage::Skills, &keys) {
                    if self.skill_open.as_ref() == Some(&was) {
                        self.skill_open = Some(Some(now));
                    }
                }
                let open_at = self.skill_open.as_ref().map(|o| o.as_deref());
                if let Pane::Item(i) = popup::shows(split, open_at, &keys, false).pane {
                    if !filled(self, &skills[i].name) {
                        self.fill_skill(&skills[i], window, cx);
                    }
                }
            }
            Screen::Proposals => {
                let proposals = self.model.proposals.clone();
                let keys: Vec<&str> = proposals.iter().map(|k| k.id.as_str()).collect();
                let open_at = self.proposal_open.as_ref().map(|k| Some(k.id.as_str()));
                if let Pane::Item(i) = popup::shows(split, open_at, &keys, false).pane {
                    if !filled(self, &proposals[i].id) {
                        self.fill_proposal(&proposals[i], window, cx);
                    }
                }
            }
            Screen::Programs => {
                let keys: Vec<String> = self
                    .model
                    .installed
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let names: Vec<&str> = keys.iter().map(String::as_str).collect();
                let open_at = self.program_open.as_ref().map(|o| o.as_deref());
                if let Pane::Item(i) = popup::shows(split, open_at, &names, false).pane {
                    if !filled(self, &keys[i]) {
                        self.fill_brain(&keys[i], window, cx);
                    }
                }
            }
            _ => {}
        }
    }

    /// Back to the open list page's list, from what was picked, where they show one at
    /// a time. What its form holds stays as its draft.
    fn list_back(&mut self, cx: &mut Context<Self>) {
        self.leave_form(cx);
        match self.popup {
            Some(Screen::Mcp) => self.mcp_open = None,
            Some(Screen::Skills) => self.skill_open = None,
            Some(Screen::Proposals) => self.proposal_open = None,
            Some(Screen::Programs) => self.program_open = None,
            Some(Screen::Routines) => self.routine_open = None,
            Some(Screen::Installed) => self.model.spools.picked = None,
            _ => {}
        }
        cx.notify();
    }

    /// A list page: its title, the find field with the page's action beside it, then a
    /// box with the list down its left and the one picked beside it; or, without room
    /// for both, the list until one is picked, and then the one picked with the way back.
    pub(super) fn list_detail(&mut self, page: ListPage, cx: &mut Context<Self>) -> Div {
        let title = div()
            .min_h(px(control::BUTTON))
            .flex()
            .items_center()
            .text_size(px(size::HEADING - 2.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(self.p.ink)
            .child(page.title)
            .into_any_element();
        self.list_layout(title, page, cx)
    }

    /// A list page under a head of its own, such as Spools' title with its tabs.
    pub(super) fn list_layout(
        &mut self,
        title: AnyElement,
        page: ListPage,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let split = self.list_split();
        let list_w = self.list_ui.shape.and_then(|s| s.list);
        let pane_shown = !matches!(page.shows.pane, Pane::Nothing);
        let find = page.shows.list.then(|| {
            div()
                .min_h(px(control::BUTTON_SMALL))
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(
                    div().flex_1().min_w_0().text_size(px(size::SMALL)).child(
                        Input::new(&self.list_ui.find_in)
                            .prefix(Icon::new(IconName::Search).small().text_color(p.ink_3))
                            .cleanable(true),
                    ),
                )
                .children(page.action)
        });
        let mut boxed = div()
            .flex_1()
            .min_h_0()
            .flex()
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(p.line)
            .bg(p.bg)
            .overflow_hidden();
        if page.shows.list {
            boxed = boxed.child(self.list_pane(
                &page.found,
                page.shows,
                page.row,
                page.head,
                page.foot,
                list_w,
                cx,
            ));
        }
        if pane_shown && (split || !page.shows.list) {
            let mut column = div().flex().flex_col().gap(px(space::XL)).p(px(space::XL));
            if !split {
                let back = div()
                    .id("list-back")
                    .size(px(control::ICON))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::ROW))
                    .text_color(p.ink_2)
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
                    .child(Icon::new(Ico::ArrowLeft).small())
                    .on_click(cx.listener(|this, _, _, cx| this.list_back(cx)));
                column = column.child(div().mb(-px(space::M)).child(back));
            }
            boxed = boxed.child(
                div()
                    .id(SharedString::from(format!(
                        "{}-pane-{}",
                        page.id, page.pane_key
                    )))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .child(column.children(page.pane)),
            );
        }
        div().flex_1().min_h_0().child(
            div()
                .id(page.id)
                .size_full()
                .flex()
                .flex_col()
                .gap(px(space::M))
                .pt(px(space::XL))
                .px(px(space::XL))
                .pb(px(space::XL))
                .child(title)
                .children(find)
                .child(boxed),
        )
    }

    /// A list page's list: the row over it, the rows the find field found, drawn as
    /// they come on screen, and the row under it.
    #[allow(clippy::too_many_arguments)]
    fn list_pane(
        &mut self,
        found: &[usize],
        shows: Shows,
        row: fn(&mut Loom, usize, bool, &mut Context<Loom>) -> AnyElement,
        head: Option<AnyElement>,
        foot: Option<AnyElement>,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let picked = match shows.pane {
            Pane::Item(i) => Some(i),
            _ => None,
        };
        let found: Rc<Vec<usize>> = Rc::new(found.to_vec());
        let none = found.is_empty() && !self.list_query(cx).is_empty();
        let this = cx.entity().downgrade();
        let drawn = found.clone();
        let rows = uniform_list("list-rows", found.len(), move |range, _, cx| {
            range
                .filter_map(|i| {
                    let ix = drawn[i];
                    this.update(cx, |loom, cx| row(loom, ix, picked == Some(ix), cx))
                        .ok()
                })
                .collect::<Vec<_>>()
        })
        .flex_1()
        .p(px(space::XS));
        div()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .map(|d| match width {
                Some(w) => d.w(px(w)).flex_none().border_r_1().border_color(p.line),
                None => d.flex_1().min_w_0(),
            })
            .when_some(head, |d, h| {
                d.child(
                    div()
                        .px(px(space::XS))
                        .pt(px(space::XS))
                        .pb(px(space::XS))
                        .border_b_1()
                        .border_color(p.line)
                        .child(h),
                )
            })
            .when(none, |d| {
                d.child(
                    div()
                        .px(px(space::M))
                        .py(px(space::M))
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_3)
                        .child("Nothing found"),
                )
            })
            .child(rows)
            .when_some(foot, |d, f| {
                d.child(
                    div()
                        .p(px(space::XS))
                        .border_t_1()
                        .border_color(p.line)
                        .child(f),
                )
            })
    }

    /// A row of a list page's list: its icon, its name over a short line of its state,
    /// and a dot in its state's colour; lit while it's the one picked.
    pub(super) fn list_row(
        &self,
        id: impl Into<SharedString>,
        lead: AnyElement,
        name: impl Into<SharedString>,
        line: String,
        dot: Option<popup::Dot>,
        picked: bool,
    ) -> Stateful<Div> {
        self.list_row_unsaved(id, lead, name, line, dot, false, picked)
    }

    /// A list row, as `list_row` draws it, with a dot in the ink an unread reply's dot
    /// has before its state's dot while it has changes not saved.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn list_row_unsaved(
        &self,
        id: impl Into<SharedString>,
        lead: AnyElement,
        name: impl Into<SharedString>,
        line: String,
        dot: Option<popup::Dot>,
        unsaved: bool,
        picked: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        let dot = dot.map(|d| match d {
            popup::Dot::Ok => p.ok,
            popup::Dot::Warn => p.warn,
            popup::Dot::Bad => p.bad,
            popup::Dot::Quiet => p.ink_3,
        });
        div()
            .id(id.into())
            .w_full()
            .h(px(control::LIST_ROW))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::S))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .when(picked, |d| d.bg(p.active))
            .when(!picked, |d| {
                d.soft_hover(transparent_black(), p.hover, None)
            })
            .child(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(size::BODY - 0.5))
                            .text_color(p.ink)
                            .truncate()
                            .child(name.into()),
                    )
                    .when(!line.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .truncate()
                                .child(line),
                        )
                    }),
            )
            .when(unsaved, |d| {
                d.child(div().size(px(6.0)).flex_none().rounded_full().bg(p.ink))
            })
            .when_some(dot, |d, c| {
                d.child(div().size(px(6.0)).flex_none().rounded_full().bg(c))
            })
    }

    /// The head of what's picked beside a list: its icon, its name with a line under it,
    /// and what can be done with it.
    pub(super) fn pane_head(
        &self,
        lead: Option<AnyElement>,
        title: impl Into<SharedString>,
        line: Option<String>,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        self.pane_head_noted(lead, title, line, None, actions)
    }

    /// The head of what's picked, as `pane_head` draws it, with a second line under its
    /// first in an ink of its own: what a routine costs.
    pub(super) fn pane_head_noted(
        &self,
        lead: Option<AnyElement>,
        title: impl Into<SharedString>,
        line: Option<String>,
        note: Option<(String, Hsla)>,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        let p = self.p;
        let line = line.filter(|l| !l.is_empty());
        let note = note.filter(|(n, _)| !n.is_empty());
        div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .children(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(size::BODY + 2.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .truncate()
                            .child(title.into()),
                    )
                    .when_some(line, |d, l| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .line_clamp(2)
                                .child(l),
                        )
                    })
                    .when_some(note, |d, (n, ink)| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(ink)
                                .truncate()
                                .child(n),
                        )
                    }),
            )
            .when_some(actions, |d, a| d.child(div().flex_none().child(a)))
            .into_any_element()
    }

    /// A row's icon in a bigger square, for the head of what's picked.
    pub(super) fn big_tile(&self, icon: Ico, ink: Option<Hsla>) -> AnyElement {
        let p = self.p;
        div()
            .size(px(control::ICON_LARGE + 6.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .border_1()
            .border_color(p.line)
            .bg(self.card_fill())
            .text_color(ink.unwrap_or(p.ink_2))
            .child(Icon::new(icon))
            .into_any_element()
    }

    /// The popup's bottom-right corner, which resizes it: two short strokes, and the
    /// pointer that says so.
    pub(super) fn popup_grip(&self, size: (f32, f32), cx: &mut Context<Self>) -> impl IntoElement {
        let ink = self.p.ink_3;
        div()
            .id("popup-grip")
            .absolute()
            .right_0()
            .bottom_0()
            .size(px(control::GRIP))
            .cursor(CursorStyle::ResizeUpLeftDownRight)
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let (r, bt) = (b.right().as_f32() - 3.0, b.bottom().as_f32() - 3.0);
                        for k in [8.0, 4.0] {
                            let mut line = PathBuilder::stroke(px(1.0));
                            line.move_to(point(px(r - k), px(bt)));
                            line.line_to(point(px(r), px(bt - k)));
                            if let Ok(path) = line.build() {
                                window.paint_path(path, ink);
                            }
                        }
                    },
                )
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    let at = (e.position.x.as_f32(), e.position.y.as_f32());
                    this.list_ui.drag = Some((at, size));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    /// While the popup's corner is dragged, follows the pointer anywhere in the window,
    /// and keeps the size where it's let go. It draws nothing.
    pub(super) fn popup_dragger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let up = this.clone();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = this.update(cx, |this, cx| {
                        let Some((from, start)) = this.list_ui.drag else {
                            return;
                        };
                        if !e.dragging() {
                            this.popup_drag_end(cx);
                            return;
                        }
                        let v = window.viewport_size();
                        let to = (e.position.x.as_f32(), e.position.y.as_f32());
                        this.prefs.popup_size = Some(popup::dragged(
                            (v.width.as_f32(), v.height.as_f32()),
                            start,
                            from,
                            to,
                        ));
                        cx.notify();
                    });
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = up.update(cx, |this, cx| this.popup_drag_end(cx));
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }

    /// The popup's corner is let go: its size is kept for next time.
    fn popup_drag_end(&mut self, cx: &mut Context<Self>) {
        if self.list_ui.drag.take().is_some() {
            self.prefs.save(&data_dir());
            cx.notify();
        }
    }
}
