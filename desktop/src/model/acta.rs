//! Acta beside the chat (PLAN.md, "Acta beside the chat"): a card's side preview, the
//! form that makes a card from a message, and a board's columns and list, each in the
//! panel on the right. The model holds what each shows, the form's defaults and checks,
//! the board's grouping and filters, and what the person's changes send; the instance
//! calls Acta as them.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use super::{s, ActaCard, Model};

/// The columns of a board, in order, by status.
pub const COLUMNS: [&str; 4] = ["todo", "in_progress", "review", "done"];
/// Every status a card moves to from its chip's menu, in order.
pub const STATUSES: [&str; 5] = ["todo", "in_progress", "review", "done", "canceled"];
/// A card's sizes, smallest first.
pub const SIZES: [&str; 5] = ["xs", "s", "m", "l", "xl"];
/// A card's priorities, as the form's segments show them.
pub const PRIORITIES: [&str; 4] = ["low", "medium", "high", "urgent"];
/// How many of a board's done cards its view asks for: the latest.
const DONE_MOST: u32 = 30;
/// How many of its open cards.
const OPEN_MOST: u32 = 200;
/// The longest title the form fills in from a message's first line, in characters.
const TITLE_MOST: usize = 120;

/// A status in words: "In progress".
pub fn status_name(status: &str) -> &'static str {
    match status {
        "in_progress" => "In progress",
        "review" => "Review",
        "done" => "Done",
        "canceled" => "Canceled",
        _ => "Todo",
    }
}

/// A priority in words: "High".
pub fn priority_name(priority: &str) -> &'static str {
    match priority {
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "urgent" => "Urgent",
        _ => "None",
    }
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// One of a card's subtasks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub done: bool,
}

/// One of a card's comments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comment {
    pub by: String,
    /// When, as Acta gives it (RFC 3339).
    pub at: String,
    /// Markdown.
    pub body: String,
}

/// A card as its side preview shows it (`card.get`): as a pill has it, with its whole
/// description, its priority, its subtasks and its last three comments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardDetail {
    pub card: ActaCard,
    pub description: String,
    pub priority: String,
    pub checklist: Vec<Subtask>,
    pub comments: Vec<Comment>,
}

impl CardDetail {
    pub fn of(v: &Value) -> CardDetail {
        let list = |k: &str| {
            v.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        CardDetail {
            card: ActaCard::of(v),
            description: s(v, "description"),
            priority: s(v, "priority"),
            checklist: list("checklist")
                .iter()
                .map(|t| Subtask {
                    id: s(t, "id"),
                    title: s(t, "title"),
                    done: t.get("done").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect(),
            comments: list("comments")
                .iter()
                .map(|c| Comment {
                    by: s(c, "by"),
                    at: s(c, "at"),
                    body: s(c, "body"),
                })
                .collect(),
        }
    }

    /// While it's looked up: what a pill knew of it.
    fn known(card: &ActaCard) -> CardDetail {
        CardDetail {
            card: card.clone(),
            description: card.text.clone(),
            ..Default::default()
        }
    }
}

/// When a comment was written, as its line shows it: "25 Nov".
pub fn comment_day(at: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = at
        .get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .filter(|m| (1..=12).contains(m));
    let day = at.get(8..10).and_then(|d| d.parse::<u32>().ok());
    match (month, day) {
        (Some(m), Some(d)) => format!("{d} {}", MONTHS[m - 1]),
        _ => String::new(),
    }
}

/// One of a board's cards, as its columns and list show it (`acta.tasks`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardTask {
    pub id: String,
    pub title: String,
    pub status: String,
    pub assignee: String,
    pub labels: Vec<String>,
    pub size: String,
    pub priority: String,
}

impl BoardTask {
    pub fn of(v: &Value) -> BoardTask {
        BoardTask {
            id: s(v, "id"),
            title: s(v, "title"),
            status: s(v, "status"),
            assignee: s(v, "assignee"),
            labels: strings(v, "labels"),
            size: s(v, "size"),
            priority: s(v, "priority"),
        }
    }

    /// Its assignee's initial, for its row: "E".
    pub fn initial(&self) -> String {
        self.assignee
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default()
    }
}

/// A board, as `acta.board` gives it: its labels and its people.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardInfo {
    pub board: String,
    pub name: String,
    pub prefix: String,
    pub labels: Vec<String>,
    pub people: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardTab {
    #[default]
    Columns,
    List,
}

/// What Acta's panel beside the chat shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Panel {
    Card(Box<CardPanel>),
    Make(MakeForm),
    Board(BoardView),
}

/// A card's side preview.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardPanel {
    pub board: String,
    pub id: String,
    /// As `card.get` last gave it, or what a pill knew while it's looked up.
    pub detail: Option<CardDetail>,
    /// Being looked up, or changed.
    pub loading: bool,
    pub error: Option<String>,
    /// The menu under its status chip is open.
    pub status_menu: bool,
    /// What it was before a change on its way, to go back to if Acta refuses it.
    before: Option<CardDetail>,
}

/// The form that makes a card from a message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MakeForm {
    /// The message's id, which the card is kept with.
    pub message: String,
    pub board: String,
    /// What the title and description were filled with.
    pub title: String,
    pub description: String,
    /// The board's labels picked.
    pub labels: Vec<String>,
    pub size: Option<String>,
    pub priority: Option<String>,
    /// The board picker's list is open.
    pub board_menu: bool,
    /// card.create is on its way: Make does nothing until it's answered.
    pub making: bool,
    pub error: Option<String>,
}

/// A board's view: its columns, or its list with search and filters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardView {
    pub board: String,
    pub tab: BoardTab,
    /// Its open cards in board order, then its latest done.
    pub tasks: Vec<BoardTask>,
    /// What's still to come: the open cards and the done.
    waiting: u8,
    pub search: String,
    pub label: Option<String>,
    pub assignee: Option<String>,
    /// The board picker's list is open.
    pub board_menu: bool,
    pub error: Option<String>,
    /// Cards moved, until Acta answers: each with the status it had.
    moving: Vec<(String, String)>,
}

