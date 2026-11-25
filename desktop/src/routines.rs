//! The Routines page in the popup: the week's calendar over the person's routines, and
//! each routine's page, with its schedule set by fields (PLAN.md, "Routines"), when it
//! runs next, and its history. The instance works out when routines run and what to do
//! about a run that went wrong; this only draws and asks.

use super::*;
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};

/// The week's grid: pixels to an hour, and the hours it shows at the least.
const HOUR_PX: f32 = 15.0;
const FIRST_HOUR: u32 = 7;
const LAST_HOUR: u32 = 21;

/// How many runs the history's strip shows.
const STRIP: usize = 30;

/// The routine page's schedule as the person changes it, and the rest of the page's
/// state: which run is open, and which week the calendar shows.
pub(super) struct RoutineUi {
    pub schedule: model::Schedule,
    /// How many hours apart, for Every few hours, or minutes for one set every N minutes
    /// that aren't whole hours (`minutes`).
    pub every_in: Entity<InputState>,
    pub minutes: bool,
    pub from_in: Entity<InputState>,
    pub to_in: Entity<InputState>,
    /// The time "+ Time" adds, while it's open.
    pub time_in: Entity<InputState>,
    pub adding_time: bool,
    /// The run open in the history, by id.
    pub run_open: Option<String>,
    /// The week the calendar shows, in weeks from this one.
    pub week: i64,
}

impl RoutineUi {
    pub fn new(window: &mut Window, cx: &mut Context<Loom>) -> RoutineUi {
        let field = |window: &mut Window, cx: &mut Context<Loom>, placeholder: &'static str| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let every_in = field(window, cx, "2");
        let from_in = field(window, cx, "08:00");
        let to_in = field(window, cx, "20:00");
        let time_in = field(window, cx, "HH:MM");
        // A change to the hours or the window asks again when it runs next.
        for state in [&every_in, &from_in, &to_in] {
            cx.subscribe_in(
                state,
                window,
                |this: &mut Loom, _, event: &InputEvent, _, cx| {
                    if let InputEvent::Change = event {
                        this.routine_schedule_changed(cx);
                    }
                },
            )
            .detach();
        }
        // Enter, or leaving the field, adds the time typed.
        cx.subscribe_in(
            &time_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let typed = state.read(cx).value().to_string();
                    this.add_routine_time(&typed, matches!(event, InputEvent::Blur), cx);
                    state.update(cx, |s, cx| s.set_value("", window, cx));
                }
            },
        )
        .detach();
        RoutineUi {
            schedule: model::Schedule::default(),
            every_in,
            minutes: false,
            from_in,
            to_in,
            time_in,
            adding_time: false,
            run_open: None,
            week: 0,
        }
    }
}

/// When a next run is, for the routine page's "Next": "Today 08:00", "Tomorrow 17:00",
/// "Fri 08:00" within the week, and "3 Dec 08:00" after.
fn next_words(ms: i64, today: NaiveDate) -> String {
    let Some(t) = Local.timestamp_millis_opt(ms).single() else {
        return String::new();
    };
    let days = (t.date_naive() - today).num_days();
    match days {
        0 => t.format("Today %H:%M").to_string(),
        1 => t.format("Tomorrow %H:%M").to_string(),
        2..=6 => t.format("%a %H:%M").to_string(),
        _ => t.format("%-d %b %H:%M").to_string(),
    }
}

/// The Monday a week starts on, `offset` weeks from this one, and the week from its
/// midnight to the next Monday's (Unix ms), on this computer's clock.
fn week_of(offset: i64) -> Option<(NaiveDate, i64, i64)> {
    let today = Local::now().date_naive();
    let monday = today - chrono::Days::new(u64::from(today.weekday().num_days_from_monday()));
    let weeks = chrono::Days::new(7 * offset.unsigned_abs());
    let monday = if offset < 0 {
        monday.checked_sub_days(weeks)?
    } else {
        monday.checked_add_days(weeks)?
    };
    let midnight = |d: NaiveDate| {
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
            .earliest()
            .map(|t| t.timestamp_millis())
    };
    let next = monday.checked_add_days(chrono::Days::new(7))?;
    Some((monday, midnight(monday)?, midnight(next)?))
}

/// Where a moment falls in the week from `monday`: its day, 0 for Monday, and its
/// minute of the day.
fn in_week(ms: i64, monday: NaiveDate) -> Option<(usize, u32)> {
    let t = Local.timestamp_millis_opt(ms).single()?;
    let day = usize::try_from((t.date_naive() - monday).num_days()).ok()?;
    (day < 7).then(|| (day, t.hour() * 60 + t.minute()))
}

/// The hours the week's grid shows: from the earliest run to the latest, and at least
/// FIRST_HOUR to LAST_HOUR.
fn grid_hours(minutes: impl Iterator<Item = u32>) -> (u32, u32) {
    minutes.fold((FIRST_HOUR, LAST_HOUR), |(a, b), m| {
        (a.min(m / 60), b.max((m / 60 + 1).min(24)))
    })
}

