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
    /// The event it runs on, for "On an event", as it's picked, and the T3 thread typed.
    pub event: model::watch::RoutineEvent,
    pub thread_in: Entity<InputState>,
    /// Its script, a command or a Calx script (`use_calx` says which shows), and the
    /// seconds it may take, as typed, and whether it runs its script only.
    pub gate_in: Entity<InputState>,
    pub calx_in: Entity<TextareaState>,
    pub use_calx: bool,
    pub gate_seconds_in: Entity<InputState>,
    pub script_only: bool,
    /// Its runs' model and effort as picked, values its agent's program takes; empty for
    /// its agent's own.
    pub model: String,
    pub effort: String,
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
        let thread_in = field(window, cx, "Thread id");
        let gate_in = field(window, cx, "gh pr view 8006 --json state | ./changed.sh");
        let gate_seconds_in = field(window, cx, "60");
        let calx_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 24)
                .placeholder("fn main() { … }")
        });
        for state in [&thread_in, &gate_in, &gate_seconds_in] {
            super::form_drafts::watch(state, window, cx);
        }
        super::form_drafts::watch(&calx_in, window, cx);
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
            event: model::watch::RoutineEvent::default(),
            thread_in,
            gate_in,
            calx_in,
            use_calx: false,
            gate_seconds_in,
            script_only: false,
            model: String::new(),
            effort: String::new(),
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

    /// Opens the routine form for a new one: empty, or as the person left it.
    pub(super) fn new_routine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        _ = self.model.load_routine_runs(None);
        let own = model::drafts::RoutineDraft::new(self.model.selected.clone());
        let own = model::drafts::Form::Routine(Box::new(own));
        self.fill_form(DraftPage::Routines, None, own, window, cx);
        self.routine_open = Some(None);
        self.routine_schedule_changed(cx);
    }

    /// Opens routine `r`'s page, to change it and see its history: its form as it is,
    /// or as the person left it.
    pub(super) fn open_routine(
        &mut self,
        r: &model::Routine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let own = model::drafts::Form::Routine(Box::new(model::drafts::RoutineDraft::of(r)));
        self.fill_form(DraftPage::Routines, Some(r.id.clone()), own, window, cx);
        if let Some(f) = self.model.load_routine_runs(Some(&r.id)) {
            self.send(f);
        }
        let f = self.model.load_routine_state(&r.id);
        self.send(f);
        self.routine_open = Some(Some(r.id.clone()));
        self.routine_schedule_changed(cx);
    }

    /// The routines page: the person's routines down the left, and beside them the
    /// week's calendar until one is picked, then its page, or a new one's. Where they
    /// show one at a time, the calendar is over the list.
    pub(super) fn routines_page(&mut self, cx: &mut Context<Self>) -> Div {
        let routines = self.model.routines.clone();
        if routines.is_empty() && self.routine_open.is_none() {
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
        let split = self.list_split();
        let keys: Vec<&str> = routines.iter().map(|r| r.id.as_str()).collect();
        let open = self.routine_open.clone();
        let shows = popup::shows(split, open.as_ref().map(|o| o.as_deref()), &keys, true);
        let query = self.list_query(cx);
        let found = (0..routines.len())
            .filter(|&i| {
                let r = &routines[i];
                let agent = self.model.agent(&r.agent).map(|a| a.name.clone());
                let text = format!("{} {} {}", r.name, agent.unwrap_or_default(), r.when);
                popup::finds(&query, &text)
            })
            .collect();
        let (pane_key, pane) = match shows.pane {
            Pane::Overview => ("week".to_string(), vec![self.routine_week(cx)]),
            Pane::Item(i) => (routines[i].id.clone(), self.routine_form(cx)),
            Pane::New => ("new".to_string(), self.routine_form(cx)),
            Pane::Nothing => (String::new(), Vec::new()),
        };
        // Side by side, the calendar is the list's first row; one at a time, it's over
        // the list.
        let head = if split {
            let lit = shows.pane == Pane::Overview;
            Some(
                self.list_row(
                    "routines-week",
                    self.tile(Ico::CalendarDays),
                    "Calendar",
                    String::new(),
                    None,
                    lit,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.leave_form(cx);
                    this.routine_open = None;
                    cx.notify();
                }))
                .into_any_element(),
            )
        } else {
            Some(
                div()
                    .p(px(space::S))
                    .child(self.routine_week(cx))
                    .into_any_element(),
            )
        };
        let action = Some(self.new_routine_button(cx));
        self.list_detail(
            ListPage {
                id: "routines-page",
                title: "Routines",
                action,
                shows,
                found,
                row: Loom::routine_row,
                head,
                foot: None,
                pane_key,
                pane,
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
                    c.bg(motion::mix(self.card_fill(), p.button, 0.035))
                });
            let in_day: Vec<&Mark> = marks.iter().filter(|m| m.day == d).collect();
            for m in &in_day {
                let id = m.id.clone();
                if m.tick {
                    // In the lanes right of the blocks, one a routine.
                    let lane = tick_lanes.iter().position(|l| *l == m.id).unwrap_or(0) % 4;
                    let lane_w = (1.0 - block_share - 0.06) / lanes as f32;
                    column = column.child(
                        div()
                            .absolute()
                            .top(px(top_of(m.minute) - 1.0))
                            .h(px(2.0))
                            .left(relative(block_share + 0.03 + lane as f32 * lane_w))
                            .w(relative(lane_w * 0.8))
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

    /// What a routine's row and its page's head say: its agent and tint, when it runs
    /// next ("next at 08:00", or paused), how its last run went, unless it went well, and
    /// its flow's steps while it runs, with whether it was stopped.
    fn routine_words(
        &self,
        r: &model::Routine,
    ) -> (String, Hsla, String, String, Option<(String, bool)>) {
        let p = self.p;
        let (agent, tint) = match self.model.agent(&r.agent) {
            Some(a) => (a.name.clone(), thread::color(&a.color, p.dark)),
            None => ("a deleted agent".to_string(), p.ink_3),
        };
        // One on an event or its webhook alone has no next run: it says what runs it.
        let next = if !r.on {
            "paused".to_string()
        } else if let Some(when) = self.model.routine_unscheduled(r) {
            when
        } else if r.next == 0 {
            "paused".to_string()
        } else if day_of(r.next) == "Today" {
            format!("next at {}", time_of(r.next))
        } else {
            format!("next {}", next_words(r.next, Local::now().date_naive()))
        };
        let last = match r.last_status.as_str() {
            "" | "done" | "quiet" => String::new(),
            "running" => "running now".into(),
            "waiting" => "waiting for the chat".into(),
            "skipped" => format!("skipped at {}", time_of(r.last_at)),
            _ => format!("failed at {}: {}", time_of(r.last_at), r.last_error),
        };
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
        (agent, tint, next, last, running)
    }

    /// A routine's clock, in its agent's colour while it's on.
    fn routine_lead(&self, r: &model::Routine, tint: Hsla, big: bool) -> AnyElement {
        let p = self.p;
        let ink = if r.on { tint } else { p.ink_3 };
        if big {
            return self.big_tile(Ico::Clock, Some(ink));
        }
        div()
            .size(px(control::LEAD))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .border_1()
            .border_color(p.line)
            .bg(p.bg)
            .text_color(ink)
            .child(Icon::new(Ico::Clock).small())
            .into_any_element()
    }

    /// One routine, as a row of the list: how its last run went when it went wrong, or
    /// when it runs next, and a dot for how it is; it opens its page.
    fn routine_row(&mut self, ix: usize, picked: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(r) = self.model.routines.get(ix).cloned() else {
            return div().into_any_element();
        };
        let (_, tint, next, last, running) = self.routine_words(&r);
        let line = match running {
            Some(_) => "running now".to_string(),
            None if !last.is_empty() => last,
            None if r.cost.wasted => r.cost.warning(),
            None => next,
        };
        self.list_row_unsaved(
            format!("routine-{}", r.id),
            self.routine_lead(&r, tint, false),
            r.name.clone(),
            line,
            popup::routine_dot(&r),
            self.unsaved(DraftPage::Routines, &r.id),
            picked,
        )
        .on_click(cx.listener(move |this, _, window, cx| this.open_routine(&r, window, cx)))
        .into_any_element()
    }

    /// The head of a routine's page: its agent, when it runs and next, and how its last
    /// run went, or its flow's steps while it runs, with a Stop; and the way to its
    /// thread, for one that has one.
    fn routine_head(&self, r: &model::Routine, cx: &mut Context<Self>) -> AnyElement {
        let (agent, tint, next, last, running) = self.routine_words(r);
        let when = self.model.routine_when(r);
        let mut line = if when == next {
            format!("{agent} · {when}")
        } else {
            format!("{agent} · {when} · {next}")
        };
        if !last.is_empty() {
            line = format!("{line} · {last}");
        }
        let mut controls = div().flex().gap(px(space::XS));
        if let Some((text, stopped)) = running {
            line = text;
            if !stopped {
                let routine = r.id.clone();
                controls = controls.child(
                    self.small_button(format!("stop-flow-{}", r.id), "Stop", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
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
                        this.open_thread(&agent, &conversation, cx);
                    })),
            );
        }
        // What it costs, in the warn ink while it spends a lot for nothing.
        let p = self.p;
        let ink = if r.cost.wasted { p.warn } else { p.ink_3 };
        self.pane_head_noted(
            Some(self.routine_lead(r, tint, true)),
            r.name.clone(),
            Some(line),
            Some((r.cost.words(), ink)),
            Some(controls.into_any_element()),
        )
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
        // On an event, once there's something to watch: an Acta board, or a T3 server.
        let watchable = !self.model.acta_boards.is_empty()
            || !self.model.event_servers("t3").is_empty()
            || repeat == "event";
        let mut kinds = Vec::new();
        for (kind, label) in [
            ("hours", "Every few hours"),
            ("days", "On days"),
            ("webhook", "Webhook"),
            ("event", "On an event"),
        ] {
            if kind == "event" && !watchable {
                continue;
            }
            let on = repeat == kind || (kind == "webhook" && repeat.is_empty());
            kinds.push(
                self.segment(format!("routine-repeat-{kind}"), label, on)
                    // Four fit a settings panel's width when they sit closer.
                    .when(watchable, |d| d.px(px(space::S)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.routine_ui.schedule.repeat = kind.into();
                        this.routine_ui.adding_time = false;
                        if kind == "event" && this.routine_ui.event.integration.is_empty() {
                            this.routine_ui.event = this.new_event("acta");
                        }
                        this.routine_schedule_changed(cx);
                    })),
            );
        }
        // Its choices go under "Repeat" when the panel's too narrow for both, and onto a
        // second line when it's narrower still.
        let mut rows = vec![div()
            .min_h(px(control::ROW))
            .px(px(space::L))
            .py(px(space::S + 2.0))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(space::S))
            .child(
                div()
                    .text_size(px(size::BODY))
                    .text_color(self.p.ink)
                    .child("Repeat"),
            )
            .child(self.segments(kinds).flex_wrap())
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
        if repeat == "event" {
            rows.extend(self.routine_event_rows(cx));
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

    /// A new event for "On an event": on Acta, a card on the person's first board moving
    /// to Done; on T3, the first T3 server's thread finishing. Acta when T3 is asked for
    /// and there's none, and T3 when Acta is and the person has no boards.
    fn new_event(&self, integration: &str) -> model::watch::RoutineEvent {
        let boards = &self.model.acta_boards;
        let t3 = self.model.event_servers("t3");
        let on_t3 = (integration == "t3" && !t3.is_empty()) || boards.is_empty();
        if on_t3 && !t3.is_empty() {
            return model::watch::RoutineEvent::new_t3(&t3[0]);
        }
        model::watch::RoutineEvent::new_acta(boards.first().map_or("", |(_, b)| b.as_str()))
    }

    /// "On an event"'s rows: Acta or T3 when the person has both, which server when they
    /// have several, and on Acta the board and what happens on it (a move to a status,
    /// a new card or a comment), or on T3 the thread.
    fn routine_event_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let e = self.routine_ui.event.clone();
        let mut rows = Vec::new();
        let boards = self.model.acta_boards.clone();
        let t3 = self.model.event_servers("t3");
        if !boards.is_empty() && !t3.is_empty() {
            let mut kinds = Vec::new();
            for (kind, label) in [("acta", "Acta"), ("t3", "T3")] {
                kinds.push(
                    self.segment(
                        format!("routine-event-{kind}"),
                        label,
                        e.integration == kind,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.routine_ui.event.integration != kind {
                            this.routine_ui.event = this.new_event(kind);
                            this.keep_draft(cx);
                        }
                        cx.notify();
                    })),
                );
            }
            rows.push(
                self.row(
                    None,
                    "On",
                    None,
                    Some(self.segments(kinds).into_any_element()),
                )
                .into_any_element(),
            );
        }
        // Which server, when there are several of its kind.
        let servers = self.model.event_servers(&e.integration);
        if servers.len() > 1 {
            let mut chips = div().flex().flex_wrap().gap(px(space::XS));
            for name in servers {
                let on = e.server == name
                    || (e.server.is_empty() && e.integration == "acta" && name == "acta");
                let picked = name.clone();
                chips = chips.child(
                    self.chip(
                        SharedString::from(format!("routine-server-{name}")),
                        &name,
                        on,
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let picked = picked.clone();
                        this.change_event(cx, |e| e.server = picked);
                    })),
                );
            }
            rows.push(self.field_row("Server", chips));
        }
        if e.integration == "t3" {
            rows.push(self.field_row("Thread", Input::new(&self.routine_ui.thread_in)));
            return rows;
        }
        // The board, from the person's own.
        let mut chips = div().flex().flex_wrap().gap(px(space::XS));
        for (_, board) in boards {
            let picked = board.clone();
            chips = chips.child(
                self.chip(
                    SharedString::from(format!("routine-board-{board}")),
                    &model::watch::board_label(&board),
                    e.board == board,
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let picked = picked.clone();
                    this.change_event(cx, |e| {
                        // One card's event is on its board: another board is any card's.
                        if e.board != picked {
                            e.card.clear();
                        }
                        e.board = picked;
                    });
                })),
            );
        }
        rows.push(self.field_row("Board", chips));
        // What happens on it: a move to a status, a new card, or a comment.
        let mut kinds = Vec::new();
        for (kind, label) in [
            ("moved", "Moves to"),
            ("created", "New card"),
            ("commented", "Comment"),
        ] {
            kinds.push(
                self.segment(format!("routine-event-kind-{kind}"), label, e.kind == kind)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change_event(cx, |e| {
                            e.kind = kind.into();
                            if kind == "moved" && e.to.is_empty() {
                                e.to = "done".into();
                            }
                        });
                    })),
            );
        }
        rows.push(
            self.row(
                None,
                "Event",
                None,
                Some(self.segments(kinds).into_any_element()),
            )
            .into_any_element(),
        );
        if e.kind == "moved" {
            let mut chips = div().flex().flex_wrap().gap(px(space::XS));
            for (key, label) in model::watch::STATUSES {
                chips = chips.child(
                    self.chip(
                        SharedString::from(format!("routine-status-{key}")),
                        label,
                        e.to == key,
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change_event(cx, |e| e.to = key.into());
                    })),
                );
            }
            rows.push(self.field_row("Status", chips));
        }
        rows
    }

    /// Changes the event "On an event" has picked, which the routine's draft keeps.
    fn change_event(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut model::watch::RoutineEvent),
    ) {
        change(&mut self.routine_ui.event);
        self.keep_draft(cx);
        cx.notify();
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
        if !r.chosen.is_empty() {
            detail = detail.child(part("Chosen", r.chosen.clone(), p.ink_2));
        }
        if !r.printed.trim().is_empty() {
            detail = detail.child(
                part("Printed", r.printed.trim().to_string(), p.ink_2).font_family(font::MONO),
            );
        }
        detail.into_any_element()
    }

    /// The routine's script (PLAN.md, "Routines"): a command or a Calx script, the
    /// seconds it may take, and Run test, with how its test went under it, or else what
    /// its last run printed.
    fn routine_script_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let ui = &self.routine_ui;
        let calx = ui.use_calx;
        let mut kinds = div().flex().gap(px(space::XS + 2.0));
        for (word, is_calx) in [("Command", false), ("Calx", true)] {
            kinds = kinds.child(
                self.chip(
                    format!("routine-script-{word}"),
                    word,
                    calx == is_calx,
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.routine_ui.use_calx = is_calx;
                    this.keep_draft(cx);
                    cx.notify();
                })),
            );
        }
        let testing =
            self.model.routine_test.as_ref().map(|t| t.status.as_str()) == Some("running");
        let test = self
            .small_button("test-routine", "Run test", false)
            .when(testing, |b| b.opacity(0.5))
            .on_click(cx.listener(move |this, _, _, cx| {
                if testing {
                    return;
                }
                let Some(form) = this.routine_form_now(cx) else {
                    cx.notify();
                    return;
                };
                let f = this.model.test_routine(&form);
                this.send(f);
                cx.notify();
            }));
        let head = div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .child(
                div()
                    .w(px(layout::FIELD_LABEL))
                    .flex_none()
                    .text_size(px(size::BODY))
                    .text_color(p.ink)
                    .child("Script"),
            )
            .child(kinds);
        // Under the script, the seconds it may take, and Run test.
        let foot = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .w(px(56.0))
                    .flex_none()
                    .child(Input::new(&ui.gate_seconds_in)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("s"),
            )
            .child(div().flex_1())
            .child(test);
        let field = if calx {
            Textarea::new(&ui.calx_in).into_any_element()
        } else {
            Input::new(&ui.gate_in).into_any_element()
        };
        let mut row = div()
            .px(px(space::L))
            .py(px(space::S + 2.0))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .child(head)
            .child(div().font_family(font::MONO).child(field))
            .child(foot);
        if let Some(line) = self.routine_script_result() {
            row = row.child(line);
        }
        row.into_any_element()
    }

    /// Under the script: how its test went, a pill and a line with what it printed,
    /// posted and wrote; or, before one, what its last run printed.
    fn routine_script_result(&self) -> Option<AnyElement> {
        let p = self.p;
        let out = |label: &str, text: &str| {
            div()
                .flex()
                .gap(px(space::S))
                .child(
                    div()
                        .flex_none()
                        .w(px(52.0))
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(font::MONO)
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_2)
                        .line_clamp(6)
                        .child(text.trim().to_string()),
                )
        };
        if let Some(t) = &self.model.routine_test {
            let tint = match t.status.as_str() {
                "failed" => p.bad,
                "running" => p.ink_3,
                _ => p.ink,
            };
            let mut d = div().flex().flex_col().gap(px(space::XS)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
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
                            .child(t.pill()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_2)
                            .child(t.said.clone()),
                    ),
            );
            if !t.printed.trim().is_empty() {
                d = d.child(out("Printed", &t.printed));
            }
            if !t.errors.trim().is_empty() {
                d = d.child(out("Errors", &t.errors));
            }
            for text in &t.posted {
                d = d.child(out("Posts", text));
            }
            if !t.wrote.is_empty() {
                d = d.child(out("State", &t.wrote.join(" · ")));
            }
            return Some(d.into_any_element());
        }
        let last = self
            .model
            .routine_runs
            .iter()
            .find(|r| !r.printed.trim().is_empty())?;
        Some(out("Last run", &last.printed).into_any_element())
    }

    /// What the open routine's script keeps between runs (`loom state`), a row a key,
    /// each with Forget.
    fn routine_state_group(&self, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.p;
        if self.model.routine_state.is_empty() {
            return None;
        }
        let mut rows = Vec::new();
        for (key, value) in &self.model.routine_state {
            let routine = id.to_string();
            let forget = key.clone();
            rows.push(
                div()
                    .px(px(space::L))
                    .py(px(space::S))
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(
                        div()
                            .w(px(layout::FIELD_LABEL))
                            .flex_none()
                            .truncate()
                            .font_family(font::MONO)
                            .text_size(px(size::SMALL))
                            .text_color(p.ink)
                            .child(key.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(font::MONO)
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_2)
                            .child(value.clone()),
                    )
                    .child(
                        self.small_button(format!("forget-state-{key}"), "Forget", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.forget_routine_state(&routine, &forget);
                                this.send(f);
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        Some(self.group(Some("State"), rows))
    }

    /// The routine's model or effort (`effort`), as chips of what its agent's program
    /// offers for `option`: its agent's own, `own`, marked default, which it follows
    /// until another is picked.
    fn routine_pick_row(
        &self,
        label: &str,
        option: &model::ConfigOption,
        own: String,
        effort: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picked = if effort {
            &self.routine_ui.effort
        } else {
            &self.routine_ui.model
        };
        let on = if option.values.iter().any(|(v, _)| v == picked) {
            picked.clone()
        } else {
            own.clone()
        };
        let mut row = div().flex().flex_wrap().gap(px(space::XS + 2.0));
        for (value, name) in &option.values {
            let is_own = *value == own;
            let chosen = if is_own { String::new() } else { value.clone() };
            row = row.child(
                self.chip(
                    format!("routine-{}-{value}", option.category),
                    name,
                    *value == on,
                    is_own,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if effort {
                        this.routine_ui.effort = chosen.clone();
                    } else {
                        this.routine_ui.model = chosen.clone();
                    }
                    cx.notify();
                })),
            );
        }
        self.field_row(label, row)
    }

    /// What the routine form says, to save or to test: none, with a notice why, while
    /// something in it can't be saved.
    fn routine_form_now(&mut self, cx: &mut Context<Self>) -> Option<model::RoutineForm> {
        let editing = self.routine_open.clone().flatten().unwrap_or_default();
        let name = self.routine_name_in.read(cx).value().to_string();
        let prompt = self.routine_prompt_in.read(cx).value().to_string();
        let Some(agent) = self.routine_agent.clone() else {
            self.model.notices.push("Pick the agent it runs.".into());
            cx.notify();
            return None;
        };
        let schedule = self.routine_schedule(cx).to_json();
        let repeat = self.routine_ui.schedule.repeat.clone();
        let webhook = self.routine_webhook || matches!(repeat.as_str(), "" | "webhook");
        // On an event: the one picked. Off one it ran on alone: taken away. One
        // it runs on beside its schedule stays as it is.
        let event = if repeat == "event" {
            let mut e = self.routine_ui.event.clone();
            e.thread = self.routine_ui.thread_in.read(cx).value().to_string();
            if let Some(why) = e.missing() {
                self.model.notices.push(why.into());
                cx.notify();
                return None;
            }
            Some(e.to_json())
        } else {
            let saved = self.model.routines.iter().find(|r| r.id == editing);
            saved
                .filter(|r| r.event.is_some() && r.schedule.repeat.is_empty())
                .map(|_| Value::Null)
        };
        // The flow, one step after another; one an agent made
        // with steps side by side stays as it is.
        let flow = if self.routine_flow_kept.is_empty() {
            let mut steps = Vec::new();
            let mut before = "routine".to_string();
            for (k, (agent, prompt_in)) in self.routine_flow.iter().enumerate() {
                let Some(agent) = agent.clone() else {
                    self.model
                        .notices
                        .push(format!("Pick step {}'s agent.", k + 1));
                    cx.notify();
                    return None;
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
        // Its script is the one shown, a command or a Calx script.
        let ui = &self.routine_ui;
        let (gate, calx) = if ui.use_calx {
            (String::new(), ui.calx_in.read(cx).value().to_string())
        } else {
            (
                ui.gate_in.read(cx).value().trim().to_string(),
                String::new(),
            )
        };
        let seconds = ui.gate_seconds_in.read(cx).value().trim().to_string();
        let gate_seconds = match seconds.parse::<i64>() {
            Ok(s) if s > 0 => s,
            _ if seconds.is_empty() || (gate.is_empty() && calx.trim().is_empty()) => 0,
            _ => {
                self.model
                    .notices
                    .push("Write the seconds its script may take as a number.".into());
                cx.notify();
                return None;
            }
        };
        // A model or effort the agent's program doesn't offer, as when the agent
        // was changed, goes back to the agent's own.
        let offered = |category: &str, value: &str| {
            let a = self.model.agent(&agent);
            let options = a.map(|a| a.picks()).unwrap_or_default();
            match options.iter().find(|o| o.category == category) {
                Some(o) if !o.values.iter().any(|(v, _)| v == value) => String::new(),
                _ => value.to_string(),
            }
        };
        let model_picked = offered("model", &ui.model);
        let effort_picked = offered("thought_level", &ui.effort);
        Some(model::RoutineForm {
            id: editing,
            name,
            agent,
            prompt,
            schedule,
            gate,
            calx,
            gate_seconds,
            script_only: ui.script_only,
            model: model_picked,
            effort: effort_picked,
            notify: self.routine_notify,
            on: self.routine_on,
            catch_up: self.routine_catch_up,
            post: if self.routine_thread {
                "thread"
            } else {
                "chat"
            }
            .into(),
            webhook,
            flow,
            event,
        })
    }

    /// The page of the routine picked, beside the list, with its form and history, or a
    /// new routine's form.
    fn routine_form(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.p;
        let open = self.routine_open.clone().unwrap_or_default();
        let head = match open
            .as_ref()
            .and_then(|id| self.model.routines.iter().find(|r| &r.id == id))
            .cloned()
        {
            Some(r) => self.routine_head(&r, cx),
            None => self.pane_head(None, "New routine", None, None),
        };
        let mut groups = vec![head];
        let chips = self.agent_chips(
            "routine-agent",
            self.routine_agent.clone().as_deref(),
            cx,
            |this, id| this.routine_agent = Some(id),
        );
        let script_only = self.routine_ui.script_only;
        let mut first = vec![
            self.field_row("Name", Input::new(&self.routine_name_in)),
            self.field_row("Agent", chips),
            self.switch_row(
                "routine-script-only",
                "Script only",
                "",
                script_only,
                cx,
                |this| this.routine_ui.script_only = !this.routine_ui.script_only,
            ),
        ];
        // One that runs its script only asks its agent nothing, so it has no prompt,
        // model or effort.
        if !script_only {
            first.push(
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
            );
        }
        first.push(self.routine_script_row(cx));
        // Its model and effort, from those its agent's program offers.
        let agent = self
            .routine_agent
            .as_deref()
            .and_then(|id| self.model.agent(id))
            .cloned();
        if let (Some(a), false) = (agent, script_only) {
            for option in a.picks() {
                let effort = option.category == "thought_level";
                let label = if effort { "Effort" } else { "Model" };
                first.push(self.routine_pick_row(label, &option, a.chosen(&option.id), effort, cx));
            }
        }
        // What its script may choose for a run in place of these, set by an agent.
        let may = open
            .as_ref()
            .and_then(|id| self.model.routines.iter().find(|r| &r.id == id))
            .map(|r| r.gate_may_said.clone())
            .filter(|said| !said.is_empty() && !script_only);
        if let Some(said) = may {
            first.push(self.note_row(said, p.ink_3));
        }
        groups.push(self.group(None, first));
        groups.push(self.routine_when_group(open.as_deref(), cx));
        let by_hook_alone = matches!(self.routine_ui.schedule.repeat.as_str(), "" | "webhook");
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
        // On a schedule or an event, it can run by webhook too; with Repeat on Webhook,
        // that's all.
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
        if let Some(state) = open
            .as_deref()
            .and_then(|id| self.routine_state_group(id, cx))
        {
            groups.push(state);
        }
        // Its runs, which leave nothing in the chat but what they post.
        if open.is_some() && !self.model.routine_runs.is_empty() {
            groups.push(self.routine_history_group(cx));
        }
        let editing = open.clone().unwrap_or_default();
        let save = self
            .small_button("save-routine", "Save", true)
            .on_click(cx.listener(move |this, _, _, cx| {
                let Some(form) = this.routine_form_now(cx) else {
                    cx.notify();
                    return;
                };
                let now = this.form_now(DraftPage::Routines, cx);
                let f = this.model.save_routine(&form);
                // Its draft stays until the instance says it's saved; the form closes.
                let key = (!editing.is_empty()).then(|| editing.clone());
                this.form_saving(DraftPage::Routines, key, &f, now);
                this.send(f);
                this.leave_form(cx);
                this.routine_open = None;
                cx.notify();
            }));
        let cancel = self
            .small_button("cancel-routine", "Cancel", false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.routine_open = None;
                this.model.drafts.discard();
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
                            this.model.drafts.discard();
                            cx.notify();
                        })),
                )
                .child(
                    self.small_button("run-routine", "Run now", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.run_routine(&run);
                            this.send(f);
                            this.leave_form(cx);
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
        groups
    }

    /// Fills the routine form's flow: the steps it edits, one after another, each its
    /// agent and prompt, or `kept`, steps an agent put side by side, kept as they are.
    pub(super) fn set_routine_flow(
        &mut self,
        steps: &[(Option<String>, String)],
        kept: Vec<model::FlowStep>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.routine_flow.clear();
        self.routine_flow_kept = kept;
        for (agent, prompt) in steps {
            let prompt = prompt.clone();
            let input = cx.new(|cx| {
                let mut t = TextareaState::new(window, cx)
                    .auto_grow(2, 10)
                    .placeholder("What this agent is asked, with the answer before it.");
                t.set_value(prompt, window, cx);
                t
            });
            form_drafts::watch(&input, window, cx);
            self.routine_flow.push((agent.clone(), input));
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
                                form_drafts::watch(&input, window, cx);
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
