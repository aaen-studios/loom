//! Emoji in the window (PLAN.md, "Emoji"): the suggestion row over the composer while
//! a name is typed after a colon, the picker the smiley opens, each emoji as a piece in
//! the composer, the card over one in the chat, and Settings → Emoji. What an emoji is
//! called, what matches and the picker's groups come from the core (`crate::emoji`).

use super::*;
use crate::emoji::{self, Item};

/// The picker's size.
const PICKER_W: f32 = 372.0;
const PICKER_H: f32 = 400.0;
/// A tile in the picker's grid, and how many to a line.
const TILE: f32 = 37.0;
const COLS: usize = 9;
/// The picker's search, a tab and its foot, as tall as they are.
const SEARCH: f32 = 30.0;
const TAB: f32 = 30.0;
const FOOT: f32 = 52.0;
/// The picker around its grid: its border, the search under its margin, the tabs, the
/// two lines and the foot, with the gaps between them.
const CHROME: f32 = 2.0 + space::S + SEARCH + TAB + 2.0 + FOOT + 5.0 * space::XS;
/// An emoji in a tile, in the suggestion row, and big in the picker's foot.
const IN_TILE: f32 = 26.0;
const IN_ROW: f32 = 22.0;
const BIG: f32 = 36.0;
/// An emoji in a tile of Settings → Emoji.
const IN_SETTINGS: f32 = 40.0;
/// The hands the skin tones are shown with, none first.
const HANDS: [&str; 6] = ["✋", "✋🏻", "✋🏼", "✋🏽", "✋🏾", "✋🏿"];
/// The smiley's id, and so the key its hover is kept under.
const SMILEY: &str = "emoji";

/// How a piece in the composer that's an emoji is named: its id is this and what it
/// is (the character, or the person's own token).
pub(super) const PIECE: &str = model::EMOJI;

#[derive(Default)]
pub(super) struct EmojiUi {
    pub picker: Option<Picker>,
    /// What was typed when the suggestion row was shut with Escape: it opens again once
    /// that changes.
    shut_at: Option<String>,
    /// What was told the core last, so the same isn't told again.
    told: Option<String>,
    /// One of the person's own whose name is being changed on Settings → Emoji, by id.
    pub renaming: Option<String>,
    /// Where the composer is in the window, for the picker to fit over it, as it is with
    /// a new chat's middle block down (`hero_lift`).
    pub composer_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// How far up a new chat's middle block is, and how far it goes (`hero_lift`).
    lift: Cell<(f32, f32)>,
    /// Where the top of each piece of a new chat's greeting is with the block down.
    greeting_at: [Rc<Cell<Option<f32>>>; 2],
}

/// The side of the composer the picker opens on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Above,
    Below,
}

/// Where the picker opens by a composer from `top` to `bottom` down a window `high`
/// tall, and how tall it is. Above it, all of it, where that fits, as over a chat's
/// composer at the window's foot. Where it doesn't, on the side with more room, as under
/// a new chat's composer in the middle of the window, as tall as that room, so it never
/// covers the composer or leaves the window. Where neither side has four lines of its
/// grid, it takes the larger, and its grid scrolls.
fn picker_place(composer: Option<(f32, f32)>, high: f32) -> (Side, f32) {
    let Some((top, bottom)) = composer else {
        return (Side::Above, PICKER_H);
    };
    let (above, below) = picker_room(top, bottom, high);
    let (side, room) = if above >= PICKER_H || above >= below {
        (Side::Above, above)
    } else {
        (Side::Below, below)
    };
    (side, room.clamp(CHROME, PICKER_H))
}

/// The room for the picker over a composer from `top` to `bottom` down a window `high`
/// tall, and under it.
fn picker_room(top: f32, bottom: f32, high: f32) -> (f32, f32) {
    // Under the title bar, with the gap over the composer.
    let above = top - layout::TITLEBAR - 2.0 * space::S;
    // The gap under the composer, and inside the window's foot.
    let below = high - bottom - space::S - space::M;
    (above, below)
}

/// How far a new chat's middle block, its greeting and its composer from `top` to
/// `bottom` down a window `high` tall, moves up while the picker is open. Where neither
/// side of the composer has room for all of the picker, as far as gives it all of its
/// height under it, and no further than leaves the composer 12 px under the title bar,
/// so at the least heights the picker takes what there is and its grid scrolls.
/// Elsewhere it stays.
fn picker_lift(top: f32, bottom: f32, high: f32) -> f32 {
    let (above, below) = picker_room(top, bottom, high);
    if above.max(below) >= PICKER_H {
        return 0.0;
    }
    (PICKER_H - below)
        .min(top - layout::TITLEBAR - space::M)
        .max(0.0)
}

/// How much shows of a piece of a new chat's greeting whose top is at `top` with the
/// middle block down, the block being `now` up of the `full` way it goes for the picker
/// (`picker_lift`). A piece that would pass under the title bar fades as the block
/// rises, gone as it reaches the title bar, and back as it comes down; the rest stay.
fn picker_fade(top: f32, now: f32, full: f32) -> f32 {
    let room = top - layout::TITLEBAR;
    if full <= room {
        return 1.0;
    }
    (1.0 - now / room.max(1.0)).clamp(0.0, 1.0)
}

/// A piece of a new chat's greeting: its agent's line, or its name.
#[derive(Clone, Copy)]
pub(super) enum Greeting {
    Line,
    Name,
}

pub(super) struct Picker {
    /// The emoji under the pointer, for the foot.
    hovered: Option<Item>,
    /// The six hands are open in the foot.
    tones: bool,
    scroll: UniformListScrollHandle,
}

/// A line of the picker's grid: a group's heading, or up to `COLS` tiles, the last of
/// Your emoji's ending with the dashed + that opens Settings → Emoji.
#[derive(Clone)]
enum Line {
    Head(String, String),
    Tiles(Vec<Item>, bool),
}

/// The grid's lines: each group with emoji under its heading, Your emoji even with none
/// (for its +). With a search, what it found.
fn lines(groups: &[emoji::Group], found: Option<&[Item]>) -> Vec<Line> {
    let mut out = Vec::new();
    let mut add = |id: &str, name: &str, items: &[Item], plus: bool| {
        out.push(Line::Head(id.to_string(), name.to_string()));
        let mut chunks: Vec<Vec<Item>> = items.chunks(COLS).map(|c| c.to_vec()).collect();
        if plus && chunks.last().is_none_or(|c| c.len() == COLS) {
            chunks.push(Vec::new());
        }
        let n = chunks.len();
        for (k, c) in chunks.into_iter().enumerate() {
            out.push(Line::Tiles(c, plus && k + 1 == n));
        }
    };
    if let Some(found) = found {
        if !found.is_empty() {
            add("found", "Found", found, false);
        }
        return out;
    }
    for g in groups {
        let plus = g.id == "custom";
        if g.emoji.is_empty() && !plus {
            continue;
        }
        add(&g.id, &g.name, &g.emoji, plus);
    }
    out
}