impl Loom {
    /// The schedule the routine page shows now, with what's typed in its fields.
    fn routine_schedule(&self, cx: &App) -> model::Schedule {
        let ui = &self.routine_ui;
        let mut s = ui.schedule.clone();
        if s.repeat == "hours" {
            let n = ui
                .every_in
                .read(cx)
                .value()
                .trim()
                .parse::<i64>()
                .unwrap_or(0);
            s.every = if ui.minutes { n } else { n * 60 };
            s.from = ui.from_in.read(cx).value().trim().to_string();
            s.to = ui.to_in.read(cx).value().trim().to_string();
        }
        s
    }

    /// The routine page's schedule changed: asks when it would run next.
    pub(super) fn routine_schedule_changed(&mut self, cx: &mut Context<Self>) {
        if self.routine_open.is_none() {
            return;
        }
        let schedule = self.routine_schedule(cx).to_json();
        if let Some(f) = self.model.ask_schedule_next(&schedule) {
            self.send(f);
        }
        cx.notify();
    }

    /// Adds a time of day to the routine's times, as typed. A time that isn't one says
    /// so, unless the field was only left empty or as it was.
    fn add_routine_time(&mut self, typed: &str, left: bool, cx: &mut Context<Self>) {
        if !self.routine_ui.adding_time {
            return;
        }
        self.routine_ui.adding_time = false;
        match model::clock_of(typed) {
            Some(t) => {
                let times = &mut self.routine_ui.schedule.times;
                if !times.contains(&t) {
                    times.push(t);
                    times.sort();
                }
            }
            None if left && typed.trim().is_empty() => {}
            None => self.model.notices.push("Write a time as HH:MM".into()),
        }
        self.routine_schedule_changed(cx);
    }

    /// Fills the routine page's schedule fields from `s`.
    fn set_routine_schedule(
        &mut self,
        s: model::Schedule,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let minutes = s.every % 60 != 0;
        let every = if minutes { s.every } else { s.every / 60 };
        let ui = &mut self.routine_ui;
        ui.every_in
            .update(cx, |f, cx| f.set_value(every.to_string(), window, cx));
        ui.from_in
            .update(cx, |f, cx| f.set_value(s.from.clone(), window, cx));
        ui.to_in
            .update(cx, |f, cx| f.set_value(s.to.clone(), window, cx));
        ui.minutes = minutes;
        ui.adding_time = false;
        ui.run_open = None;
        ui.schedule = s;
    }

