//! Each item's form on a Settings list page, kept as the person left it
//! (`model::drafts`): what the form's fields hold, read and filled in as an item is
//! picked, and whether a row has changes not saved, for its dot.

use super::*;
use crate::model::drafts::{self, Form, Page, RoutineDraft};

/// The kinds of form whose drafts are kept on the popup's page `screen`: Skills has the
/// skills and what's proposed at the top of its list, and Spools' Installed the person's
/// MCP servers.
pub(super) fn draft_pages(screen: Screen) -> &'static [Page] {
    match screen {
        Screen::Programs => &[Page::Brains],
        Screen::Skills => &[Page::Skills, Page::Proposals],
        Screen::Installed => &[Page::Mcp],
        Screen::Routines => &[Page::Routines],
        _ => &[],
    }
}

/// A change typed in a list page's form keeps it as its item's draft, and its row's dot
/// comes or goes.
pub(super) fn watch<T: EventEmitter<InputEvent>>(
    state: &Entity<T>,
    window: &mut Window,
    cx: &mut Context<Loom>,
) {
    cx.subscribe_in(
        state,
        window,
        |this: &mut Loom, _, event: &InputEvent, _, cx| {
            if let InputEvent::Change = event {
                if this.keep_draft(cx) {
                    cx.notify();
                }
            }
        },
    )
    .detach();
}

fn put(state: &Entity<InputState>, value: &str, window: &mut Window, cx: &mut Context<Loom>) {
    let value = value.to_string();
    state.update(cx, |s, cx| s.set_value(value, window, cx));
}

fn put_area(
    state: &Entity<TextareaState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<Loom>,
) {
    let value = value.to_string();
    state.update(cx, |s, cx| s.set_value(value, window, cx));
}

impl Loom {
    /// The proposal the form is filled in for.
    fn proposal_filled(&self) -> Option<&model::Proposal> {
        let (_, id) = self.model.drafts.open_key()?;
        let id = id.as_deref()?;
        self.proposal_open
            .as_ref()
            .filter(|k| k.id == id)
            .or_else(|| self.model.proposals.iter().find(|k| k.id == id))
    }

    /// What the form of `page` holds now.
    pub(super) fn form_now(&self, page: Page, cx: &App) -> Option<Form> {
        let text = |s: &Entity<InputState>| s.read(cx).value().to_string();
        let area = |s: &Entity<TextareaState>| s.read(cx).value().to_string();
        Some(match page {
            Page::Mcp => Form::Fields(vec![
                text(&self.mcp_name_in),
                text(&self.mcp_cmd_in),
                area(&self.mcp_settings_in),
                text(&self.mcp_client_in),
                text(&self.mcp_client_secret_in),
            ]),
            Page::Skills => Form::Fields(vec![
                text(&self.skill_name_in),
                text(&self.skill_desc_in),
                area(&self.skill_body_in),
            ]),
            Page::Proposals => Form::Fields(match self.proposal_filled()?.kind.as_str() {
                "skill" => vec![
                    text(&self.skill_name_in),
                    text(&self.skill_desc_in),
                    area(&self.skill_body_in),
                ],
                "fix" => vec![area(&self.skill_body_in)],
                _ => vec![text(&self.proposal_note_in)],
            }),
            Page::Brains => drafts::brain_fields(
                &text(&self.setting_key_in),
                &text(&self.setting_value_in),
                &text(&self.share_in),
            ),
            Page::Routines => {
                let ui = &self.routine_ui;
                Form::Routine(Box::new(RoutineDraft {
                    name: text(&self.routine_name_in),
                    prompt: area(&self.routine_prompt_in),
                    agent: self.routine_agent.clone(),
                    schedule: ui.schedule.clone(),
                    every: text(&ui.every_in),
                    minutes: ui.minutes,
                    from: text(&ui.from_in),
                    to: text(&ui.to_in),
                    notify: self.routine_notify,
                    on: self.routine_on,
                    catch_up: self.routine_catch_up,
                    thread: self.routine_thread,
                    webhook: self.routine_webhook,
                    flow: self
                        .routine_flow
                        .iter()
                        .map(|(agent, prompt)| (agent.clone(), area(prompt)))
                        .collect(),
                    flow_kept: self.routine_flow_kept.clone(),
                    event: model::watch::RoutineEvent {
                        thread: text(&ui.thread_in),
                        ..ui.event.clone()
                    },
                    gate: text(&ui.gate_in),
                    calx: area(&ui.calx_in),
                    use_calx: ui.use_calx,
                    gate_seconds: text(&ui.gate_seconds_in),
                    script_only: ui.script_only,
                    model: ui.model.clone(),
                    effort: ui.effort.clone(),
                }))
            }
        })
    }