/// The tab icon for one of the picker's groups.
fn group_icon(id: &str) -> Ico {
    match id {
        "recent" => Ico::Clock,
        "custom" => Ico::Sticker,
        "smileys" => Ico::FaceSlightlySmiling,
        "people" => Ico::Hand,
        "nature" => Ico::Leaf,
        "food" => Ico::Apple,
        "travel" => Ico::Plane,
        "activities" => Ico::Volleyball,
        "objects" => Ico::Lightbulb,
        "symbols" => Ico::Heart,
        _ => Ico::Flag,
    }
}

/// An emoji drawn `side` pixels square: a built-in one as its Twemoji picture, sharp at
/// the window's scale, one of the person's own as its picture once fetched.
pub(super) fn picture(text: &str, custom: Option<&str>, side: f32) -> AnyElement {
    picture_at(text, custom, side, false)
}

/// `picture`, under the pointer or not: one of the person's own that moves plays as
/// Play animated emoji and the system's reduce motion say, and is its first frame
/// otherwise.
fn picture_at(text: &str, custom: Option<&str>, side: f32, hovered: bool) -> AnyElement {
    let blank = || div().size(px(side)).flex_none().into_any_element();
    if let Some(id) = custom {
        let Some((_, animated, Some(path))) = emoji::own_one(id) else {
            return blank();
        };
        let source = match animated && !emoji::playing(hovered) {
            true => match emoji::still(&path) {
                Some(first) => ImageSource::Render(first),
                None => return blank(),
            },
            false => ImageSource::from(std::path::PathBuf::from(path)),
        };
        return img(source)
            .size(px(side))
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element();
    }
    let name = emoji::picture_of(text).or_else(|| emoji::find(text).first().map(|(_, n)| *n));
    match name.and_then(|n| emoji::image(n, emoji::device(side))) {
        Some(drawn) => img(ImageSource::Render(drawn))
            .size(px(side))
            .flex_none()
            .into_any_element(),
        None => blank(),
    }
}

/// Words on one line with each built-in emoji in them as its picture, `side` square, for a
/// line outside the chat's Markdown, such as a reply's quote. Without emoji, the words as
/// they are, so they still end in "…" where they're cut.
pub(super) fn inline_words(text: &str, side: f32) -> AnyElement {
    let found = emoji::find(text);
    if found.is_empty() {
        return SharedString::from(text.to_string()).into_any_element();
    }
    let mut row = div()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap();
    let mut at = 0;
    for (r, _) in found {
        if at < r.start {
            row = row.child(text[at..r.start].to_string());
        }
        row = row.child(
            div()
                .mx(px(1.0))
                .child(picture(&text[r.clone()], None, side)),
        );
        at = r.end;
    }
    if at < text.len() {
        row = row.child(text[at..].to_string());
    }
    row.into_any_element()
}

/// An item's own in the composer, as a piece: its id and what it is.
fn piece_of(item: &Item) -> (String, String) {
    (format!("{PIECE}{}", item.emoji), item.emoji.clone())
}

/// A piece in the composer that's an emoji, as `composer_token` draws it.
pub(super) fn composer_piece(words: &str) -> AnyElement {
    let side = (size::COMPOSER * emoji::IN_TEXT).round();
    let custom = emoji::tokens(words).first().map(|t| t.id.clone());
    div()
        .flex()
        .items_center()
        .px(px(1.0))
        .child(picture(words, custom.as_deref(), side))
        .into_any_element()
}

