//! Live mode in the window (PLAN.md, "Live mode"): talking with an agent, in place of the
//! chat's header, messages and composer while it runs, with the sidebar beside it. One
//! thread is held at both ends across the view, waving in ink by the level of what
//! plays while the agent speaks, moving lightly in grey by the mic's while it listens,
//! and lying nearly still while it thinks; under it the words being said, and Text,
//! Mute and End. The core does the listening, the turn-taking and the speaking
//! (core/live.calx); this records, plays and draws (`crate::voice`, `model::live`).

use super::*;
use crate::model::live::{self as live_model, Live, Main, State as LiveState};
use crate::model::voice::{self as voice_model, Tab};

/// Voice mode's icon on the composer's main button: one wave, as the thread live mode
/// draws, on Lucide's 24 grid with its stroke (LOO-443).
const THREAD_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12c2.5 0 3-5 6-5s3.5 10 7 10 3.5-5 7-5"/></svg>"#;
/// The main button's icons, as big as `Icon::small`, and how long one takes to swap.
const ICON: f32 = 14.0;
const ICON_SWAP: f32 = 0.13;

/// The room the thread has to swing in, and its share of the view's height.
const THREAD_ROOM: f32 = 220.0;
const SWING: f32 = 0.36;
/// The points along it, and its stroke.
const POINTS: usize = 120;
const STROKE: f32 = 1.6;
/// The round buttons at the bottom, and the words' size.
const ROUND: f32 = 46.0;
const WORDS: f32 = 15.0;
const WORDS_MOST: f32 = 560.0;

impl Loom {
    /// Whether live mode takes the chat's place now: it's on, in the chat open.
    pub(super) fn live_shown(&self) -> bool {
        let l = &self.model.talk;
        l.on && self.screen == Screen::Chat
            && self
                .model
                .conversation()
                .is_some_and(|c| c == l.conversation)
    }

