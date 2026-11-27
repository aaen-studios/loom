//! The settings that belong to this device (`crate::prefs`): their pages in the popup,
//! and putting a change into effect at once.

use super::*;

/// One of the person's pages in the popup: its id, icon, name and screen.
pub(super) type Page = (&'static str, Ico, &'static str, Screen);

/// The person's pages in the popup, under their headings.
pub(super) fn settings_groups() -> Vec<(Option<&'static str>, Vec<Page>)> {
    vec![
        (
            Some("You"),
            vec![
                (
                    "pop-profile",
                    Ico::CircleUserRound,
                    "About you",
                    Screen::Profile,
                ),
                (
                    "pop-appearance",
                    Ico::Palette,
                    "Appearance",
                    Screen::Appearance,
                ),
                ("pop-chat", Ico::MessageSquare, "Chat", Screen::ChatPrefs),
                (
                    "pop-emoji",
                    Ico::FaceSlightlySmiling,
                    "Emoji",
                    Screen::Emoji,
                ),
                (
                    "pop-app",
                    Ico::Bell,
                    "App & notifications",
                    Screen::AppPrefs,
                ),
            ],
        ),
        (
            Some("Agents"),
            vec![
                (
                    "pop-defaults",
                    Ico::SlidersHorizontal,
                    "Defaults",
                    Screen::Defaults,
                ),
                ("pop-rules", Ico::ShieldCheck, "Rules", Screen::Rules),
                ("pop-programs", Ico::Brain, "Brains", Screen::Programs),
                ("pop-skills", Ico::BookOpen, "Skills", Screen::Skills),
                (
                    "pop-proposals",
                    Ico::Lightbulb,
                    "Proposals",
                    Screen::Proposals,
                ),
                ("pop-spools", Ico::Spool, "Spools", Screen::Spools),
                ("pop-mcp", Ico::Plug, "MCP servers", Screen::Mcp),
                ("pop-secrets", Ico::KeyRound, "Secrets", Screen::Secrets),
                ("pop-plugins", Ico::Puzzle, "Plugins", Screen::Plugins),
            ],
        ),
        (
            Some("Work"),
            vec![
                ("pop-routines", Ico::Clock, "Routines", Screen::Routines),
                ("pop-tasks", Ico::ListTodo, "Tasks", Screen::Tasks),
            ],
        ),
        (
            None,
            vec![("pop-import", Ico::Import, "Import", Screen::Import)],
        ),
    ]
}

/// How a page lays out in the popup (PLAN.md, "The settings popup"): the pages of things
/// to pick from as a list beside the one picked, Emoji's tiles and Spools' Find across
/// the whole width, and every other page as a column of rows.
pub(super) fn page_kind(screen: Screen) -> crate::model::popup::Kind {
    use crate::model::popup::Kind;
    match screen {
        Screen::Mcp | Screen::Skills | Screen::Proposals | Screen::Routines | Screen::Installed => {
            Kind::List
        }
        Screen::Emoji | Screen::Spools => Kind::Grid,
        _ => Kind::Column,
    }
}