    /// Opens the routine form, empty, for a new one.
    pub(super) fn new_routine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        _ = self.model.load_routine_runs(None);
        self.routine_name_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.set_routine_schedule(model::Schedule::default(), window, cx);
        self.routine_prompt_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.routine_agent = self.model.selected.clone();
        self.routine_notify = true;
        self.routine_on = true;
        self.routine_catch_up = true;
        self.routine_thread = false;
        self.routine_webhook = false;
        self.set_routine_flow(&[], window, cx);
        self.routine_open = Some(None);
        self.routine_schedule_changed(cx);
    }

    /// Opens routine `r`'s page, to change it and see its history.
    fn open_routine(&mut self, r: &model::Routine, window: &mut Window, cx: &mut Context<Self>) {
        self.routine_name_in
            .update(cx, |s, cx| s.set_value(r.name.clone(), window, cx));
        self.set_routine_schedule(r.schedule.clone(), window, cx);
        self.routine_prompt_in
            .update(cx, |s, cx| s.set_value(r.prompt.clone(), window, cx));
        self.routine_agent = Some(r.agent.clone());
        self.routine_notify = r.notify;
        self.routine_on = r.on;
        self.routine_catch_up = r.catch_up;
        self.routine_thread = r.post == "thread";
        self.routine_webhook = !r.hook.is_empty();
        self.set_routine_flow(&r.flow, window, cx);
        if let Some(f) = self.model.load_routine_runs(Some(&r.id)) {
            self.send(f);
        }
        self.routine_open = Some(Some(r.id.clone()));
        self.routine_schedule_changed(cx);
    }

    /// The routines page: the week's calendar over the person's routines, and the page
    /// of a new or changed one.
    pub(super) fn routines_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.routine_open.is_some() {
            return self.routine_form(cx);
        }
        if self.model.routines.is_empty() {
            let groups = vec![self.empty(
                Ico::Clock,
                "No routines yet",
                None,
                Some(self.new_routine_button(cx)),
            )];
            return self.settings_page("routines-page", "Routines", None, groups);
        }
        // The week the calendar shows, asked for once, and again as routines change.
        if let Some((_, from, to)) = week_of(self.routine_ui.week) {
            if self.model.routine_week_span != (from, to) {
                let f = self.model.load_routine_week(from, to);
                self.send(f);
            }
        }
        // Its title, the week, then the routines, each a row of the one group, as a list.
        let n = self.model.routines.len();
        self.list_page(
            "routines-page",
            n + 2,
            layout::SETTINGS,
            |this, ix, cx| {
                if ix == 0 {
                    let action = Some(this.new_routine_button(cx));
                    return this.list_head("Routines", action);
                }
                if ix == 1 {
                    return div()
                        .pb(px(space::XL))
                        .child(this.routine_week(cx))
                        .into_any_element();
                }
                let n = this.model.routines.len();
                match this.model.routines.get(ix - 2).cloned() {
                    Some(r) => {
                        let row = this.routine_row(r, cx);
                        this.group_item(ix - 2, n, row)
                    }
                    None => div().into_any_element(),
                }
            },
            cx,
        )
    }

    /// The week's calendar: days across and hours down, each routine a small block in
    /// its agent's colour at its times, and one every few hours or minutes as thin ticks.
    /// Today stands out, and arrows go to other weeks.
    fn routine_week(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let offset = self.routine_ui.week;
        let Some((monday, _, _)) = week_of(offset) else {
            return div().into_any_element();
        };
        let today = Local::now().date_naive();
        // Each routine's runs in the week: its day, minute, name, colour, and whether it
        // runs often enough to be ticks.
        struct Mark {
            day: usize,
            minute: u32,
            id: String,
            name: String,
            tint: Hsla,
            tick: bool,
        }
        let mut marks = Vec::new();
        let mut tick_lanes: Vec<String> = Vec::new();
        for (id, times) in &self.model.routine_week {
            let Some(r) = self.model.routines.iter().find(|r| &r.id == id) else {
                continue;
            };
            let tint = match self.model.agent(&r.agent) {
                Some(a) => thread::color(&a.color, p.dark),
                None => p.ink_3,
            };
            let tick = r.schedule.frequent();
            if tick && !tick_lanes.contains(id) {
                tick_lanes.push(id.clone());
            }
            for &t in times {
                if let Some((day, minute)) = in_week(t, monday) {
                    marks.push(Mark {
                        day,
                        minute,
                        id: id.clone(),
                        name: r.name.clone(),
                        tint,
                        tick,
                    });
                }
            }
        }
        let (first, last) = grid_hours(marks.iter().map(|m| m.minute));
        let height = (last - first) as f32 * HOUR_PX;
        let top_of = |minute: u32| (minute as f32 / 60.0 - first as f32) * HOUR_PX;
        // Blocks take the column, or most of it beside the ticks' lanes.
        let lanes = tick_lanes.len().min(4);
        let block_share = if lanes > 0 { 0.7 } else { 1.0 };

        // The head: the arrows, and which week it is.
        let label = if offset == 0 {
            "This week".to_string()
        } else {
            let sunday = monday + chrono::Days::new(6);
            if monday.month() == sunday.month() {
                format!("{} – {}", monday.format("%-d"), sunday.format("%-d %B"))
            } else {
                format!("{} – {}", monday.format("%-d %b"), sunday.format("%-d %b"))
            }
        };
        let arrow = |id: &'static str, icon: Ico, step: i64| {
            div()
                .id(id)
                .size(px(control::ICON_SMALL))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .text_color(p.ink_2)
                .soft_hover(transparent_black(), p.hover, None)
                .child(Icon::new(icon).small())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.routine_ui.week += step;
                    cx.notify();
                }))
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::M))
            .py(px(space::S))
            .child(arrow("week-back", Ico::ChevronLeft, -1))
            .child(arrow("week-on", Ico::ChevronRight, 1))
            .child(
                div()
                    .id("week-now")
                    .pl(px(space::XS))
                    .text_size(px(size::SMALL))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink)
                    .when(offset != 0, |d| d.cursor_pointer())
                    .child(label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.routine_ui.week = 0;
                        cx.notify();
                    })),
            );

        // The days across, today standing out.
        const GUTTER: f32 = 30.0;
        let mut days_row = div()
            .flex()
            .pl(px(GUTTER))
            .pr(px(space::S))
            .pb(px(space::XS));
        for d in 0..7u64 {
            let date = monday + chrono::Days::new(d);
            let is_today = date == today;
            days_row = days_row.child(
                div().flex_1().flex().justify_center().child(
                    div()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(px(radius::ROW))
                        .text_size(px(size::CAPTION))
                        .when(is_today, |d| {
                            d.bg(p.button)
                                .text_color(p.button_ink)
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .when(!is_today, |d| d.text_color(p.ink_3))
                        .child(date.format("%a %-d").to_string()),
                ),
            );
        }

        // The hours down, a line at every other one.
        let mut hours = div().relative().w(px(GUTTER)).h(px(height)).flex_none();
        let mut lines = div().absolute().inset_0();
        for h in (first..=last).filter(|h| h % 2 == 0) {
            let y = top_of(h * 60);
            if h < last {
                hours = hours.child(
                    div()
                        .absolute()
                        .top(px(y - 6.0))
                        .left_0()
                        .text_size(px(10.0))
                        .text_color(p.ink_3)
                        .child(format!("{h:02}")),
                );
            }
            if h > first && h < last {
                lines = lines.child(
                    div()
                        .absolute()
                        .top(px(y))
                        .left_0()
                        .right_0()
                        .h(px(1.0))
                        .bg(p.line),
                );
            }
        }
        let mut columns = div().relative().flex_1().h(px(height)).flex().child(lines);
        for d in 0..7usize {
            let is_today = monday + chrono::Days::new(d as u64) == today;
            let mut column = div()
                .relative()
                .flex_1()
                .h_full()
                .border_l_1()
                .border_color(p.line)
                .when(is_today, |c| {
                    c.bg(motion::mix(self.card_fill(), p.button, 0.06))
                });
            let in_day: Vec<&Mark> = marks.iter().filter(|m| m.day == d).collect();
            for m in &in_day {
                let id = m.id.clone();
                if m.tick {
                    let lane = tick_lanes.iter().position(|l| *l == m.id).unwrap_or(0) % 4;
                    column = column.child(
                        div()
                            .absolute()
                            .top(px(top_of(m.minute) - 1.0))
                            .h(px(2.0))
                            .right(relative(
                                (lanes - 1 - lane.min(lanes - 1)) as f32 * 0.075 + 0.02,
                            ))
                            .w(relative(0.06))
                            .rounded(px(1.0))
                            .bg(m.tint),
                    );
                    continue;
                }
                // Blocks within the hour share it side by side.
                let near: Vec<&&Mark> = in_day
                    .iter()
                    .filter(|o| !o.tick && o.minute.abs_diff(m.minute) < 60)
                    .collect();
                let rank = near
                    .iter()
                    .position(|o| o.minute == m.minute && o.id == m.id)
                    .unwrap_or(0);
                let share = block_share / near.len().max(1) as f32;
                column = column.child(
                    div()
                        .id(SharedString::from(format!(
                            "week-{}-{d}-{}",
                            m.id, m.minute
                        )))
                        .absolute()
                        .top(px(top_of(m.minute)))
                        .h(px(HOUR_PX - 1.0))
                        .left(relative(rank as f32 * share))
                        .w(relative(share))
                        .px(px(1.0))
                        .child(
                            div()
                                .size_full()
                                .rounded(px(3.0))
                                .border_l_2()
                                .border_color(m.tint)
                                .bg(motion::mix(self.card_fill(), m.tint, 0.32))
                                .px(px(3.0))
                                .overflow_hidden()
                                .text_size(px(10.0))
                                .line_height(px(HOUR_PX - 1.0))
                                .text_color(p.ink)
                                .truncate()
                                .child(m.name.clone()),
                        )
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let r = this.model.routines.iter().find(|r| r.id == id).cloned();
                            if let Some(r) = r {
                                this.open_routine(&r, window, cx);
                            }
                        })),
                );
            }
            columns = columns.child(column);
        }
        let grid = div()
            .flex()
            .pr(px(space::S))
            .pb(px(space::M))
            .child(hours)
            .child(columns);
        div()
            .flex()
            .flex_col()
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(p.line)
            .bg(self.card_fill())
            .overflow_hidden()
            .child(head)
            .child(days_row)
            .child(grid)
            .into_any_element()
    }

    /// One routine, as a row of the routines page: when it runs next and how its last
    /// run went, or its flow's steps while it runs, which opens it to change.
    fn routine_row(&self, r: model::Routine, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let (agent, tint) = match self.model.agent(&r.agent) {
            Some(a) => (a.name.clone(), thread::color(&a.color, p.dark)),
            None => ("a deleted agent".to_string(), p.ink_3),
        };
        let next = if !r.on || r.next == 0 {
            "paused".to_string()
        } else if day_of(r.next) == "Today" {
            format!("next at {}", time_of(r.next))
        } else {
            format!("next {}", next_words(r.next, Local::now().date_naive()))
        };
        let last = match r.last_status.as_str() {
            "" | "done" | "quiet" => String::new(),
            "running" => " · running now".into(),
            "waiting" => " · waiting for the chat".into(),
            "skipped" => format!(" · skipped at {}", time_of(r.last_at)),
            _ => format!(" · failed at {}: {}", time_of(r.last_at), r.last_error),
        };
        let mut sub = format!("{agent} · {} · {next}{last}", r.when);
        // Its flow, while it runs: how far each step has got, and a Stop.
        let running = self
            .model
            .flows
            .iter()
            .find(|f| f.routine == r.id)
            .map(|f| {
                let steps: Vec<String> = f
                    .steps
                    .iter()
                    .skip(1)
                    .map(|(name, _, status)| format!("step {name} {status}"))
                    .collect();
                (steps.join(" · "), f.stopped)
            });
        let mut controls = div().flex().gap(px(space::XS));
        if let Some((text, stopped)) = running {
            sub = text;
            if !stopped {
                let routine = r.id.clone();
                controls = controls.child(
                    self.small_button(format!("stop-flow-{}", r.id), "Stop", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            let f = this.model.stop_flow(&routine);
                            this.send(f);
                            cx.notify();
                        })),
                );
            }
        }
        // A routine with a thread of its own: the way there.
        if r.post == "thread" && !r.conversation.is_empty() {
            let conversation = r.conversation.clone();
            let agent = r.agent.clone();
            controls = controls.child(
                self.small_button(format!("thread-{}", r.id), "Thread", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_thread(&agent, &conversation, cx);
                    })),
            );
        }
        let lead = div()
            .size(px(control::LEAD))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .border_1()
            .border_color(p.line)
            .bg(p.bg)
            .text_color(if r.on { tint } else { p.ink_3 })
            .child(Icon::new(Ico::Clock).small())
            .into_any_element();
        self.row(
            Some(lead),
            r.name.clone(),
            Some(sub),
            Some(controls.into_any_element()),
        )
        .id(SharedString::from(format!("routine-{}", r.id)))
        .cursor_pointer()
        .soft_hover(transparent_black(), p.hover, None)
        .on_click(cx.listener(move |this, _, window, cx| this.open_routine(&r, window, cx)))
        .into_any_element()
    }

    /// The routines page's New routine, the page's own button while it has none.
    fn new_routine_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.small_button("new-routine", "New routine", self.model.routines.is_empty())
            .on_click(cx.listener(|this, _, window, cx| this.new_routine(window, cx)))
            .into_any_element()
    }

    /// The routine's schedule (PLAN.md, "Routines"): Repeat, then the days and times,
    /// or every few hours within a window, and when it runs next.
    fn routine_when_group(&mut self, open: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let repeat = self.routine_ui.schedule.repeat.clone();
        let mut kinds = Vec::new();
        for (kind, label) in [
            ("hours", "Every few hours"),
            ("days", "On days"),
            ("webhook", "Webhook"),
        ] {
            let on = repeat == kind || (kind == "webhook" && repeat.is_empty());
            kinds.push(
                self.segment(format!("routine-repeat-{kind}"), label, on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.routine_ui.schedule.repeat = kind.into();
                        this.routine_ui.adding_time = false;
                        this.routine_schedule_changed(cx);
                    })),
            );
        }
        let mut rows = vec![self
            .row(
                None,
                "Repeat",
                None,
                Some(self.segments(kinds).into_any_element()),
            )
            .into_any_element()];
        if repeat == "hours" {
            let small = |state: &Entity<InputState>, w: f32| {
                div().w(px(w)).child(Input::new(state).small())
            };
            let word = |text: &'static str| {
                div()
                    .text_size(px(size::BODY))
                    .text_color(p.ink_2)
                    .child(text)
            };
            let unit = if self.routine_ui.minutes {
                "minutes, from"
            } else {
                "hours, from"
            };
            rows.push(
                div()
                    .px(px(space::L))
                    .py(px(space::S + 2.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(space::S))
                    .child(word("Every"))
                    .child(small(&self.routine_ui.every_in, 52.0))
                    .child(word(unit))
                    .child(small(&self.routine_ui.from_in, 72.0))
                    .child(word("to"))
                    .child(small(&self.routine_ui.to_in, 72.0))
                    .into_any_element(),
            );
        }
        if repeat == "hours" || repeat == "days" {
            rows.push(self.routine_days_row(cx));
        }
        if repeat == "days" {
            rows.push(self.routine_times_row(cx));
        }
        // When it runs next: the instance's word for the schedule as it stands, or the
        // routine's own while it's unchanged, so one every N minutes counts from its last.
        if repeat == "hours" || repeat == "days" {
            let saved = open.and_then(|id| self.model.routines.iter().find(|r| r.id == id));
            let now = self.routine_schedule(cx).to_json();
            let next = match saved {
                Some(r) if r.schedule.to_json() == now && !r.upcoming.is_empty() => {
                    r.upcoming.clone()
                }
                _ => self.model.schedule_next.clone(),
            };
            if !next.is_empty() {
                let today = Local::now().date_naive();
                let words: Vec<String> = next.iter().map(|&t| next_words(t, today)).collect();
                rows.push(
                    div()
                        .px(px(space::L))
                        .py(px(space::S + 2.0))
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_2)
                        .child(format!("Next: {}", words.join(", ")))
                        .into_any_element(),
                );
            }
        }
        self.group(Some("When"), rows)
    }

    /// The days a routine runs on, as seven round toggles, with "Weekdays" or "Every day"
    /// beside them when that's what's picked.
    fn routine_days_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let days = self.routine_ui.schedule.days;
        let mut toggles = div().flex().items_center().gap(px(6.0));
        for (k, letter) in ["M", "T", "W", "T", "F", "S", "S"].into_iter().enumerate() {
            let on = days & (1 << k) != 0;
            toggles = toggles.child(
                div()
                    .id(SharedString::from(format!("routine-day-{k}")))
                    .role(Role::Button)
                    .aria_label(model::DAY_KEYS[k])
                    .size(px(control::ICON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_size(px(size::SMALL))
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(p.button)
                            .text_color(p.button_ink)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!on, |d| {
                        d.border_1()
                            .border_color(p.line_strong)
                            .soft_ink(p.ink_2, p.ink)
                    })
                    .child(letter)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.routine_ui.schedule.days ^= 1 << k;
                        this.routine_schedule_changed(cx);
                    })),
            );
        }
        let named = match days {
            model::EVERY_DAY => "Every day",
            model::WEEKDAYS => "Weekdays",
            _ => "",
        };
        self.row(
            None,
            "Days",
            None,
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .when(!named.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(size::SMALL))
                                .text_color(p.ink_3)
                                .child(named),
                        )
                    })
                    .child(toggles)
                    .into_any_element(),
            ),
        )
        .into_any_element()
    }

    /// The times a routine runs at, as chips with ×, and "+ Time".
    fn routine_times_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let mut chips = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(px(space::XS));
        for t in self.routine_ui.schedule.times.clone() {
            let gone = t.clone();
            chips = chips.child(
                div()
                    .h(px(control::CHIP))
                    .pl(px(space::S + 2.0))
                    .pr(px(space::XS))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .rounded(px(radius::CHIP))
                    .border_1()
                    .border_color(p.line_strong)
                    .bg(p.bg)
                    .text_size(px(size::SMALL))
                    .text_color(p.ink)
                    .child(t.clone())
                    .child(
                        div()
                            .id(SharedString::from(format!("routine-time-{t}")))
                            .role(Role::Button)
                            .aria_label(format!("Remove {t}"))
                            .size(px(18.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .soft_ink(p.ink_3, p.ink)
                            .child(Icon::new(Ico::X).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.routine_ui.schedule.times.retain(|x| *x != gone);
                                this.routine_schedule_changed(cx);
                            })),
                    ),
            );
        }
        if self.routine_ui.adding_time {
            chips = chips.child(
                div()
                    .w(px(76.0))
                    .child(Input::new(&self.routine_ui.time_in).small()),
            );
        } else if self.routine_ui.schedule.times.len() < 24 {
            chips = chips.child(
                div()
                    .id("routine-add-time")
                    .role(Role::Button)
                    .h(px(control::CHIP))
                    .px(px(space::S + 2.0))
                    .flex()
                    .items_center()
                    .rounded(px(radius::CHIP))
                    .border_1()
                    .border_color(p.line)
                    .text_size(px(size::SMALL))
                    .cursor_pointer()
                    .soft_ink(p.ink_2, p.ink)
                    .child("+ Time")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.routine_ui.adding_time = true;
                        this.routine_ui
                            .time_in
                            .update(cx, |s, cx| s.focus(window, cx));
                        cx.notify();
                    })),
            );
        }
        self.row(None, "Times", None, Some(chips.into_any_element()))
            .into_any_element()
    }

    /// The colour of a run's mark and pill, by how it ended.
    fn run_tint(&self, mark: model::RunMark) -> Hsla {
        let p = self.p;
        match mark {
            model::RunMark::Posted => p.ok,
            model::RunMark::Quiet => p.ink_3,
            model::RunMark::Failed => p.bad,
            model::RunMark::Skipped => p.warn,
            model::RunMark::Going => p.button,
        }
    }

    /// The routine's history: a strip of its last 30 runs, a mark each coloured by how it
    /// ended, with a legend, over the list of its runs. A run opens to what it said, or
    /// why it failed and what to do.
    fn routine_history_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let runs = self.model.routine_runs.clone();
        let open = self.routine_ui.run_open.clone();
        let mut strip = div().flex().items_end().gap(px(3.0));
        for r in runs.iter().take(STRIP).rev() {
            let id = r.run.clone();
            let picked = open.as_deref() == Some(r.run.as_str());
            strip = strip.child(
                div()
                    .id(SharedString::from(format!("run-mark-{}", r.run)))
                    .w(px(9.0))
                    .h(px(22.0))
                    .rounded(px(3.0))
                    .bg(self.run_tint(r.mark()))
                    .cursor_pointer()
                    .when(picked, |d| d.border_2().border_color(p.ink))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let ui = &mut this.routine_ui;
                        ui.run_open =
                            (ui.run_open.as_deref() != Some(id.as_str())).then(|| id.clone());
                        cx.notify();
                    })),
            );
        }
        let mut legend = div().flex().flex_wrap().gap(px(space::M));
        for (mark, word) in [
            (model::RunMark::Posted, "Posted"),
            (model::RunMark::Quiet, "Quiet"),
            (model::RunMark::Failed, "Failed"),
            (model::RunMark::Skipped, "Skipped"),
        ] {
            legend = legend.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(div().size(px(8.0)).rounded(px(2.0)).bg(self.run_tint(mark)))
                    .child(word),
            );
        }
        let mut rows = vec![div()
            .px(px(space::L))
            .py(px(space::M))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .child(strip)
            .child(legend)
            .into_any_element()];
        let now = Local::now();
        for r in runs.iter() {
            let id = r.run.clone();
            let tint = self.run_tint(r.mark());
            let picked = open.as_deref() == Some(r.run.as_str());
            let line = div()
                .id(SharedString::from(format!("run-{}", r.run)))
                .px(px(space::L))
                .py(px(space::S + 2.0))
                .flex()
                .items_center()
                .gap(px(space::M))
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .child(
                    div()
                        .w(px(96.0))
                        .flex_none()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(when_sent(r.at, now)),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(px(radius::ROW))
                        .bg(motion::mix(self.card_fill(), tint, 0.18))
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(tint)
                        .child(r.pill()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_2)
                        .child(r.line()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(r.took()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let ui = &mut this.routine_ui;
                    ui.run_open = (ui.run_open.as_deref() != Some(id.as_str())).then(|| id.clone());
                    cx.notify();
                }));
            let mut item = div().flex().flex_col().child(line);
            if picked {
                item = item.child(self.run_detail(r));
            }
            rows.push(item.into_any_element());
        }
        self.group(Some("History"), rows)
    }

    /// An open run: what it posted and what its agent answered, or why it failed or was
    /// skipped, and what to do.
    fn run_detail(&self, r: &model::RoutineRun) -> AnyElement {
        let p = self.p;
        let part = |label: &str, text: String, ink: Hsla| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_3)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(px(size::SMALL))
                        .line_height(rems(1.45))
                        .text_color(ink)
                        .child(text),
                )
        };
        let mut detail = div()
            .px(px(space::L))
            .pb(px(space::M))
            .flex()
            .flex_col()
            .gap(px(space::M));
        for text in &r.posted {
            detail = detail.child(part("Posted", text.clone(), p.ink));
        }
        if !r.error.is_empty() {
            let ink = if r.mark() == model::RunMark::Failed {
                p.bad
            } else {
                p.ink
            };
            detail = detail.child(part("Why", r.error.clone(), ink));
        }
        if !r.hint.is_empty() {
            detail = detail.child(part("What to do", r.hint.clone(), p.ink));
        }
        let said = r.said.trim();
        if !said.is_empty() && !r.posted.iter().any(|t| t.trim() == said) {
            detail = detail.child(part("Its answer", said.to_string(), p.ink_2));
        }
        detail.into_any_element()
    }

    /// The form for a new routine, or one being changed, with its history.
    fn routine_form(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let open = self.routine_open.clone().unwrap_or_default();
        let mut groups = Vec::new();
        let chips = self.agent_chips(
            "routine-agent",
            self.routine_agent.clone().as_deref(),
            cx,
            |this, id| this.routine_agent = Some(id),
        );
        groups.push(self.group(
            None,
            vec![
                self.field_row("Name", Input::new(&self.routine_name_in)),
                self.field_row("Agent", chips),
                div()
                    .p(px(space::L))
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .text_color(p.ink)
                            .child("Prompt"),
                    )
                    .child(Textarea::new(&self.routine_prompt_in))
                    .into_any_element(),
            ],
        ));
        groups.push(self.routine_when_group(open.as_deref(), cx));
        let by_hook_alone = !matches!(self.routine_ui.schedule.repeat.as_str(), "hours" | "days");
        let mut options = vec![
            self.switch_row("routine-on", "On", "", self.routine_on, cx, |this| {
                this.routine_on = !this.routine_on
            }),
            self.switch_row(
                "routine-notify",
                "Notify me",
                "",
                self.routine_notify,
                cx,
                |this| this.routine_notify = !this.routine_notify,
            ),
            self.switch_row(
                "routine-catch-up",
                "Catch up",
                "",
                self.routine_catch_up,
                cx,
                |this| this.routine_catch_up = !this.routine_catch_up,
            ),
            self.switch_row(
                "routine-post",
                "A thread of its own",
                "",
                self.routine_thread,
                cx,
                |this| this.routine_thread = !this.routine_thread,
            ),
        ];
        // On a schedule, it can run by webhook too; with Repeat on Webhook, that's all.
        if !by_hook_alone {
            options.push(self.switch_row(
                "routine-webhook",
                "Webhook too",
                "",
                self.routine_webhook,
                cx,
                |this| this.routine_webhook = !this.routine_webhook,
            ));
        }
        // Where to post, once it has a webhook: to give GitHub or anything else.
        let hook = open
            .as_ref()
            .and_then(|id| self.model.routines.iter().find(|r| &r.id == id))
            .filter(|r| !r.hook.is_empty() && (self.routine_webhook || by_hook_alone))
            .map(|r| (format!("/hooks/{}", r.hook), r.secret.clone()));
        if let Some((path, secret)) = hook {
            let copy_path = path.clone();
            options.push(
                self.row(
                    None,
                    path,
                    Some("Bearer the secret, or signed as GitHub signs".into()),
                    Some(
                        div()
                            .flex()
                            .gap(px(space::XS))
                            .child(
                                self.small_button("copy-hook", "Copy address", false)
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_path.clone(),
                                        ));
                                    }),
                            )
                            .child(
                                self.small_button("copy-secret", "Copy secret", false)
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            secret.clone(),
                                        ));
                                    }),
                            )
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        groups.push(self.group(Some("Options"), options));
        groups.push(self.routine_flow_group(cx));
        // Its runs, which leave nothing in the chat but what they post.
        if open.is_some() && !self.model.routine_runs.is_empty() {
            groups.push(self.routine_history_group(cx));
        }
        let editing = open.clone().unwrap_or_default();
        let save = self
            .small_button("save-routine", "Save", true)
            .on_click(cx.listener(move |this, _, _, cx| {
                let name = this.routine_name_in.read(cx).value().to_string();
                let prompt = this.routine_prompt_in.read(cx).value().to_string();
                let Some(agent) = this.routine_agent.clone() else {
                    this.model.notices.push("Pick the agent it runs.".into());
                    cx.notify();
                    return;
                };
                let schedule = this.routine_schedule(cx).to_json();
                let webhook = this.routine_webhook || schedule.is_null();
                // The flow, one step after another; one an agent made
                // with steps side by side stays as it is.
                let flow = if this.routine_flow_kept.is_empty() {
                    let mut steps = Vec::new();
                    let mut before = "routine".to_string();
                    for (k, (agent, prompt_in)) in this.routine_flow.iter().enumerate() {
                        let Some(agent) = agent.clone() else {
                            this.model
                                .notices
                                .push(format!("Pick step {}'s agent.", k + 1));
                            cx.notify();
                            return;
                        };
                        let name = (k + 1).to_string();
                        steps.push(model::FlowStep {
                            name: name.clone(),
                            agent,
                            prompt: prompt_in.read(cx).value().to_string(),
                            after: vec![before],
                        });
                        before = name;
                    }
                    Some(steps)
                } else {
                    None
                };
                let form = model::RoutineForm {
                    id: editing.clone(),
                    name,
                    agent,
                    prompt,
                    schedule,
                    notify: this.routine_notify,
                    on: this.routine_on,
                    catch_up: this.routine_catch_up,
                    post: if this.routine_thread {
                        "thread"
                    } else {
                        "chat"
                    }
                    .into(),
                    webhook,
                    flow,
                };
                let f = this.model.save_routine(&form);
                this.send(f);
                this.routine_open = None;
                cx.notify();
            }));
        let cancel = self
            .small_button("cancel-routine", "Cancel", false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.routine_open = None;
                cx.notify();
            }));
        let mut left = div().flex().gap(px(space::S));
        if let Some(id) = open.clone() {
            let run = id.clone();
            left = left
                .child(
                    self.small_button("delete-routine", "Delete", false)
                        .text_color(p.bad)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.delete_routine(&id);
                            this.send(f);
                            this.routine_open = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.small_button("run-routine", "Run now", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.run_routine(&run);
                            this.send(f);
                            this.routine_open = None;
                            cx.notify();
                        })),
                );
        }
        groups.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(left)
                .child(div().flex().gap(px(space::S)).child(cancel).child(save))
                .into_any_element(),
        );
        let title = if open.is_some() {
            "Change routine"
        } else {
            "New routine"
        };
        self.settings_page("routines-page", title, None, groups)
    }

    /// Fills the routine form's flow from `flow`: steps it edits when they go one after
    /// another, or kept as they are when an agent put some side by side.
    fn set_routine_flow(
        &mut self,
        flow: &[model::FlowStep],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.routine_flow.clear();
        self.routine_flow_kept.clear();
        if !model::flow_is_chain(flow) {
            self.routine_flow_kept = flow.to_vec();
            return;
        }
        for step in flow {
            let prompt = step.prompt.clone();
            let input = cx.new(|cx| {
                let mut t = TextareaState::new(window, cx)
                    .auto_grow(2, 10)
                    .placeholder("What this agent is asked, with the answer before it.");
                t.set_value(prompt, window, cx);
                t
            });
            self.routine_flow.push((Some(step.agent.clone()), input));
        }
    }

    /// The routine form's flow (PLAN.md, "Routine flows"): who its answer goes to next,
    /// step by step.
    fn routine_flow_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let name_of = |id: &str| {
            self.model
                .agent(id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "a deleted agent".into())
        };
        let mut rows = Vec::new();
        if !self.routine_flow_kept.is_empty() {
            let lines: Vec<String> = self
                .routine_flow_kept
                .iter()
                .map(|s| {
                    format!(
                        "{} ({}), after {}: {}",
                        s.name,
                        name_of(&s.agent),
                        s.after.join(" and "),
                        s.prompt
                    )
                })
                .collect();
            rows.push(
                self.row(
                    None,
                    "Steps side by side",
                    Some("Set up by an agent".into()),
                    Some(
                        self.small_button("flow-off", "Take it off", false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.routine_flow_kept.clear();
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
            rows.push(self.note_row(lines.join("\n"), p.ink_2));
            return self.group(Some("Then"), rows);
        }
        for (k, (picked, prompt_in)) in self.routine_flow.iter().enumerate() {
            let from = if k == 0 {
                "the routine's answer".to_string()
            } else {
                format!("step {k}'s answer")
            };
            let remove = self
                .small_button(format!("flow-{k}-remove"), "Remove", false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if k < this.routine_flow.len() {
                        this.routine_flow.remove(k);
                    }
                    cx.notify();
                }));
            let mut pick = div().flex().flex_wrap().gap(px(space::XS));
            let agents = self.model.agents.clone();
            for a in agents.iter() {
                let id = a.id.clone();
                let on = picked.as_deref() == Some(a.id.as_str());
                pick = pick.child(
                    self.segment(format!("flow-{k}-agent-{}", a.id), &a.name, on)
                        .when(!on, |d| d.border_1().border_color(p.line))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(step) = this.routine_flow.get_mut(k) {
                                step.0 = Some(id.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            rows.push(
                div()
                    .p(px(space::L))
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(size::BODY))
                                    .text_color(p.ink)
                                    .child(format!("Step {}, with {from}", k + 1)),
                            )
                            .child(remove),
                    )
                    .child(pick)
                    .child(Textarea::new(prompt_in))
                    .into_any_element(),
            );
        }
        if self.routine_flow.len() < 20 {
            rows.push(
                self.row(
                    None,
                    "Hand the answer on",
                    None,
                    Some(
                        self.small_button("flow-add", "Add a step", false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let input = cx.new(|cx| {
                                    TextareaState::new(window, cx).auto_grow(2, 10).placeholder(
                                        "What this agent is asked, with the answer before it.",
                                    )
                                });
                                this.routine_flow.push((None, input));
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        self.group(Some("Then"), rows)
    }
}