impl BoardView {
    pub fn loading(&self) -> bool {
        self.waiting > 0
    }
}

/// What an answer from the instance was asked for.
#[derive(Clone, Debug, PartialEq)]
enum Ask {
    Card { board: String, id: String },
    Move { board: String, id: String },
    Create,
    Board,
    Tasks { board: String, done: bool },
    Watches,
    Follow(Watch),
    Unfollow(Watch),
}

/// A watch of the person's on an Acta board, or one card on it (PLAN.md, "Watching
/// outside events"), as far as Follow needs it: what it follows and the chat it posts in.
#[derive(Clone, Debug, PartialEq)]
pub struct Watch {
    /// "" while `watch.put` is under way.
    pub id: String,
    pub board: String,
    /// "" for the whole board.
    pub card: String,
    pub conversation: String,
}

impl Watch {
    /// A watch as the instance gives it, when it's on an Acta board and not a routine's.
    pub fn of(w: &Value) -> Option<Watch> {
        let integration = s(w, "integration");
        if !(integration.is_empty() || integration == "acta") {
            return None;
        }
        if w.pointer("/wake/routine").is_some() {
            return None;
        }
        let target = w.get("target")?;
        let board = s(target, "board");
        if board.is_empty() || s(w, "id").is_empty() {
            return None;
        }
        Some(Watch {
            id: s(w, "id"),
            board,
            card: s(target, "card"),
            conversation: s(w, "conversation"),
        })
    }

    fn same(&self, board: &str, card: &str, conversation: &str) -> bool {
        self.board == board && self.card == card && self.conversation == conversation
    }
}

/// Acta beside the chat, in the model.
#[derive(Clone, Debug, Default)]
pub struct Acta {
    pub panel: Option<Panel>,
    /// Each board's labels and people, by its ref, as `acta.board` last gave them.
    pub boards: HashMap<String, BoardInfo>,
    /// Each of the person's boards' names, by its ref, from `acta.boards`.
    pub names: HashMap<String, String>,
    /// The boards cards were made on here, the last first.
    pub used: Vec<String>,
    /// The messages whose pills a change here redrew, for the instance to keep them so.
    pub again: Vec<String>,
    /// The person's watches on Acta, once `watch.list` gave them, for Follow.
    pub watches: Option<Vec<Watch>>,
    watches_asked: bool,
    asks: HashMap<String, Ask>,
}

/// A message's first line as a card's title: without a heading's #, a list's mark or a
/// quote's >, without emphasis and code marks, cut at a word to fit.
pub fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut t = line;
    loop {
        let before = t;
        t = t.trim_start_matches(['#', '>']).trim_start();
        for mark in ["- ", "* ", "+ ", "[ ] ", "[x] "] {
            t = t.strip_prefix(mark).unwrap_or(t);
        }
        if t == before {
            break;
        }
    }
    let t = t.replace("**", "").replace("__", "").replace('`', "");
    let t = t.trim();
    if t.chars().count() <= TITLE_MOST {
        return t.to_string();
    }
    let cut: String = t.chars().take(TITLE_MOST).collect();
    let kept = match cut.rfind(' ') {
        Some(k) if k > cut.len() / 2 => &cut[..k],
        _ => &cut,
    };
    format!("{}…", kept.trim_end())
}

/// A board's cards in its columns, Todo, In progress, Review and Done, each in board
/// order. A canceled card is in none.
pub fn columns_of(tasks: &[BoardTask]) -> Vec<(&'static str, Vec<BoardTask>)> {
    COLUMNS
        .iter()
        .map(|c| {
            (
                *c,
                tasks.iter().filter(|t| t.status == *c).cloned().collect(),
            )
        })
        .collect()
}

/// A board's cards the list shows: those whose id or title has `search` in it, in any
/// case, with `label` and `assignee` when they're picked.
pub fn filtered(
    tasks: &[BoardTask],
    search: &str,
    label: Option<&str>,
    assignee: Option<&str>,
) -> Vec<BoardTask> {
    let q = search.trim().to_lowercase();
    tasks
        .iter()
        .filter(|t| {
            q.is_empty() || t.id.to_lowercase().contains(&q) || t.title.to_lowercase().contains(&q)
        })
        .filter(|t| label.is_none_or(|l| t.labels.iter().any(|x| x == l)))
        .filter(|t| assignee.is_none_or(|a| t.assignee == a))
        .cloned()
        .collect()
}

impl Model {
    fn panel_cmd(&mut self, name: &str, args: Value, ask: Ask) -> Value {
        let id = self.id("app.panel");
        self.acta.asks.insert(id.clone(), ask);
        json!({"t": "cmd", "id": id, "name": name, "args": args})
    }

    /// Whether the person has Acta connected: the composer knows their boards.
    pub fn has_acta(&self) -> bool {
        !self.acta_boards.is_empty()
    }

    /// The person's boards for a picker, each its ref and name: those cards were made on
    /// here first, the last first, then the rest in Acta's order.
    pub fn board_choices(&self) -> Vec<(String, String)> {
        let mut out: Vec<String> = self
            .acta
            .used
            .iter()
            .filter(|b| self.acta_boards.iter().any(|(_, x)| x == *b))
            .cloned()
            .collect();
        for (_, b) in &self.acta_boards {
            if !out.contains(b) {
                out.push(b.clone());
            }
        }
        out.into_iter()
            .map(|b| {
                let name = self.board_name(&b);
                (b, name)
            })
            .collect()
    }