/// What each of the person's pages has on it, for search to find besides its name.
fn page_words(screen: Screen) -> &'static str {
    match screen {
        Screen::Profile => "profile you name instructions what agents know",
        Screen::Appearance => "theme dark light mode system colours colors text size font fonts geist code monospace bigger smaller",
        Screen::ChatPrefs => {
            "tool calls tools hidden folded thoughts thinking context ring compacted compaction fresh session send enter ctrl composer replies messages bubbles plain style vibrant colour color search sidebar find width wide narrow comfortable full column"
        }
        Screen::AppPrefs => {
            "start login startup tray close background awake sleep notifications sound preview updates version restart"
        }
        Screen::Programs => "brains models agent programs claude code codex gemini opencode acp registry install tokens limits share",
        Screen::Defaults => {
            "defaults every agent brain backup model effort permission mode ask allow instructions busy queue steer group chats router haiku"
        }
        Screen::Rules => {
            "rules allow deny ask what if permissions site path file folder command shell tool webhook routine"
        }
        Screen::Skills => "skills how to",
        Screen::Proposals => "proposals proposed suggestions suggested keep drop notes skills learn teach recorded",
        Screen::Mcp => "mcp servers tools connectors",
        Screen::Spools => {
            "spools integrations catalog connect installed mcp servers acta t3 github notion linear vercel cloudflare playwright home assistant"
        }
        Screen::Secrets => "secrets keys passwords sites bank sealed",
        Screen::Plugins => "plugins panels",
        Screen::Emoji => "emoji emojis custom upload skin tone animated gif emoticons smiley twemoji",
        Screen::Routines => "routines schedule every day hours times weekdays cron webhook history runs calendar week",
        Screen::Tasks => "tasks jobs long work",
        Screen::Import => "import openbot grok bring over",
        _ => "",
    }
}

/// The person's pages that `query` finds, by their name or what's on them.
pub(super) fn settings_found(query: &str) -> Vec<Page> {
    let q = query.trim().to_lowercase();
    settings_groups()
        .into_iter()
        .flat_map(|(_, pages)| pages)
        .filter(|(_, _, label, screen)| {
            q.split_whitespace().all(|word| {
                label.to_lowercase().contains(word) || page_words(*screen).contains(word)
            })
        })
        .collect()
}

/// Whether the system is dark.
pub(super) fn system_dark(window: &Window) -> bool {
    !matches!(
        window.appearance(),
        WindowAppearance::Light | WindowAppearance::VibrantLight
    )
}

impl Loom {
    /// Changes the prefs with `change`, keeps them, and puts them into effect.
    pub(super) fn set_prefs(
        &mut self,
        change: impl FnOnce(&mut Prefs),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        change(&mut self.prefs);
        self.prefs.save(&data_dir());
        crate::prefs::use_fonts(&self.prefs);
        crate::theme(cx, self.p.dark);
        self.apply_theme(window, cx);
        let ctrl_enter = self.prefs.ctrl_enter;
        self.composer
            .update(cx, |s, cx| s.set_submit_on_enter(!ctrl_enter, cx));
        shell::keep_in_tray(self.prefs.tray);
        self.refresh_rows(cx);
        self.scroller.update(cx, |s, cx| s.remeasure(cx));
        cx.notify();
    }

    /// Keeps where the window is now, when that changed, so it opens there next time.
    pub(super) fn keep_place(&mut self) {
        if let Some(p) = shell::window_place(self.hwnd) {
            if self.place != Some(p) {
                p.save(&data_dir());
                self.place = Some(p);
            }
        }
    }

