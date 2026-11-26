//! Group chats on screen (PLAN.md, "Group chats"): their rows in the sidebar's list, the
//! chat's head while one is open, the line over each reply that says how it came to its
//! agent, and the panel beside the chat that makes one or changes it. The instance
//! picks who answers; this only draws it and sends what the person changes.

use super::*;

/// The name and colour a reply in a group chat is drawn with: its own agent's.
pub(super) fn reply_look(p: Palette, m: &Message) -> Option<(String, Hsla)> {
    let r = m.routed.as_ref()?;
    let agents = MENTION_AGENTS.with(|a| a.borrow().clone());
    let a = agents.iter().find(|a| a.id == r.agent)?;
    Some((a.name.clone(), thread::color(&a.color, p.dark)))
}

/// The line over a reply in a group chat: "→ Ledger · money question", with the
/// agent's thread in its shape and colour. A click offers the chat's other members, to
/// send the person's message to instead.
pub(super) fn route_line(p: Palette, m: &Message, this: WeakEntity<Loom>) -> Option<Stateful<Div>> {
    let r = m.routed.clone()?;
    let agents = MENTION_AGENTS.with(|a| a.borrow().clone());
    let a = agents.iter().find(|a| a.id == r.agent)?;
    let hex = mention_color(p, &a.color);
    let glyph = mention_thread(a, &hex);
    let ink = hex_color(&hex).unwrap_or(p.ink_2);
    let said = r.said.trim().to_string();
    let spoken = if said.is_empty() {
        format!("Answered by {}", a.name)
    } else {
        format!("Answered by {}, {said}", a.name)
    };
    let (message, agent) = (m.id.clone(), r.agent.clone());
    let can_resend = !m.live && !m.queued;
    Some(
        div()
            .id(SharedString::from(format!("routed-{}", m.id)))
            .role(Role::Button)
            .aria_label(spoken)
            .flex()
            .items_center()
            .gap(px(space::XS))
            .text_size(px(size::CAPTION))
            .text_color(p.ink_3)
            .when(can_resend, |d| {
                d.cursor_pointer()
                    .tooltip(tip("Send to another agent"))
                    .on_mouse_down(MouseButton::Left, move |e, _, cx| {
                        cx.stop_propagation();
                        let (message, agent) = (message.clone(), agent.clone());
                        let _ = this.update(cx, |loom, cx| {
                            loom.open_ctx(e.position, Ctx::Resend(message, agent), cx)
                        });
                    })
            })
            .child("→")
            .when_some(glyph, |d, path| {
                d.child(img(std::path::PathBuf::from(path)).size(px(14.0)))
            })
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(ink)
                    .child(a.name.clone()),
            )
            .when(!said.is_empty(), |d| d.child(format!("· {said}"))),
    )
}