/// The words and what each emoji in them is, for the pieces the composer doesn't have
/// yet: each built-in emoji, and each token of the person's own, outside code and the
/// pieces already there.
fn unpieced(
    text: &str,
    pieces: &[std::ops::Range<usize>],
) -> Vec<(std::ops::Range<usize>, String)> {
    let code = emoji::code_ranges(text);
    let taken = |r: &std::ops::Range<usize>| {
        pieces
            .iter()
            .chain(code.iter())
            .any(|p| p.start < r.end && r.start < p.end)
    };
    let mut out: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for t in emoji::tokens(text) {
        if !taken(&t.range) && emoji::own_one(&t.id).is_some() {
            out.push((t.range.clone(), text[t.range].to_string()));
        }
    }
    for (r, _) in emoji::find(text) {
        if !taken(&r) && !out.iter().any(|(o, _)| o.start < r.end && r.start < o.end) {
            out.push((r.clone(), text[r].to_string()));
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

/// A key the suggestion row takes while it's open.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum RowKey {
    Back,
    On,
    Take,
    Shut,
}

impl Loom {
    /// Follows what's typed in the composer: each emoji in it becomes a piece drawn with
    /// its picture, and the core hears what's typed up to the cursor, for the suggestion
    /// row or a name closed with its colon.
    pub(super) fn track_emoji(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (text, cursor, pieces) = {
            let c = self.composer.read(cx);
            let pieces: Vec<std::ops::Range<usize>> =
                c.tokens().iter().map(|t| t.range()).collect();
            (c.value().to_string(), c.cursor(), pieces)
        };
        let new = unpieced(&text, &pieces);
        if !new.is_empty() {
            self.composer.update(cx, |c, cx| {
                // From the end, so the ranges before stay where they are.
                for (r, words) in new.into_iter().rev() {
                    let _ = c.replace_range_with_token(
                        r,
                        InlineToken::new(format!("{PIECE}{words}"), words),
                        window,
                        cx,
                    );
                }
            });
            return;
        }
        let before = text[..cursor.min(text.len())].to_string();
        if self.emoji_ui.told.as_deref() == Some(before.as_str()) {
            return;
        }
        self.emoji_ui.told = Some(before.clone());
        if self
            .emoji_ui
            .shut_at
            .as_deref()
            .is_some_and(|s| s != before)
        {
            self.emoji_ui.shut_at = None;
        }
        if !before.contains([':', ';', '<', '=']) {
            self.model.emoji.row = None;
            return;
        }
        let f = self.model.emoji.typed(&before, self.prefs.emoticons);
        self.send(f);
    }

    /// The suggestion row, unless it was shut for what's typed.
    pub(super) fn emoji_row(&self) -> Option<emoji::Row> {
        if self.emoji_ui.shut_at.is_some() || self.plus.is_some() || self.mention.is_some() {
            return None;
        }
        self.model.emoji.row.clone()
    }

    /// Makes what the core said of what was typed, in the window: a name closed with its
    /// colon, or an emoticon, becomes its emoji where it was typed, however late the answer
    /// came (`Emojis::conversions`, LOO-370). Then fetches the pictures the chat met.
    pub(super) fn take_emoji(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        emoji::set_scale(window.scale_factor());
        emoji::sync_own(&self.model.emoji);
        for id in emoji::wanted() {
            if let Some(f) = self.model.emoji.fetch(&id) {
                self.send(f);
            }
        }
        let (value, cursor) = {
            let c = self.composer.read(cx);
            (c.value().to_string(), c.cursor())
        };
        let (todo, cursor) = self.model.emoji.conversions(&value, cursor);
        if todo.is_empty() {
            return;
        }
        self.composer.update(cx, |c, cx| {
            for k in &todo {
                let (id, words) = piece_of(&k.item);
                let end = k.range.start + words.len();
                let made = c
                    .replace_range_with_token(
                        k.range.clone(),
                        InlineToken::new(id, words),
                        window,
                        cx,
                    )
                    .is_ok();
                if made && k.space {
                    c.set_selected_range(end..end, cx);
                    c.insert(" ", window, cx);
                }
            }
            let to = cursor.min(c.value().len());
            c.set_selected_range(to..to, cx);
            c.focus(window, cx);
        });
        for k in &todo {
            let used = self.model.emoji.used(&k.item.emoji);
            self.send(used);
        }
        for f in self.model.emoji.asks_again() {
            self.send(f);
        }
        cx.notify();
    }

    /// Puts an emoji in the composer as a piece: in place of `suffix` just before the
    /// cursor (what's typed of its name), or at the cursor, which goes on past it. With
    /// `focus`, the composer takes the keys.
    fn put_emoji(
        &mut self,
        item: &Item,
        suffix: Option<&str>,
        space: bool,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, words) = piece_of(item);
        self.composer.update(cx, |c, cx| {
            let cursor = c.cursor();
            let value = c.value().to_string();
            let done = match suffix {
                Some(s) if value[..cursor.min(value.len())].ends_with(s) => c
                    .replace_range_with_token(
                        cursor - s.len()..cursor,
                        InlineToken::new(id, words),
                        window,
                        cx,
                    )
                    .is_ok(),
                Some(_) => false,
                None => c
                    .replace_with_token(InlineToken::new(id, words), window, cx)
                    .is_ok(),
            };
            if done && space {
                c.insert(" ", window, cx);
            }
            if focus {
                c.focus(window, cx);
            }
        });
        let used = self.model.emoji.used(&item.emoji);
        self.send(used);
        self.model.emoji.row = None;
        cx.notify();
    }

    /// The suggestion row's keys while it's open: ← → (and ↑ ↓) move, Tab or Enter
    /// picks, Escape shuts it. Shut, they go on to the composer.
    pub(super) fn emoji_row_key(
        &mut self,
        key: RowKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(row) = self.emoji_row() else {
            return false;
        };
        let n = row.items.len();
        match key {
            RowKey::Back => {
                if let Some(r) = self.model.emoji.row.as_mut() {
                    r.lit = (r.lit + n - 1) % n;
                }
            }
            RowKey::On => {
                if let Some(r) = self.model.emoji.row.as_mut() {
                    r.lit = (r.lit + 1) % n;
                }
            }
            RowKey::Take => {
                let item = row.items[row.lit.min(n - 1)].clone();
                self.put_emoji(&item, Some(&row.suffix), false, true, window, cx);
            }
            RowKey::Shut => self.emoji_ui.shut_at = self.emoji_ui.told.clone(),
        }
        cx.stop_propagation();
        cx.notify();
        true
    }

    /// The suggestion row over the composer: up to 8 emoji, the lit one on a light
    /// background, and its name at the end in mono.
    pub(super) fn emoji_row_view(&self, row: &emoji::Row, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let k = motion::arrive(&SharedString::from("emoji-row"), 0.16);
        let lit = row.items.get(row.lit).cloned();
        let mut tiles = div().flex().items_center().gap(px(2.0));
        for (n, item) in row.items.iter().enumerate() {
            let chosen = item.clone();
            let suffix = row.suffix.clone();
            tiles = tiles.child(
                div()
                    .id(SharedString::from(format!("emoji-row-{n}")))
                    .role(Role::MenuItem)
                    .aria_label(format!(":{}:", item.name))
                    .size(px(IN_ROW + 12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .when(n == row.lit, |d| d.bg(p.hover))
                    .child(picture(&item.emoji, item.custom.as_deref(), IN_ROW))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if let Some(r) = this.model.emoji.row.as_mut() {
                            if r.lit != n {
                                r.lit = n;
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.put_emoji(&chosen, Some(&suffix), false, true, window, cx);
                    })),
            );
        }
        div()
            .occlude()
            .opacity(k)
            .mb(px(space::S - 6.0 * (1.0 - k)))
            .p(px(space::XS))
            .flex()
            .items_center()
            .gap(px(space::M))
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.emoji_ui.shut_at = this.emoji_ui.told.clone();
                cx.notify();
            }))
            .child(tiles)
            .when_some(lit, |d, item| {
                d.child(
                    div()
                        .pr(px(space::S))
                        .font_family(crate::prefs::mono_font())
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(format!(":{}:", item.name)),
                )
            })
    }

    /// The smiley between + and the mic, which opens the picker and shuts it again. A
    /// right-click has Hide emoji button.
    pub(super) fn emoji_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let open = self.emoji_ui.picker.is_some();
        div()
            .id(SMILEY)
            .role(Role::Button)
            .aria_label("Emoji")
            .size(px(control::ICON_COMPOSER))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .text_color(p.ink_2)
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, None)
            .when(open, |d| d.bg(p.hover))
            .when(!open, |d| d.tooltip(tip("Emoji")))
            .child(Icon::new(Ico::FaceSlightlySmiling).small())
            .on_click(cx.listener(|this, _, window, cx| {
                if this.emoji_ui.picker.is_some() {
                    this.shut_emoji_picker(window, cx);
                } else {
                    this.open_emoji_picker(window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_ctx(e.position, Ctx::EmojiButton, cx);
                }),
            )
    }

    pub(super) fn open_emoji_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.plus = None;
        self.mention = None;
        self.model.emoji.tone = self.prefs.skin_tone;
        if let Some(f) = self.model.emoji.open_picker() {
            self.send(f);
        }
        self.emoji_ui.picker = Some(Picker {
            hovered: None,
            tones: false,
            scroll: UniformListScrollHandle::new(),
        });
        self.emoji_search.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Shuts the picker, and the composer takes the keys.
    pub(super) fn shut_emoji_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_emoji_picker(cx);
        self.composer.update(cx, |c, cx| c.focus(window, cx));
    }

    /// Shuts the picker and leaves the keys where they are, for a click elsewhere, which
    /// takes them where it was made.
    pub(super) fn close_emoji_picker(&mut self, cx: &mut Context<Self>) {
        self.emoji_ui.picker = None;
        self.model.emoji.shut_picker();
        cx.notify();
    }

    /// What's typed in the picker's search: the core finds it.
    pub(super) fn find_emoji(&mut self, cx: &mut Context<Self>) {
        let q = self.emoji_search.read(cx).value().trim().to_string();
        if q.is_empty() {
            self.model.emoji.found = None;
        } else {
            let f = self.model.emoji.find(&q);
            self.send(f);
        }
        if let Some(p) = &self.emoji_ui.picker {
            p.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn set_tone(&mut self, tone: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.set_prefs(|p| p.skin_tone = tone, window, cx);
        self.model.emoji.tone = tone;
        if let Some(f) = self.model.emoji.picker(true) {
            self.send(f);
        }
        if let Some(p) = self.emoji_ui.picker.as_mut() {
            p.tones = false;
        }
        self.find_emoji(cx);
    }

    /// How far a new chat's middle block is up for the picker under it (`picker_lift`):
    /// it eases up as the picker opens and back as it shuts, as the picker comes in, and
    /// with reduce motion it's there at once.
    pub(super) fn hero_lift(&self, window: &Window, cx: &App) -> f32 {
        let open = self.emoji_ui.picker.is_some();
        let eased = motion::level(&SharedString::from("emoji-lift"), open, 0.16);
        let k = if !cx.reduce_motion() {
            eased
        } else if open {
            1.0
        } else {
            0.0
        };
        let high = f32::from(window.viewport_size().height);
        let full = self.emoji_ui.composer_at.get().map_or(0.0, |b| {
            picker_lift(f32::from(b.top()), f32::from(b.bottom()), high)
        });
        // In whole pixels of the screen, as the layout places things, so the composer
        // measured with it, put back down by it, is where it was.
        let scale = window.scale_factor();
        let now = (k * full * scale).round() / scale;
        self.emoji_ui.lift.set((now, full));
        now
    }

    /// A piece of a new chat's greeting, fading as it would pass under the title bar
    /// while the middle block is up for the picker (`picker_fade`).
    pub(super) fn greeting(&self, piece: Greeting, child: impl IntoElement) -> Div {
        let (now, full) = self.emoji_ui.lift.get();
        let at = self.emoji_ui.greeting_at[piece as usize].clone();
        let shows = at.get().map_or(1.0, |top| picker_fade(top, now, full));
        div()
            .relative()
            .opacity(shows)
            // Where it is with the block down.
            .child(
                canvas(
                    move |b, _, _| at.set(Some(f32::from(b.top()) + now)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(child)
    }

    /// The picker, by the composer at its right, above it or below it (`picker_place`),
    /// the composer being `lift` up from where it's measured (`hero_lift`): the search, a
    /// tab for each group, the grid, and the foot with the emoji under the pointer and
    /// the skin tones.
    pub(super) fn emoji_picker_view(
        &self,
        lift: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let picker = self.emoji_ui.picker.as_ref()?;
        let p = self.p;
        let k = motion::arrive(&SharedString::from("emoji-picker"), 0.16);
        let searching = !self.emoji_search.read(cx).value().trim().is_empty();
        let found = self
            .model
            .emoji
            .found
            .as_ref()
            .filter(|_| searching)
            .map(|(_, items)| items.as_slice());
        let all = Rc::new(lines(&self.model.emoji.groups, found));
        let search = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .mx(px(space::S))
            .mt(px(space::S))
            .px(px(space::S))
            .h(px(SEARCH))
            .rounded(px(radius::INPUT))
            .bg(p.hover)
            .text_size(px(size::SMALL))
            .child(Icon::new(IconName::Search).small().text_color(p.ink_3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    // What's typed is cleared first, then the picker shuts.
                    .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                        cx.stop_propagation();
                        let typed = this.emoji_search.read(cx).value().to_string();
                        match this.model.emoji.picker_escape(&typed) {
                            emoji::PickerEscape::Clear => {
                                this.emoji_search
                                    .update(cx, |s, cx| s.set_value("", window, cx));
                                this.find_emoji(cx);
                            }
                            emoji::PickerEscape::Shut => this.shut_emoji_picker(window, cx),
                        }
                    }))
                    .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                        // The first found goes in, and the picker stays for another.
                        let first = this
                            .model
                            .emoji
                            .found
                            .as_ref()
                            .and_then(|(_, items)| items.first().cloned());
                        if let Some(item) = first {
                            cx.stop_propagation();
                            this.pick_from_picker(&item, window, cx);
                        }
                    }))
                    .child(Input::new(&self.emoji_search).appearance(false).small()),
            );
        // The tab of the group at the grid's top is lit; a click goes to its heading.
        let top = {
            let s = picker.scroll.0.borrow();
            let y = -f32::from(s.base_handle.offset().y);
            ((y / TILE).floor().max(0.0)) as usize
        };
        let lit_group = all
            .iter()
            .take(top + 1)
            .filter_map(|l| match l {
                Line::Head(id, _) => Some(id.clone()),
                _ => None,
            })
            .next_back();
        let mut tabs = div().flex().items_center().px(px(space::S)).gap(px(1.0));
        if !searching {
            for g in &self.model.emoji.groups {
                if g.emoji.is_empty() && g.id != "custom" {
                    continue;
                }
                let at = all
                    .iter()
                    .position(|l| matches!(l, Line::Head(id, _) if *id == g.id))
                    .unwrap_or(0);
                let on = lit_group.as_deref() == Some(g.id.as_str());
                tabs = tabs.child(
                    div()
                        .id(SharedString::from(format!("emoji-tab-{}", g.id)))
                        .role(Role::Tab)
                        .aria_label(g.name.clone())
                        .size(px(TAB))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::ROW))
                        .cursor_pointer()
                        .text_color(if on { p.ink } else { p.ink_3 })
                        .when(on, |d| d.bg(p.hover))
                        .soft_hover(transparent_black(), p.hover, None)
                        .tooltip(tip_text(g.name.clone()))
                        .child(Icon::new(group_icon(&g.id)).small())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = &this.emoji_ui.picker {
                                p.scroll.scroll_to_item(at, ScrollStrategy::Top);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        let this = cx.entity().downgrade();
        let drawn = all.clone();
        let grid = uniform_list("emoji-grid", all.len(), move |range, _, _| {
            range
                .map(|i| grid_line(p, &drawn[i], i, this.clone()))
                .collect::<Vec<_>>()
        })
        .track_scroll(&picker.scroll)
        .flex_1()
        .px(px(space::S));
        let empty = all.is_empty();
        let foot = self.emoji_foot(picker, cx);
        let composer = self
            .emoji_ui
            .composer_at
            .get()
            .map(|b| (f32::from(b.top()) - lift, f32::from(b.bottom()) - lift));
        let (side, high) = picker_place(composer, f32::from(window.viewport_size().height));
        // It opens from the composer's right edge, by the smiley, the gap from it growing
        // as it comes in.
        let gap = space::S - 6.0 * (1.0 - k);
        let (edge, anchor, gap) = match side {
            Side::Above => (
                div().absolute().top_0().right_0(),
                Anchor::BottomRight,
                -gap,
            ),
            Side::Below => (div().absolute().bottom_0().right_0(), Anchor::TopRight, gap),
        };
        let picker = div()
            .id("emoji-picker")
            .occlude()
            .opacity(k)
            .w(px(PICKER_W))
            .h(px(high))
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .overflow_hidden()
            // A click outside shuts it, but one on the smiley, which shuts it itself.
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if !motion::hovered(&motion::hover_key(SMILEY)) {
                    this.close_emoji_picker(cx);
                }
            }))
            // The grid's scroll lights another tab.
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .child(search)
            .when(!searching, |d| d.child(tabs))
            .child(div().h(px(1.0)).bg(p.line))
            .when(empty, |d| {
                d.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_3)
                        .child(if searching { "No emoji found" } else { "" }),
                )
            })
            .when(!empty, |d| d.child(grid))
            .child(div().h(px(1.0)).bg(p.line))
            .child(foot);
        Some(
            edge.child(deferred(
                anchored()
                    .anchor(anchor)
                    .offset(point(px(0.0), px(gap)))
                    .child(picker),
            )),
        )
    }

    /// The picker's foot: the emoji under the pointer big with its names, and the skin
    /// tone, a hand, which opens the six.
    fn emoji_foot(&self, picker: &Picker, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let tone = self.prefs.skin_tone.min(5) as usize;
        let mut foot = div()
            .h(px(FOOT))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M));
        if picker.tones {
            for (t, hand) in HANDS.iter().enumerate() {
                foot = foot.child(
                    div()
                        .id(SharedString::from(format!("emoji-tone-{t}")))
                        .role(Role::Button)
                        .aria_label(format!("Skin tone {t}"))
                        .size(px(32.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::ROW))
                        .cursor_pointer()
                        .when(t == tone, |d| d.bg(p.hover))
                        .soft_hover(transparent_black(), p.hover, None)
                        .child(picture(hand, None, 22.0))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_tone(t as u32, window, cx);
                        })),
                );
            }
            return foot;
        }
        let shown = picker.hovered.clone();
        foot.when_some(shown, |d, item| {
            let mut names = div().flex().flex_col().min_w_0().flex_1();
            for (n, name) in item.names.iter().take(2).enumerate() {
                names = names.child(
                    div()
                        .truncate()
                        .font_family(crate::prefs::mono_font())
                        .text_size(px(if n == 0 { size::SMALL } else { size::CAPTION }))
                        .text_color(if n == 0 { p.ink } else { p.ink_3 })
                        .child(format!(":{name}:")),
                );
            }
            d.child(picture(&item.emoji, item.custom.as_deref(), BIG))
                .child(names)
        })
        .when(picker.hovered.is_none(), |d| d.child(div().flex_1()))
        .child(
            div()
                .id("emoji-tones")
                .role(Role::Button)
                .aria_label("Skin tone")
                .size(px(32.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .tooltip(tip("Skin tone"))
                .child(picture(HANDS[tone], None, 22.0))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.emoji_ui.picker.as_mut() {
                        p.tones = true;
                    }
                    cx.notify();
                })),
        )
    }

    fn hover_emoji(&mut self, item: Option<Item>, cx: &mut Context<Self>) {
        if let Some(p) = self.emoji_ui.picker.as_mut() {
            if p.hovered != item {
                p.hovered = item;
                cx.notify();
            }
        }
    }

    /// Picks an emoji in the picker: it goes in the composer at the cursor, and the
    /// picker stays open for another, its search keeping the keys.
    fn pick_from_picker(&mut self, item: &Item, window: &mut Window, cx: &mut Context<Self>) {
        self.put_emoji(item, None, false, false, window, cx);
        self.emoji_search.update(cx, |s, cx| s.focus(window, cx));
    }

    /// Settings → Emoji: the person's own as tiles, an Add tile first and how many there
    /// are, then how emoji are shown and typed.
    pub(super) fn emoji_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let own = self.model.emoji.own.clone();
        for c in &own {
            if let Some(f) = self.model.emoji.fetch(&c.id) {
                self.send(f);
            }
        }
        emoji::sync_own(&self.model.emoji);
        let uploading = self.model.emoji.uploading.is_some();
        let mut tiles = div().flex().flex_wrap().gap(px(space::S));
        tiles = tiles.child(
            div()
                .id("emoji-add")
                .role(Role::Button)
                .aria_label("Add emoji")
                .w(px(92.0))
                .h(px(76.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(space::XS))
                .rounded(px(radius::CARD))
                .border_1()
                .border_dashed()
                .border_color(p.line_strong)
                .text_color(p.ink_2)
                .text_size(px(size::CAPTION))
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .child(
                    Icon::new(if uploading {
                        Ico::Loader
                    } else {
                        Ico::ImagePlus
                    })
                    .small(),
                )
                .child(if uploading { "Adding…" } else { "Add" })
                .when(!uploading, |d| {
                    d.on_click(cx.listener(|this, _, _, cx| this.pick_emoji_file(cx)))
                }),
        );
        for c in own.iter() {
            tiles = tiles.child(self.emoji_tile(c, cx));
        }
        let count = format!("{} of {}", own.len(), crate::emoji::MOST);
        let undo = self.model.emoji.deleted.clone().map(|gone| {
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .text_size(px(size::SMALL))
                .text_color(p.ink_2)
                .child(format!("Deleted :{}:", gone.name))
                .child(
                    self.small_button("emoji-undo", "Undo", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.emoji.restore() {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                )
        });
        let error = self.model.emoji.error.clone();
        let yours = div()
            .p(px(space::M))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(tiles)
            .child(
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(count),
            )
            .when_some(undo, |d, u| d.child(u))
            .when_some(error, |d, e| {
                d.child(div().text_size(px(size::SMALL)).text_color(p.bad).child(e))
            });
        let play = self.pref_choice(
            "emoji-play",
            self.prefs.emoji_hover,
            &[(false, "Always"), (true, "On hover")],
            |p, v| p.emoji_hover = v,
            cx,
        );
        let emoticons = self.prefs.emoticons;
        let tone = self.prefs.skin_tone.min(5) as usize;
        let mut hands = div().flex().items_center().gap(px(2.0));
        for (t, hand) in HANDS.iter().enumerate() {
            hands = hands.child(
                div()
                    .id(SharedString::from(format!("emoji-skin-{t}")))
                    .role(Role::Button)
                    .aria_label(format!("Skin tone {t}"))
                    .size(px(30.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .when(t == tone, |d| {
                        d.bg(p.hover).border_1().border_color(p.line_strong)
                    })
                    .soft_hover(transparent_black(), p.hover, None)
                    .child(picture(hand, None, 20.0))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_tone(t as u32, window, cx);
                    })),
            );
        }
        let groups = vec![
            self.group(Some("Your emoji"), vec![yours.into_any_element()]),
            self.group(
                None,
                vec![
                    self.row(None, "Play animated emoji", None, Some(play))
                        .into_any_element(),
                    self.row(None, "Skin tone", None, Some(hands.into_any_element()))
                        .into_any_element(),
                    // Its 🙂 as Twemoji draws it, not in the system's font.
                    self.row(
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .text_size(px(size::BODY))
                                .text_color(p.ink)
                                .child("Turn :) into")
                                .child(picture("🙂", None, (size::BODY * emoji::IN_TEXT).round()))
                                .into_any_element(),
                        ),
                        "",
                        None,
                        Some(
                            self.switch("pref-emoticons", emoticons)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_prefs(|p| p.emoticons = !emoticons, window, cx);
                                }))
                                .into_any_element(),
                        ),
                    )
                    .into_any_element(),
                    self.pref_switch(
                        "pref-emoji-button",
                        "Show the emoji button in the composer",
                        "",
                        self.prefs.emoji_button,
                        |p, v| p.emoji_button = v,
                        cx,
                    ),
                ],
            ),
        ];
        self.settings_page("emoji-page", "Emoji", None, groups)
    }

    /// One of the person's own on Settings → Emoji: the picture at 40 px, its `:name:`
    /// under it, which a click renames in place, a GIF mark on one that moves, and × to
    /// delete it under the pointer.
    fn emoji_tile(&self, c: &emoji::Custom, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let group = SharedString::from(format!("emoji-tile-{}", c.id));
        let renaming = self.emoji_ui.renaming.as_deref() == Some(c.id.as_str());
        let (id, rename_id) = (c.id.clone(), c.id.clone());
        let name = c.name.clone();
        let label: AnyElement = if renaming {
            div()
                .w_full()
                .text_size(px(size::CAPTION))
                .font_family(crate::prefs::mono_font())
                .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                    cx.stop_propagation();
                    this.finish_rename(window, cx);
                }))
                .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                    cx.stop_propagation();
                    this.emoji_ui.renaming = None;
                    cx.notify();
                }))
                .child(Input::new(&self.emoji_rename).appearance(false).xsmall())
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("emoji-name-{}", c.id)))
                .max_w_full()
                .truncate()
                .font_family(crate::prefs::mono_font())
                .text_size(px(size::CAPTION))
                .text_color(p.ink_2)
                .cursor_text()
                .tooltip(tip("Rename"))
                .child(format!(":{}:", c.name))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.emoji_ui.renaming = Some(rename_id.clone());
                    let name = name.clone();
                    this.emoji_rename.update(cx, |s, cx| {
                        s.set_value(name, window, cx);
                        s.focus(window, cx);
                    });
                    cx.notify();
                }))
                .into_any_element()
        };
        div()
            .group(group.clone())
            .relative()
            .w(px(92.0))
            .h(px(76.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::XS))
            .px(px(space::XS))
            .rounded(px(radius::CARD))
            .bg(p.hover.opacity(0.5))
            .child(picture(
                &format!("<:{}:{}>", c.name, c.id),
                Some(&c.id),
                IN_SETTINGS,
            ))
            .child(label)
            .when(c.animated, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(24.0))
                        .right(px(6.0))
                        .px(px(3.0))
                        .rounded(px(3.0))
                        .bg(p.raised)
                        .border_1()
                        .border_color(p.line)
                        .text_size(px(9.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.ink_2)
                        .child("GIF"),
                )
            })
            .child(
                div()
                    .id(SharedString::from(format!("emoji-delete-{}", c.id)))
                    .role(Role::Button)
                    .aria_label(format!("Delete :{}:", c.name))
                    .absolute()
                    .top(px(3.0))
                    .right(px(3.0))
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .bg(p.raised)
                    .border_1()
                    .border_color(p.line_strong)
                    .text_color(p.ink_2)
                    .cursor_pointer()
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .child(Icon::new(Ico::X).xsmall())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.emoji.delete(&id);
                        this.send(f);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn finish_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.emoji_ui.renaming.take() else {
            return;
        };
        let name = self
            .emoji_rename
            .read(cx)
            .value()
            .trim()
            .trim_matches(':')
            .to_string();
        let same = self.model.emoji.own(&id).is_some_and(|c| c.name == name);
        if !name.is_empty() && !same {
            let f = self.model.emoji.rename(&id, &name);
            self.send(f);
        }
        cx.notify();
    }

    /// Asks the system for a picture to add as one of the person's own.
    fn pick_emoji_file(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |loom, cx| {
                    if let Some(path) = paths.first() {
                        let f = loom.model.emoji.upload(&path.to_string_lossy());
                        loom.send(f);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}

/// One line of the picker's grid.
fn grid_line(p: Palette, line: &Line, i: usize, this: WeakEntity<Loom>) -> AnyElement {
    match line {
        Line::Head(_, name) => div()
            .id(("emoji-head", i))
            .h(px(TILE))
            .flex()
            .items_end()
            .pb(px(space::XS))
            .px(px(space::XS))
            .text_size(px(size::CAPTION))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.ink_3)
            .child(name.clone())
            .into_any_element(),
        Line::Tiles(items, plus) => {
            let mut row = div()
                .id(("emoji-line", i))
                .h(px(TILE))
                .flex()
                .items_center();
            for (n, item) in items.iter().enumerate() {
                let (hover, pick) = (this.clone(), this.clone());
                let (shown, chosen) = (item.clone(), item.clone());
                let name = item.names.first().cloned().unwrap_or_default();
                row = row.child(
                    div()
                        .id(SharedString::from(format!("emoji-{i}-{n}")))
                        .role(Role::Button)
                        .aria_label(format!(":{name}:"))
                        .size(px(TILE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::ROW))
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover))
                        .child(picture(&item.emoji, item.custom.as_deref(), IN_TILE))
                        .on_mouse_move(move |_, _, cx| {
                            let shown = shown.clone();
                            let _ = hover.update(cx, |loom, cx| loom.hover_emoji(Some(shown), cx));
                        })
                        .on_click(move |_, window, cx| {
                            let _ = pick
                                .update(cx, |loom, cx| loom.pick_from_picker(&chosen, window, cx));
                        }),
                );
            }
            if *plus {
                let open = this.clone();
                row = row.child(
                    div()
                        .id(("emoji-plus", i))
                        .role(Role::Button)
                        .aria_label("Add emoji")
                        .size(px(TILE - 6.0))
                        .m(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::ROW))
                        .border_1()
                        .border_dashed()
                        .border_color(p.line_strong)
                        .text_color(p.ink_3)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover))
                        .tooltip(tip("Add emoji"))
                        .child(Icon::new(Ico::Plus).small())
                        .on_click(move |_, _, cx| {
                            let _ = open.update(cx, |loom, cx| {
                                loom.close_emoji_picker(cx);
                                loom.open(Screen::Emoji, cx);
                            });
                        }),
                );
            }
            row.into_any_element()
        }
    }
}