    /// A board's name: as Acta names it, or its part after the workspace.
    pub fn board_name(&self, board: &str) -> String {
        self.acta
            .names
            .get(board)
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| board.rsplit('/').next().unwrap_or_default().to_string())
    }

    /// Asks for a board's labels and people, unless they're known.
    fn ask_board(&mut self, board: &str) -> Option<Value> {
        if board.is_empty() || self.acta.boards.contains_key(board) {
            return None;
        }
        Some(self.panel_cmd("acta.board", json!({"board": board}), Ask::Board))
    }

    /// A card a message has, as its pill knows it.
    fn pill_of(&self, board: &str, id: &str) -> Option<ActaCard> {
        self.messages
            .iter()
            .filter_map(|m| m.cards.get(id))
            .find(|c| c.board == board && !c.missing)
            .cloned()
    }

    // --- A card's side preview ---

    /// Opens a card's side preview, from its pill or a board's row, and looks it up.
    pub fn open_card(&mut self, board: &str, id: &str) -> Value {
        let detail = self.pill_of(board, id).map(|c| CardDetail::known(&c));
        self.acta.panel = Some(Panel::Card(Box::new(CardPanel {
            board: board.to_string(),
            id: id.to_string(),
            detail,
            loading: true,
            ..Default::default()
        })));
        self.panel_cmd(
            "card.get",
            json!({"board": board, "id": id}),
            Ask::Card {
                board: board.to_string(),
                id: id.to_string(),
            },
        )
    }

    fn card_panel(&mut self) -> Option<&mut CardPanel> {
        match self.acta.panel.as_mut() {
            Some(Panel::Card(c)) => Some(c.as_mut()),
            _ => None,
        }
    }

    /// Opens or shuts the side preview's status menu.
    pub fn toggle_status_menu(&mut self) {
        if let Some(c) = self.card_panel() {
            c.status_menu = !c.status_menu;
        }
    }

    /// Moves the card in the side preview to `status`: shown at once, and back as it
    /// was if Acta refuses.
    pub fn move_card(&mut self, status: &str) -> Option<Value> {
        let c = self.card_panel()?;
        c.status_menu = false;
        let detail = c.detail.as_mut()?;
        if detail.card.status == status || !STATUSES.contains(&status) {
            return None;
        }
        if c.before.is_none() {
            c.before = Some(detail.clone());
        }
        detail.card.status = status.to_string();
        c.loading = true;
        let (board, id) = (c.board.clone(), c.id.clone());
        Some(self.panel_cmd(
            "card.update",
            json!({"board": board, "id": id, "status": status}),
            Ask::Move { board, id },
        ))
    }

    /// Ticks one of the side preview's subtasks, or takes its tick off.
    pub fn toggle_subtask(&mut self, subtask: &str) -> Option<Value> {
        let c = self.card_panel()?;
        let detail = c.detail.as_mut()?;
        let before = detail.clone();
        let t = detail.checklist.iter_mut().find(|t| t.id == subtask)?;
        t.done = !t.done;
        let done = t.done;
        detail.card.subtasks_done = detail.checklist.iter().filter(|t| t.done).count() as u32;
        if c.before.is_none() {
            c.before = Some(before);
        }
        c.loading = true;
        let (board, id) = (c.board.clone(), c.id.clone());
        Some(self.panel_cmd(
            "card.update",
            json!({"board": board, "id": id, "subtask": subtask, "done": done}),
            Ask::Move { board, id },
        ))
    }

    /// The pills of the messages shown that are this card, as it is now.
    fn patch_pills(&mut self, card: &ActaCard) -> bool {
        let mut changed = false;
        for m in self.messages.iter_mut() {
            let Some(had) = m.cards.get(&card.id) else {
                continue;
            };
            if had.board != card.board {
                continue;
            }
            let now = ActaCard {
                made: had.made,
                ..card.clone()
            };
            if *had != now {
                Arc::make_mut(&mut m.cards).insert(card.id.clone(), now);
                changed = true;
                if !self.acta.again.contains(&m.id) {
                    self.acta.again.push(m.id.clone());
                }
            }
        }
        changed
    }

    // --- Making a card ---

    /// Opens the form that makes a card from a message, filled from its words: the title
    /// its first line, the description all of it, the board the last used.
    pub fn start_card(&mut self, message: &str, text: &str) -> Option<Value> {
        let board = self
            .board_choices()
            .first()
            .map(|(b, _)| b.clone())
            .unwrap_or_default();
        self.acta.panel = Some(Panel::Make(MakeForm {
            message: message.to_string(),
            board: board.clone(),
            title: title_from(text),
            description: text.trim().to_string(),
            ..Default::default()
        }));
        self.ask_board(&board)
    }

    fn form(&mut self) -> Option<&mut MakeForm> {
        match self.acta.panel.as_mut() {
            Some(Panel::Make(f)) => Some(f),
            _ => None,
        }
    }

    /// Opens or shuts the board picker of the form or the board view.
    pub fn toggle_board_menu(&mut self) {
        match self.acta.panel.as_mut() {
            Some(Panel::Make(f)) => f.board_menu = !f.board_menu,
            Some(Panel::Board(b)) => b.board_menu = !b.board_menu,
            _ => {}
        }
    }

    /// The form's board: its labels picked go, as they were the old board's.
    pub fn form_board(&mut self, board: &str) -> Option<Value> {
        let f = self.form()?;
        f.board_menu = false;
        if f.board != board {
            f.board = board.to_string();
            f.labels.clear();
        }
        self.ask_board(board)
    }

    /// Picks one of the board's labels, or unpicks it.
    pub fn toggle_label(&mut self, label: &str) {
        if let Some(f) = self.form() {
            match f.labels.iter().position(|l| l == label) {
                Some(k) => {
                    f.labels.remove(k);
                }
                None => f.labels.push(label.to_string()),
            }
        }
    }

    /// The form's size: `size`, or none when it's picked again.
    pub fn form_size(&mut self, size: &str) {
        if let Some(f) = self.form() {
            f.size = (f.size.as_deref() != Some(size)).then(|| size.to_string());
        }
    }

    /// The form's priority: `priority`, or none when it's picked again.
    pub fn form_priority(&mut self, priority: &str) {
        if let Some(f) = self.form() {
            f.priority = (f.priority.as_deref() != Some(priority)).then(|| priority.to_string());
        }
    }

    /// Whether Make makes the card, with `title` as typed: a board and a title, and
    /// nothing on its way.
    pub fn can_make(&self, title: &str) -> bool {
        match &self.acta.panel {
            Some(Panel::Make(f)) => {
                !f.making
                    && !f.board.is_empty()
                    && !title.trim().is_empty()
                    && title.trim().len() <= 500
            }
            _ => false,
        }
    }

    /// Makes the card, with the title and description as typed: kept with its message,
    /// which shows its pill once the instance says.
    pub fn make_card(&mut self, title: &str, description: &str) -> Option<Value> {
        if !self.can_make(title) {
            return None;
        }
        let f = self.form()?;
        f.making = true;
        f.error = None;
        let mut args = json!({
            "board": f.board,
            "title": title.trim(),
            "description": description,
            "labels": f.labels,
            "message": f.message,
        });
        if let Some(size) = &f.size {
            args["size"] = json!(size);
        }
        if let Some(priority) = &f.priority {
            args["priority"] = json!(priority);
        }
        let board = f.board.clone();
        self.acta.used.retain(|b| *b != board);
        self.acta.used.insert(0, board);
        Some(self.panel_cmd("card.create", args, Ask::Create))
    }

    // --- A board ---

    /// Opens a board's view, `board` or else the one last used, and asks for its open
    /// cards, its latest done and its labels and people.
    pub fn open_board(&mut self, board: Option<&str>) -> Vec<Value> {
        let board = board
            .map(str::to_string)
            .or_else(|| self.board_choices().first().map(|(b, _)| b.clone()));
        let Some(board) = board else {
            return Vec::new();
        };
        let tab = match &self.acta.panel {
            Some(Panel::Board(b)) => b.tab,
            _ => BoardTab::Columns,
        };
        self.acta.panel = Some(Panel::Board(BoardView {
            board: board.clone(),
            tab,
            waiting: 2,
            ..Default::default()
        }));
        let mut out = vec![
            self.panel_cmd(
                "acta.tasks",
                json!({"board": board, "limit": OPEN_MOST}),
                Ask::Tasks {
                    board: board.clone(),
                    done: false,
                },
            ),
            self.panel_cmd(
                "acta.tasks",
                json!({"board": board, "status": ["done"], "limit": DONE_MOST}),
                Ask::Tasks {
                    board: board.clone(),
                    done: true,
                },
            ),
        ];
        out.extend(self.ask_board(&board));
        out
    }

    fn board_view(&mut self) -> Option<&mut BoardView> {
        match self.acta.panel.as_mut() {
            Some(Panel::Board(b)) => Some(b),
            _ => None,
        }
    }

    pub fn board_tab(&mut self, tab: BoardTab) {
        if let Some(b) = self.board_view() {
            b.tab = tab;
        }
    }

    pub fn board_search(&mut self, q: &str) {
        if let Some(b) = self.board_view() {
            b.search = q.to_string();
        }
    }

    /// The list's label filter: `label`, or none when it's picked again.
    pub fn board_label(&mut self, label: &str) {
        if let Some(b) = self.board_view() {
            b.label = (b.label.as_deref() != Some(label)).then(|| label.to_string());
        }
    }

    /// The list's assignee filter: `who`, or none when they're picked again.
    pub fn board_assignee(&mut self, who: &str) {
        if let Some(b) = self.board_view() {
            b.assignee = (b.assignee.as_deref() != Some(who)).then(|| who.to_string());
        }
    }

    /// The labels and people the list filters by: the board's, and any its cards have.
    pub fn board_filters(&self) -> (Vec<String>, Vec<String>) {
        let Some(Panel::Board(b)) = &self.acta.panel else {
            return (Vec::new(), Vec::new());
        };
        let info = self.acta.boards.get(&b.board);
        let mut labels: Vec<String> = info.map(|i| i.labels.clone()).unwrap_or_default();
        let mut people: Vec<String> = info.map(|i| i.people.clone()).unwrap_or_default();
        for t in &b.tasks {
            for l in &t.labels {
                if !labels.contains(l) {
                    labels.push(l.clone());
                }
            }
            if !t.assignee.is_empty() && !people.contains(&t.assignee) {
                people.push(t.assignee.clone());
            }
        }
        (labels, people)
    }

    /// A card dragged to another column: moved there at once, and back if Acta refuses.
    /// None when it's dropped where it was.
    pub fn drop_task(&mut self, id: &str, status: &str) -> Option<Value> {
        if !COLUMNS.contains(&status) {
            return None;
        }
        let b = self.board_view()?;
        let t = b.tasks.iter_mut().find(|t| t.id == id)?;
        if t.status == status {
            return None;
        }
        b.moving.push((id.to_string(), t.status.clone()));
        t.status = status.to_string();
        b.error = None;
        let board = b.board.clone();
        Some(self.panel_cmd(
            "card.update",
            json!({"board": board, "id": id, "status": status}),
            Ask::Move {
                board,
                id: id.to_string(),
            },
        ))
    }

    /// Closes Acta's panel.
    pub fn close_acta(&mut self) {
        self.acta.panel = None;
    }

    /// Shuts the menu open in Acta's panel, its status's or a board picker, as Esc does
    /// before it closes the panel. Gives whether one was open.
    pub fn shut_acta_menu(&mut self) -> bool {
        match self.acta.panel.as_mut() {
            Some(Panel::Card(c)) if c.status_menu => c.status_menu = false,
            Some(Panel::Make(f)) if f.board_menu => f.board_menu = false,
            Some(Panel::Board(b)) if b.board_menu => b.board_menu = false,
            _ => return false,
        }
        true
    }

    // --- Following (PLAN.md, "Watching outside events") ---

    /// `watch.list`, once, the first time a panel of Acta's is open, for Follow.
    pub fn watches_fetch(&mut self) -> Option<Value> {
        if self.acta.panel.is_none() || self.acta.watches_asked {
            return None;
        }
        self.acta.watches_asked = true;
        Some(self.panel_cmd("watch.list", json!({}), Ask::Watches))
    }

    /// Whether the person follows the board, or its card, in the chat shown: a watch on
    /// it posts there. None until their watches are known, or with no chat shown.
    pub fn following(&self, board: &str, card: Option<&str>) -> Option<bool> {
        let watches = self.acta.watches.as_ref()?;
        let chat = self.conversation()?;
        let card = card.unwrap_or("");
        Some(watches.iter().any(|w| w.same(board, card, &chat)))
    }

    /// Follow, or stop following, the board or its card in the chat shown: `watch.put`
    /// with this chat for its lines, or `watch.remove`. It shows at once, and goes back
    /// as it was when the instance refuses.
    pub fn toggle_follow(&mut self, board: &str, card: Option<&str>) -> Option<Value> {
        let following = self.following(board, card)?;
        let chat = self.conversation()?;
        let card = card.unwrap_or("").to_string();
        let watches = self.acta.watches.as_mut()?;
        if following {
            let at = watches.iter().position(|w| w.same(board, &card, &chat))?;
            // While its watch.put is under way, it stays.
            if watches[at].id.is_empty() {
                return None;
            }
            let w = watches.remove(at);
            let id = w.id.clone();
            return Some(self.panel_cmd("watch.remove", json!({"id": id}), Ask::Unfollow(w)));
        }
        let w = Watch {
            id: String::new(),
            board: board.to_string(),
            card: card.clone(),
            conversation: chat.clone(),
        };
        watches.push(w.clone());
        let mut target = json!({"board": board});
        if !card.is_empty() {
            target["card"] = json!(card);
        }
        Some(self.panel_cmd(
            "watch.put",
            json!({"target": target, "conversation": chat}),
            Ask::Follow(w),
        ))
    }

    /// Shows Acta's words for a refused Follow on the panel open.
    fn panel_error(&mut self, error: String) {
        match self.acta.panel.as_mut() {
            Some(Panel::Card(c)) => c.error = Some(error),
            Some(Panel::Board(b)) => b.error = Some(error),
            _ => {}
        }
    }

    /// `watch.updated` and `watch.removed`, from this device or another.
    pub(super) fn take_watch_event(&mut self, kind: &str, f: &Value) {
        let Some(watches) = self.acta.watches.as_mut() else {
            return;
        };
        if kind == "watch.removed" {
            let id = s(f, "watch");
            watches.retain(|w| w.id != id);
            return;
        }
        let Some(w) = f.get("watch").and_then(Watch::of) else {
            return;
        };
        // Made here: it takes the place of the one shown while it was made.
        watches.retain(|x| {
            x.id != w.id && !(x.id.is_empty() && x.same(&w.board, &w.card, &w.conversation))
        });
        watches.push(w);
    }

    /// The answers to what Acta's panels asked. Gives whether `id` was one.
    pub(super) fn take_panel_answer(&mut self, id: &str, f: &Value) -> bool {
        let Some(ask) = self.acta.asks.remove(id) else {
            return false;
        };
        let error = f.get("error").map(|e| s(e, "message"));
        let value = f.get("value").cloned().unwrap_or(Value::Null);
        match ask {
            Ask::Board => {
                if let Some(b) = value.get("board").filter(|_| error.is_none()) {
                    let info = BoardInfo {
                        board: s(b, "board"),
                        name: s(b, "name"),
                        prefix: s(b, "prefix"),
                        labels: strings(b, "labels"),
                        people: strings(b, "people"),
                    };
                    self.acta.boards.insert(info.board.clone(), info);
                }
            }
            Ask::Tasks { board, done } => {
                let tasks: Vec<BoardTask> = value
                    .get("tasks")
                    .and_then(Value::as_array)
                    .map(|l| l.iter().map(BoardTask::of).collect())
                    .unwrap_or_default();
                if let Some(b) = self.board_view().filter(|b| b.board == board) {
                    b.waiting = b.waiting.saturating_sub(1);
                    if let Some(e) = error {
                        b.error = Some(e);
                    }
                    // The open first, then the done, whichever came first.
                    b.tasks.retain(|t| (t.status == "done") != done);
                    if done {
                        b.tasks.extend(tasks);
                    } else {
                        let mut all = tasks;
                        all.append(&mut b.tasks);
                        b.tasks = all;
                    }
                }
            }
            Ask::Card { board, id } | Ask::Move { board, id } => {
                let card = value
                    .get("card")
                    .filter(|_| error.is_none())
                    .map(CardDetail::of);
                if let Some(d) = &card {
                    self.patch_pills(&d.card);
                }
                if let Some(c) = self.card_panel().filter(|c| c.board == board && c.id == id) {
                    c.loading = false;
                    match card.clone() {
                        Some(d) => {
                            c.detail = Some(d);
                            c.before = None;
                            c.error = None;
                        }
                        None => {
                            if let Some(before) = c.before.take() {
                                c.detail = Some(before);
                            }
                            c.error = error.clone();
                        }
                    }
                }
                if let Some(b) = self.board_view().filter(|b| b.board == board) {
                    let had = b.moving.iter().position(|(t, _)| *t == id);
                    let was = had.map(|k| b.moving.remove(k).1);
                    match (&card, was) {
                        // Acta refused the move: back where it was.
                        (None, Some(was)) => {
                            if let Some(t) = b.tasks.iter_mut().find(|t| t.id == id) {
                                t.status = was;
                            }
                            b.error = error;
                        }
                        (Some(d), _) => {
                            if let Some(t) = b.tasks.iter_mut().find(|t| t.id == id) {
                                t.status = d.card.status.clone();
                                t.title = d.card.title.clone();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ask::Create => {
                if let Some(f) = self.form() {
                    f.making = false;
                    if value.get("card").is_some() && error.is_none() {
                        // Made: the form goes, and the message shows its pill.
                        self.acta.panel = None;
                    } else {
                        f.error = Some(error.unwrap_or_else(|| "Acta didn't answer.".into()));
                    }
                }
            }
            Ask::Watches => {
                let list = value.get("watches").and_then(Value::as_array);
                match list.filter(|_| error.is_none()) {
                    Some(list) => {
                        self.acta.watches = Some(list.iter().filter_map(Watch::of).collect());
                    }
                    // Asked again with the next panel.
                    None => self.acta.watches_asked = false,
                }
            }
            Ask::Follow(asked) => {
                let made = value
                    .get("watch")
                    .and_then(Watch::of)
                    .filter(|_| error.is_none());
                if let Some(watches) = self.acta.watches.as_mut() {
                    let at = watches.iter().position(|w| w.id.is_empty() && *w == asked);
                    // Its watch.updated, which comes first, may have put it in already.
                    match (made, at) {
                        (Some(w), Some(k)) => watches[k] = w,
                        (None, Some(k)) => {
                            watches.remove(k);
                        }
                        _ => {}
                    }
                }
                if let Some(e) = error {
                    self.panel_error(e);
                }
            }
            Ask::Unfollow(w) => {
                if let Some(e) = error {
                    if let Some(watches) = self.acta.watches.as_mut() {
                        watches.push(w);
                    }
                    self.panel_error(e);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acta_model() -> Model {
        let mut m = Model::new();
        m.acta_boards = vec![
            ("LOO".into(), "aaen-studios/loom".into()),
            ("CAL".into(), "aaen-studios/calcite".into()),
        ];
        m.acta
            .names
            .insert("aaen-studios/loom".into(), "loom".into());
        m
    }

    fn answer(m: &mut Model, cmd: &Value, f: Value) -> bool {
        let id = cmd["id"].as_str().unwrap().to_string();
        let mut f = f;
        f["id"] = json!(id);
        m.take_panel_answer(&id, &f)
    }

    fn task(id: &str, status: &str, assignee: &str, labels: &[&str]) -> BoardTask {
        BoardTask {
            id: id.into(),
            title: format!("Card {id}"),
            status: status.into(),
            assignee: assignee.into(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_title_is_the_first_line_without_its_marks() {
        assert_eq!(
            title_from("\n\n## **Fix** the `pill`\nMore."),
            "Fix the pill"
        );
        assert_eq!(title_from("- [ ] Ship it"), "Ship it");
        assert_eq!(title_from("> Quoted"), "Quoted");
        assert_eq!(title_from(""), "");
        let long = "word ".repeat(40);
        let t = title_from(&long);
        assert!(t.ends_with("word…") && t.chars().count() <= TITLE_MOST + 1);
    }

    #[test]
    fn the_form_is_filled_from_the_message_on_the_last_board_used() {
        let mut m = acta_model();
        let ask = m.start_card("m_1", "The pill needs a preview\nWith the description.");
        assert_eq!(ask.unwrap()["name"], "acta.board");
        let Some(Panel::Make(f)) = &m.acta.panel else {
            panic!("no form")
        };
        assert_eq!(f.board, "aaen-studios/loom");
        assert_eq!(f.title, "The pill needs a preview");
        assert_eq!(
            f.description,
            "The pill needs a preview\nWith the description."
        );
        // Made on calcite, calcite comes first next time.
        assert!(m.form_board("aaen-studios/calcite").is_some());
        m.toggle_label("engine");
        m.form_size("s");
        m.form_size("s");
        m.form_priority("high");
        assert!(!m.can_make("  "));
        let made = m.make_card("Smooth it", "Words").unwrap();
        assert_eq!(made["name"], "card.create");
        assert_eq!(
            made["args"],
            json!({"board": "aaen-studios/calcite", "title": "Smooth it", "description": "Words", "labels": ["engine"], "message": "m_1", "priority": "high"})
        );
        // Make does nothing while it's on its way.
        assert!(!m.can_make("Smooth it") && m.make_card("Smooth it", "").is_none());
        assert!(answer(
            &mut m,
            &made,
            json!({"value": {"card": {"id": "LOO-400"}}})
        ));
        assert!(m.acta.panel.is_none());
        assert_eq!(
            m.board_choices()
                .iter()
                .map(|(b, _)| b.as_str())
                .collect::<Vec<_>>(),
            ["aaen-studios/calcite", "aaen-studios/loom"]
        );
        assert_eq!(m.board_choices()[1].1, "loom");
        assert_eq!(m.board_choices()[0].1, "calcite");
    }

    #[test]
    fn a_new_board_drops_the_labels_and_a_refusal_keeps_the_form() {
        let mut m = acta_model();
        m.start_card("m_1", "Title");
        m.toggle_label("desktop");
        m.toggle_label("Bug");
        m.toggle_label("desktop");
        let Some(Panel::Make(f)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(f.labels, ["Bug"]);
        m.form_board("aaen-studios/calcite");
        let Some(Panel::Make(f)) = &m.acta.panel else {
            panic!()
        };
        assert!(f.labels.is_empty());
        let made = m.make_card("Title", "").unwrap();
        answer(
            &mut m,
            &made,
            json!({"error": {"code": "mcp_failed", "message": "nope isn't one of the board's labels."}}),
        );
        let Some(Panel::Make(f)) = &m.acta.panel else {
            panic!("the form went")
        };
        assert!(!f.making);
        assert_eq!(
            f.error.as_deref(),
            Some("nope isn't one of the board's labels.")
        );
        // Esc shuts the board picker first, and keeps the form.
        m.toggle_board_menu();
        assert!(m.shut_acta_menu());
        assert!(!m.shut_acta_menu());
        assert!(matches!(&m.acta.panel, Some(Panel::Make(f)) if !f.board_menu));
    }

    #[test]
    fn the_side_preview_moves_its_card_and_goes_back_when_refused() {
        let mut m = acta_model();
        let get = m.open_card("aaen-studios/loom", "LOO-206");
        assert_eq!(
            get["args"],
            json!({"board": "aaen-studios/loom", "id": "LOO-206"})
        );
        let detail = json!({"card": {"id": "LOO-206", "board": "aaen-studios/loom", "title": "Usage", "status": "in_progress", "subtasks": 2, "subtasks_done": 1, "description": "## Why", "checklist": [{"id": "s1", "title": "a", "done": true}, {"id": "s2", "title": "b", "done": false}], "comments": [{"by": "Elliot", "at": "2026-11-25T10:00:00Z", "body": "Hi"}]}});
        answer(&mut m, &get, json!({"value": detail}));
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        let d = c.detail.clone().unwrap();
        assert!(!c.loading);
        assert_eq!(d.description, "## Why");
        assert_eq!(d.checklist.len(), 2);
        assert_eq!(comment_day(&d.comments[0].at), "25 Nov");
        // Moved at once, and back as it was when Acta refuses.
        assert!(m.move_card("in_progress").is_none());
        let moved = m.move_card("review").unwrap();
        assert_eq!(moved["args"]["status"], "review");
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(c.detail.as_ref().unwrap().card.status, "review");
        answer(
            &mut m,
            &moved,
            json!({"error": {"code": "unavailable", "message": "Acta didn't answer."}}),
        );
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(c.detail.as_ref().unwrap().card.status, "in_progress");
        assert_eq!(c.error.as_deref(), Some("Acta didn't answer."));
        // A tick counts at once.
        let tick = m.toggle_subtask("s2").unwrap();
        assert_eq!(
            tick["args"],
            json!({"board": "aaen-studios/loom", "id": "LOO-206", "subtask": "s2", "done": true})
        );
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(c.detail.as_ref().unwrap().card.subtasks_done, 2);
    }

    #[test]
    fn a_card_that_changed_changes_its_pills() {
        let mut m = acta_model();
        // A card made from the message: its pill shows under it.
        let msg = m.message_of(&json!({
            "id": "m_1", "role": "person", "at": 1, "blocks": [{"kind": "text", "text": "Make it"}],
            "made_cards": [{"id": "LOO-206", "board": "aaen-studios/loom"}],
            "cards": {"LOO-206": {"id": "LOO-206", "board": "aaen-studios/loom", "title": "Usage", "status": "todo"}}
        }));
        assert_eq!(msg.cards["LOO-206"].made, Some(0));
        m.messages.push(msg);
        let get = m.open_card("aaen-studios/loom", "LOO-206");
        // While it's looked up, the preview has what the pill knew.
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(c.detail.as_ref().unwrap().card.status, "todo");
        answer(
            &mut m,
            &get,
            json!({"value": {"card": {"id": "LOO-206", "board": "aaen-studios/loom", "status": "done"}}}),
        );
        let now = &m.messages[0].cards["LOO-206"];
        assert_eq!(now.status, "done");
        assert_eq!(now.made, Some(0));
        // The instance is asked to keep the message so, once.
        let refresh = m.card_fetches(crate::model::now_ms());
        let asked = refresh
            .iter()
            .find(|f| f["name"] == "cards.refresh")
            .expect("cards.refresh");
        assert_eq!(asked["args"]["messages"], json!(["m_1"]));
        assert!(m
            .card_fetches(crate::model::now_ms())
            .iter()
            .all(|f| f["name"] != "cards.refresh"));
    }

    #[test]
    fn follow_makes_a_watch_for_the_chat_and_goes_back_when_refused() {
        let mut m = acta_model();
        m.open_room = Some("c_1".into());
        // The watches are asked for once a panel is open, and once.
        assert!(m.watches_fetch().is_none());
        let _ = m.open_card("aaen-studios/loom", "LOO-206");
        let list = m.watches_fetch().expect("watch.list");
        assert_eq!(list["name"], "watch.list");
        assert!(m.watches_fetch().is_none());
        assert_eq!(m.following("aaen-studios/loom", Some("LOO-206")), None);
        answer(
            &mut m,
            &list,
            json!({"value": {"watches": [
                {"id": "w_1", "integration": "acta", "target": {"board": "aaen-studios/loom"}, "conversation": "c_2"},
                {"id": "w_2", "integration": "acta", "target": {"board": "aaen-studios/loom"}, "conversation": "", "wake": {"routine": "r_1"}},
                {"id": "w_9", "integration": "t3", "target": {"thread": "th_1"}, "conversation": "c_1"}
            ]}}),
        );
        // The board's watch posts in another chat, and a routine's isn't one to follow.
        assert_eq!(m.following("aaen-studios/loom", None), Some(false));
        assert_eq!(m.acta.watches.as_ref().unwrap().len(), 1);
        let put = m
            .toggle_follow("aaen-studios/loom", Some("LOO-206"))
            .unwrap();
        assert_eq!(put["name"], "watch.put");
        assert_eq!(
            put["args"],
            json!({"target": {"board": "aaen-studios/loom", "card": "LOO-206"}, "conversation": "c_1"})
        );
        // Shown at once; pressed again while it's made, it stays.
        assert_eq!(
            m.following("aaen-studios/loom", Some("LOO-206")),
            Some(true)
        );
        assert!(m
            .toggle_follow("aaen-studios/loom", Some("LOO-206"))
            .is_none());
        // Its watch.updated comes first, then the answer: one watch.
        let made = json!({"id": "w_3", "integration": "acta", "target": {"board": "aaen-studios/loom", "card": "LOO-206"}, "conversation": "c_1"});
        m.take(&json!({"t": "event", "kind": "watch.updated", "watch": made}));
        answer(&mut m, &put, json!({"value": {"watch": made}}));
        let ids: Vec<String> = m
            .acta
            .watches
            .as_ref()
            .unwrap()
            .iter()
            .map(|w| w.id.clone())
            .collect();
        assert_eq!(ids, ["w_1", "w_3"]);
        // Following stops with watch.remove, and is back when that's refused.
        let remove = m
            .toggle_follow("aaen-studios/loom", Some("LOO-206"))
            .unwrap();
        assert_eq!(remove["name"], "watch.remove");
        assert_eq!(remove["args"], json!({"id": "w_3"}));
        assert_eq!(
            m.following("aaen-studios/loom", Some("LOO-206")),
            Some(false)
        );
        answer(
            &mut m,
            &remove,
            json!({"error": {"code": "not_found", "message": "There's no watch w_3."}}),
        );
        assert_eq!(
            m.following("aaen-studios/loom", Some("LOO-206")),
            Some(true)
        );
        // Following the board, refused: back as it was, with why on the panel.
        let put = m.toggle_follow("aaen-studios/loom", None).unwrap();
        answer(
            &mut m,
            &put,
            json!({"error": {"code": "not_found", "message": "Acta isn't connected."}}),
        );
        assert_eq!(m.following("aaen-studios/loom", None), Some(false));
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(c.error.as_deref(), Some("Acta isn't connected."));
        // Taken away on another device.
        m.take(&json!({"t": "event", "kind": "watch.removed", "watch": "w_3"}));
        assert_eq!(
            m.following("aaen-studios/loom", Some("LOO-206")),
            Some(false)
        );
        // With no chat shown, there's nothing to follow in.
        m.open_room = None;
        assert_eq!(m.following("aaen-studios/loom", None), None);
    }

    #[test]
    fn a_board_is_grouped_in_its_columns_and_filtered_in_its_list() {
        let tasks = vec![
            task("LOO-1", "todo", "Elliot", &["desktop"]),
            task("LOO-2", "in_progress", "", &["server"]),
            task("LOO-3", "todo", "Kim", &["desktop", "Bug"]),
            task("LOO-4", "done", "Elliot", &[]),
            task("LOO-5", "canceled", "Elliot", &[]),
        ];
        let cols = columns_of(&tasks);
        let ids = |c: &[BoardTask]| c.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        assert_eq!(cols.len(), 4);
        assert_eq!(ids(&cols[0].1), ["LOO-1", "LOO-3"]);
        assert_eq!(ids(&cols[1].1), ["LOO-2"]);
        assert!(cols[2].1.is_empty());
        assert_eq!(ids(&cols[3].1), ["LOO-4"]);
        assert_eq!(ids(&filtered(&tasks, "loo-3", None, None)), ["LOO-3"]);
        assert_eq!(
            ids(&filtered(&tasks, "card", Some("desktop"), None)),
            ["LOO-1", "LOO-3"]
        );
        assert_eq!(
            ids(&filtered(&tasks, "", Some("desktop"), Some("Kim"))),
            ["LOO-3"]
        );
        assert_eq!(tasks[0].initial(), "E");
        assert_eq!(tasks[1].initial(), "");
    }

    #[test]
    fn the_board_view_asks_for_its_cards_and_moves_a_dragged_one() {
        let mut m = acta_model();
        let asks = m.open_board(None);
        assert_eq!(asks.len(), 3);
        assert_eq!(
            asks[0]["args"],
            json!({"board": "aaen-studios/loom", "limit": 200})
        );
        assert_eq!(
            asks[1]["args"],
            json!({"board": "aaen-studios/loom", "status": ["done"], "limit": 30})
        );
        assert_eq!(asks[2]["name"], "acta.board");
        // The done may come first; the open go before them.
        answer(
            &mut m,
            &asks[1],
            json!({"value": {"tasks": [{"id": "LOO-4", "title": "d", "status": "done"}]}}),
        );
        answer(
            &mut m,
            &asks[0],
            json!({"value": {"tasks": [{"id": "LOO-1", "title": "a", "status": "todo", "assignee": "Elliot"}]}}),
        );
        answer(
            &mut m,
            &asks[2],
            json!({"value": {"board": {"board": "aaen-studios/loom", "labels": ["Bug"], "people": ["Elliot", "Kim"]}}}),
        );
        let Some(Panel::Board(b)) = &m.acta.panel else {
            panic!()
        };
        assert!(!b.loading());
        assert_eq!(
            b.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["LOO-1", "LOO-4"]
        );
        assert_eq!(
            m.board_filters(),
            (
                vec!["Bug".to_string()],
                vec!["Elliot".to_string(), "Kim".to_string()]
            )
        );
        // Dropped where it was: nothing. Elsewhere: moved, and back when refused.
        assert!(m.drop_task("LOO-1", "todo").is_none());
        assert!(m.drop_task("LOO-1", "canceled").is_none());
        let moved = m.drop_task("LOO-1", "review").unwrap();
        assert_eq!(
            moved["args"],
            json!({"board": "aaen-studios/loom", "id": "LOO-1", "status": "review"})
        );
        let status = |m: &Model| match &m.acta.panel {
            Some(Panel::Board(b)) => b.tasks[0].status.clone(),
            _ => String::new(),
        };
        assert_eq!(status(&m), "review");
        answer(
            &mut m,
            &moved,
            json!({"error": {"code": "mcp_failed", "message": "No."}}),
        );
        assert_eq!(status(&m), "todo");
        let again = m.drop_task("LOO-1", "done").unwrap();
        answer(
            &mut m,
            &again,
            json!({"value": {"card": {"id": "LOO-1", "board": "aaen-studios/loom", "title": "a", "status": "done"}}}),
        );
        assert_eq!(status(&m), "done");
        m.board_tab(BoardTab::List);
        m.board_label("Bug");
        m.board_label("Bug");
        m.board_assignee("Kim");
        let Some(Panel::Board(b)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(
            (b.tab, b.label.clone(), b.assignee.as_deref()),
            (BoardTab::List, None, Some("Kim"))
        );
    }

    #[test]
    fn boards_is_in_the_plus_menu_with_acta_connected() {
        use crate::model::PlusRow;
        let mut m = Model::new();
        assert!(!m.plus_groups("", false).concat().contains(&PlusRow::Boards));
        m = acta_model();
        assert_eq!(m.plus_groups("", false)[1].last(), Some(&PlusRow::Boards));
        assert!(m
            .plus_groups("boa", false)
            .concat()
            .contains(&PlusRow::Boards));
        assert!(!m
            .plus_groups("emoji", false)
            .concat()
            .contains(&PlusRow::Boards));
    }

    #[test]
    fn an_answer_for_another_card_or_board_changes_nothing() {
        let mut m = acta_model();
        let first = m.open_card("aaen-studios/loom", "LOO-1");
        m.open_card("aaen-studios/loom", "LOO-2");
        answer(
            &mut m,
            &first,
            json!({"value": {"card": {"id": "LOO-1", "board": "aaen-studios/loom", "title": "One"}}}),
        );
        let Some(Panel::Card(c)) = &m.acta.panel else {
            panic!()
        };
        assert_eq!(c.id, "LOO-2");
        assert!(c.loading && c.detail.is_none());
        assert!(!m.take_panel_answer("app.other_1", &json!({})));
    }
}
