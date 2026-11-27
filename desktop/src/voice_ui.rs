//! Loom's voice in the window (PLAN.md, "Voice"): Settings › Voice, with its engines as
//! tiles and their rows under them; the composer's mic, which records while loom
//! dictates and puts the words the core hears in at the caret; and the voice_setup card
//! an agent puts in its reply. The core picks the engine, talks to the service and
//! writes the speech; this records, plays and draws (`crate::voice`).

use super::*;
use crate::model::voice::{self as voice_model, Action, Menu as VoiceMenu, Tab, Tile};

/// A menu of languages or voices, as tall as it may be before it scrolls.
const MENU_MOST: f32 = 240.0;
/// The ink square on a tile, with the engine's icon on it.
const MARK: f32 = 32.0;

/// The words of the Shortcut row's first choice.
pub(super) const SHORTCUT: &str = "Ctrl+Shift+Space";

/// Whether `k` is the dictation shortcut, Ctrl+Shift+Space.
pub(super) fn is_shortcut(k: &Keystroke) -> bool {
    k.modifiers.control && k.modifiers.shift && !k.modifiers.alt && k.key == "space"
}

/// The engine's maker's mark on its ink square, in the ink on it, as Spools draws an
/// entry's; where it has none, Cloud for a service and Laptop for a model on the device.
fn engine_mark(p: Palette, t: &Tile) -> Div {
    let square = div()
        .size(px(MARK))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::ROW))
        .bg(p.button)
        .text_color(p.button_ink);
    match &t.icon_svg {
        Some(mark) => square.child(
            svg()
                .path(crate::runtime_svg(&format!("voice/{}.svg", t.mark), mark))
                .size(px((MARK * 0.56).round()))
                .text_color(p.button_ink),
        ),
        None => square.child(
            Icon::new(if t.icon == "laptop" {
                Ico::Laptop
            } else {
                Ico::Cloud
            })
            .small(),
        ),
    }
}

/// "In use", with a check in a ring, in ink.
fn in_use_mark(p: Palette, scale: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::XS + 2.0))
        .text_size(px(size::SMALL * scale))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.ink)
        .child(
            div()
                .size(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(p.ink)
                .child(Icon::new(Ico::Check).xsmall()),
        )
        .child("In use")
}

/// A tile's head: its ink square, its name and its one quiet line.
fn tile_head(p: Palette, t: &Tile, scale: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::M))
        .child(engine_mark(p, t))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(size::BODY * scale))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.ink)
                        .truncate()
                        .child(t.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(size::SMALL * scale))
                        .text_color(p.ink_2)
                        .line_clamp(2)
                        .child(t.line.clone()),
                ),
        )
}

/// A tile's box: outlined in ink while it's in use.
fn tile_box(p: Palette, fill: Hsla, in_use: bool) -> Div {
    div()
        .min_w_0()
        .p(px(space::L))
        .flex()
        .flex_col()
        .gap(px(space::M))
        .justify_between()
        .rounded(px(radius::CARD))
        .border_1()
        .border_color(if in_use { p.ink } else { p.line })
        .bg(fill)
}

/// A setting and its value, read only, as a card shows them.
fn shown_row(p: Palette, name: &str, value: &str, scale: f32) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(space::M))
        .py(px(space::XS + 2.0))
        .text_size(px(size::SMALL * scale))
        .child(div().text_color(p.ink_2).child(name.to_string()))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink)
                .child(value.to_string()),
        )
}

