//! Spools in the settings popup (PLAN.md, "The integrations catalog"). Find has loom's
//! own two as wide cards, each with loom's knot tied to the integration's logo by a
//! thread that hangs slack until it's connected, then the category pills and a grid of
//! the rest; a tile, or a card's Connect or Manage, opens its pane over the grid's
//! right, where it's connected step by step. Installed lists the person's servers beside
//! the one picked. `model::spools` decides what each shows; this only draws it.

use super::*;
use crate::model::popup::Dot;
use crate::model::spools::{self, Act, Entry, Flow, Stage, Step};

/// How many tiles a row of the grid has.
const ACROSS: usize = 4;

/// Spools' own boxes: its search, and the open form's, each field's and each secret's
/// by its name, for the entry (and whether it's another of it) they were made for.
pub(super) struct SpoolsUi {
    pub search: Entity<InputState>,
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
            |_: &mut Loom, _, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            },
        )
        .detach();
        SpoolsUi {
            search,
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
        let entries = self.model.spools.entries.clone();
        let pill = self.model.spools.pill.clone();
        let (cards, tiles) = spools::found(&entries, pill.as_deref(), &query);
        let open = self.model.spools.flow.as_ref().map(|f| f.entry.clone());
        let mut column = div()
            .px(px(space::XL))
            .pt(px(space::S))
            .pb(px(space::XXL))
            .flex()
            .flex_col()
            .gap(px(space::L));
        if !cards.is_empty() {
            let mut row = div().flex().gap(px(space::M));
            for &i in &cards {
                row = row.child(self.spool_card(&entries[i], cx));
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
        if tiles.is_empty() && cards.is_empty() {
            column = column.child(
                div()
                    .py(px(space::M))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("Nothing found"),
            );
        }
        for chunk in tiles.chunks(ACROSS) {
            let mut row = div().flex().gap(px(space::S));
            for &i in chunk {
                let picked = open.as_deref() == Some(entries[i].id.as_str());
                row = row.child(self.spool_tile(&entries[i], picked, cx));
            }
            // The last row's gaps take a tile's room, padding and edge too, so its tiles
            // are as wide as the others.
            for _ in chunk.len()..ACROSS {
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .p(px(space::M))
                        .border_1()
                        .border_color(transparent_black()),
                );
            }
            column = column.child(row);
        }
        let flow = self.model.spools.flow.clone();
        let pane = flow.and_then(|flow| {
            let entry = entries.iter().find(|e| e.id == flow.entry)?.clone();
            Some(self.spool_pane(&entry, &flow, cx))
        });
        page.child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .child(
                    div()
                        .id("spools-find")
                        .size_full()
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

    /// The colour an entry's card and thread take: its badge's, softly.
    fn spool_tint(&self, e: &Entry) -> Hsla {
        match e.badge.as_ref().filter(|b| !b.color.is_empty()) {
            Some(b) => thread::color(&b.color, self.p.dark),
            None => self.p.ink_3,
        }
    }

    /// One of loom's own as a wide card in a soft tint of its colour: loom's knot tied
    /// to its logo, slack until it's connected and taut once it is; its name and line;
    /// and Connect, or ✓ Connected (or what's wrong) with Manage, and Connect another
    /// for one there can be several of.
    fn spool_card(&self, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let status = spools::card_status(e, &self.model.mcp_servers);
        let taut = matches!(status, Some((_, Dot::Ok)));
        let tint = self.spool_tint(e);
        let fill = motion::mix(self.card_fill(), tint, 0.07);
        let edge = motion::mix(p.line, tint, 0.3);
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
            .child(server_badge(p, e.badge.as_ref(), &e.server, BADGE));
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
            let ink = match dot {
                Dot::Ok | Dot::Quiet => p.ok,
                Dot::Warn => p.warn,
                Dot::Bad => p.bad,
            };
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(space::XS + 2.0))
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(ink)
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
    /// in the colour of what's wrong), and its name. It opens the entry's pane.
    fn spool_tile(&self, e: &Entry, picked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let fill = self.card_fill();
        let id = e.id.clone();
        let mark = spools::card_status(e, &self.model.mcp_servers).map(|(_, dot)| match dot {
            Dot::Ok | Dot::Quiet => div().text_color(p.ok).child(Icon::new(Ico::Check).xsmall()),
            Dot::Warn | Dot::Bad => div()
                .mt(px(space::XS))
                .size(px(6.0))
                .rounded_full()
                .bg(if dot == Dot::Warn { p.warn } else { p.bad }),
        });
        div()
            .id(SharedString::from(format!("spool-tile-{id}")))
            .role(Role::Button)
            .aria_label(e.name.clone())
            .flex_1()
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
                    .child(server_badge(
                        p,
                        e.badge.as_ref(),
                        &e.server,
                        BADGE_STEP + 4.0,
                    ))
                    .children(mark),
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

    /// An entry's pane, over the grid's right: its logo, name and maker; where it runs
    /// and how it signs in; its tools, each marked reads or changes; and connecting it,
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
            .child(server_badge(p, e.badge.as_ref(), &e.server, BADGE))
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
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .truncate()
                            .child(format!("by {}", e.maker)),
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
        let mut tools = div().flex().flex_col().gap(px(space::XS + 2.0));
        let shown = spools::tools_shown(&e.tools, flow.all_tools);
        let hidden = e.tools.len() - shown.len();
        for (name, changes) in shown.iter().map(|&i| &e.tools[i]) {
            let (kind, words) = if *changes {
                ("changes", "changes")
            } else {
                ("reads", "reads")
            };
            tools = tools.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(crate::prefs::mono_font())
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink)
                            .child(name.clone()),
                    )
                    .children(effect_pill(p, kind, words)),
            );
        }
        if hidden > 0 {
            tools = tools.child(
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
                ),
            );
        }
        let tools_head = div()
            .flex()
            .items_baseline()
            .gap(px(space::S))
            .text_size(px(size::SMALL))
            .child(
                div()
                    .text_color(p.ink_2)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Tools"),
            )
            .when(!e.tools_checked, |d| {
                d.child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child("not checked"),
                )
            });
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(space::L))
            .p(px(space::L))
            .child(head)
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .child(e.description.clone()),
            )
            .child(lines);
        if !e.tools.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(tools_head)
                    .child(tools),
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
        div()
            .id("spool-pane")
            .occlude()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-space::XL * (1.0 - k)))
            .w(px(layout::DETAIL_LEAST))
            .opacity(k)
            .bg(p.bg)
            .border_l_1()
            .border_t_1()
            .border_color(p.line_strong)
            .rounded_tl(px(radius::CARD))
            .shadow_lg()
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
                let mut chips = div().flex().flex_wrap().gap(px(space::S));
                for a in self.model.agents.clone().iter() {
                    let on = flow.agents.contains(&a.id);
                    let id = a.id.clone();
                    chips = chips.child(
                        self.member_chip(format!("spool-agent-{}", a.id), a, on)
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
        let going = self
            .model
            .spools
            .connects
            .get(&m.name)
            .is_some_and(|c| c.step == Step::Adding);
        let (words, dot) = if going {
            ("Connecting…".to_string(), Dot::Quiet)
        } else {
            (spools::status_line(m), spools::status_dot(m))
        };
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
        let mut chips = div().flex().flex_wrap().gap(px(space::S));
        for a in self.model.agents.clone().iter() {
            let on = m.agents.contains(&a.id);
            let (server, id) = (m.name.clone(), a.id.clone());
            chips = chips.child(
                self.member_chip(format!("spool-{}-agent-{}", m.name, a.id), a, on)
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
        let p = self.p;
        let m = &self.model.mcp_servers[row.server];
        let name = m.name.clone();
        let badge = row
            .entry
            .and_then(|e| self.model.spools.entries[e].badge.clone());
        self.list_row(
            format!("installed-{name}"),
            server_badge(p, badge.as_ref(), &name, control::LEAD).into_any_element(),
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

    /// The server picked on Installed: its logo and name, with its host or its own name
    /// under it, then how it stands, its agents, and what can be done with it.
    fn installed_pane(&mut self, row: &spools::Row, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.p;
        let m = self.model.mcp_servers[row.server].clone();
        let entry = row.entry.map(|e| self.model.spools.entries[e].clone());
        let badge = entry.as_ref().and_then(|e| e.badge.clone());
        let line = match &entry {
            Some(e) if e.several() => spools::host(&m.url),
            Some(e) if e.name != m.name => m.name.clone(),
            _ => String::new(),
        };
        let mut out = vec![self.pane_head(
            Some(server_badge(p, badge.as_ref(), &m.name, BADGE).into_any_element()),
            row.title.clone(),
            Some(line),
            None,
        )];
        out.extend(self.server_block(&m, cx));
        out
    }
}
