//! Changes made on a Settings list page and not saved yet (PLAN.md, "The settings
//! popup"): each item's form as the person left it when they picked another or closed
//! the popup, shown again when it's picked, until Save or Cancel. They're kept while
//! the app runs, as the composer's drafts are, each server's with its model.

use std::collections::HashMap;

use super::{flow_is_chain, FlowStep, McpServer, Proposal, Routine, Schedule, Skill};

/// A list page with a form for each of its items.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Page {
    Brains,
    Skills,
    Proposals,
    Mcp,
    Routines,
}

/// An item's form: its page, and its name or id, or none for a new one's.
pub type Key = (Page, Option<String>);

/// What a page's form holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Form {
    /// Its fields' text, in the order the page puts them.
    Fields(Vec<String>),
    /// A routine's form.
    Routine(Box<RoutineDraft>),
}

/// An MCP server's form as it opens: its name, its command or address, its settings,
/// and its own client and secret; all empty for a new one.
pub fn mcp_form(m: Option<&McpServer>) -> Form {
    let Some(m) = m else {
        return Form::Fields(vec![String::new(); 5]);
    };
    let line = if m.url.is_empty() {
        m.command.clone()
    } else {
        m.url.clone()
    };
    Form::Fields(vec![
        m.name.clone(),
        line,
        m.settings_text(),
        m.client_id.clone(),
        m.client_secret.clone().unwrap_or_default(),
    ])
}

/// A skill's form as it opens: its name, when it's for, and how; all empty for a new one.
pub fn skill_form(k: Option<&Skill>) -> Form {
    Form::Fields(match k {
        Some(k) => vec![k.name.clone(), k.description.clone(), k.body.clone()],
        None => vec![String::new(); 3],
    })
}

/// A proposal's form as it opens, by what it is: a skill's name, when it's for and how,
/// a fix's how, or a note's text.
pub fn proposal_form(k: &Proposal) -> Form {
    Form::Fields(match k.kind.as_str() {
        "skill" => vec![k.name.clone(), k.description.clone(), k.body.clone()],
        "fix" => vec![k.body.clone()],
        _ => vec![k.text.clone()],
    })
}

/// A brain's form as it opens: a setting's name and value to add, and the tokens to
/// share, all empty.
pub fn brain_form() -> Form {
    brain_fields("", "", "")
}

/// A brain's form holding a setting's `key` and `value` to add, and `tokens` to share.
pub fn brain_fields(key: &str, value: &str, tokens: &str) -> Form {
    Form::Fields(vec![key.into(), value.into(), tokens.into()])
}

/// A routine's form: its name, prompt and agent, its schedule as it's picked and its
/// fields' text (how many hours or minutes apart, and the window it runs in), its
/// switches, and its flow: the steps it edits, each its agent and prompt, or one an
/// agent made with steps side by side, kept as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoutineDraft {
    pub name: String,
    pub prompt: String,
    pub agent: Option<String>,
    pub schedule: Schedule,
    pub every: String,
    pub minutes: bool,
    pub from: String,
    pub to: String,
    pub notify: bool,
    pub on: bool,
    pub catch_up: bool,
    pub thread: bool,
    pub webhook: bool,
    pub flow: Vec<(Option<String>, String)>,
    pub flow_kept: Vec<FlowStep>,
}

impl RoutineDraft {
    /// Routine `r`'s form as it opens.
    pub fn of(r: &Routine) -> RoutineDraft {
        let chain = flow_is_chain(&r.flow);
        RoutineDraft {
            name: r.name.clone(),
            prompt: r.prompt.clone(),
            agent: Some(r.agent.clone()),
            notify: r.notify,
            on: r.on,
            catch_up: r.catch_up,
            thread: r.post == "thread",
            webhook: !r.hook.is_empty(),
            flow: if chain {
                r.flow
                    .iter()
                    .map(|s| (Some(s.agent.clone()), s.prompt.clone()))
                    .collect()
            } else {
                Vec::new()
            },
            flow_kept: if chain { Vec::new() } else { r.flow.clone() },
            ..RoutineDraft::default()
        }
        .with_schedule(r.schedule.clone())
    }