impl Loom {
    /// Before Settings › Voice draws: the key field of the tile it's open on, and the
    /// voices of the speech engine in use, asked for once.
    pub(super) fn ready_voice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (&self.model.voice.adding, &self.voice_key) {
            (Some(_), None) => {
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder("Key").masked(true));
                cx.subscribe_in(
                    &input,
                    window,
                    |this: &mut Loom, _, event: &InputEvent, _, cx| {
                        if let InputEvent::PressEnter { .. } = event {
                            this.voice_save_key(cx);
                        }
                    },
                )
                .detach();
                input.update(cx, |s, cx| s.focus(window, cx));
                self.voice_key = Some(input);
            }
            (None, Some(_)) => self.voice_key = None,
            _ => {}
        }
        if let Some(f) = self.model.voice.want_voices() {
            let engine = self.model.voice.engine(Tab::Speech);
            self.model.voice.voices.insert(engine, Ok(Vec::new()));
            self.send(f);
        }
    }

    /// Opens Settings › Voice on `tab`.
    pub(super) fn open_voice(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.model.voice.tab = tab;
        self.model.voice.adding = None;
        self.model.voice.menu = None;
        self.open(Screen::Voice, cx);
    }

    /// Save, on a tile's key field.
    fn voice_save_key(&mut self, cx: &mut Context<Self>) {
        let (Some((tab, id)), Some(input)) =
            (self.model.voice.adding.clone(), self.voice_key.clone())
        else {
            return;
        };
        let key = input.read(cx).value().to_string();
        if key.trim().is_empty() {
            return;
        }
        for f in self.model.voice.save_key(tab, &id, &key) {
            self.send(f);
        }
        self.voice_key = None;
        cx.notify();
    }

    /// Voice's title with its tabs beside it: Dictation and Speech.
    fn voice_head(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let on = self.model.voice.tab;
        let tab = |id: &'static str, label: &'static str, t: Tab| {
            div()
                .id(id)
                .role(Role::Tab)
                .aria_label(label)
                .h(px(control::SEGMENT))
                .px(px(space::M))
                .flex()
                .items_center()
                .rounded(px(radius::ROW))
                .text_size(px(size::SMALL))
                .cursor_pointer()
                .when(on == t, |d| {
                    d.bg(p.active)
                        .text_color(p.ink)
                        .font_weight(FontWeight::MEDIUM)
                })
                .when(on != t, |d| d.soft_ink(p.ink_2, p.ink))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.voice.tab = t;
                    this.model.voice.adding = None;
                    this.model.voice.menu = None;
                    cx.notify();
                }))
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
                    .child("Voice"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .child(tab("voice-dictation", "Dictation", Tab::Dictation))
                    .child(tab("voice-speech", "Speech", Tab::Speech)),
            )
    }

    /// An engine as a tile: its head, then "In use" and its button, or the key field
    /// with Cancel and Save while it's open.
    fn voice_tile(&self, tab: Tab, t: &Tile, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let adding = self
            .model
            .voice
            .adding
            .as_ref()
            .is_some_and(|(k, id)| *k == tab && *id == t.id);
        let mut tile = tile_box(p, self.card_fill(), t.in_use).child(tile_head(p, t, 1.0));
        if adding {
            if let Some(input) = &self.voice_key {
                let cancel = self
                    .small_button(format!("voice-cancel-{}", t.id), "Cancel", false)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model.voice.adding = None;
                        this.voice_key = None;
                        cx.notify();
                    }));
                let save = self
                    .small_button(format!("voice-save-{}", t.id), "Save", true)
                    .on_click(cx.listener(|this, _, _, cx| this.voice_save_key(cx)));
                tile = tile
                    .child(
                        div()
                            .text_size(px(size::SMALL))
                            .child(Input::new(input).mask_toggle()),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(space::S))
                            .child(cancel)
                            .child(save),
                    );
                return tile.into_any_element();
            }
        }
        let button = t.action().map(|a| {
            let id = t.id.clone();
            self.small_button(
                format!("voice-{}-{}", a.label().to_lowercase(), t.id),
                a.label(),
                a == Action::Use,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                match a {
                    Action::Use => {
                        let f = this.model.voice.use_engine(tab, &id);
                        this.send(f);
                    }
                    Action::AddKey | Action::ChangeKey => {
                        this.model.voice.adding = Some((tab, id.clone()));
                        this.model.voice.menu = None;
                    }
                }
                cx.notify();
            }))
        });
        tile.child(
            div()
                .min_h(px(control::BUTTON_SMALL))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(space::S))
                .child(div().when(t.in_use, |d| d.child(in_use_mark(p, 1.0))))
                .children(button),
        )
        .into_any_element()
    }

    /// A row's picker: what's picked and a chevron, with the choices in a menu under it.
    fn voice_picker(
        &self,
        which: VoiceMenu,
        now: String,
        choices: Vec<(String, String)>,
        picked: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.p;
        let open = self.model.voice.menu == Some(which);
        let id = match which {
            VoiceMenu::Language => "voice-language",
            VoiceMenu::Voice => "voice-voice",
        };
        let button = div()
            .id(id)
            .role(Role::Button)
            .aria_label(now.clone())
            .h(px(control::BUTTON_SMALL))
            .px(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::S))
            .rounded(px(radius::BUTTON))
            .border_1()
            .border_color(p.line_strong)
            .cursor_pointer()
            .text_size(px(size::SMALL))
            .text_color(p.ink)
            .soft_hover(transparent_black(), p.hover, None)
            .child(now)
            .child(
                div()
                    .text_color(p.ink_3)
                    .child(Icon::new(Ico::ChevronDown).xsmall()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.voice.menu = if this.model.voice.menu == Some(which) {
                    None
                } else {
                    Some(which)
                };
                cx.notify();
            }));
        let menu = open.then(|| {
            let mut list = div()
                .id(SharedString::from(format!("{id}-menu")))
                .occlude()
                .w(px(layout::MENU))
                .max_h(px(MENU_MOST))
                .overflow_y_scroll()
                .p(px(space::XS))
                .flex()
                .flex_col()
                .bg(p.raised)
                .border_1()
                .border_color(p.line_strong)
                .rounded(px(radius::CARD))
                .shadow_lg()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.model.voice.menu = None;
                    cx.notify();
                }));
            for (n, (value, name)) in choices.into_iter().enumerate() {
                let on = value == picked;
                let picked_name = name.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("{id}-{n}")))
                        .role(Role::MenuItem)
                        .aria_label(name.clone())
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
                        .child(div().flex_1().child(name))
                        .when(on, |d| {
                            d.child(
                                div()
                                    .text_color(p.ink_2)
                                    .child(Icon::new(Ico::Check).xsmall()),
                            )
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.model.voice.menu = None;
                            let f = match which {
                                VoiceMenu::Language => this
                                    .model
                                    .voice
                                    .set(Tab::Dictation, serde_json::json!({"language": value})),
                                VoiceMenu::Voice => this.model.voice.set(
                                    Tab::Speech,
                                    serde_json::json!({"voice": value, "voice_name": picked_name}),
                                ),
                            };
                            this.send(f);
                            cx.notify();
                        })),
                );
            }
            list
        });
        div()
            .relative()
            .child(button)
            .when_some(menu, |d, menu| {
                d.child(
                    div()
                        .absolute()
                        .top(px(control::BUTTON_SMALL + 4.0))
                        .right_0()
                        .child(deferred(
                            anchored()
                                .anchor(Anchor::TopRight)
                                .snap_to_window_with_margin(px(space::S))
                                .child(menu),
                        )),
                )
            })
            .into_any_element()
    }

    /// The rows of Dictation: Language, Shortcut and Hold to talk.
    fn dictation_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let v = &self.model.voice;
        let languages = v.languages.clone();
        let language = self.voice_picker(
            VoiceMenu::Language,
            v.language_name(),
            languages,
            v.setting(Tab::Dictation, "language"),
            cx,
        );
        let shortcut = self.pref_choice(
            "voice-shortcut",
            self.prefs.dictation_shortcut,
            &[(true, SHORTCUT), (false, "Off")],
            |p, v| p.dictation_shortcut = v,
            cx,
        );
        vec![
            self.row(None, "Language", None, Some(language))
                .into_any_element(),
            self.row(None, "Shortcut", None, Some(shortcut))
                .into_any_element(),
            self.pref_switch(
                "voice-hold",
                "Hold to talk",
                "",
                self.prefs.hold_to_talk,
                |p, v| p.hold_to_talk = v,
                cx,
            ),
        ]
    }

    /// The rows of Speech: Voice, with its preview, Speed and Read replies aloud.
    fn speech_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.p;
        let v = &self.model.voice;
        let mut rows = Vec::new();
        let engine = v.engine(Tab::Speech);
        if !engine.is_empty() {
            let picked = v.setting(Tab::Speech, "voice");
            let (choices, why) = match v.voices.get(&engine) {
                Some(Ok(list)) => (list.clone(), None),
                Some(Err(e)) => (Vec::new(), Some(e.clone())),
                None => (Vec::new(), None),
            };
            let now = v.voice_name();
            let id = if picked.is_empty() {
                choices
                    .first()
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default()
            } else {
                picked.clone()
            };
            let picker = self.voice_picker(
                VoiceMenu::Voice,
                if now.is_empty() {
                    "…".into()
                } else {
                    now.clone()
                },
                choices,
                picked,
                cx,
            );
            let play = div()
                .id("voice-preview")
                .role(Role::Button)
                .aria_label("Play")
                .size(px(control::BUTTON_SMALL))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::BUTTON))
                .border_1()
                .border_color(p.line_strong)
                .text_color(p.ink)
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .tooltip(tip("Play"))
                .child(Icon::new(Ico::Play).xsmall())
                .on_click(cx.listener(move |this, _, _, _| {
                    let f = this.model.voice.preview(&id, &now);
                    this.send(f);
                }));
            rows.push(
                self.row(
                    None,
                    "Voice",
                    why,
                    Some(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(picker)
                            .child(play)
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        let speed = v.speed();
        let mut speeds = Vec::new();
        for s in voice_model::SPEEDS {
            let label = voice_model::speed_label(s);
            speeds.push(
                self.segment(
                    format!("voice-speed-{label}"),
                    &label,
                    (speed - s).abs() < 0.05,
                )
                .on_click(cx.listener(move |this, _, _, _| {
                    let f = this
                        .model
                        .voice
                        .set(Tab::Speech, serde_json::json!({"speed": s}));
                    this.send(f);
                })),
            );
        }
        rows.push(
            self.row(
                None,
                "Speed",
                None,
                Some(self.segments(speeds).into_any_element()),
            )
            .into_any_element(),
        );
        let read = v.setting(Tab::Speech, "read_aloud");
        let mut reads = Vec::new();
        for (value, label) in [
            ("never", "Never"),
            ("live", "Only in live"),
            ("always", "Always"),
        ] {
            let on = read == value || (read.is_empty() && value == "never");
            reads.push(
                self.segment(format!("voice-read-{value}"), label, on)
                    .on_click(cx.listener(move |this, _, _, _| {
                        let f = this
                            .model
                            .voice
                            .set(Tab::Speech, serde_json::json!({"read_aloud": value}));
                        this.send(f);
                    })),
            );
        }
        rows.push(
            self.wrapping_row(
                "Read replies aloud",
                self.segments(reads).into_any_element(),
            )
            .into_any_element(),
        );
        rows
    }

    /// Settings › Voice: its title and tabs, the tab's engines as tiles, two to a row,
    /// and its rows under them.
    pub(super) fn voice_page(&mut self, cx: &mut Context<Self>) -> Div {
        let tab = self.model.voice.tab;
        let mut column = div()
            .w_full()
            .max_w(px(layout::SETTINGS))
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .child(self.voice_head(cx));
        if !self.model.voice.came {
            let body = match self.model.voice.said.clone() {
                Some(e) => self.empty(Ico::Mic, &e, None, None),
                None => div().into_any_element(),
            };
            column = column.child(body);
        } else {
            let tiles: Vec<Tile> = self.model.voice.tiles(tab).to_vec();
            let mut grid = div().grid().grid_cols(2).gap(px(space::M));
            for t in &tiles {
                grid = grid.child(self.voice_tile(tab, t, cx));
            }
            let rows = match tab {
                Tab::Dictation => self.dictation_rows(cx),
                Tab::Speech => self.speech_rows(cx),
            };
            column = column.child(grid).child(self.group(None, rows));
        }
        let page = div()
            .id("voice-page")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(column);
        div().flex_1().min_h_0().child(page)
    }

    /// The composer's mic: a click starts loom's dictation and another stops it, or with
    /// Hold to talk, it records while it's held. Lit while it hears.
    pub(super) fn composer_mic(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let on = self.mic.is_some();
        let hold = self.prefs.hold_to_talk;
        let name = if on { "Stop dictating" } else { "Dictate" };
        div()
            .id("dictate")
            .role(Role::Button)
            .aria_label(name)
            .size(px(control::ICON_COMPOSER))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .cursor_pointer()
            .when(on, |d| {
                d.text_color(p.button_ink).soft_hover(
                    p.button,
                    motion::mix(p.button, p.bg, 0.18),
                    None,
                )
            })
            .when(!on, |d| {
                d.text_color(p.ink_2)
                    .soft_hover(transparent_black(), p.hover, None)
            })
            .tooltip(tip(if hold { "Hold to dictate" } else { "Dictate" }))
            .child(Icon::new(Ico::Mic).small())
            .when(hold, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.start_dictation(window, cx)),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.stop_dictation(cx)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.stop_dictation(cx)),
                )
            })
            .when(!hold, |d| {
                d.on_click(cx.listener(|this, _, window, cx| this.toggle_dictation(window, cx)))
            })
    }

    /// The mic, or the shortcut, pressed without Hold to talk.
    pub(super) fn toggle_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mic.is_some() {
            self.stop_dictation(cx);
        } else {
            self.start_dictation(window, cx);
        }
    }

    /// Starts loom's dictation at the composer's caret: the mic records, and the core
    /// hears it with the engine in use. With none, Settings › Voice opens instead.
    pub(super) fn start_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mic.is_some() {
            return;
        }
        let v = &self.model.voice;
        if v.came && v.engine(Tab::Dictation).is_empty() {
            self.open_voice(Tab::Dictation, cx);
            return;
        }
        match crate::voice::Mic::start() {
            Ok(mic) => {
                let at = self.composer.read(cx).cursor();
                let f = self.model.voice.listen(at);
                self.send(f);
                self.mic = Some(mic);
                self.composer.update(cx, |c, cx| c.focus(window, cx));
            }
            Err(why) => {
                self.model.notices.push(why);
                self.take_notice(cx);
            }
        }
        cx.notify();
    }

    /// Stops the mic: the last of what it heard goes to the core, which writes the last
    /// words.
    pub(super) fn stop_dictation(&mut self, cx: &mut Context<Self>) {
        let Some(mic) = self.mic.take() else {
            return;
        };
        for f in mic.stop() {
            self.send(f);
        }
        self.send(serde_json::json!({"t": "voice.stop"}));
        if let Some(d) = &mut self.model.voice.dictating {
            if d.state == "listening" {
                d.state = "finishing".into();
            }
        }
        cx.notify();
    }

    /// What the mic heard since, to the core; and the mic let go once the core says
    /// dictation ended, such as when it failed.
    pub(super) fn pump_mic(&mut self) {
        let Some(mic) = &self.mic else {
            return;
        };
        let frames = mic.frames();
        let ended = self.model.voice.dictating.is_none()
            || self
                .model
                .voice
                .dictating
                .as_ref()
                .is_some_and(|d| d.state == "failed" || d.state == "done");
        for f in frames {
            self.send(f);
        }
        if ended {
            self.mic = None;
        }
    }

    /// Before the window draws: dictation's words into the composer at their place,
    /// Settings › Voice opened when dictation has no engine, and why voice failed.
    pub(super) fn take_voice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.voice.words.is_some() {
            let text = self.composer.read(cx).value().to_string();
            if let Some((range, put)) = self.model.voice.composer_words(&text) {
                self.composer.update(cx, |c, cx| {
                    c.set_selected_range(range, cx);
                    c.replace(put, window, cx);
                });
            }
        }
        if std::mem::take(&mut self.model.voice.open_page) {
            self.mic = None;
            let tab = self.model.voice.tab;
            self.open_voice(tab, cx);
        }
        if let Some(why) = self.model.voice.said.take() {
            if !why.is_empty() {
                self.model.notices.push(why);
                self.take_notice(cx);
            }
        }
    }
}