    /// Fills the form of `page` with `form`.
    fn put_form(&mut self, page: Page, form: Form, window: &mut Window, cx: &mut Context<Self>) {
        let at = |f: &[String], i: usize| f.get(i).cloned().unwrap_or_default();
        match (page, form) {
            (Page::Mcp, Form::Fields(f)) => {
                put(&self.mcp_name_in, &at(&f, 0), window, cx);
                put(&self.mcp_cmd_in, &at(&f, 1), window, cx);
                put_area(&self.mcp_settings_in, &at(&f, 2), window, cx);
                put(&self.mcp_client_in, &at(&f, 3), window, cx);
                put(&self.mcp_client_secret_in, &at(&f, 4), window, cx);
            }
            (Page::Skills, Form::Fields(f)) => {
                put(&self.skill_name_in, &at(&f, 0), window, cx);
                put(&self.skill_desc_in, &at(&f, 1), window, cx);
                put_area(&self.skill_body_in, &at(&f, 2), window, cx);
            }
            (Page::Proposals, Form::Fields(f)) => {
                let kind = self.proposal_filled().map(|k| k.kind.clone());
                match kind.as_deref() {
                    Some("skill") => {
                        put(&self.skill_name_in, &at(&f, 0), window, cx);
                        put(&self.skill_desc_in, &at(&f, 1), window, cx);
                        put_area(&self.skill_body_in, &at(&f, 2), window, cx);
                    }
                    Some("fix") => put_area(&self.skill_body_in, &at(&f, 0), window, cx),
                    Some(_) => put(&self.proposal_note_in, &at(&f, 0), window, cx),
                    None => {}
                }
            }
            (Page::Brains, Form::Fields(f)) => {
                put(&self.setting_key_in, &at(&f, 0), window, cx);
                put(&self.setting_value_in, &at(&f, 1), window, cx);
                put(&self.share_in, &at(&f, 2), window, cx);
            }
            (Page::Routines, Form::Routine(d)) => {
                let d = *d;
                put(&self.routine_name_in, &d.name, window, cx);
                put_area(&self.routine_prompt_in, &d.prompt, window, cx);
                put(&self.routine_ui.every_in.clone(), &d.every, window, cx);
                put(&self.routine_ui.from_in.clone(), &d.from, window, cx);
                put(&self.routine_ui.to_in.clone(), &d.to, window, cx);
                put(
                    &self.routine_ui.thread_in.clone(),
                    &d.event.thread,
                    window,
                    cx,
                );
                put(&self.routine_ui.gate_in.clone(), &d.gate, window, cx);
                put_area(&self.routine_ui.calx_in.clone(), &d.calx, window, cx);
                put(
                    &self.routine_ui.gate_seconds_in.clone(),
                    &d.gate_seconds,
                    window,
                    cx,
                );
                let ui = &mut self.routine_ui;
                ui.event = d.event;
                ui.model = d.model;
                ui.effort = d.effort;
                ui.use_calx = d.use_calx;
                ui.script_only = d.script_only;
                ui.minutes = d.minutes;
                ui.adding_time = false;
                ui.run_open = None;
                ui.schedule = d.schedule;
                self.routine_agent = d.agent;
                self.routine_notify = d.notify;
                self.routine_on = d.on;
                self.routine_catch_up = d.catch_up;
                self.routine_thread = d.thread;
                self.routine_webhook = d.webhook;
                self.set_routine_flow(&d.flow, d.flow_kept, window, cx);
            }
            _ => {}
        }
    }

    /// Keeps what the open form holds as its item's draft while it's changed, while the
    /// popup shows its page. Whether its row's dot came or went.
    pub(super) fn keep_draft(&mut self, cx: &App) -> bool {
        let Some((page, _)) = self.model.drafts.open_key().cloned() else {
            return false;
        };
        if !self.popup.is_some_and(|s| draft_pages(s).contains(&page)) {
            return false;
        }
        let Some(now) = self.form_now(page, cx) else {
            return false;
        };
        self.model.drafts.edit(now)
    }

    /// The form's left as it is, its draft kept, and filled in for nothing.
    pub(super) fn leave_form(&mut self, cx: &App) {
        self.keep_draft(cx);
        self.model.drafts.leave();
    }

    /// Fills the form of `page` for item `key`, or a new one's for none, whose own is
    /// `own`: with its draft if it has one. The one it was filled in for is left first.
    pub(super) fn fill_form(
        &mut self,
        page: Page,
        key: Option<String>,
        own: Form,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_form(cx);
        let shown = self.model.drafts.open((page, key), own);
        self.put_form(page, shown, window, cx);
        if let Some(now) = self.form_now(page, cx) {
            self.model.drafts.filled(now);
        }
    }

    /// The open form of `page`, holding `now` (read before the command changes anything),
    /// is being saved as item `key`, or a new one for none, by the command `cmd`: its
    /// draft and dot stay until the instance answers that it's saved.
    pub(super) fn form_saving(
        &mut self,
        page: Page,
        key: Option<String>,
        cmd: &Value,
        now: Option<Form>,
    ) {
        if let (Some(id), Some(now)) = (cmd.get("id").and_then(Value::as_str), now) {
            self.model.drafts.saving(id.to_string(), (page, key), now);
        }
    }

    /// Whether item `key` of `page` has changes not saved, for its row's dot.
    pub(super) fn unsaved(&self, page: Page, key: &str) -> bool {
        self.model.drafts.has(page, Some(key))
    }
}
