//! Spools in the settings popup (PLAN.md, "The integrations catalog"). Find has loom's
//! own two as wide cards, each with loom's knot tied to the integration's logo by a
//! thread that hangs slack until it's connected, then the category pills and a grid of
//! the rest; a tile, or a card's Connect or Manage, opens its detail beside the grid,
//! which narrows for it, where it's connected step by step. Installed lists the
//! person's servers beside the one picked. `model::spools` decides what each shows; this only draws it.

use super::*;
use crate::model::popup::Dot;
use crate::model::spools::{self, Act, Entry, Flow, Stage, Step};

/// How many tiles a row of the grid has.
const ACROSS: u16 = 4;

/// Spools' own boxes: its search, and the open form's, each field's and each secret's
/// by its name, for the entry (and whether it's another of it) they were made for.
pub(super) struct SpoolsUi {
    pub search: Entity<InputState>,
    /// Counts what's typed in the search, so only the last of it, once typing pauses,
    /// asks the registry.
    typed: u64,
    pub fields: Vec<(String, Entity<InputState>)>,
    pub secrets: Vec<(String, Entity<InputState>)>,
    pub made_for: Option<(String, bool)>,
}

impl SpoolsUi {
    pub fn new(window: &mut Window, cx: &mut Context<Loom>) -> SpoolsUi {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search Spools"));
        cx.subscribe_in(
            &search,
            window,
            |this: &mut Loom, state, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    // The catalog filters at once; the registry is asked once typing
                    // pauses, not at each letter.
                    let query = state.read(cx).value().to_string();
                    this.spools_ui.typed += 1;
                    let typed = this.spools_ui.typed;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(300))
                            .await;
                        let _ = this.update(cx, |loom, cx| {
                            if loom.spools_ui.typed != typed {
                                return;
                            }
                            if let Some(frame) = loom.model.spool_search(&query) {
                                loom.send(frame);
                            }
                            cx.notify();
                        });
                    })
                    .detach();
                    cx.notify();
                }
            },
        )
        .detach();
        SpoolsUi {
            search,
            typed: 0,
            fields: Vec::new(),
            secrets: Vec::new(),
            made_for: None,
        }
    }
}

impl Loom {
    /// Opens Spools for `agent`: from its composer's + menu, or its settings.
    pub(super) fn open_spools_for(&mut self, agent: Option<String>, cx: &mut Context<Self>) {
        self.open(Screen::Spools, cx);
        self.model.spools.for_agent = agent;
    }

    /// An agent's MCP servers, as its settings show them, with Add from Spools beside
    /// them, which opens Spools for the agent.
    pub(super) fn add_from_spools(&self, servers: Div, cx: &mut Context<Self>) -> Div {
        let add = self
            .small_button("edit-mcp-spools", "Add from Spools", false)
            .flex_none()
            .on_click(cx.listener(|this, _, _, cx| {
                let agent = this.model.editing.clone();
                this.open_spools_for(agent, cx);
            }));
        div()
            .flex()
            .items_start()
            .gap(px(space::S))
            .child(div().flex_1().min_w_0().child(servers))
            .child(add)
    }

    /// One of Spools' tabs, without asking for it all again.
    fn spools_tab(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.settings_last = screen;
        self.popup = Some(screen);
        cx.notify();
    }