    /// The colours, as the prefs and the system have them.
    pub(super) fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dark = self.prefs.dark(system_dark(window));
        if dark != self.p.dark {
            self.p = Palette::new(dark);
            crate::theme(cx, dark);
            shell::follow_theme(self.hwnd, dark);
            cx.notify();
        }
    }

    /// What the prefs ask of the system while loom runs: keeping the computer awake
    /// while an agent works, and updating on its own while the person is away.
    pub(super) fn tick_prefs(&mut self, cx: &mut Context<Self>) {
        let working =
            !self.model.runs.is_empty() || self.away.iter().any(|a| !a.model.runs.is_empty());
        let awake = self.prefs.awake && working;
        if awake != self.awake {
            self.awake = awake;
            shell::keep_awake(awake);
        }
        // Not a release that didn't start the last time it went in: the update icon
        // still offers it, saying why.
        if self.prefs.auto_update
            && !working
            && self.hub().update_auto
            && !shell::window_shown(self.hwnd)
        {
            if let Some((_, path)) = self.hub().update.clone() {
                // Once: if the swap fails, the sidebar still offers it.
                self.prefs.auto_update = false;
                match shell::apply_update(std::path::Path::new(&path), true) {
                    Ok(()) => cx.quit(),
                    Err(e) => self
                        .model
                        .notices
                        .push(format!("loom couldn't update itself: {e}")),
                }
            }
        }
    }

    /// A choice of a few for the prefs, side by side.
    pub(super) fn pref_choice<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        now: T,
        choices: &[(T, &'static str)],
        set: fn(&mut Prefs, T),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = Vec::new();
        for &(value, label) in choices {
            items.push(
                self.segment(format!("{id}-{label}"), label, now == value)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_prefs(|p| set(p, value), window, cx);
                    })),
            );
        }
        self.segments(items).into_any_element()
    }

    /// A row with a switch for one of the prefs.
    pub(super) fn pref_switch(
        &self,
        id: &'static str,
        title: &str,
        sub: &str,
        on: bool,
        set: fn(&mut Prefs, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let toggle = self
            .switch(id, on)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_prefs(|p| set(p, !on), window, cx);
            }));
        self.row(
            None,
            title.to_string(),
            (!sub.is_empty()).then(|| sub.to_string()),
            Some(toggle.into_any_element()),
        )
        .into_any_element()
    }

    /// Appearance: the theme, and how big the chat's words are.
    pub(super) fn appearance_page(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.pref_choice(
            "theme",
            self.prefs.theme,
            &[
                (Theme::System, "System"),
                (Theme::Light, "Light"),
                (Theme::Dark, "Dark"),
            ],
            |p, v| p.theme = v,
            cx,
        );
        let text = self.pref_choice(
            "text",
            self.prefs.text,
            &[
                (Text::Small, "Small"),
                (Text::Normal, "Default"),
                (Text::Large, "Large"),
            ],
            |p, v| p.text = v,
            cx,
        );
        let font = self.pref_choice(
            "font",
            self.prefs.system_font,
            &[(false, "Geist"), (true, "System")],
            |p, v| p.system_font = v,
            cx,
        );
        let mono = self.pref_choice(
            "mono",
            self.prefs.system_mono,
            &[(false, "Geist Mono"), (true, "System")],
            |p, v| p.system_mono = v,
            cx,
        );
        let groups = vec![
            self.group(
                None,
                vec![
                    self.row(None, "Theme", None, Some(theme))
                        .into_any_element(),
                    self.row(None, "Chat text", None, Some(text))
                        .into_any_element(),
                ],
            ),
            self.group(
                Some("Fonts"),
                vec![
                    self.row(None, "Words", None, Some(font)).into_any_element(),
                    self.row(None, "Code", None, Some(mono)).into_any_element(),
                ],
            ),
        ];
        self.settings_page("appearance-page", "Appearance", None, groups)
    }

    /// Chat: how much of an agent's work its replies show, and how the composer sends.
    pub(super) fn chat_prefs_page(&mut self, cx: &mut Context<Self>) -> Div {
        let tools = self.pref_choice(
            "tools",
            self.prefs.tools,
            &[
                (Tools::Hidden, "Hidden"),
                (Tools::Folded, "Folded"),
                (Tools::All, "Every call"),
            ],
            |p, v| p.tools = v,
            cx,
        );
        let tools_sub = match self.prefs.tools {
            Tools::Hidden => "Failed ones too",
            Tools::Folded => "",
            Tools::All => "",
        };
        let thoughts = self.pref_choice(
            "thoughts",
            self.prefs.thoughts,
            &[
                (Thoughts::Hidden, "Hidden"),
                (Thoughts::Folded, "Folded"),
                (Thoughts::Open, "Open"),
            ],
            |p, v| p.thoughts = v,
            cx,
        );
        let width = self.pref_choice(
            "width",
            self.prefs.width,
            &[
                (Width::Normal, "Normal"),
                (Width::Comfortable, "Comfortable"),
                (Width::Full, "Full width"),
            ],
            |p, v| p.width = v,
            cx,
        );
        let style = self.pref_choice(
            "style",
            self.prefs.bubbles,
            &[(true, "Bubbles"), (false, "Plain")],
            |p, v| p.bubbles = v,
            cx,
        );
        let messages = vec![
            self.row(None, "Chat width", None, Some(width))
                .into_any_element(),
            self.row(None, "Agent replies", None, Some(style))
                .into_any_element(),
            self.pref_switch(
                "pref-vibrant",
                "Vibrant bubbles",
                "",
                self.prefs.vibrant(),
                |p, v| p.vibrant = Some(v),
                cx,
            ),
            self.row(None, "Tool calls", Some(tools_sub.into()), Some(tools))
                .into_any_element(),
            self.row(None, "Thoughts", None, Some(thoughts))
                .into_any_element(),
            self.pref_switch(
                "pref-context",
                "Show context",
                "",
                self.prefs.show_context,
                |p, v| p.show_context = v,
                cx,
            ),
        ];
        let composer = vec![self.pref_switch(
            "pref-ctrl-enter",
            "Send with Ctrl+Enter",
            "",
            self.prefs.ctrl_enter,
            |p, v| p.ctrl_enter = v,
            cx,
        )];
        let sidebar = vec![self.pref_switch(
            "pref-search",
            "Search in the sidebar",
            "",
            self.prefs.search,
            |p, v| p.search = v,
            cx,
        )];
        let groups = vec![
            self.group(Some("Messages"), messages),
            self.group(Some("Composer"), composer),
            self.group(Some("Sidebar"), sidebar),
        ];
        self.settings_page("chat-prefs-page", "Chat", None, groups)
    }

    /// App & notifications: how loom runs on this computer, and how it tells the person.
    pub(super) fn app_prefs_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let login = self.starts_at_login;
        let login_toggle =
            self.switch("pref-login", login)
                .on_click(cx.listener(move |this, _, _, cx| {
                    shell::set_start_at_login(!login);
                    this.starts_at_login = shell::starts_at_login();
                    if let Some(t) = &this.tray {
                        t.set_login(this.starts_at_login);
                    }
                    cx.notify();
                }));
        let computer = vec![
            self.row(
                None,
                "Start at login",
                None,
                Some(login_toggle.into_any_element()),
            )
            .into_any_element(),
            self.pref_switch(
                "pref-tray",
                "Keep running when closed",
                "",
                self.prefs.tray,
                |p, v| p.tray = v,
                cx,
            ),
            self.pref_switch(
                "pref-awake",
                "Keep the computer awake while an agent works",
                "",
                self.prefs.awake,
                |p, v| p.awake = v,
                cx,
            ),
        ];
        let mut notify = vec![self.pref_switch(
            "pref-notify",
            "Notifications",
            "",
            self.prefs.notify,
            |p, v| p.notify = v,
            cx,
        )];
        if self.prefs.notify {
            notify.push(self.pref_switch(
                "pref-preview",
                "Show what was said",
                "",
                self.prefs.preview,
                |p, v| p.preview = v,
                cx,
            ));
            notify.push(self.pref_switch(
                "pref-sound",
                "Play a sound",
                "",
                self.prefs.sound,
                |p, v| p.sound = v,
                cx,
            ));
        }
        let check = self
            .button("pref-check-update", "Check now", false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.update_said = Some("Looking…".into());
                this.send_to(
                    "",
                    &serde_json::json!({"t": "update.check", "id": "app.update"}),
                );
                cx.notify();
            }));
        // A line only when there's news: what Check now found, or one ready to go in.
        let status = self.update_said.clone().or_else(|| {
            self.hub()
                .update
                .as_ref()
                .map(|(v, _)| format!("loom {v} is ready to restart into"))
        });
        let mut updates = vec![self
            .row(
                None,
                format!("loom {}", crate::VERSION),
                status,
                Some(check.into_any_element()),
            )
            .into_any_element()];
        updates.push(self.pref_switch(
            "pref-auto-update",
            "Update on its own",
            "While no agent is working",
            self.prefs.auto_update,
            |p, v| p.auto_update = v,
            cx,
        ));
        let _ = p;
        // Twemoji's licence asks for its name, the licence and a link (design/twemoji).
        let twemoji = self
            .button("credit-twemoji", "Licence", false)
            .on_click(cx.listener(|_, _, _, cx| {
                cx.open_url("https://creativecommons.org/licenses/by/4.0/");
            }));
        let credits = vec![self
            .row(
                None,
                "Emoji by Twemoji",
                Some("© Twitter, Jason Sofonia and Justine De Caires, CC-BY 4.0, github.com/jdecked/twemoji".into()),
                Some(twemoji.into_any_element()),
            )
            .into_any_element()];
        let groups = vec![
            self.group(Some("This computer"), computer),
            self.group(Some("Notifications"), notify),
            self.group(Some("Updates"), updates),
            self.group(Some("Credits"), credits),
        ];
        self.settings_page("app-prefs-page", "App & notifications", None, groups)
    }

    /// A notification from the core, as the prefs let it show.
    pub(super) fn notify(&self, title: &str, body: &str, about: shell::About) {
        notify_as(&self.prefs, title, body, about);
    }
}