/// A click on one of the person's own in the chat, by its link: a right-click, after the
/// message's menu opened on the button going down, gives that menu Copy name.
pub(super) fn own_clicked(loom: &WeakEntity<Loom>, url: &str, cx: &mut App) {
    let Some(id) = url.strip_prefix(emoji::OWN_LINK) else {
        return;
    };
    let _ = loom.update(cx, |loom, cx| {
        if loom.ctx.is_some() {
            loom.ctx_emoji = Some(id.to_string());
            cx.notify();
        }
    });
}

/// The card over an emoji in the chat: it big, its name, and for one of the person's
/// own, Yours, and animated for one that moves.
pub(super) struct EmojiCard {
    p: Palette,
    link: String,
    text: String,
    custom: Option<String>,
    names: Vec<String>,
    about: Option<String>,
}

impl EmojiCard {
    /// The card for the emoji `link` names, as the chat's `link_tooltip` builds it.
    pub(super) fn view(link: &str, window: &mut Window, cx: &mut App) -> AnyView {
        let p = EMOJI_PALETTE
            .with(|c| c.get())
            .unwrap_or_else(|| Palette::new(false));
        let card = if let Some(id) = link.strip_prefix(emoji::OWN_LINK) {
            let (name, animated, _) = emoji::own_one(id).unwrap_or_default();
            EmojiCard {
                p,
                link: link.to_string(),
                text: format!("<:{name}:{id}>"),
                custom: Some(id.to_string()),
                names: vec![name],
                about: Some(
                    if animated {
                        "Yours · animated"
                    } else {
                        "Yours"
                    }
                    .to_string(),
                ),
            }
        } else {
            let file = link.strip_prefix(emoji::LINK).unwrap_or_default();
            let text = emoji::text_of(file).unwrap_or_default().to_string();
            let names = EMOJI_NAMES
                .with(|n| n.borrow().get(&emoji::untoned(&text)).cloned())
                .unwrap_or_default();
            EmojiCard {
                p,
                link: link.to_string(),
                text,
                custom: None,
                names,
                about: None,
            }
        };
        emoji::set_hovered(Some(card.link.clone()));
        let _ = window;
        cx.new(|_| card).into()
    }
}