    /// Readies the open form's boxes before it's drawn: made anew for each entry, its
    /// first one ready to type in.
    pub(super) fn ready_spools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(flow) = self.model.spools.flow.clone() else {
            self.spools_ui.made_for = None;
            return;
        };
        if flow.stage != Stage::Form {
            self.spools_ui.made_for = None;
            return;
        }
        let key = (flow.entry.clone(), flow.another);
        if self.spools_ui.made_for.as_ref() == Some(&key) {
            return;
        }
        let Some(entry) = self.model.entry(&flow.entry).cloned() else {
            return;
        };
        self.spools_ui.fields = entry
            .fields
            .iter()
            .map(|f| {
                let placeholder = f.placeholder.clone();
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                (f.name.clone(), input)
            })
            .collect();
        self.spools_ui.secrets = entry
            .secrets
            .iter()
            .filter(|k| !k.have)
            .map(|k| {
                let placeholder = k.placeholder.clone();
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .masked(true)
                });
                (k.name.clone(), input)
            })
            .collect();
        if let Some((_, first)) = self
            .spools_ui
            .fields
            .first()
            .or(self.spools_ui.secrets.first())
        {
            first.update(cx, |s, cx| s.focus(window, cx));
        }
        self.spools_ui.made_for = Some(key);
    }

    /// Spools' title with its tabs beside it: Find, and Installed with how many.
    fn spools_head(&self, installed: bool, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let tab = |id: &'static str, label: String, on: bool| {
            div()
                .id(id)
                .role(Role::Tab)
                .aria_label(label.clone())
                .h(px(control::SEGMENT))
                .px(px(space::M))
                .flex()
                .items_center()
                .rounded(px(radius::ROW))
                .text_size(px(size::SMALL))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(p.active)
                        .text_color(p.ink)
                        .font_weight(FontWeight::MEDIUM)
                })
                .when(!on, |d| d.soft_ink(p.ink_2, p.ink))
                .child(label)
        };
        let count = self.model.mcp_servers.len();
        let installed_label = if count == 0 {
            "Installed".to_string()
        } else {
            format!("Installed · {count}")
        };
        div()
            .min_h(px(control::BUTTON))
            .flex()
            .items_center()
            .gap(px(space::M))
            .child(
                div()
                    .text_size(px(size::HEADING - 2.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink)
                    .child("Spools"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .child(tab("spools-find", "Find".into(), !installed).on_click(
                        cx.listener(|this, _, _, cx| this.spools_tab(Screen::Spools, cx)),
                    ))
                    .child(
                        tab("spools-installed", installed_label, installed).on_click(
                            cx.listener(|this, _, _, cx| this.spools_tab(Screen::Installed, cx)),
                        ),
                    ),
            )
    }

    /// Spools' Find tab: the search, loom's two as wide cards, the pills, and the rest
    /// as tiles, four to a row; and the pane of the one opened, over the grid's right.
    pub(super) fn spools_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let head = self.spools_head(false, cx);
        let search = div().text_size(px(size::SMALL)).child(
            Input::new(&self.spools_ui.search)
                .prefix(Icon::new(IconName::Search).small().text_color(p.ink_3))
                .cleanable(true),
        );
        let top = div()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::M))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(head)
            .child(search);
        let page = div().flex_1().min_h_0().flex().flex_col().child(top);
        if !self.model.spools.came {
            let body = match self.model.spools.error.clone() {
                Some(e) => self.empty(Ico::Spool, &e, None, None),
                None => div().into_any_element(),
            };
            return page.child(div().px(px(space::XL)).child(body));
        }
        let query = self.spools_ui.search.read(cx).value().trim().to_string();
        let pill = self.model.spools.pill.clone();
        let shown = self.model.spools_shown(&query);
        let open = self.model.spools.flow.as_ref().map(|f| f.entry.clone());
        // With an entry open, the grid narrows beside its detail, as Settings' lists do.
        let narrow = open.is_some();
        let mut column = div()
            .px(px(space::XL))
            .pt(px(space::S))
            .pb(px(space::XXL))
            .flex()
            .flex_col()
            .gap(px(space::L));
        if !shown.cards.is_empty() {
            let mut row = div()
                .flex()
                .when(narrow, |d| d.flex_col())
                .gap(px(space::M));
            for e in &shown.cards {
                row = row.child(self.spool_card(e, cx));
            }
            column = column.child(row);
        }
        // The pills: All, then each category.
        let mut pills = div().flex().flex_wrap().gap(px(space::XS + 2.0));
        let all = std::iter::once(None).chain(
            spools::pills(&self.model.spools.categories)
                .into_iter()
                .map(Some),
        );
        for (n, c) in all.enumerate() {
            let on = c == pill;
            let label = c.clone().unwrap_or_else(|| "All".into());
            pills = pills.child(
                self.spool_pill(SharedString::from(format!("spools-pill-{n}")), &label, on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.spools.pill = c.clone();
                        cx.notify();
                    })),
            );
        }
        column = column.child(pills);
        let quiet = |text: String| {
            div()
                .py(px(space::M))
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child(text)
        };
        if shown.nothing {
            column = column.child(quiet("Nothing found".into()));
        }
        // A grid of equal columns, so every tile is as wide as every other, whatever
        // its row.
        if !shown.tiles.is_empty() {
            let mut grid = div()
                .grid()
                .grid_cols(if narrow { 2 } else { ACROSS })
                .gap(px(space::S));
            for e in &shown.tiles {
                let picked = open.as_deref() == Some(e.id.as_str());
                grid = grid.child(self.spool_tile(e, picked, cx));
            }
            column = column.child(grid);
        }
        if let Some(line) = shown.line {
            column = column.child(quiet(line));
        }
        let flow = self.model.spools.flow.clone();
        let pane = flow.and_then(|flow| {
            let entry = self.model.entry(&flow.entry)?.clone();
            Some(self.spool_pane(&entry, &flow, cx))
        });
        page.child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(
                    div()
                        .id("spools-find")
                        .h_full()
                        .when(narrow, |d| d.w(px(layout::LIST)).flex_none())
                        .when(!narrow, |d| d.flex_1())
                        .min_w_0()
                        .overflow_y_scroll()
                        .child(column),
                )
                .children(pane),
        )
    }

    /// A category pill, filled while it's the one picked.
    fn spool_pill(&self, id: SharedString, label: &str, on: bool) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.to_string())
            .h(px(control::SEGMENT))
            .px(px(space::M))
            .flex()
            .items_center()
            .rounded_full()
            .text_size(px(size::SMALL))
            .cursor_pointer()
            .when(on, |d| d.bg(p.button).text_color(p.button_ink))
            .when(!on, |d| {
                d.border_1().border_color(p.line).soft_hover(
                    transparent_black(),
                    p.hover,
                    Some((p.ink_2, p.ink)),
                )
            })
            .child(label.to_string())
    }

    /// An integration's logo, `size` square: the icon its server gave of its own, once
    /// it's fetched (`own`, a server's, before its entry's); else its entry's mark, drawn
    /// on the badge's colour as its initial would be; else its initial.
    fn spool_logo(
        &self,
        own: Option<&model::Badge>,
        entry: Option<&Entry>,
        server: &str,
        size: f32,
    ) -> Div {
        let p = self.p;
        let base = entry.and_then(|e| e.badge.as_ref());
        let fetched = |b: &&model::Badge| badge_icon(b.icon_for(p.dark)).is_some();
        if let Some(b) = own.filter(fetched).or(base.filter(fetched)) {
            return badge_view(p, b, size);
        }
        let Some((id, mark)) = entry.and_then(|e| Some((e.id.as_str(), e.icon_svg.as_deref()?)))
        else {
            return server_badge(p, base.or(own), server, size);
        };
        let fill = match base.filter(|b| !b.color.is_empty()) {
            Some(b) => thread::color(&b.color, p.dark),
            None => p.ink_3,
        };
        let path = crate::runtime_svg(&format!("spools/{id}.svg"), mark);
        div()
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(if size >= BADGE {
                radius::CARD
            } else {
                radius::ROW
            }))
            .bg(fill)
            .child(
                svg()
                    .path(path)
                    .size(px((size * 0.56).round()))
                    .text_color(motion::ink_on(fill, p.button_ink, p.button)),
            )
    }

    /// One of loom's own entries' logos on a tile in ink, black in light and white in
    /// dark, as the featured cards and an open entry draw it: its mark, or its initial.
    fn spool_mark_ink(&self, e: &Entry, size: f32) -> Div {
        let p = self.p;
        let tile = div()
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(if size >= BADGE {
                radius::CARD
            } else {
                radius::ROW
            }))
            .bg(p.button)
            .text_color(p.button_ink);
        match e.icon_svg.as_deref() {
            Some(mark) => tile.child(
                svg()
                    .path(crate::runtime_svg(&format!("spools/{}.svg", e.id), mark))
                    .size(px((size * 0.56).round()))
                    .text_color(p.button_ink),
            ),
            None => tile
                .text_size(px(size * 0.42))
                .font_weight(FontWeight::SEMIBOLD)
                .child(e.name.chars().next().unwrap_or('?').to_string()),
        }
    }

    /// One of loom's own as a wide card, in black and white as the rest of the app: loom's
    /// knot tied to its logo, on a tile in ink, by a thread that hangs grey and slack until
    /// it's connected and is pulled taut in ink once it is; its name and line;
    /// and Connect, or ✓ Connected (or what's wrong) with Manage, and Connect another
    /// for one there can be several of.
    fn spool_card(&self, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let status = spools::card_status(e, &self.model.mcp_servers);
        let taut = matches!(status, Some((_, Dot::Ok)));
        let tint = if taut { p.ink } else { p.ink_3 };
        let fill = self.card_fill();
        let edge = p.line;
        let knot = div()
            .size(px(BADGE))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(p.line_strong)
            .bg(p.bg)
            .child(svg().path("logo/knot.svg").size(px(20.0)).text_color(p.ink));
        let link = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .child(knot)
            .child(thread::tie(
                format!("spool-tie/{}", e.id),
                taut,
                tint,
                MARK_H * 2.0,
            ))
            .child(self.spool_mark_ink(e, BADGE));
        let id = e.id.clone();
        let mut actions = div().flex().items_center().gap(px(space::S));
        if e.connected() {
            if e.several() {
                let another = id.clone();
                actions = actions.child(
                    self.small_button(format!("spool-another-{id}"), "Connect another", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.model.spool_open(&another);
                            if let Some(f) = this.model.spool_connect(true) {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                );
            }
            let manage = id.clone();
            actions = actions.child(
                self.small_button(format!("spool-manage-{id}"), "Manage", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.model.spool_open(&manage);
                        cx.notify();
                    })),
            );
        } else {
            let connect = id.clone();
            actions = actions.child(
                self.small_button(format!("spool-connect-{id}"), "Connect", true)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.model.spool_open(&connect);
                        if let Some(f) = this.model.spool_connect(false) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        let state = status.map(|(words, dot)| {
            let (ink, words_ink) = match dot {
                Dot::Ok => (p.ink, p.ink),
                Dot::Quiet => (p.ink_3, p.ink_2),
                Dot::Warn => (p.warn, p.warn),
                Dot::Bad => (p.bad, p.bad),
            };
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(space::XS + 2.0))
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(words_ink)
                .when(dot == Dot::Ok, |d| d.child(Icon::new(Ico::Check).small()))
                .when(dot != Dot::Ok, |d| {
                    d.child(div().size(px(6.0)).flex_none().rounded_full().bg(ink))
                })
                .child(div().min_w_0().truncate().child(words))
        });
        div()
            .id(SharedString::from(format!("spool-card-{id}")))
            .flex_1()
            .min_w_0()
            .p(px(space::L))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(edge)
            .bg(fill)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.spool_open(&id);
                cx.notify();
            }))
            .child(link)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .truncate()
                            .child(e.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_2)
                            .truncate()
                            .child(e.description.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(space::S))
                    .min_h(px(control::BUTTON_SMALL))
                    .child(div().children(state))
                    .child(actions),
            )
            .into_any_element()
    }

    /// An entry as a tile in the grid: its logo, a check once it's connected (or a dot
    /// in the colour of how else it stands), and its name. It opens the entry's pane.
    fn spool_tile(&self, e: &Entry, picked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let fill = self.card_fill();
        let id = e.id.clone();
        let mark = spools::card_status(e, &self.model.mcp_servers).map(|(_, dot)| match dot {
            Dot::Ok => div().text_color(p.ok).child(Icon::new(Ico::Check).xsmall()),
            Dot::Quiet | Dot::Warn | Dot::Bad => div()
                .mt(px(space::XS))
                .size(px(6.0))
                .rounded_full()
                .bg(match dot {
                    Dot::Warn => p.warn,
                    Dot::Bad => p.bad,
                    _ => p.ink_3,
                }),
        });
        div()
            .id(SharedString::from(format!("spool-tile-{id}")))
            .role(Role::Button)
            .aria_label(e.name.clone())
            .min_w_0()
            .h(px(control::TILE))
            .p(px(space::M))
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(radius::CARD))
            .border_1()
            .cursor_pointer()
            .when(picked, |d| d.border_color(p.line_strong).bg(p.active))
            .when(!picked, |d| {
                d.border_color(p.line).soft_hover(fill, p.hover, None)
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(self.spool_logo(None, Some(e), &e.server, BADGE_STEP + 4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::XS + 2.0))
                            .when(e.community, |d| d.child(community_mark(p)))
                            .children(mark),
                    ),
            )
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink)
                    .truncate()
                    .child(e.name.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.spool_open(&id);
                cx.notify();
            }))
            .into_any_element()
    }

    /// An entry's detail, beside the narrowed grid: its logo, name and maker; where it runs
    /// and how it signs in; its tools, in a column of what reads and one of what changes; and connecting it,
    /// step by step, or how its servers stand once it has some.
    fn spool_pane(&mut self, e: &Entry, flow: &Flow, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let k = motion::arrive(&SharedString::from("spool-pane"), motion::SLIDE);
        let close =
            self.close_button("spool-pane-close")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model.spool_close();
                    cx.notify();
                }));
        let head = div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .child(if e.featured() {
                self.spool_mark_ink(e, BADGE)
            } else {
                self.spool_logo(None, Some(e), &e.server, BADGE)
            })
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
                            .child(e.name.clone()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .truncate()
                                    .child(format!("by {}", e.maker)),
                            )
                            .when(e.community, |d| d.child(community_mark(p))),
                    ),
            )
            .child(close);
        let lines = card_lines(
            p,
            &[
                ("Runs".to_string(), e.runs_words()),
                ("Signs in".to_string(), e.sign_in_words()),
            ],
            1.0,
        );
        // Its tools in two quiet columns, what only reads and what changes something,
        // each under its count, a few of each until Show all.
        let shown = spools::tools_shown(&e.tools, flow.all_tools);
        let hidden = e.tools.len() - shown.len();
        let column = |changes: bool| {
            let total = e.tools.iter().filter(|t| t.1 == changes).count();
            let mut c = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(
                    div()
                        .pb(px(2.0))
                        .text_size(px(size::SMALL))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_2)
                        .child(format!(
                            "{} · {total}",
                            if changes { "Changes" } else { "Reads" }
                        )),
                );
            for (name, _) in shown
                .iter()
                .map(|&i| &e.tools[i])
                .filter(|t| t.1 == changes)
            {
                c = c.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(crate::prefs::mono_font())
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink)
                        .child(name.clone()),
                );
            }
            (c, total)
        };
        let (reads, reads_n) = column(false);
        let (changes, changes_n) = column(true);
        let tools = div()
            .flex()
            .items_start()
            .gap(px(space::XL))
            .when(reads_n > 0, |d| d.child(reads))
            .when(changes_n > 0, |d| d.child(changes));
        let more = (hidden > 0).then(|| {
            div().flex().child(
                div()
                    .id("spool-all-tools")
                    .role(Role::Button)
                    .text_size(px(size::SMALL))
                    .cursor_pointer()
                    .soft_ink(p.ink_2, p.ink)
                    .child(format!("Show all {}", e.tools.len()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(flow) = this.model.spools.flow.as_mut() {
                            flow.all_tools = true;
                        }
                        cx.notify();
                    })),
            )
        });
        let unchecked = (!e.tools_checked).then(|| {
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Tools not checked")
        });
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(space::L))
            .p(px(space::XL))
            .child(head)
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .child(e.description.clone()),
            )
            // One loom can't connect runs nowhere it can say; its why, below, says so.
            .when(e.unsupported.is_empty(), |d| d.child(lines));
        if !e.tools.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(tools)
                    .children(more)
                    .children(unchecked),
            );
        }
        let steps = self.spool_steps(e, flow, cx);
        body = body.child(
            div()
                .pt(px(space::L))
                .border_t_1()
                .border_color(p.line)
                .flex()
                .flex_col()
                .gap(px(space::M))
                .children(steps),
        );
        // Beside the grid, not over it: the grid narrows to make room (`spools_page`).
        div()
            .id("spool-pane")
            .flex_1()
            .min_w(px(layout::DETAIL_LEAST))
            .h_full()
            .opacity(k)
            .border_l_1()
            .border_color(p.line)
            .child(
                div()
                    .id(SharedString::from(format!("spool-pane-{}", e.id)))
                    .size_full()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    /// Where connecting an entry is, in its pane: Connect; its form; Sign in; which
    /// agents get it; why it failed; or, once it's connected, how each of its servers
    /// stands.
    fn spool_steps(&mut self, e: &Entry, flow: &Flow, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.p;
        let said = |text: &str| {
            div()
                .text_size(px(size::SMALL))
                .text_color(p.bad)
                .truncate()
                .child(text.to_string())
                .into_any_element()
        };
        let quiet = |text: &str| {
            div()
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child(text.to_string())
                .into_any_element()
        };
        let mut out = Vec::new();
        match flow.stage {
            Stage::About if e.connected() => {
                let servers: Vec<model::McpServer> = e
                    .servers
                    .iter()
                    .filter_map(|n| self.model.mcp_server(n).cloned())
                    .collect();
                let several = servers.len() > 1;
                for m in &servers {
                    if several {
                        out.push(
                            div()
                                .text_size(px(size::SMALL))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.ink)
                                .child(spools::host(&m.url))
                                .into_any_element(),
                        );
                    }
                    out.extend(self.server_block(m, cx));
                }
                if e.several() {
                    out.push(
                        div()
                            .flex()
                            .child(
                                self.small_button("spool-pane-another", "Connect another", false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(f) = this.model.spool_connect(true) {
                                            this.send(f);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .into_any_element(),
                    );
                }
            }
            // One from the registry loom can't connect says why, in Connect's place.
            Stage::About if !e.unsupported.is_empty() => out.push(quiet(&e.unsupported)),
            Stage::About => {
                out.push(
                    div()
                        .flex()
                        .child(
                            self.button_owned("spool-pane-connect", "Connect", true)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(f) = this.model.spool_connect(false) {
                                        this.send(f);
                                    }
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                );
            }
            Stage::Form => {
                for f in &e.fields {
                    if let Some((_, input)) =
                        self.spools_ui.fields.iter().find(|(n, _)| *n == f.name)
                    {
                        out.push(
                            self.field(
                                &f.label,
                                div().text_size(px(size::SMALL)).child(Input::new(input)),
                            )
                            .into_any_element(),
                        );
                    }
                }
                for key in &e.secrets {
                    let value = match self.spools_ui.secrets.iter().find(|(n, _)| *n == key.name) {
                        Some((_, input)) => div()
                            .text_size(px(size::SMALL))
                            .child(
                                Input::new(input).mask_toggle().prefix(
                                    div()
                                        .text_color(p.ink_3)
                                        .child(Icon::new(Ico::Lock).xsmall()),
                                ),
                            )
                            .into_any_element(),
                        None => quiet("Saved"),
                    };
                    out.push(self.field(&key.label, value).into_any_element());
                }
                if !e.note.is_empty() {
                    out.push(
                        div()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(e.note.clone())
                            .into_any_element(),
                    );
                }
                if !flow.said.is_empty() {
                    out.push(said(&flow.said));
                }
                let connect = self
                    .small_button("spool-form-connect", "Connect", true)
                    .on_click(cx.listener(|this, _, _, cx| this.spool_submit(cx)));
                let cancel = self
                    .small_button("spool-form-cancel", "Cancel", false)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.spool_back();
                        cx.notify();
                    }));
                out.push(
                    div()
                        .flex()
                        .gap(px(space::S))
                        .child(connect)
                        .child(cancel)
                        .into_any_element(),
                );
            }
            Stage::Going(Step::Adding) => out.push(quiet("Connecting…")),
            Stage::Going(Step::SignIn) => {
                if !flow.said.is_empty() {
                    out.push(said(&flow.said));
                }
                let server = flow.server.clone();
                out.push(
                    div()
                        .flex()
                        .child(
                            self.sign_in_button(&server, cx)
                                .gap(px(space::XS))
                                .child(Icon::new(Ico::ArrowUpRight).xsmall()),
                        )
                        .into_any_element(),
                );
            }
            Stage::Going(Step::Agents) => {
                let mut chips = self.switch_box();
                for (n, a) in self.model.agents.clone().iter().enumerate() {
                    let on = flow.agents.contains(&a.id);
                    let id = a.id.clone();
                    chips = chips.child(
                        self.agent_switch_row(format!("spool-agent-{}", a.id), a, on, n == 0)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.model.spool_pick_agent(&id);
                                cx.notify();
                            })),
                    );
                }
                out.push(self.field("Agents", chips).into_any_element());
                out.push(
                    div()
                        .flex()
                        .child(
                            self.small_button("spool-agents-done", "Done", true)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(f) = this.model.spool_give() {
                                        this.send(f);
                                    }
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                );
            }
            Stage::Going(Step::Failed) => {
                out.push(said(if flow.said.is_empty() {
                    "It didn't connect"
                } else {
                    &flow.said
                }));
                out.push(
                    div()
                        .flex()
                        .child(
                            self.small_button("spool-try-again", "Try again", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model.spool_back();
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                );
            }
            // Done: how the server stands, once the list has it; until then, that it's
            // connected.
            Stage::Going(Step::Done) => match self.model.mcp_server(&flow.server).cloned() {
                Some(m) => out.extend(self.server_block(&m, cx)),
                None => out.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::XS + 2.0))
                        .text_size(px(size::SMALL))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ok)
                        .child(Icon::new(Ico::Check).small())
                        .child("Connected")
                        .into_any_element(),
                ),
            },
        }
        out
    }

    /// An agent as a row of a list of switches: its thread, its name, and whether it has
    /// the server. A click anywhere on the row turns it.
    fn agent_switch_row(
        &self,
        id: String,
        a: &model::Agent,
        on: bool,
        first: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(SharedString::from(id.clone()))
            .role(Role::Switch)
            .aria_label(a.name.clone())
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::S))
            .cursor_pointer()
            .when(!first, |d| d.border_t_1().border_color(p.line))
            .soft_hover(transparent_black(), p.hover, None)
            .child(thread::strand(
                format!("{id}-thread"),
                Mood::Idle,
                thread::color(&a.color, p.dark),
                a.outline(),
                22.0,
                10.0,
                None,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink)
                    .child(a.name.clone()),
            )
            .child(self.switch(format!("{id}-switch"), on))
    }

    /// The box the agent switch rows sit in.
    fn switch_box(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(self.p.line)
            .overflow_hidden()
    }

    /// The form's Connect, with what's typed in it.
    fn spool_submit(&mut self, cx: &mut Context<Self>) {
        let read = |boxes: &[(String, Entity<InputState>)], cx: &App| -> Vec<(String, String)> {
            boxes
                .iter()
                .map(|(n, input)| (n.clone(), input.read(cx).value().to_string()))
                .collect()
        };
        let fields = read(&self.spools_ui.fields, cx);
        let secrets = read(&self.spools_ui.secrets, cx);
        if let Some(f) = self.model.spool_submit(&fields, &secrets) {
            self.send(f);
        }
        cx.notify();
    }

    /// How one of the person's servers stands, with its dot; its agents, each a chip
    /// that turns it on or off for them; and Sign in, Reconnect, Disconnect and Remove,
    /// as it allows.
    fn server_block(&mut self, m: &model::McpServer, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.p;
        let (words, dot) = self.model.server_standing(m);
        let ink = match dot {
            Dot::Ok => p.ok,
            Dot::Warn => p.warn,
            Dot::Bad => p.bad,
            Dot::Quiet => p.ink_3,
        };
        let status = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .text_size(px(size::SMALL))
            .child(div().size(px(6.0)).flex_none().rounded_full().bg(ink))
            .child(
                div()
                    .min_w_0()
                    .text_color(if dot == Dot::Bad { p.bad } else { p.ink })
                    .line_clamp(2)
                    .child(words),
            );
        let mut chips = self.switch_box();
        for (n, a) in self.model.agents.clone().iter().enumerate() {
            let on = m.agents.contains(&a.id);
            let (server, id) = (m.name.clone(), a.id.clone());
            chips = chips.child(
                self.agent_switch_row(format!("spool-{}-agent-{}", m.name, a.id), a, on, n == 0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.model.spool_toggle_agent(&server, &id) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        let mut actions = div().flex().flex_wrap().items_center().gap(px(space::S));
        for act in spools::acts(m) {
            let name = m.name.clone();
            let id = format!("spool-{}-{act:?}", m.name);
            let button = match act {
                Act::SignIn => self.sign_in_button(&name, cx),
                Act::Try => self.small_button(id, "Try it", false).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let f = this.model.spool_try(&name);
                        this.send(f);
                        cx.notify();
                    },
                )),
                Act::Reconnect => self
                    .small_button(id, "Reconnect", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.model.spool_reconnect(&name) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
                Act::Disconnect => {
                    self.small_button(id, "Disconnect", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.sign_out_mcp(&name);
                            this.send(f);
                            cx.notify();
                        }))
                }
                Act::Remove => self
                    .small_button(id, "Remove", false)
                    .text_color(p.bad)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.spool_remove(&name);
                        this.send(f);
                        cx.notify();
                    })),
            };
            actions = actions.child(button);
        }
        let mut out = vec![status.into_any_element()];
        if !self.model.agents.is_empty() {
            out.push(self.field("Agents", chips).into_any_element());
        }
        out.push(actions.into_any_element());
        out
    }

    /// Spools' Installed tab: the person's servers down the left, as `model::spools`
    /// orders them, each with its dot, and the one picked beside them.
    pub(super) fn installed_page(&mut self, cx: &mut Context<Self>) -> Div {
        let head = self.spools_head(true, cx).into_any_element();
        let rows = self.model.installed_rows();
        if rows.is_empty() {
            let find = self
                .small_button("installed-find", "Find one", true)
                .on_click(cx.listener(|this, _, _, cx| this.spools_tab(Screen::Spools, cx)))
                .into_any_element();
            let empty = self.empty(Ico::Spool, "Nothing connected yet", None, Some(find));
            return div().flex_1().min_h_0().child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(space::XL))
                    .pt(px(space::XL))
                    .px(px(space::XL))
                    .child(head)
                    .child(empty),
            );
        }
        let keys: Vec<String> = rows
            .iter()
            .map(|r| self.model.mcp_servers[r.server].name.clone())
            .collect();
        let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let picked = self.model.spools.picked.clone();
        let shows = popup::shows(
            self.list_split(),
            picked.as_deref().map(Some),
            &key_refs,
            false,
        );
        let query = self.list_query(cx);
        let found = (0..rows.len())
            .filter(|&i| {
                let m = &self.model.mcp_servers[rows[i].server];
                popup::finds(
                    &query,
                    &format!("{} {} {}", rows[i].title, m.name, rows[i].line),
                )
            })
            .collect();
        let (pane_key, pane) = match shows.pane {
            Pane::Item(i) => {
                let row = rows[i].clone();
                (keys[i].clone(), self.installed_pane(&row, cx))
            }
            _ => (String::new(), Vec::new()),
        };
        self.list_layout(
            head,
            ListPage {
                id: "installed-page",
                title: "Spools",
                action: None,
                shows,
                found,
                row: Loom::installed_row,
                head: None,
                foot: None,
                pane_key,
                pane,
            },
            cx,
        )
    }

    /// One of the person's servers as a row of Installed: its logo, its name, the line
    /// under it, and its dot.
    fn installed_row(&mut self, ix: usize, picked: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.model.installed_rows().get(ix).cloned() else {
            return div().into_any_element();
        };
        let m = &self.model.mcp_servers[row.server];
        let name = m.name.clone();
        let logo = self
            .spool_logo(
                m.badge.as_ref(),
                row.entry.map(|e| &self.model.spools.entries[e]),
                &name,
                control::LEAD,
            )
            .into_any_element();
        self.list_row(
            format!("installed-{name}"),
            logo,
            row.title.clone(),
            row.line.clone(),
            row.dot,
            picked,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.model.spools.picked = Some(name.clone());
            cx.notify();
        }))
        .into_any_element()
    }

    /// The server picked on Installed: its logo and name, with its own name under it
    /// where that isn't its entry's and its host where it has one (`installed_under`),
    /// then how it stands, its agents, and what can be done with it.
    fn installed_pane(&mut self, row: &spools::Row, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let m = self.model.mcp_servers[row.server].clone();
        let entry = row.entry.map(|e| self.model.spools.entries[e].clone());
        let under = spools::installed_under(&m, entry.as_ref());
        let logo = self
            .spool_logo(m.badge.as_ref(), entry.as_ref(), &m.name, BADGE)
            .into_any_element();
        let mut out = vec![self.pane_head(
            Some(logo),
            row.title.clone(),
            (!under.is_empty()).then_some(under),
            None,
        )];
        out.extend(self.server_block(&m, cx));
        out
    }
}

/// The mark of a server from the MCP Registry, not one of loom's catalog.
fn community_mark(p: Palette) -> Div {
    div()
        .flex_none()
        .px(px(space::S))
        .py(px(1.0))
        .rounded_full()
        .border_1()
        .border_color(p.line)
        .text_size(px(size::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.ink_2)
        .child("Community")
}