/// A notification from a core, as `prefs` let it show.
pub(super) fn notify_as(prefs: &Prefs, title: &str, body: &str, about: shell::About) {
    if !prefs.notify {
        return;
    }
    let body = if prefs.preview {
        body
    } else {
        "Sent you a message"
    };
    shell::notify(title, body, Some(about), prefs.sound);
}

#[cfg(test)]
mod tests {
    use super::{page_kind, settings_found, settings_groups, Screen};
    use crate::model::popup::Kind;

    #[test]
    fn pages_of_things_to_pick_from_are_lists() {
        let kinds = |kind: Kind| -> Vec<&str> {
            settings_groups()
                .into_iter()
                .flat_map(|(_, pages)| pages)
                .filter(|(_, _, _, s)| page_kind(*s) == kind)
                .map(|(_, _, label, _)| label)
                .collect()
        };
        assert_eq!(
            kinds(Kind::List),
            ["Skills", "Proposals", "MCP servers", "Routines"]
        );
        assert_eq!(kinds(Kind::Grid), ["Emoji", "Spools"]);
        assert!(kinds(Kind::Column).contains(&"Appearance"));
        assert_eq!(page_kind(Screen::Server), Kind::Column);
        // Brains is one column, each brain opening in place.
        assert_eq!(page_kind(Screen::Programs), Kind::Column);
        // Spools' Installed tab is a list.
        assert_eq!(page_kind(Screen::Installed), Kind::List);
    }

    #[test]
    fn search_finds_a_page_by_what_is_on_it() {
        let screens = |q: &str| -> Vec<Screen> {
            settings_found(q)
                .into_iter()
                .map(|(_, _, _, s)| s)
                .collect()
        };
        assert!(screens("dark") == vec![Screen::Appearance]);
        assert!(screens("Tool calls") == vec![Screen::ChatPrefs]);
        assert!(screens("context compacted") == vec![Screen::ChatPrefs]);
        assert!(screens("brains") == vec![Screen::Programs]);
        assert!(screens("claude code") == vec![Screen::Programs]);
        assert!(screens("steer") == vec![Screen::Defaults]);
        assert!(screens("webhook deny") == vec![Screen::Rules]);
        assert!(screens("Group chats") == vec![Screen::Defaults]);
        assert!(screens("router model") == vec![Screen::Defaults]);
        assert!(screens("integrations") == vec![Screen::Spools]);
        assert!(screens("github") == vec![Screen::Spools]);
        assert!(screens("zzz").is_empty());
    }
}