    /// The composer's main button (LOO-443): voice mode while the composer is empty,
    /// which starts live mode with the agent here, Send once there's something to send,
    /// and Stop while the agent answers. Each icon fades and grows into the next, in the
    /// same place.
    pub(super) fn composer_main(&self, main: Main, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let tip_words = match main {
            Main::Live if self.prefs.live_shortcut => {
                if cfg!(target_os = "macos") {
                    "Start voice mode  ⌃⇧Space"
                } else {
                    "Start voice mode  Ctrl+Shift+Space"
                }
            }
            _ => main.name(),
        };
        // How far each icon is in: its opacity, and its size from 60% to whole.
        let level = |which: Main| {
            motion::level(
                &SharedString::from(format!("composer-main-{which:?}")),
                main == which,
                ICON_SWAP,
            )
        };
        let grow = |k: f32| Transformation::scale(size(0.6 + 0.4 * k, 0.6 + 0.4 * k));
        let layer = |k: f32, icon: AnyElement| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .opacity(k)
                .child(icon)
        };
        let (stop, send, live) = (level(Main::Stop), level(Main::Send), level(Main::Live));
        div()
            .id("send")
            .role(Role::Button)
            .aria_label(main.name())
            .relative()
            .size(px(control::ICON_COMPOSER))
            .flex_none()
            .rounded(px(radius::BUTTON))
            .soft_hover(p.button, motion::mix(p.button, p.bg, 0.18), None)
            .text_color(p.button_ink)
            .cursor_pointer()
            .tooltip(tip(tip_words))
            .when(stop > 0.0, |d| {
                d.child(layer(
                    stop,
                    Icon::new(IconName::Square)
                        .small()
                        .transform(grow(stop))
                        .into_any_element(),
                ))
            })
            .when(send > 0.0, |d| {
                d.child(layer(
                    send,
                    Icon::new(IconName::ArrowUp)
                        .small()
                        .transform(grow(send))
                        .into_any_element(),
                ))
            })
            .when(live > 0.0, |d| {
                d.child(layer(
                    live,
                    svg()
                        .path(crate::runtime_svg("loom/thread.svg", THREAD_ICON))
                        .flex_none()
                        .size(px(ICON))
                        .text_color(p.button_ink)
                        .with_transformation(grow(live))
                        .into_any_element(),
                ))
            })
            .on_click(cx.listener(move |this, _, window, cx| match main {
                Main::Stop => {
                    if let Some(f) = this.model.cancel() {
                        this.send(f);
                    }
                }
                Main::Live => this.start_live(cx),
                Main::Send => {
                    let text = composer_text(this.composer.read(cx));
                    if let Some(frame) = this.model.send(&text) {
                        this.send(frame);
                        this.composer
                            .update(cx, |s, cx| s.set_value("", window, cx));
                        if this.emoji_ui.picker.is_some() {
                            this.close_emoji_picker(cx);
                        }
                    }
                }
            }))
    }

    /// Starts live mode with the agent open, in its chat. Dictation stops first.
    pub(super) fn start_live(&mut self, cx: &mut Context<Self>) {
        let (Some(agent), Some(conversation)) =
            (self.model.selected.clone(), self.model.conversation())
        else {
            return;
        };
        self.stop_dictation(cx);
        self.send(Live::start(&agent, &conversation));
        cx.notify();
    }

    /// Ends live mode: the chat comes back, with the talk in it.
    pub(super) fn end_live(&mut self, cx: &mut Context<Self>) {
        self.send(Live::end());
        self.model.talk.on = false;
        self.live_mic = None;
        crate::voice::hush();
        cx.notify();
    }

    /// The live shortcut: starts it, or ends it.
    pub(super) fn toggle_live(&mut self, cx: &mut Context<Self>) {
        if self.model.talk.on {
            self.end_live(cx);
        } else if self.screen == Screen::Chat {
            self.start_live(cx);
        }
    }

    /// Before the window draws: speech to play or stop, the mic on while live mode
    /// listens and off while it's muted or over, Settings › Voice opened when it has no
    /// engine, and why something failed.
    pub(super) fn take_live(&mut self, cx: &mut Context<Self>) {
        self.sample_live();
        // Another chat opened: live mode ends, as it's this chat's.
        if self.model.talk.on
            && !self.live_sample
            && self.screen == Screen::Chat
            && self
                .model
                .conversation()
                .is_some_and(|c| c != self.model.talk.conversation)
        {
            self.end_live(cx);
        }
        let l = &mut self.model.talk;
        if std::mem::take(&mut l.stop) {
            crate::voice::hush();
        }
        if let Some(file) = l.play.take() {
            if !file.is_empty() {
                crate::voice::speak(&file);
            }
        }
        let wants_mic = l.on && !l.muted && !self.live_sample;
        if wants_mic && self.live_mic.is_none() {
            // The meter lets the mic go first.
            self.meter = None;
            match self.start_mic(cx) {
                Ok(mic) => self.live_mic = Some(mic),
                Err(why) => {
                    self.model.notices.push(why);
                    self.take_notice(cx);
                    self.end_live(cx);
                }
            }
        } else if !wants_mic && self.live_mic.is_some() {
            self.live_mic = None;
        }
        // As dictation does, it opens the page without a word: the page says the rest.
        if let Some((kind, _)) = self.model.talk.open_page.take() {
            self.live_mic = None;
            let tab = if kind == "speech" {
                Tab::Speech
            } else {
                Tab::Dictation
            };
            self.open_voice(tab, cx);
        }
        if let Some(why) = self.model.talk.said.take() {
            if !why.is_empty() {
                self.model.notices.push(why);
                self.take_notice(cx);
            }
        }
    }

    /// What the mic heard since, to the core, while live mode listens.
    pub(super) fn pump_live_mic(&mut self) {
        let Some(mic) = &self.live_mic else {
            return;
        };
        for f in mic.frames() {
            self.send(f);
        }
    }

    /// The perf build's look at live mode with no mic or service (`LOOM_LIVE=speaking`
    /// or `listening`): made-up frames, once a chat is open, and again as they run out.
    fn sample_live(&mut self) {
        if !cfg!(feature = "perf") {
            return;
        }
        let Ok(state) = std::env::var("LOOM_LIVE") else {
            return;
        };
        let (Some(agent), Some(conversation)) =
            (self.model.selected.clone(), self.model.conversation())
        else {
            return;
        };
        let now = Instant::now();
        let l = &self.model.talk;
        let stale = !l.on || (state == "speaking" && l.playing(now).is_none());
        if !stale {
            return;
        }
        let name = self
            .model
            .agent(&agent)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        self.live_sample = true;
        for f in live_model::sample(&state, &agent, &name, &conversation) {
            self.model.talk.take_at(&f, now);
        }
        self.model.talk.play = None;
    }

    /// The header while live mode runs: the agent's name, what it's doing in quiet ink,
    /// and the time it has run.
    pub(super) fn live_header(&self) -> Div {
        let p = self.p;
        let l = &self.model.talk;
        let now = Instant::now();
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .overflow_hidden()
            .px(px(space::L))
            .gap(px(space::M))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(px(space::S))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(size::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.ink)
                            .child(l.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_3)
                            .child(if l.muted && l.state == LiveState::Listening {
                                "Muted"
                            } else {
                                l.state.word()
                            }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child(l.clock(now)),
            )
    }

    /// One of the round buttons at the bottom.
    fn live_button(
        &self,
        id: &'static str,
        icon: Ico,
        name: &'static str,
        lit: bool,
        inked: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(name)
            .size(px(ROUND))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .tooltip(tip(name))
            .when(inked, |d| {
                d.text_color(p.button_ink).soft_hover(
                    p.button,
                    motion::mix(p.button, p.bg, 0.18),
                    None,
                )
            })
            .when(!inked, |d| {
                d.border_1()
                    .border_color(p.line_strong)
                    .text_color(if lit { p.ink } else { p.ink_2 })
                    .soft_hover(
                        if lit { p.active } else { transparent_black() },
                        p.hover,
                        None,
                    )
            })
            .child(Icon::new(icon).small())
    }

    /// The live view, in the place of the chat's messages and composer.
    pub(super) fn live_view(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let now = Instant::now();
        let swing = self.model.talk.swing(now);
        let inked = self.model.talk.inked(now);
        let (words, said) = self.model.talk.caption(now);
        let show_words = self.model.talk.show_words;
        let muted = self.model.talk.muted;
        let reduce = cx.reduce_motion();
        // Each frame draws again while it runs: the thread, the words and the clock.
        let me = cx.entity_id();
        let ink = if inked { p.ink } else { p.ink_3 };
        let started = self.live_t0;
        let thread = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if !reduce {
                    window.request_animation_frame();
                }
                window.on_next_frame(move |_, cx| cx.notify(me));
                let (x0, w) = (bounds.origin.x.as_f32(), bounds.size.width.as_f32());
                let (y0, h) = (bounds.origin.y.as_f32(), bounds.size.height.as_f32());
                let cy = y0 + h / 2.0;
                let reach = h * SWING;
                let t = if reduce {
                    0.0
                } else {
                    started.elapsed().as_secs_f32()
                };
                let mut pb = PathBuilder::stroke(px(STROKE));
                for i in 0..=POINTS {
                    let u = i as f32 / POINTS as f32;
                    let y = cy + swing * reach * live_model::wave(u, t);
                    let pt = point(px(x0 + u * w), px(y));
                    if i == 0 {
                        pb.move_to(pt)
                    } else {
                        pb.line_to(pt)
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, ink);
                }
            },
        )
        .w_full()
        .h(px(THREAD_ROOM));
        let caption = div()
            .w_full()
            .max_w(px(WORDS_MOST))
            .min_h(px(WORDS * 1.6 * 3.0))
            .flex()
            .flex_wrap()
            .justify_center()
            .content_start()
            .gap_x(px(WORDS * 0.28))
            .text_size(px(WORDS))
            .line_height(relative(1.6))
            .when(show_words, |d| {
                d.children(words.into_iter().enumerate().map(|(i, w)| {
                    div()
                        .text_color(if i < said { p.ink } else { p.ink_3 })
                        .child(w)
                }))
            });
        let text = self
            .live_button("live-text", Ico::TextAlignStart, "Text", show_words, false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.model.talk.show_words = !this.model.talk.show_words;
                cx.notify();
            }));
        let mute = self
            .live_button(
                "live-mute",
                Ico::MicOff,
                if muted { "Unmute" } else { "Mute" },
                muted,
                false,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                let f = this.model.talk.mute();
                this.model.talk.muted = !this.model.talk.muted;
                this.send(f);
                cx.notify();
            }));
        let end = self
            .live_button("live-end", Ico::X, "End", false, true)
            .on_click(cx.listener(|this, _, _, cx| this.end_live(cx)));
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .px(px(space::XXL))
            .bg(p.bg)
            .child(div().flex_1())
            .child(thread)
            .child(caption)
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::L))
                    .pb(px(space::XXL))
                    .child(text)
                    .child(mute)
                    .child(end),
            )
    }

    /// The rows of the Voice page's Live tab: Microphone, Shortcut, Talk over a reply to
    /// stop it, Send when I stop talking, and Show what's said.
    pub(super) fn live_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let v = &self.model.voice;
        let shortcut = self.pref_choice(
            "live-shortcut",
            self.prefs.live_shortcut,
            &[(true, voice_ui::SHORTCUT), (false, "Off")],
            |p: &mut crate::prefs::Prefs, v: bool| {
                p.live_shortcut = v;
                if v {
                    p.dictation_shortcut = false;
                }
            },
            cx,
        );
        let barge = v.live_switch("barge_in");
        let captions = v.live_switch("captions");
        let pause = v.pause();
        let setting = |id: &'static str, key: &'static str, on: bool, cx: &mut Context<Self>| {
            self.switch(id, on)
                .on_click(cx.listener(move |this, _, _, _| {
                    let f = this
                        .model
                        .voice
                        .set(Tab::Live, serde_json::json!({ key: !on }));
                    this.send(f);
                }))
                .into_any_element()
        };
        let barge_switch = setting("live-barge", "barge_in", barge, cx);
        let captions_switch = setting("live-captions", "captions", captions, cx);
        let mut pauses = Vec::new();
        for s in voice_model::PAUSES {
            let label = format!("After {s} s");
            pauses.push(
                self.segment(format!("live-pause-{s}"), &label, (pause - s).abs() < 0.05)
                    .on_click(cx.listener(move |this, _, _, _| {
                        let f = this
                            .model
                            .voice
                            .set(Tab::Live, serde_json::json!({"pause": s}));
                        this.send(f);
                    })),
            );
        }
        vec![
            self.mic_row(cx),
            self.row(None, "Shortcut", None, Some(shortcut))
                .into_any_element(),
            self.row(
                None,
                "Talk over a reply to stop it",
                None,
                Some(barge_switch),
            )
            .into_any_element(),
            self.wrapping_row(
                "Send when I stop talking",
                self.segments(pauses).into_any_element(),
            )
            .into_any_element(),
            self.row(None, "Show what's said", None, Some(captions_switch))
                .into_any_element(),
        ]
    }
}