    /// A new routine's form, for `agent`: on, notifying and catching up, on a new
    /// routine's schedule.
    pub fn new(agent: Option<String>) -> RoutineDraft {
        RoutineDraft {
            agent,
            notify: true,
            on: true,
            catch_up: true,
            ..RoutineDraft::default()
        }
        .with_schedule(Schedule::default())
    }

    /// With schedule `s`, and its fields' text as they show it: the hours apart, or the
    /// minutes for one every N minutes that aren't whole hours.
    fn with_schedule(mut self, s: Schedule) -> RoutineDraft {
        self.minutes = s.every % 60 != 0;
        let every = if self.minutes { s.every } else { s.every / 60 };
        self.every = every.to_string();
        self.from = s.from.clone();
        self.to = s.to.clone();
        self.schedule = s;
        self
    }
}

/// A server's list pages' forms: the item the form is filled in for now, with what it
/// was filled in with, and each item's form as the person left it, changed and not
/// saved.
#[derive(Default)]
pub struct Drafts {
    open: Option<(Key, Form)>,
    kept: HashMap<Key, Form>,
}

impl Drafts {
    /// The form is filled in for `key`, whose own is `own`: with its draft, if it has
    /// one, or else its own.
    pub fn open(&mut self, key: Key, own: Form) -> Form {
        let shown = self.kept.get(&key).cloned().unwrap_or_else(|| own.clone());
        self.open = Some((key, own));
        shown
    }

    /// What the form holds once it's filled in with its item's own, as its fields took
    /// it (a line field takes a line break out): what it was filled in with, so it isn't
    /// taken for a change. A draft filled in is still a change.
    pub fn filled(&mut self, now: Form) {
        if let Some((key, own)) = self.open.as_mut() {
            if !self.kept.contains_key(key) {
                *own = now;
            }
        }
    }

    /// The item the form is filled in for now.
    pub fn open_key(&self) -> Option<&Key> {
        self.open.as_ref().map(|(key, _)| key)
    }

    /// Whether the form is filled in for `key` on `page` now.
    pub fn is_open(&self, page: Page, key: Option<&str>) -> bool {
        self.open_key()
            .is_some_and(|(p, k)| *p == page && k.as_deref() == key)
    }

    /// What the form holds now, for the item it's filled in for: kept as its draft
    /// while it isn't what it was filled in with, and let go once it is again. Whether
    /// that changes if the item has a draft, so its row's dot comes or goes.
    pub fn edit(&mut self, now: Form) -> bool {
        let Some((key, own)) = &self.open else {
            return false;
        };
        let had = self.kept.contains_key(key);
        if now == *own {
            self.kept.remove(key);
        } else {
            self.kept.insert(key.clone(), now);
        }
        had != self.kept.contains_key(key)
    }

    /// The form is left, its draft kept, and filled in for nothing.
    pub fn leave(&mut self) {
        self.open = None;
    }

    /// The form was saved as `now`, by `key`, which a rename changes: no draft for it,
    /// by its old key or its new one, and the form's filled in with what was saved.
    pub fn saved(&mut self, key: Key, now: Form) {
        if let Some((old, _)) = self.open.take() {
            self.kept.remove(&old);
        }
        self.kept.remove(&key);
        self.open = Some((key, now));
    }

    /// The form's changes are thrown away (Cancel), or its item went (deleted, kept or
    /// dropped): its draft goes, and the form's filled in for nothing until it's filled
    /// in again.
    pub fn discard(&mut self) {
        if let Some((key, _)) = self.open.take() {
            self.kept.remove(&key);
        }
    }