/// Speech the core wrote, played, or the speech playing stopped (`core.speak`).
pub(super) fn play(v: &Value) {
    if v.get("stop").and_then(Value::as_bool) == Some(true) {
        crate::voice::hush();
    } else if let Some(file) = v.get("file").and_then(Value::as_str) {
        crate::voice::speak(file);
    }
}

/// Loom's own card for an engine an agent set up (voice_setup), in the cards' frame:
/// the engine's tile and its rows as the agent set them, read only, and while it waits
/// for the person's key, the key field, masked, with Add; and Voice settings, to the
/// page.
pub(super) fn setup_card(
    p: Palette,
    card: &voice_model::Card,
    key: &str,
    tint: Hsla,
    scale: f32,
    secrets: &HashMap<String, Entity<InputState>>,
    this: WeakEntity<Loom>,
) -> Div {
    let fill = if p.dark { p.raised_2 } else { p.bg };
    let tile = tile_box(p, fill, !card.asking)
        .child(tile_head(p, &card.tile, scale))
        .when(!card.asking, |d| d.child(in_use_mark(p, scale)));
    let mut rows = div().flex().flex_col();
    for (i, (name, value)) in card.rows.iter().enumerate() {
        rows = rows.child(
            shown_row(p, name, value, scale).when(i > 0, |d| d.border_t_1().border_color(p.line)),
        );
    }
    let kind = card.kind;
    let settings = {
        let this = this.clone();
        div()
            .id(SharedString::from(format!("voice-card-settings-{key}")))
            .role(Role::Button)
            .aria_label("Voice settings")
            .flex()
            .items_center()
            .gap(px(space::XS))
            .text_size(px(size::SMALL * scale))
            .cursor_pointer()
            .soft_ink(p.ink_2, p.ink)
            .child("Voice settings")
            .child(Icon::new(Ico::ArrowRight).xsmall())
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |loom, cx| loom.open_voice(kind, cx));
            })
    };
    let mut frame = CardFrame::new()
        .title(card.title.clone())
        .when_waits(card.asking, tint)
        .row(tile);
    if !card.rows.is_empty() {
        frame = frame.row(rows);
    }
    if card.asking {
        if let Some(input) = secrets.get(&voice_key_name(&card.id)) {
            frame = frame.row(
                div()
                    .text_size(px(size::SMALL))
                    .child(Input::new(input).mask_toggle()),
            );
            let add = {
                let (card, input) = (card.clone(), input.clone());
                card_button(p, format!("voice-card-add-{key}"), "Add", true).on_click(
                    move |_, _, cx| {
                        let typed = input.read(cx).value().to_string();
                        if typed.trim().is_empty() {
                            return;
                        }
                        let _ = this.update(cx, |loom, _| {
                            loom.send(voice_model::card_answer(&card, &typed))
                        });
                    },
                )
            };
            frame = frame.action(add);
        }
    }
    div()
        .max_w(px(CARD_MOST))
        .child(frame.foot(settings).draw(p, scale))
}

/// The key field's name on a voice_setup card, among the cards' fields.
pub(super) fn voice_key_name(card: &str) -> String {
    format!("{card}/key")
}