impl Loom {
    /// Shows a group chat in the chat's place.
    pub(super) fn open_room(&mut self, id: String, cx: &mut Context<Self>) {
        self.screen = Screen::Chat;
        self.popup = None;
        self.jumping = None;
        self.picked = None;
        // An agent's own page beside the chat isn't the group chat's.
        if self.side.is_some()
            && self.side != Some(Screen::Plugins)
            && self.side != Some(Screen::Room)
        {
            self.side = None;
        }
        self.model.editing = None;
        self.menu = None;
        self.model.show_room(&id);
        if self.side == Some(Screen::Room) {
            self.model.room_form = model::RoomForm::default();
            self.fill_room_form(cx);
            let f = self.model.ask_room_usage(&id);
            self.send(f);
        }
        self.focus_composer = true;
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Opens the panel that makes a group chat: no one in it yet, or the agent whose
    /// chat is open.
    pub(super) fn new_room(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ctx = None;
        self.model.start_room_form();
        // The open group chat's name, if it was still to go in, isn't the new one's.
        self.room_name_fill = None;
        self.room_name_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.show_side(Screen::Room, cx);
        self.room_name_in.update(cx, |s, cx| s.focus(window, cx));
    }

    /// Opens the open group chat's settings beside it.
    pub(super) fn room_settings(&mut self, cx: &mut Context<Self>) {
        self.ctx = None;
        self.model.room_form = model::RoomForm::default();
        self.fill_room_form(cx);
        if let Some(id) = self.model.open_room.clone() {
            let f = self.model.ask_room_usage(&id);
            self.send(f);
        }
        self.show_side(Screen::Room, cx);
    }

    fn fill_room_form(&mut self, cx: &mut Context<Self>) {
        let name = self
            .model
            .room_open()
            .map(|r| r.name.clone())
            .unwrap_or_default();
        self.room_name_fill = Some(name);
        cx.notify();
    }

    /// Its members' colours, the chat's own first when it has one: four at most.
    pub(super) fn room_colors(&self, r: &model::Room) -> Vec<Hsla> {
        let p = self.p;
        let mut colors: Vec<Hsla> = Vec::new();
        if !r.color.is_empty() {
            colors.push(thread::color(&r.color, p.dark));
        }
        colors.extend(
            r.members
                .iter()
                .filter_map(|m| self.model.agent(m))
                .map(|a| thread::color(&a.color, p.dark)),
        );
        colors.truncate(4);
        colors
    }

    pub(super) fn room_mark(&self, r: &model::Room, w: f32, h: f32) -> Div {
        div()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .child(thread::weave(self.room_colors(r), 0.0, 1.0, h))
    }

    /// An empty group chat: its members' threads woven large, its name, and the
    /// composer.
    pub(super) fn room_hero(&self, r: &model::Room, composer: Option<Div>) -> Div {
        let p = self.p;
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::XL))
            .px(px(space::XL))
            .pb(px(60.0))
            .child(self.greeting(Greeting::Line, self.room_mark(r, 140.0, 20.0)))
            .child(
                self.greeting(
                    Greeting::Name,
                    div()
                        .text_size(px(size::HERO))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.ink)
                        .child(r.name.clone()),
                ),
            )
            .when_some(composer, |d, c| {
                d.child(div().w_full().max_w(px(space::COLUMN)).child(c))
            })
    }

    /// A group chat's row in the sidebar's list: its members' threads woven, its name,
    /// and a dot at its end while a reply in it hasn't been seen. A click opens it, a
    /// right-click its settings.
    pub(super) fn room_row(&self, r: &model::Room, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let selected =
            self.model.open_room.as_deref() == Some(r.id.as_str()) && self.screen == Screen::Chat;
        let unread = self.model.room_unread(&r.id);
        let (id, menu_id) = (r.id.clone(), r.id.clone());
        div()
            .id(SharedString::from(format!("room-{}", r.id)))
            .role(Role::Button)
            .aria_label(r.name.clone())
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .when(selected, |d| d.bg(p.active))
            .when(!selected, move |d| {
                d.soft_hover(transparent_black(), p.hover, None)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.open_room(id.clone(), cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_ctx(e.position, Ctx::Room(menu_id.clone()), cx);
                }),
            )
            .child(self.room_mark(r, 26.0, 14.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(size::BODY))
                    .font_weight(if unread {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(p.ink)
                    .truncate()
                    .child(r.name.clone()),
            )
            .when(unread, |d| {
                d.child(div().size(px(6.0)).flex_none().rounded_full().bg(p.ink))
            })
    }

    /// The chat's head while a group chat is open: its mark, its name and how many are
    /// in it, which open its settings beside it. A narrow header drops how many, then
    /// shortens the name (`fit`), and then (`cut`) both are whole under the pointer.
    pub(super) fn room_head(
        &self,
        r: &model::Room,
        fit: &model::header::Fit,
        cut: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = self.p;
        let open = self.side == Some(Screen::Room);
        let whole = format!("{} · {}", r.name, members_line(r.members.len()));
        div()
            .id("room-head")
            .role(Role::Button)
            .aria_label(r.name.clone())
            .occlude()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .when(open, |d| d.bg(p.hover))
            .soft_hover(transparent_black(), p.hover, None)
            .when(cut, |d| d.tooltip(super::tip_text(whole)))
            .on_click(cx.listener(|this, _, _, cx| {
                if this.side == Some(Screen::Room) {
                    this.close_side(cx);
                } else {
                    this.room_settings(cx);
                }
            }))
            .child(self.room_mark(r, super::HEAD_MARK, 14.0))
            .child(
                div()
                    .min_w_0()
                    .when_some(fit.name, |d, most| d.max_w(px(most)))
                    .truncate()
                    .text_size(px(size::BODY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink)
                    .child(r.name.clone()),
            )
            .when(fit.members, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(members_line(r.members.len())),
                )
            })
    }

    /// How wide the group chat's head is, whole, `name` its name's words, and its
    /// members line with the gap before it (`model::header::Sizes`).
    pub(super) fn room_head_widths(&self, r: &model::Room, window: &Window) -> (f32, f32, f32) {
        let name = super::ui_text_width(&r.name, size::BODY, FontWeight::MEDIUM, window);
        let members = space::S
            + super::ui_text_width(
                &members_line(r.members.len()),
                size::CAPTION,
                FontWeight::NORMAL,
                window,
            );
        (
            2.0 * space::S + super::HEAD_MARK + space::S + name + members,
            name,
            members,
        )
    }

    /// A right-click on a group chat's row: its settings, and Delete for one the person
    /// made.
    pub(super) fn room_menu(&self, panel: Div, room: String, cx: &mut Context<Self>) -> Div {
        let everyone = self.model.room(&room).is_some_and(|r| r.everyone);
        let open = room.clone();
        let panel = panel.child(
            self.menu_item("ctx-room-settings", Ico::Settings, "Settings", false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ctx = None;
                    this.open_room(open.clone(), cx);
                    this.room_settings(cx);
                })),
        );
        if everyone {
            return panel;
        }
        panel.child(
            self.menu_item("ctx-room-delete", Ico::Trash, "Delete group chat", true)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.ctx = None;
                    let f = this.model.delete_room(&room);
                    this.send(f);
                    cx.notify();
                })),
        )
    }

    /// The menu the line over a reply opens: the chat's other members, to send the
    /// person's message to instead. Loom keeps it as a correction the router follows.
    pub(super) fn resend_menu(
        &self,
        panel: Div,
        message: String,
        agent: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let mut panel = panel.child(
            div()
                .px(px(space::M))
                .py(px(space::XS))
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Send to"),
        );
        for (n, member) in self.model.resend_to(&agent).into_iter().enumerate() {
            let Some(a) = self.model.agent(&member).cloned() else {
                continue;
            };
            let message = message.clone();
            panel = panel.child(
                div()
                    .id(SharedString::from(format!("ctx-resend-{n}")))
                    .role(Role::MenuItem)
                    .aria_label(format!("Send to {}", a.name))
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::M))
                    .py(px(space::S))
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink)
                    .soft_hover(transparent_black(), p.hover, None)
                    .child(thread::strand(
                        format!("resend-{}", a.id),
                        Mood::Idle,
                        thread::color(&a.color, p.dark),
                        a.outline(),
                        20.0,
                        8.0,
                        None,
                    ))
                    .child(div().flex_1().child(a.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ctx = None;
                        let f = this.model.resend(&message, &member);
                        this.send(f);
                        cx.notify();
                    })),
            );
        }
        panel
    }

    /// The panel beside the chat for group chats: the form that makes one, or the open
    /// one's name, members, default, colour and the corrections the router follows.
    pub(super) fn room_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if let Some(name) = self.room_name_fill.take() {
            self.room_name_in
                .update(cx, |s, cx| s.set_value(name, window, cx));
        }
        // What it shows, and whether Make is on, is the model's: the form's own values
        // while one is made, never the open group chat's.
        let name = self.room_name_in.read(cx).value().to_string();
        let panel = self.model.room_panel(&name);
        let making = panel.making();
        let mut page = div()
            .w_full()
            .max_w(px(layout::PAGE))
            .flex()
            .flex_col()
            .gap(px(space::L));
        // Its name: kept with Save while it's made, or at once on Enter or Save after.
        let name_row = div()
            .flex()
            .gap(px(space::S))
            .child(div().flex_1().child(Input::new(&self.room_name_in)))
            .when(!making, |d| {
                d.child(
                    self.button("room-rename", "Save", false)
                        .on_click(cx.listener(|this, _, _, cx| this.rename_room(cx))),
                )
            });
        page = page.child(self.field("Name", name_row));
        // Its members: every one of the person's agents, each on or off. Everyone's
        // are all of them, always. While one is made, the label counts the picked.
        if panel.members_row {
            let mut chips = div().flex().flex_wrap().gap(px(space::S));
            for a in self.model.agents.iter() {
                let on = panel.members.contains(&a.id);
                let id = a.id.clone();
                chips = chips.child(
                    self.member_chip(format!("room-member-{}", a.id), a, on)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_member(&id, cx);
                        })),
                );
            }
            page = page.child(match &panel.members_count {
                Some(c) => self.counted_field("Members", c, chips),
                None => self.field("Members", chips),
            });
        }
        // Its colour, or none.
        page = page.child(self.field("Colour", self.room_swatches(&panel.color, cx)));
        if making {
            let make = self
                .button("room-make", "Make", true)
                .when(!panel.can_make(), |d| d.opacity(0.5))
                .on_click(cx.listener(|this, _, _, cx| this.make_room(cx)));
            let cancel = self
                .button("room-cancel", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| this.close_side(cx)));
            page = page.child(div().flex().gap(px(space::S)).child(make).child(cancel));
            if let Some(e) = self.model.room_error.clone() {
                page = page.child(
                    div()
                        .text_size(px(size::SMALL))
                        .text_color(self.p.bad)
                        .child(e),
                );
            }
        } else if let Some(r) = panel.room {
            page = page
                .child(self.field("When it can't pick", self.default_chips(&r, cx)))
                .child(self.field("Corrections", self.corrections(&r, cx)));
            // What it follows outside loom, to take away (watch_view.rs).
            if let Some(list) = self.following_list(model::watch::Follower::Room(&r.id), cx) {
                page = page.child(self.field("Following", list));
            }
            if let Some(u) = self.model.open_room_usage().cloned() {
                page = page.child(self.field("Usage", self.room_usage(&u)));
            }
        }
        div().flex_1().min_h_0().child(
            div()
                .id("room-panel")
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

    /// A field whose label has a count after it, as `field` draws its label: ink-3
    /// until it's met, then the ok colour.
    fn counted_field(
        &self,
        label: &str,
        count: &model::MembersCount,
        input: impl IntoElement,
    ) -> Div {
        let p = self.p;
        div()
            .flex()
            .flex_col()
            .gap(px(space::XS + 2.0))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(space::S))
                    .text_size(px(size::SMALL))
                    .child(
                        div()
                            .text_color(p.ink_2)
                            .font_weight(FontWeight::MEDIUM)
                            .child(label.to_string()),
                    )
                    .child(
                        div()
                            .text_color(if count.met { p.ok } else { p.ink_3 })
                            .child(count.text.clone()),
                    ),
            )
            .child(input)
    }

    /// What the group chat used: its members' turns, then the router's picks.
    fn room_usage(&self, u: &model::RoomUsage) -> Div {
        let line = |label: &str, today: i64, month: i64| {
            div()
                .flex()
                .gap(px(space::S))
                .text_size(px(size::SMALL))
                .child(
                    div()
                        .w(px(64.0))
                        .text_color(self.p.ink_2)
                        .child(label.to_string()),
                )
                .child(div().text_color(self.p.ink).child(usage_line(today, month)))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(line("Agents", u.tokens_today, u.tokens_month))
            .child(line("Router", u.router_today, u.router_month))
    }

    pub(super) fn member_chip(&self, id: String, a: &model::Agent, on: bool) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(SharedString::from(id))
            .role(Role::CheckBox)
            .aria_label(a.name.clone())
            .h(px(control::CHIP))
            .px(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::S))
            .rounded(px(radius::CHIP))
            .text_size(px(size::SMALL))
            .cursor_pointer()
            .border_1()
            .border_color(if on { p.ink_3 } else { p.line })
            .when(on, |d| d.bg(p.active).text_color(p.ink))
            .when(!on, |d| {
                d.text_color(p.ink_2).soft_hover(
                    transparent_black(),
                    p.hover,
                    Some((p.ink_2, p.ink)),
                )
            })
            .child(thread::strand(
                format!("room-chip-{}", a.id),
                Mood::Idle,
                thread::color(&a.color, p.dark),
                a.outline(),
                18.0,
                8.0,
                None,
            ))
            .child(a.name.clone())
    }

    fn room_swatches(&self, color: &str, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let swatch = |id: ElementId, picked: bool| {
            div()
                .id(id)
                .size(px(control::ICON))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::BUTTON))
                .border_2()
                .border_color(if picked { p.ink_2 } else { transparent_black() })
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
        };
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(space::XS))
            .child(
                swatch("room-swatch-none".into(), color.is_empty())
                    .tooltip(tip("None"))
                    .child(
                        div()
                            .size(px(18.0))
                            .rounded(px(radius::ROW))
                            .border_1()
                            .border_color(p.line_strong),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.set_room_color("", cx))),
            );
        for (i, (c, title)) in THREAD_COLORS.iter().enumerate() {
            let c: &'static str = c;
            row = row.child(
                swatch(("room-swatch", i).into(), color == c)
                    .tooltip(tip(title))
                    .child(
                        div()
                            .size(px(18.0))
                            .rounded(px(radius::ROW))
                            .bg(thread::color(c, p.dark)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.set_room_color(c, cx))),
            );
        }
        row
    }

    /// Who a message goes to when the router can't pick: the one that answered last,
    /// or one member.
    fn default_chips(&self, r: &model::Room, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().flex_wrap().gap(px(space::S));
        let room = r.id.clone();
        row = row.child(
            self.chip(
                "room-default-last",
                "Last to answer",
                r.default.is_empty(),
                false,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let f = this
                    .model
                    .update_room(&room, serde_json::json!({"default": ""}));
                this.send(f);
                cx.notify();
            })),
        );
        for m in &r.members {
            let Some(a) = self.model.agent(m) else {
                continue;
            };
            let (room, member) = (r.id.clone(), m.clone());
            row = row.child(
                self.chip(
                    SharedString::from(format!("room-default-{m}")),
                    &a.name,
                    &r.default == m,
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let f = this
                        .model
                        .update_room(&room, serde_json::json!({"default": member}));
                    this.send(f);
                    cx.notify();
                })),
            );
        }
        row
    }

    /// The messages the person sent to another member, newest first, each with who it
    /// went to and an × that forgets it.
    fn corrections(&self, r: &model::Room, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col();
        if r.corrections.is_empty() {
            return list.child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("None yet"),
            );
        }
        for c in r.corrections.iter().rev() {
            let to = self
                .model
                .agent(&c.to)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "An agent".into());
            let (room, id) = (r.id.clone(), c.id.clone());
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .py(px(space::S))
                    .border_b_1()
                    .border_color(p.line)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink)
                                    .truncate()
                                    .child(c.text.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(format!("→ {to}")),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("forget-{}", c.id)))
                            .role(Role::Button)
                            .aria_label("Forget")
                            .size(px(control::ICON_SMALL))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_3)
                            .cursor_pointer()
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
                            .tooltip(tip("Forget"))
                            .child(Icon::new(Ico::X).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.forget_correction(&room, &id);
                                this.send(f);
                                cx.notify();
                            })),
                    ),
            );
        }
        list
    }

    fn toggle_member(&mut self, agent: &str, cx: &mut Context<Self>) {
        if let Some(f) = self.model.toggle_room_member(agent) {
            self.send(f);
        }
        cx.notify();
    }

    fn set_room_color(&mut self, color: &str, cx: &mut Context<Self>) {
        if let Some(f) = self.model.set_room_color(color) {
            self.send(f);
        }
        cx.notify();
    }

    fn rename_room(&mut self, cx: &mut Context<Self>) {
        let name = self.room_name_in.read(cx).value().trim().to_string();
        let Some(r) = self.model.room_changing().map(|r| r.id.clone()) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let f = self
            .model
            .update_room(&r, serde_json::json!({"name": name}));
        self.send(f);
        cx.notify();
    }

    /// Makes the group chat in the form: the chat shows it once it's made. Make does
    /// nothing while it waits, and turns on again if no answer comes in MAKE_WAIT.
    pub(super) fn make_room(&mut self, cx: &mut Context<Self>) {
        let name = self.room_name_in.read(cx).value().to_string();
        if let Some(f) = self.model.make_form_room(&name) {
            self.send(f);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(model::MAKE_WAIT).await;
                let _ = this.update(cx, |this, cx| {
                    if this.model.make_timed_out(Instant::now()) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Enter in the name field: makes the group chat, or keeps the new name.
    pub(super) fn room_name_entered(&mut self, cx: &mut Context<Self>) {
        if self.model.room_changing().is_some() {
            self.rename_room(cx);
        } else {
            self.make_room(cx);
        }
    }
}

impl Loom {
    /// Agent defaults' Group chats: the program the router runs on, and its model, from
    /// what an agent on that program says it offers.
    pub(super) fn router_group(&self, d: &model::Defaults, cx: &mut Context<Self>) -> AnyElement {
        let put = |args: Value| {
            cx.listener(move |this: &mut Loom, _: &ClickEvent, _: &mut Window, cx| {
                let f = this.model.put_defaults(args.clone());
                this.send(f);
                cx.notify();
            })
        };
        let programs = self.model.programs.clone();
        let mut items = vec![self
            .segment("router-brain-auto", "Claude", d.router_brain.is_empty())
            .on_click(put(serde_json::json!({"router_brain": null})))];
        for name in programs.clone() {
            items.push(
                self.segment(
                    format!("router-brain-{name}"),
                    &self.model.program_title(&name),
                    d.router_brain == name,
                )
                .on_click(put(serde_json::json!({"router_brain": name}))),
            );
        }
        let mut rows = vec![self
            .row(
                None,
                "Router",
                None,
                Some(self.segments(items).into_any_element()),
            )
            .into_any_element()];
        // Its models, as an agent on its program has them.
        let on = if d.router_brain.is_empty() {
            programs
                .iter()
                .find(|p| p.to_lowercase().contains("claude"))
                .cloned()
                .unwrap_or_else(|| d.brain.clone())
        } else {
            d.router_brain.clone()
        };
        let offered = self
            .model
            .agents
            .iter()
            .filter(|a| a.program == on)
            .flat_map(|a| a.options.iter())
            .find(|o| o.category == "model")
            .cloned();
        let models: AnyElement = match offered {
            Some(o) => {
                let mut row = div().flex().flex_wrap().gap(px(space::S));
                for (value, label) in o.values.clone() {
                    row = row.child(
                        self.small_button(
                            format!("router-model-{value}"),
                            &label,
                            d.router_model == value,
                        )
                        .on_click(put(serde_json::json!({"router_model": value}))),
                    );
                }
                row.into_any_element()
            }
            None => div()
                .text_size(px(size::SMALL))
                .text_color(self.p.ink_2)
                .child(d.router_model.clone())
                .into_any_element(),
        };
        rows.push(
            self.row(None, "Router model", None, Some(models))
                .into_any_element(),
        );
        self.group(Some("Group chats"), rows)
    }
}

/// Tokens used, as an agent's settings say them: "850 today · 12.4k this month".
fn usage_line(today: i64, month: i64) -> String {
    format!(
        "{} today · {} this month",
        model::tokens_text(today),
        model::tokens_text(month)
    )
}

fn members_line(n: usize) -> String {
    if n == 1 {
        "1 agent".into()
    } else {
        format!("{n} agents")
    }
}
