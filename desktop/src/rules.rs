//! The Rules page in the popup: the person's rules on what agents touch, for every
//! agent and for one, to add and remove, and a what-if that asks the instance what an
//! action would get. The instance matches and decides; this only draws and asks.

use super::*;

/// What a rule or a what-if is about, as `rule.put` and `rule.check` name it, and how
/// the page says it.
const ON: [(&str, &str); 4] = [
    ("command", "Command"),
    ("path", "Path"),
    ("site", "Site"),
    ("tool", "Tool"),
];

/// Who can start a turn, as rules name it.
const STARTERS: [(&str, &str); 5] = [
    ("person", "You"),
    ("agent", "An agent"),
    ("routine", "Routine"),
    ("webhook", "Webhook"),
    ("script", "Script"),
];

/// An example of what goes in the field, for each kind.
fn example(on: &str) -> &'static str {
    match on {
        "command" => "git push, or rm -rf *",
        "path" => "~/secrets, or **/.env",
        "site" => "github.com, or *.bank.no",
        "tool" => "github.create_issue, or loom.notify",
        _ => "",
    }
}

/// How a rule's outcome reads in its row.
fn outcome_word(then: &str) -> &'static str {
    match then {
        "allow" => "Allows",
        "ask" => "Asks you",
        _ => "Denies",
    }
}

/// A rule's arguments for `rule.put`, from the form. None when it has nothing to match.
pub(super) fn rule_args(
    then: &str,
    on: &str,
    value: &str,
    effect: &str,
    by: &str,
    agent: &str,
    note: &str,
) -> Option<serde_json::Value> {
    let mut args = serde_json::json!({"then": then});
    let value = value.trim();
    if !on.is_empty() && !value.is_empty() {
        args[on] = value.into();
    }
    if !effect.is_empty() {
        args["effect"] = effect.into();
    }
    if !by.is_empty() {
        args["started_by"] = by.into();
    }
    if args.as_object().is_some_and(|m| m.len() == 1) {
        return None;
    }
    if !agent.is_empty() {
        args["agent"] = agent.into();
    }
    if !note.trim().is_empty() {
        args["note"] = note.trim().into();
    }
    Some(args)
}