impl Drop for EmojiCard {
    fn drop(&mut self) {
        if emoji::hovered().as_deref() == Some(self.link.as_str()) {
            emoji::set_hovered(None);
        }
    }
}

impl Render for EmojiCard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        div()
            .p(px(space::S))
            .flex()
            .items_center()
            .gap(px(space::M))
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .child(picture_at(
                &self.text,
                self.custom.as_deref(),
                emoji::JUMBO,
                true,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .pr(px(space::S))
                    .when_some(self.names.first().cloned(), |d, n| {
                        d.child(
                            div()
                                .font_family(crate::prefs::mono_font())
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(format!(":{n}:")),
                        )
                    })
                    .when_some(self.about.clone(), |d, a| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(a),
                        )
                    }),
            )
    }
}

thread_local! {
    /// The colours, for the card over an emoji, which the chat's tooltips build without
    /// the window's.
    static EMOJI_PALETTE: std::cell::Cell<Option<Palette>> = const { std::cell::Cell::new(None) };
    /// Each built-in emoji's names, from the picker, for its card.
    static EMOJI_NAMES: RefCell<Rc<HashMap<String, Vec<String>>>> = RefCell::new(Rc::new(HashMap::new()));
}

impl Loom {
    /// Gives the chat's cards what they draw from: the colours and the emoji's names.
    pub(super) fn sync_emoji_cards(&self) {
        EMOJI_PALETTE.with(|c| c.set(Some(self.p)));
        EMOJI_NAMES.with(|n| {
            if n.borrow().len() != self.model.emoji.names.len() {
                *n.borrow_mut() = Rc::new(self.model.emoji.names.clone());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        lines, picker_fade, picker_lift, picker_place, unpieced, Line, Side, CHROME, COLS,
        PICKER_H, TILE,
    };
    use crate::emoji::{Group, Item};

    #[test]
    fn the_picker_opens_on_the_side_of_the_composer_with_more_room() {
        // How many lines of its grid a picker so tall shows.
        let shown = |h: f32| ((h - CHROME) / TILE).floor();
        // A chat's composer at the foot of the window: above it, all of it.
        assert_eq!(
            picker_place(Some((690.0, 740.0)), 752.0),
            (Side::Above, PICKER_H)
        );
        // And in a short window, as tall as fits under the title bar.
        assert_eq!(
            picker_place(Some((410.0, 460.0)), 472.0),
            (Side::Above, 410.0 - 48.0 - 16.0)
        );
        // A new chat's composer in the middle of the window, as it is at the default
        // height: below it, with four lines of its grid.
        let (side, h) = picker_place(Some((370.0, 420.0)), 752.0);
        assert_eq!((side, h), (Side::Below, 752.0 - 420.0 - 8.0 - 12.0));
        assert!(shown(h) >= 4.0);
        // Room for all of it below and not above: below, all of it.
        assert_eq!(
            picker_place(Some((300.0, 350.0)), 1000.0),
            (Side::Below, PICKER_H)
        );
        // A new chat in a tall window, with room for all of it above: above.
        assert_eq!(
            picker_place(Some((520.0, 570.0)), 1052.0),
            (Side::Above, PICKER_H)
        );
        // Neither side with four lines, at the window's least height: the larger, and
        // its grid scrolls.
        let (side, h) = picker_place(Some((234.0, 284.0)), 480.0);
        assert_eq!((side, h), (Side::Below, 480.0 - 284.0 - 20.0));
        assert!(shown(h) < 4.0);
        // Never shorter than what's round its grid.
        assert_eq!(picker_place(Some((100.0, 150.0)), 200.0).1, CHROME);
        // Before the composer is laid out: above, all of it.
        assert_eq!(picker_place(None, 752.0), (Side::Above, PICKER_H));
    }

    #[test]
    fn a_new_chat_moves_up_as_far_as_gives_the_picker_all_of_its_height_under_it() {
        // Where the picker lands once the composer from `top` to `bottom` moved up.
        let lifted = |top: f32, bottom: f32, high: f32| {
            let lift = picker_lift(top, bottom, high);
            (lift, picker_place(Some((top - lift, bottom - lift)), high))
        };
        // A chat's composer at the window's foot has all of it above: it stays.
        assert_eq!(picker_lift(690.0, 740.0, 752.0), 0.0);
        // A new chat in a tall window, all of it above, or all of it below.
        assert_eq!(picker_lift(520.0, 570.0, 1052.0), 0.0);
        assert_eq!(picker_lift(300.0, 350.0, 1000.0), 0.0);
        // At the default height: up as far as leaves all of it below, and no further.
        assert_eq!(lifted(370.0, 420.0, 752.0), (88.0, (Side::Below, PICKER_H)));
        // A shorter window, the same.
        let (lift, place) = lifted(294.0, 344.0, 600.0);
        assert_eq!((lift, place), (164.0, (Side::Below, PICKER_H)));
        assert!(294.0 - lift > 48.0 + 12.0);
        // The window's least height: up to 12 px under the title bar, and the picker
        // takes what's below it and its grid scrolls.
        let (lift, place) = lifted(234.0, 284.0, 480.0);
        assert_eq!(234.0 - lift, 48.0 + 12.0);
        assert_eq!(place, (Side::Below, 480.0 - (284.0 - lift) - 20.0));
        assert!(place.1 < PICKER_H);
        // A tall composer there, the same.
        let (lift, place) = lifted(150.0, 330.0, 480.0);
        assert_eq!(150.0 - lift, 48.0 + 12.0);
        assert_eq!(place, (Side::Below, 480.0 - (330.0 - lift) - 20.0));
        // Already up against the title bar: it stays.
        assert_eq!(picker_lift(55.0, 300.0, 400.0), 0.0);
    }

    #[test]
    fn what_of_the_greeting_would_pass_under_the_title_bar_fades_on_the_way() {
        // With the block down, all of it shows.
        assert_eq!(picker_fade(120.0, 0.0, 174.0), 1.0);
        assert_eq!(picker_fade(120.0, 0.0, 0.0), 1.0);
        // A piece that stays under the title bar all the way up shows all the way.
        assert_eq!(picker_fade(200.0, 100.0, 100.0), 1.0);
        assert_eq!(picker_fade(200.0, 152.0, 152.0), 1.0);
        // One that would pass under it fades as it rises, gone as it reaches it, and
        // stays gone the rest of the way.
        assert_eq!(picker_fade(120.0, 36.0, 174.0), 0.5);
        assert_eq!(picker_fade(120.0, 72.0, 174.0), 0.0);
        assert_eq!(picker_fade(120.0, 174.0, 174.0), 0.0);
    }

    fn item(e: &str) -> Item {
        Item {
            emoji: e.into(),
            ..Item::default()
        }
    }

    #[test]
    fn the_grid_has_a_heading_for_each_group_and_your_emoji_its_plus() {
        let groups = vec![
            Group {
                id: "recent".into(),
                name: "Recently used".into(),
                emoji: vec![],
            },
            Group {
                id: "custom".into(),
                name: "Your emoji".into(),
                emoji: vec![],
            },
            Group {
                id: "smileys".into(),
                name: "Smileys & Emotion".into(),
                emoji: (0..COLS + 1).map(|_| item("😀")).collect(),
            },
        ];
        let l = lines(&groups, None);
        // No Recently used yet; Your emoji's heading and its + alone; two lines of smileys.
        assert_eq!(l.len(), 5);
        assert!(matches!(&l[0], Line::Head(id, _) if id == "custom"));
        assert!(matches!(&l[1], Line::Tiles(t, true) if t.is_empty()));
        assert!(matches!(&l[3], Line::Tiles(t, false) if t.len() == COLS));
        assert!(matches!(&l[4], Line::Tiles(t, false) if t.len() == 1));
        let found = [item("😭")];
        assert_eq!(lines(&groups, Some(&found)).len(), 2);
        assert!(lines(&groups, Some(&[])).is_empty());
    }

    #[test]
    fn typed_emoji_become_pieces_but_not_in_code() {
        let text = "so 😭 `😀` 👍";
        let found: Vec<String> = unpieced(text, &[]).into_iter().map(|(_, w)| w).collect();
        assert_eq!(found, vec!["😭", "👍"]);
        // One that's a piece already isn't again.
        let at = text.find('👍').unwrap();
        let piece = at..at + '👍'.len_utf8();
        assert_eq!(unpieced(text, std::slice::from_ref(&piece)).len(), 1);
    }
}