    /// Whether `key` on `page` has changes not saved.
    pub fn has(&self, page: Page, key: Option<&str>) -> bool {
        self.kept.contains_key(&(page, key.map(String::from)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SignIn;

    fn server(name: &str, url: &str) -> McpServer {
        McpServer {
            name: name.into(),
            command: String::new(),
            url: url.into(),
            settings: Vec::new(),
            sign_in: SignIn::No,
            client_id: String::new(),
            client_secret: None,
        }
    }

    fn fields(form: &Form) -> Vec<String> {
        match form {
            Form::Fields(f) => f.clone(),
            Form::Routine(_) => Vec::new(),
        }
    }

    fn key(page: Page, k: &str) -> Key {
        (page, Some(k.into()))
    }

    #[test]
    fn an_edit_is_there_again_when_its_item_is_picked_again() {
        let (github, acta) = (
            server("github", "https://api.github.com/mcp"),
            server("acta", "https://acta.example/mcp"),
        );
        let mut d = Drafts::default();
        // The first server is picked, and its address changed.
        let shown = d.open(key(Page::Mcp, "github"), mcp_form(Some(&github)));
        assert_eq!(shown, mcp_form(Some(&github)));
        let mut edited = fields(&shown);
        edited[1] = "https://api.github.com/mcp/v2".into();
        assert!(d.edit(Form::Fields(edited.clone())));
        assert!(d.has(Page::Mcp, Some("github")));
        // Another is picked: the first keeps its edit, and the other has none.
        d.edit(Form::Fields(edited.clone()));
        d.leave();
        assert_eq!(
            d.open(key(Page::Mcp, "acta"), mcp_form(Some(&acta))),
            mcp_form(Some(&acta))
        );
        assert!(!d.has(Page::Mcp, Some("acta")));
        // Back to the first: its edit, and Save saves it.
        d.leave();
        let back = d.open(key(Page::Mcp, "github"), mcp_form(Some(&github)));
        assert_eq!(fields(&back), edited);
        d.saved(key(Page::Mcp, "github"), back);
        assert!(!d.has(Page::Mcp, Some("github")));
        assert!(d.is_open(Page::Mcp, Some("github")));
    }

    #[test]
    fn closing_the_popup_keeps_an_edit_until_cancel() {
        let skill = Skill {
            name: "release-notes".into(),
            description: "Writing release notes".into(),
            body: "Plainly.".into(),
        };
        let mut d = Drafts::default();
        d.open(key(Page::Skills, "release-notes"), skill_form(Some(&skill)));
        let edited = Form::Fields(vec![
            "release-notes".into(),
            "Writing release notes".into(),
            "Plainly, and short.".into(),
        ]);
        d.edit(edited.clone());
        // The popup closes, and opens again on its first.
        d.leave();
        assert!(d.open_key().is_none());
        assert!(d.has(Page::Skills, Some("release-notes")));
        assert_eq!(
            d.open(key(Page::Skills, "release-notes"), skill_form(Some(&skill))),
            edited
        );
        // Cancel throws it away, and it fills in with its own again.
        d.discard();
        assert!(!d.has(Page::Skills, Some("release-notes")));
        assert!(d.open_key().is_none());
        assert_eq!(
            d.open(key(Page::Skills, "release-notes"), skill_form(Some(&skill))),
            skill_form(Some(&skill))
        );
    }

    #[test]
    fn an_edit_undone_is_no_draft() {
        let github = server("github", "https://api.github.com/mcp");
        let own = mcp_form(Some(&github));
        let mut d = Drafts::default();
        d.open(key(Page::Mcp, "github"), own.clone());
        let mut edited = fields(&own);
        edited[0] = "github2".into();
        assert!(d.edit(Form::Fields(edited)));
        // Typed back as it was: no dot.
        assert!(d.edit(own.clone()));
        assert!(!d.has(Page::Mcp, Some("github")));
        // Nothing changes, nothing to say.
        assert!(!d.edit(own));
    }

    #[test]
    fn a_new_ones_form_is_kept_too_and_saving_it_names_it() {
        let mut d = Drafts::default();
        d.open((Page::Mcp, None), mcp_form(None));
        let typed = Form::Fields(vec![
            "acta".into(),
            "https://acta.example/mcp".into(),
            String::new(),
            String::new(),
            String::new(),
        ]);
        d.edit(typed.clone());
        d.leave();
        assert!(d.has(Page::Mcp, None));
        assert_eq!(d.open((Page::Mcp, None), mcp_form(None)), typed);
        d.saved(key(Page::Mcp, "acta"), typed.clone());
        assert!(!d.has(Page::Mcp, None));
        assert!(!d.has(Page::Mcp, Some("acta")));
        // What was saved is what it's filled in with now: no draft until it's changed.
        assert!(!d.edit(typed));
    }

    #[test]
    fn a_rename_saved_leaves_no_draft_by_either_name() {
        let github = server("github", "https://api.github.com/mcp");
        let mut d = Drafts::default();
        d.open(key(Page::Mcp, "github"), mcp_form(Some(&github)));
        let mut renamed = fields(&mcp_form(Some(&github)));
        renamed[0] = "gh".into();
        d.edit(Form::Fields(renamed.clone()));
        d.saved(key(Page::Mcp, "gh"), Form::Fields(renamed));
        assert!(!d.has(Page::Mcp, Some("github")));
        assert!(!d.has(Page::Mcp, Some("gh")));
        assert!(d.is_open(Page::Mcp, Some("gh")));
    }

    #[test]
    fn pages_sharing_fields_keep_their_own_drafts() {
        // A skill and a proposed skill with the same name, on their own pages.
        let skill = Skill {
            name: "triage".into(),
            description: String::new(),
            body: String::new(),
        };
        let proposed = Proposal {
            id: "p1".into(),
            kind: "skill".into(),
            name: "triage".into(),
            ..Proposal::default()
        };
        let mut d = Drafts::default();
        d.open(key(Page::Skills, "triage"), skill_form(Some(&skill)));
        d.edit(Form::Fields(vec![
            "triage".into(),
            "Sorting".into(),
            String::new(),
        ]));
        d.leave();
        assert_eq!(
            d.open(key(Page::Proposals, "p1"), proposal_form(&proposed)),
            proposal_form(&proposed)
        );
        assert!(d.has(Page::Skills, Some("triage")));
        assert!(!d.has(Page::Proposals, Some("p1")));
    }

    #[test]
    fn what_the_fields_made_of_its_own_is_no_change() {
        let skill = Skill {
            name: "triage".into(),
            description: "Sorting\nissues".into(),
            body: String::new(),
        };
        let tidied = Form::Fields(vec![
            "triage".into(),
            "Sorting issues".into(),
            String::new(),
        ]);
        let mut d = Drafts::default();
        d.open(key(Page::Skills, "triage"), skill_form(Some(&skill)));
        d.filled(tidied.clone());
        assert!(!d.edit(tidied.clone()));
        assert!(!d.has(Page::Skills, Some("triage")));
        // A draft filled in stays one.
        let changed = Form::Fields(vec!["triage".into(), "Sorting".into(), String::new()]);
        d.edit(changed.clone());
        d.leave();
        assert_eq!(
            d.open(key(Page::Skills, "triage"), skill_form(Some(&skill))),
            changed
        );
        d.filled(changed.clone());
        assert!(d.has(Page::Skills, Some("triage")));
        assert!(!d.edit(changed));
        assert!(d.has(Page::Skills, Some("triage")));
    }

    #[test]
    fn nothing_is_kept_while_no_form_is_open() {
        let mut d = Drafts::default();
        assert!(!d.edit(brain_fields("ANTHROPIC_API_KEY", "", "")));
        assert!(!d.has(Page::Brains, Some("claude")));
        d.open(key(Page::Brains, "claude"), brain_form());
        assert!(d.edit(brain_fields("ANTHROPIC_API_KEY", "", "")));
        // Added: its fields are empty again, and the dot goes.
        assert!(d.edit(brain_form()));
        assert!(!d.has(Page::Brains, Some("claude")));
    }

    #[test]
    fn a_proposals_form_is_what_it_proposes() {
        let note = Proposal {
            kind: "note".into(),
            text: "Uses pnpm".into(),
            ..Proposal::default()
        };
        assert_eq!(proposal_form(&note), Form::Fields(vec!["Uses pnpm".into()]));
        let fix = Proposal {
            kind: "fix".into(),
            name: "deploy".into(),
            body: "Run the tests first.".into(),
            ..Proposal::default()
        };
        assert_eq!(
            proposal_form(&fix),
            Form::Fields(vec!["Run the tests first.".into()])
        );
    }

    #[test]
    fn an_mcp_form_has_its_command_or_its_address() {
        let local = McpServer {
            command: "npx -y acta-mcp".into(),
            client_secret: Some("secret:ACTA".into()),
            ..server("acta", "")
        };
        assert_eq!(
            fields(&mcp_form(Some(&local))),
            ["acta", "npx -y acta-mcp", "", "", "secret:ACTA"]
        );
        assert_eq!(
            fields(&mcp_form(Some(&server("gh", "https://x/mcp"))))[1],
            "https://x/mcp"
        );
        assert_eq!(fields(&mcp_form(None)), vec![String::new(); 5]);
    }

    #[test]
    fn a_routines_form_opens_as_it_is() {
        let r = Routine {
            id: "r1".into(),
            name: "Morning brief".into(),
            agent: "a1".into(),
            prompt: "What's new?".into(),
            schedule: Schedule {
                every: 30,
                ..Schedule::default()
            },
            on: true,
            post: "thread".into(),
            hook: "h1".into(),
            flow: vec![FlowStep {
                name: "1".into(),
                agent: "a2".into(),
                prompt: "Sum it up".into(),
                after: vec!["routine".into()],
            }],
            ..Routine::default()
        };
        let d = RoutineDraft::of(&r);
        assert_eq!(d.agent.as_deref(), Some("a1"));
        // Every 30 minutes isn't whole hours: its field says minutes.
        assert!(d.minutes);
        assert_eq!(d.every, "30");
        assert_eq!((d.from.as_str(), d.to.as_str()), ("08:00", "20:00"));
        assert!(d.on && !d.notify && d.thread && d.webhook);
        assert_eq!(d.flow, vec![(Some("a2".into()), "Sum it up".into())]);
        assert!(d.flow_kept.is_empty());
        // Steps side by side are kept, not edited.
        let side = Routine {
            flow: vec![
                FlowStep {
                    name: "1".into(),
                    after: vec!["routine".into()],
                    ..FlowStep::default()
                },
                FlowStep {
                    name: "2".into(),
                    after: vec!["routine".into()],
                    ..FlowStep::default()
                },
            ],
            ..r.clone()
        };
        let d = RoutineDraft::of(&side);
        assert!(d.flow.is_empty());
        assert_eq!(d.flow_kept.len(), 2);
        // A new one's: every two hours, on, notifying, catching up.
        let new = RoutineDraft::new(Some("a1".into()));
        assert_eq!((new.every.as_str(), new.minutes), ("2", false));
        assert!(new.on && new.notify && new.catch_up && !new.thread && !new.webhook);
    }

    #[test]
    fn a_routines_switch_flipped_is_a_draft() {
        let r = Routine {
            id: "r1".into(),
            name: "Morning brief".into(),
            agent: "a1".into(),
            on: true,
            ..Routine::default()
        };
        let own = Form::Routine(Box::new(RoutineDraft::of(&r)));
        let mut d = Drafts::default();
        d.open(key(Page::Routines, "r1"), own.clone());
        let mut off = RoutineDraft::of(&r);
        off.on = false;
        assert!(d.edit(Form::Routine(Box::new(off))));
        assert!(d.has(Page::Routines, Some("r1")));
        assert!(d.edit(own));
        assert!(!d.has(Page::Routines, Some("r1")));
    }
}