impl Loom {
    /// A choice of a few for the Rules page, side by side, setting what `set` does.
    fn rules_choice(
        &self,
        id: &str,
        now: &str,
        choices: &[(&'static str, &'static str)],
        set: fn(&mut Loom, &'static str),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = Vec::new();
        for &(value, label) in choices {
            items.push(
                self.segment(format!("{id}-{value}"), label, now == value)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        set(this, value);
                        cx.notify();
                    })),
            );
        }
        self.segments(items).into_any_element()
    }

    /// Every agent, or one of them, as small buttons that wrap.
    fn rules_agent_choice(
        &self,
        id: &'static str,
        now: &str,
        set: fn(&mut Loom, String),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut choices = vec![(String::new(), "Every agent".to_string())];
        choices.extend(
            self.model
                .agents
                .iter()
                .map(|a| (a.id.clone(), a.name.clone())),
        );
        let mut row = div().flex().flex_wrap().gap(px(space::S));
        for (value, label) in choices {
            let on = now == value;
            row = row.child(
                self.small_button(format!("{id}-{value}"), &label, on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        set(this, value.clone());
                        cx.notify();
                    })),
            );
        }
        row.into_any_element()
    }

    /// The Rules page: the person's rules, a form for a new one, and the what-if.
    pub(super) fn rules_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.rule_open {
            return self.rule_form(cx);
        }
        let p = self.p;
        let new_rule = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("new-rule", "New rule", primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    for s in [&this.rule_in, &this.rule_note_in] {
                        s.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                    this.rule_open = true;
                    cx.notify();
                }))
        };
        let mut groups = Vec::new();
        let rules = self.model.rules.clone();
        if rules.is_empty() {
            groups.push(self.empty(
                Ico::ShieldCheck,
                "No rules yet",
                "Allow, ask about or deny what agents touch: a command, a path, a site, a tool, or who started the turn",
                Some(new_rule(self, cx, true).into_any_element()),
            ));
        } else {
            // Every agent's first, then each agent's own, under its name.
            let mut scopes: Vec<String> = vec![String::new()];
            for r in &rules {
                if !scopes.contains(&r.agent) {
                    scopes.push(r.agent.clone());
                }
            }
            for scope in scopes {
                let mut rows = Vec::new();
                for r in rules.iter().filter(|r| r.agent == scope) {
                    let id = r.id.clone();
                    let remove = self
                        .small_button(format!("remove-rule-{id}"), "Remove", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.delete_rule(&id);
                            this.send(f);
                            cx.notify();
                        }));
                    let sub = if r.note.is_empty() {
                        outcome_word(&r.then).to_string()
                    } else {
                        format!("{} · {}", outcome_word(&r.then), r.note)
                    };
                    rows.push(
                        self.row(
                            None,
                            r.said.clone(),
                            Some(sub),
                            Some(remove.into_any_element()),
                        )
                        .into_any_element(),
                    );
                }
                if rows.is_empty() {
                    continue;
                }
                let title = if scope.is_empty() {
                    "Every agent".to_string()
                } else {
                    self.model
                        .agent(&scope)
                        .map(|a| a.name.clone())
                        .unwrap_or(scope.clone())
                };
                groups.push(self.group(Some(&title), rows));
            }
        }
        groups.push(self.what_if_group(cx));
        groups.push(
            div()
                .px(px(space::XS))
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Deny wins, then ask, then allow. With no rule, the agent's permission mode decides, and a rule that can't be read denies.")
                .into_any_element(),
        );
        let action = (!rules.is_empty()).then(|| new_rule(self, cx, false).into_any_element());
        self.settings_page("rules-page", "Rules", action, groups)
    }

    /// The what-if: an action, who started the turn and whose it is, and what it gets.
    fn what_if_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.rules_choice(
            "check-on",
            self.check_on,
            &ON,
            |this, v| this.check_on = v,
            cx,
        );
        let by = self.rules_choice(
            "check-by",
            self.check_by,
            &STARTERS,
            |this, v| this.check_by = v,
            cx,
        );
        let agent = self.rules_agent_choice(
            "check-agent",
            &self.check_agent.clone(),
            |this, v| this.check_agent = v,
            cx,
        );
        let check = self
            .small_button("check-rule", "Check", true)
            .on_click(cx.listener(|this, _, _, cx| {
                let value = this.check_in.read(cx).value().trim().to_string();
                if value.is_empty() {
                    this.model.notices.push(format!(
                        "Say what to check, such as {}",
                        example(this.check_on)
                    ));
                    cx.notify();
                    return;
                }
                let mut args = serde_json::json!({"started_by": this.check_by});
                args[this.check_on] = value.into();
                if !this.check_agent.is_empty() {
                    args["agent"] = this.check_agent.clone().into();
                }
                let f = this.model.check_rule(args);
                this.send(f);
                cx.notify();
            }));
        let (said, why) = match &self.model.what_if {
            Some(w) => (w.verdict().to_string(), Some(w.why.clone())),
            None => (
                "What would it get?".to_string(),
                Some("Try an action, and see which rule or mode decides".to_string()),
            ),
        };
        let label = ON
            .iter()
            .find(|(k, _)| *k == self.check_on)
            .map(|(_, l)| *l)
            .unwrap_or("Command");
        let rows = vec![
            self.row(None, "On", None, Some(on)).into_any_element(),
            self.field_row(label, Input::new(&self.check_in)),
            self.row(None, "Started by", None, Some(by))
                .into_any_element(),
            self.text_row("Agent", None, agent),
            self.row(None, said, why, Some(check.into_any_element()))
                .into_any_element(),
        ];
        self.group(Some("What if"), rows)
    }

    /// The form for a new rule.
    fn rule_form(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let then = self.rules_choice(
            "rule-then",
            self.rule_then,
            &[("allow", "Allow"), ("ask", "Ask"), ("deny", "Deny")],
            |this, v| this.rule_then = v,
            cx,
        );
        let mut on_choices: Vec<(&'static str, &'static str)> = ON.to_vec();
        on_choices.push(("", "Anything"));
        let on = self.rules_choice(
            "rule-on",
            self.rule_on,
            &on_choices,
            |this, v| this.rule_on = v,
            cx,
        );
        let effect = self.rules_choice(
            "rule-effect",
            self.rule_effect,
            &[("", "Any"), ("read", "Only reads"), ("write", "Changes")],
            |this, v| this.rule_effect = v,
            cx,
        );
        let mut by_choices: Vec<(&'static str, &'static str)> = vec![("", "Anyone")];
        by_choices.extend(STARTERS);
        let by = self.rules_choice(
            "rule-by",
            self.rule_by,
            &by_choices,
            |this, v| this.rule_by = v,
            cx,
        );
        let agent = self.rules_agent_choice(
            "rule-agent",
            &self.rule_agent.clone(),
            |this, v| this.rule_agent = v,
            cx,
        );
        let mut rows = vec![
            self.row(None, "Then", None, Some(then)).into_any_element(),
            self.row(None, "On", None, Some(on)).into_any_element(),
        ];
        if let Some((_, label)) = ON.iter().find(|(k, _)| *k == self.rule_on) {
            rows.push(self.field_row(label, Input::new(&self.rule_in)));
            rows.push(
                div()
                    .px(px(space::L))
                    .pb(px(space::S + 2.0))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(format!("Such as {}", example(self.rule_on)))
                    .into_any_element(),
            );
        }
        rows.push(
            self.row(
                None,
                "What it does",
                Some("Whether it only looks, or changes something".into()),
                Some(effect),
            )
            .into_any_element(),
        );
        rows.push(
            self.row(
                None,
                "Started by",
                Some("Who started the turn".into()),
                Some(by),
            )
            .into_any_element(),
        );
        rows.push(self.text_row("For", None, agent));
        rows.push(self.field_row("Note", Input::new(&self.rule_note_in)));
        let save = self
            .small_button("save-rule", "Save", true)
            .on_click(cx.listener(|this, _, _, cx| {
                let value = this.rule_in.read(cx).value().to_string();
                let note = this.rule_note_in.read(cx).value().to_string();
                let on = if value.trim().is_empty() { "" } else { this.rule_on };
                let Some(args) = rule_args(
                    this.rule_then,
                    on,
                    &value,
                    this.rule_effect,
                    this.rule_by,
                    &this.rule_agent,
                    &note,
                ) else {
                    this.model
                        .notices
                        .push("A rule needs something to match: what it's on, what it does, or who started the turn".into());
                    cx.notify();
                    return;
                };
                let f = this.model.put_rule(args);
                this.send(f);
                this.rule_open = false;
                cx.notify();
            }));
        let cancel = self
            .small_button("cancel-rule", "Cancel", false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.rule_open = false;
                cx.notify();
            }));
        let groups = vec![
            self.group(None, rows),
            self.form_actions(None, cancel, save),
        ];
        self.settings_page("rules-page", "New rule", None, groups)
    }
}

#[cfg(test)]
mod tests {
    use super::rule_args;

    #[test]
    fn a_rule_is_what_the_form_says() {
        let a = rule_args(
            "deny",
            "command",
            " git push ",
            "",
            "webhook",
            "a_1",
            " no ",
        )
        .unwrap();
        assert_eq!(
            a,
            serde_json::json!({"then": "deny", "command": "git push", "started_by": "webhook", "agent": "a_1", "note": "no"})
        );
        let b = rule_args("ask", "", "", "write", "", "", "").unwrap();
        assert_eq!(b, serde_json::json!({"then": "ask", "effect": "write"}));
        assert!(rule_args("allow", "path", "  ", "", "", "a_1", "x").is_none());
    }
}
