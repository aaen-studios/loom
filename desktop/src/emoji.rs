//! Emoji as the app draws them (PLAN.md, "Emoji"): built-in ones with Twemoji's
//! pictures, embedded from design/twemoji, and the person's own from their pictures.
//!
//! An SVG is rasterized at the size it's drawn, in device pixels, with GPUI's own
//! renderer (resvg), so an emoji is sharp at every size and scale. GPUI's `img` would
//! rasterize an SVG once, at 1×. What an emoji is called, the shortcodes and the
//! picker come from the core (`shared/emoji.calx`); this only finds and draws them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use gpui_kit::*;

include!(concat!(env!("OUT_DIR"), "/twemoji.rs"));

/// Each way an emoji is written, a tab, and its picture's name.
const SEQUENCES: &str = include_str!("../../design/twemoji/sequences.txt");

/// How big an emoji is drawn among words, against the text's size.
pub const IN_TEXT: f32 = 1.25;
/// How many of their own a person can have (protocol.md, "Emoji").
pub const MOST: usize = 200;
/// How big a message of only one to three emoji draws them (PLAN.md, "Emoji").
pub const JUMBO: f32 = 48.0;

struct Table {
    by_text: HashMap<&'static str, &'static str>,
    /// The way each picture is written first, by its name.
    by_file: HashMap<&'static str, &'static str>,
    /// The most runes an emoji is written with.
    longest: usize,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut by_text = HashMap::new();
        let mut by_file = HashMap::new();
        let mut longest = 1;
        for line in SEQUENCES.lines() {
            if line.starts_with("# ") {
                continue;
            }
            let Some((text, file)) = line.split_once('\t') else {
                continue;
            };
            longest = longest.max(text.chars().count());
            by_text.insert(text, file);
            by_file.entry(file).or_insert(text);
        }
        Table {
            by_text,
            by_file,
            longest,
        }
    })
}

/// Whether `c` can start an emoji: anything past ASCII, or a keycap's `#`, `*` or digit.
fn may_start(c: char) -> bool {
    !c.is_ascii() || matches!(c, '#' | '*' | '0'..='9')
}

/// The built-in emoji in `text`, in order: each one's bytes and its picture's name.
/// The longest way of writing one wins, so a family is one emoji, not four people.
pub fn find(text: &str) -> Vec<(Range<usize>, &'static str)> {
    let t = table();
    let mut out = Vec::new();
    let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let mut k = 0;
    while k < starts.len() {
        let at = starts[k];
        let c = text[at..].chars().next().unwrap_or(' ');
        if !may_start(c) {
            k += 1;
            continue;
        }
        let mut found = None;
        let most = t.longest.min(starts.len() - k);
        for n in (1..=most).rev() {
            let end = starts.get(k + n).copied().unwrap_or(text.len());
            if let Some(file) = t.by_text.get(&text[at..end]) {
                found = Some((n, end, *file));
                break;
            }
        }
        match found {
            Some((n, end, file)) => {
                out.push((at..end, file));
                k += n;
            }
            None => k += 1,
        }
    }
    out
}

/// The picture of the emoji written exactly `text`, if it's one.
pub fn picture_of(text: &str) -> Option<&'static str> {
    table().by_text.get(text).copied()
}

/// One of the person's own emoji in a message's text, `<:party_cat:e_3>` or
/// `<a:party_cat:e_3>` for one that moves (protocol.md, "Emoji").
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub range: Range<usize>,
    pub name: String,
    pub id: String,
    pub animated: bool,
}

/// The tokens in `text`, in order, as `shared/emoji.calx` reads them.
pub fn tokens(text: &str) -> Vec<Token> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        match token_at(text, i) {
            Some(t) => {
                i = t.range.end;
                out.push(t);
            }
            None => i += 1,
        }
    }
    out
}

fn token_at(text: &str, i: usize) -> Option<Token> {
    let b = text.as_bytes();
    let mut j = i + 1;
    let animated = b.get(j) == Some(&b'a');
    if animated {
        j += 1;
    }
    if b.get(j) != Some(&b':') {
        return None;
    }
    j += 1;
    let name_from = j;
    while j < b.len() && b[j] != b':' {
        if !(b[j].is_ascii_lowercase() || b[j].is_ascii_digit() || b[j] == b'_') {
            return None;
        }
        j += 1;
    }
    let name_len = j - name_from;
    if j >= b.len() || !(2..=32).contains(&name_len) {
        return None;
    }
    let name_to = j;
    j += 1;
    let id_from = j;
    while j < b.len() && b[j] != b'>' {
        if !(b[j].is_ascii_alphanumeric() || b[j] == b'_') {
            return None;
        }
        j += 1;
    }
    if j >= b.len() || j == id_from || j - id_from > 40 {
        return None;
    }
    Some(Token {
        range: i..j + 1,
        name: text[name_from..name_to].to_string(),
        id: text[id_from..j].to_string(),
        animated,
    })
}

/// `text` with each of the person's own emoji as `:name:`, as copying gives it.
pub fn as_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for t in tokens(text) {
        out.push_str(&text[at..t.range.start]);
        out.push_str(&format!(":{}:", t.name));
        at = t.range.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Whether a message's words are only one to three emoji, built-in or the person's
/// own, with nothing but spaces between, which the chat shows big.
pub fn jumbo(text: &str) -> bool {
    let mut marks: Vec<Range<usize>> = tokens(text).into_iter().map(|t| t.range).collect();
    let custom: Vec<Range<usize>> = marks.clone();
    for (r, _) in find(text) {
        if !custom.iter().any(|c| c.start <= r.start && r.end <= c.end) {
            marks.push(r);
        }
    }
    if marks.is_empty() || marks.len() > 3 {
        return false;
    }
    marks.sort_by_key(|r| r.start);
    let mut at = 0;
    for r in &marks {
        if !text[at..r.start].trim().is_empty() {
            return false;
        }
        at = r.end;
    }
    text[at..].trim().is_empty()
}

/// The SVG of a picture, by its name (`1f62d`).
#[cfg(test)]
pub fn svg(name: &str) -> Option<&'static [u8]> {
    SVGS.binary_search_by(|(n, _)| n.cmp(&name))
        .ok()
        .map(|i| SVGS[i].1)
}

thread_local! {
    /// The renderer GPUI made, set once the app starts (`set_renderer`).
    static RENDERER: RefCell<Option<SvgRenderer>> = const { RefCell::new(None) };
    /// Pictures drawn already, by name and side in device pixels.
    static DRAWN: RefCell<HashMap<(&'static str, u32), Arc<RenderImage>>> = RefCell::new(HashMap::new());
}

/// Keeps GPUI's SVG renderer, which the pictures are drawn with.
pub fn set_renderer(cx: &App) {
    RENDERER.with(|r| *r.borrow_mut() = Some(cx.svg_renderer()));
}

/// The picture `name` drawn `side` device pixels square, made once.
pub fn image(name: &str, side: u32) -> Option<Arc<RenderImage>> {
    let at = SVGS.binary_search_by(|(n, _)| n.cmp(&name)).ok()?;
    let (key, bytes) = SVGS[at];
    let side = side.clamp(8, 512);
    if let Some(done) = DRAWN.with(|d| d.borrow().get(&(key, side)).cloned()) {
        return Some(done);
    }
    let made = RENDERER.with(|r| {
        let r = r.borrow();
        let r = r.as_ref()?;
        let parsed = r.parse_svg(bytes).ok()?;
        let wanted = size(DevicePixels(side as i32), DevicePixels(side as i32));
        r.render_parsed(&parsed, SvgSize::ExactSize(wanted)).ok()
    })?;
    DRAWN.with(|d| {
        let mut d = d.borrow_mut();
        // The picker shows a few hundred at a time; a long session draws many more.
        if d.len() > 2000 {
            d.clear();
        }
        d.insert((key, side), made.clone());
    });
    Some(made)
}

/// How a picture is named in a message's Markdown: its name and its side in device
/// pixels, so the chat's text draws each at its size (`image_source`).
pub const SOURCE: &str = "loom-emoji:";

pub fn source(name: &str, side: u32) -> String {
    format!("{SOURCE}{name}@{side}")
}

/// The picture a `source` names, drawn.
pub fn from_source(uri: &str) -> Option<Arc<RenderImage>> {
    if let Some(path) = uri.strip_prefix(STILL_SOURCE) {
        return still(path);
    }
    let (name, side) = uri.strip_prefix(SOURCE)?.split_once('@')?;
    image(name, side.parse().ok()?)
}

/// `text` without skin tones and U+FE0F, to find an emoji's names by its plain self.
pub fn untoned(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c as u32, 0x1F3FB..=0x1F3FF | 0xFE0F))
        .collect()
}

/// How an emoji is first written, by its picture's name.
pub fn text_of(name: &str) -> Option<&'static str> {
    table().by_file.get(name).copied()
}

// --- Emoji in the chat's words ---

/// One of the person's own as the chat draws it: its name, whether it moves, and its
/// picture once fetched.
pub type Own = (String, bool, Option<String>);

thread_local! {
    /// The window's scale, for the pictures' sides in device pixels.
    static SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
    /// The person's own emoji the chat can draw: each one's name, whether it moves, and
    /// its picture once fetched, by id.
    static OWN: RefCell<HashMap<String, Own>> = RefCell::new(HashMap::new());
    /// The pictures of the person's own the chat met before they were fetched.
    static WANTED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The emoji under the pointer in the chat, while its card shows: its link.
    static HOVERED: RefCell<Option<String>> = const { RefCell::new(None) };
}

thread_local! {
    /// Play animated emoji is Always, and the system doesn't ask for less motion.
    static PLAY: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((true, false)) };
    /// The first frame of each of the person's own that moves, by its picture's path.
    static STILLS: RefCell<HashMap<String, Option<Arc<RenderImage>>>> = RefCell::new(HashMap::new());
}

/// How Play animated emoji is set, and whether the system asks for less motion.
pub fn set_play(always: bool, reduced: bool) {
    PLAY.with(|p| p.set((always, reduced)));
}

/// Whether one that moves plays now, under the pointer or not (`plays`).
pub fn playing(hovered: bool) -> bool {
    let (always, reduced) = PLAY.with(|p| p.get());
    plays(always, hovered, reduced)
}

/// How a still of one of the person's own that moves is named in a message's Markdown.
pub const STILL_SOURCE: &str = "loom-emoji-still:";

/// The first frame of the picture at `path`, made once.
pub fn still(path: &str) -> Option<Arc<RenderImage>> {
    if let Some(done) = STILLS.with(|s| s.borrow().get(path).cloned()) {
        return done;
    }
    let made = std::fs::read(path)
        .ok()
        .and_then(|bytes| image::load_from_memory(&bytes).ok())
        .map(|first| {
            let mut bgra = first.to_rgba8();
            for px in bgra.pixels_mut() {
                px.0.swap(0, 2);
            }
            Arc::new(RenderImage::new(vec![image::Frame::new(bgra)]))
        });
    STILLS.with(|s| s.borrow_mut().insert(path.to_string(), made.clone()));
    made
}

pub fn set_scale(scale: f32) {
    SCALE.with(|s| s.set(scale.max(1.0)));
}

/// `logical` pixels in device pixels, for a picture's side.
pub fn device(logical: f32) -> u32 {
    (logical * SCALE.with(|s| s.get())).ceil() as u32
}

/// Gives the chat the person's own emoji, and where each picture is.
pub fn sync_own(e: &Emojis) {
    OWN.with(|o| {
        let mut o = o.borrow_mut();
        o.clear();
        for c in &e.own {
            o.insert(
                c.id.clone(),
                (c.name.clone(), c.animated, e.paths.get(&c.id).cloned()),
            );
        }
    });
}

/// The pictures the chat wanted and doesn't have, to fetch.
pub fn wanted() -> Vec<String> {
    WANTED.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

/// One of the person's own, as the chat knows it: its name, whether it moves, and its
/// picture once there. One that isn't fetched yet is wanted.
pub fn own_one(id: &str) -> Option<Own> {
    let found = OWN.with(|o| o.borrow().get(id).cloned())?;
    if found.2.is_none() {
        WANTED.with(|w| {
            let mut w = w.borrow_mut();
            if !w.iter().any(|x| x == id) {
                w.push(id.to_string());
            }
        });
    }
    Some(found)
}

pub fn set_hovered(link: Option<String>) {
    HOVERED.with(|h| *h.borrow_mut() = link);
}

/// The emoji whose card shows, by its link.
pub fn hovered() -> Option<String> {
    HOVERED.with(|h| h.borrow().clone())
}

/// How an emoji in the chat links to its card: `loom:emoji/1f62d` for a built-in one,
/// by its picture, and `loom:emoji/own/e_3` for one of the person's own.
pub const LINK: &str = "loom:emoji/";
pub const OWN_LINK: &str = "loom:emoji/own/";
/// How a picture of the person's own is named in a message's Markdown.
pub const OWN_SOURCE: &str = "loom-emoji-own:";

/// The fenced code blocks and code spans in `text`, which emoji are never drawn in, as
/// `shared/emoji.calx` finds them: one left open runs to the end.
pub fn code_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut fence: Option<(usize, char, usize)> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map_or(0, |m| trimmed.chars().take_while(|c| *c == m).count());
        match fence {
            Some((from, m, n)) => {
                if marker == Some(m) && run >= n && trimmed[run..].trim().is_empty() {
                    out.push(from..at + line.len());
                    fence = None;
                }
            }
            None if run >= 3 => fence = Some((at, marker.unwrap_or('`'), run)),
            None => {
                let b = line.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] != b'`' {
                        i += 1;
                        continue;
                    }
                    let n = b[i..].iter().take_while(|c| **c == b'`').count();
                    let mut j = i + n;
                    let mut end = None;
                    while j < b.len() {
                        if b[j] == b'`' {
                            let m = b[j..].iter().take_while(|c| **c == b'`').count();
                            if m == n {
                                end = Some(j + m);
                                break;
                            }
                            j += m;
                        } else {
                            j += 1;
                        }
                    }
                    match end {
                        Some(e) => {
                            out.push(at + i..at + e);
                            i = e;
                        }
                        None => i += n,
                    }
                }
            }
        }
        at += line.len();
    }
    if let Some((from, _, _)) = fence {
        out.push(from..text.len());
    }
    out
}

/// A message's Markdown with each emoji in it drawn: a built-in one as its Twemoji
/// picture, one of the person's own as its picture, both `text_px` tall among words or
/// `JUMBO` in a message of only one to three, and each a link to its card. Code stays as
/// it is, and one of the person's own that isn't known stays as its token.
pub fn pictured(text: &str, text_px: f32) -> String {
    if text.is_ascii() && !text.contains("<:") && !text.contains("<a:") {
        return text.to_string();
    }
    // Escaped as the person's words are (`model::literal_html`): `\<:name:id>`.
    let side = if jumbo(&text.replace("\\<", "<")) {
        JUMBO
    } else {
        (text_px * IN_TEXT).round()
    };
    let code = code_ranges(text);
    let in_code = |r: &Range<usize>| code.iter().any(|c| c.start <= r.start && r.start < c.end);
    // Each emoji: where it is, its picture's source, its link and what it copies as.
    let mut marks: Vec<(Range<usize>, String, String, String)> = Vec::new();
    for t in tokens(text) {
        if in_code(&t.range) {
            continue;
        }
        let Some((name, animated, Some(path))) = own_one(&t.id) else {
            continue;
        };
        let mut range = t.range.clone();
        if range.start > 0 && text.as_bytes()[range.start - 1] == b'\\' {
            range.start -= 1;
        }
        // One that moves is still unless it plays: its card plays it under the pointer.
        let from = if animated && !playing(false) {
            STILL_SOURCE
        } else {
            OWN_SOURCE
        };
        marks.push((
            range,
            format!("{from}{path}"),
            format!("{OWN_LINK}{}", t.id),
            format!(":{name}:"),
        ));
    }
    for (r, file) in find(text) {
        if in_code(&r)
            || marks
                .iter()
                .any(|m| m.0.start <= r.start && r.start < m.0.end)
        {
            continue;
        }
        marks.push((
            r.clone(),
            source(file, device(side)),
            format!("{LINK}{file}"),
            text[r].to_string(),
        ));
    }
    if marks.is_empty() {
        return text.to_string();
    }
    marks.sort_by_key(|m| m.0.start);
    let mut out = String::with_capacity(text.len() + marks.len() * 96);
    let mut at = 0;
    // Inside a link's words, as a mention's, an emoji is only its picture.
    let mut depth = 0i32;
    for (range, src, link, alt) in marks {
        let before = &text[at..range.start];
        for c in before.chars() {
            match c {
                '[' => depth += 1,
                ']' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
        out.push_str(before);
        let w = side as u32;
        let picture = format!("<img src=\"{src}\" alt=\"{alt}\" width=\"{w}\" height=\"{w}\" />");
        if depth > 0 {
            out.push_str(&picture);
        } else {
            out.push_str(&format!("[{picture}]({link})"));
        }
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}

// --- What the core says of emoji (core/README.md, "Between the app and the core") ---

use serde_json::{json, Value};

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// An emoji as the core gives it, for the suggestion row and the picker: built-in, as
/// itself in the person's skin tone, or one of the person's own, as its token.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    /// What it's sent as: the character, or `<:name:id>`.
    pub emoji: String,
    /// Its names, the first its own.
    pub names: Vec<String>,
    /// The name that matched what's typed, shown at the row's end.
    pub name: String,
    /// One of the person's own: its id.
    pub custom: Option<String>,
    pub animated: bool,
    /// It has skin tones.
    pub tones: bool,
}

impl Item {
    pub fn of(v: &Value) -> Item {
        let names: Vec<String> = v
            .get("names")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| names.first().cloned())
            .unwrap_or_default();
        Item {
            emoji: s(v, "emoji"),
            names,
            name,
            custom: v.get("custom").and_then(Value::as_str).map(String::from),
            animated: v.get("animated").and_then(Value::as_bool) == Some(true),
            tones: v.get("tones").and_then(Value::as_bool) == Some(true),
        }
    }
}

/// One of the picker's groups: Recently used, Your emoji, then Unicode's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub emoji: Vec<Item>,
}

/// One of the person's own, as the instance keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Custom {
    pub id: String,
    pub name: String,
    pub file: String,
    pub animated: bool,
}

/// The suggestion row over the composer while a name is typed after a colon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// What a pick takes the place of, at the cursor: `:so`.
    pub suffix: String,
    pub items: Vec<Item>,
    pub lit: usize,
}

/// What the composer's words should become at the cursor: `suffix` there goes, and
/// `with` takes its place. `item` is the emoji, for a piece of one.
#[derive(Clone, Debug, PartialEq)]
pub struct Replace {
    pub suffix: String,
    pub with: String,
    pub item: Option<Item>,
}

/// The app's emoji: the person's own, the suggestion row, the picker's groups and
/// search, and where each of the person's own pictures is.
#[derive(Default)]
pub struct Emojis {
    /// The skin tone picked, 0 for none and 1 to 5 light to dark.
    pub tone: u32,
    pub own: Vec<Custom>,
    pub row: Option<Row>,
    /// A replacement the core said for what was typed, for the app to make.
    pub replace: Option<Replace>,
    pub groups: Vec<Group>,
    /// The picker's search: what was asked, and what it found.
    pub found: Option<(String, Vec<Item>)>,
    /// The names of each built-in emoji, by itself without skin tones, from the picker.
    pub names: HashMap<String, Vec<String>>,
    /// Where each of the person's own pictures is, once fetched, by its id.
    pub paths: HashMap<String, String>,
    fetching: std::collections::HashSet<String>,
    /// What went wrong adding, renaming or deleting one, to show.
    pub error: Option<String>,
    /// The one just deleted, for its Undo.
    pub deleted: Option<Custom>,
    /// The person's own changed, so the list is read again.
    stale: bool,
    asked: u64,
    /// The picker's groups were asked for, in this skin tone.
    picker_tone: Option<u32>,
    /// Frames to send once something the core said comes: an emoji to add once its
    /// picture went up.
    outbox: Vec<Value>,
    /// A picture going up to be one of the person's own, and the name it gets.
    pub uploading: Option<String>,
    want_picker: bool,
}

impl Emojis {
    /// What's typed up to the cursor, for the suggestion row or a replacement.
    pub fn typed(&mut self, before: &str, emoticons: bool) -> Value {
        let tone = self.tone;
        self.asked += 1;
        json!({"t": "emoji.typed", "id": format!("app.emoji.typed:{}", self.asked), "text": before, "tone": tone, "emoticons": emoticons})
    }

    pub fn find(&mut self, query: &str) -> Value {
        let tone = self.tone;
        json!({"t": "emoji.find", "id": format!("app.emoji.find:{query}"), "query": query, "tone": tone, "limit": 120})
    }

    /// The picker's groups, when they aren't there in this skin tone.
    pub fn picker(&mut self, again: bool) -> Option<Value> {
        let tone = self.tone;
        if !again && self.picker_tone == Some(tone) && !self.groups.is_empty() {
            return None;
        }
        self.picker_tone = Some(tone);
        Some(json!({"t": "emoji.picker", "id": "app.emoji.picker", "tone": tone}))
    }

    pub fn used(&mut self, emoji: &str) -> Value {
        // Recently used changes: the picker reads its groups again when it opens.
        self.picker_tone = None;
        json!({"t": "emoji.used", "emoji": emoji})
    }

    /// Fetches one of the person's own pictures, or anyone's in the chat, once.
    pub fn fetch(&mut self, id: &str) -> Option<Value> {
        if self.paths.contains_key(id) || !self.fetching.insert(id.to_string()) {
            return None;
        }
        Some(json!({"t": "emoji.file", "id": format!("app.emoji.file:{id}"), "emoji": id}))
    }

    /// Sends a picture to be one of the person's own, named after its file.
    pub fn upload(&mut self, path: &str) -> Value {
        let name = name_from_file(path);
        self.error = None;
        self.uploading = Some(name.clone());
        json!({"t": "file.upload", "id": format!("app.emoji.upload:{name}"), "path": path})
    }

    pub fn add(&self, file: &str, name: &str) -> Value {
        json!({"t": "cmd", "id": "app.emoji.cmd:add", "name": "emoji.add", "args": {"file": file, "name": name}})
    }

    pub fn rename(&self, id: &str, name: &str) -> Value {
        json!({"t": "cmd", "id": "app.emoji.cmd:rename", "name": "emoji.rename", "args": {"emoji": id, "name": name}})
    }

    pub fn delete(&mut self, id: &str) -> Value {
        self.deleted = self.own.iter().find(|c| c.id == id).cloned();
        json!({"t": "cmd", "id": "app.emoji.cmd:delete", "name": "emoji.delete", "args": {"emoji": id}})
    }

    /// Undoes the delete just made.
    pub fn restore(&mut self) -> Option<Value> {
        let gone = self.deleted.take()?;
        Some(
            json!({"t": "cmd", "id": "app.emoji.cmd:restore", "name": "emoji.restore", "args": {"emoji": gone.id}}),
        )
    }

    /// The frames to send for what changed.
    pub fn fetches(&mut self) -> Vec<Value> {
        let mut out = std::mem::take(&mut self.outbox);
        if std::mem::take(&mut self.stale) {
            out.push(json!({"t": "core.get", "id": "app.emoji.list", "what": "emoji"}));
        }
        if std::mem::take(&mut self.want_picker) {
            out.extend(self.picker(true));
        }
        out
    }

    /// One of the person's own by its id.
    pub fn own(&self, id: &str) -> Option<&Custom> {
        self.own.iter().find(|c| c.id == id)
    }

    /// Takes what the core said of emoji. Gives whether it was this, and the screen
    /// should be drawn again.
    pub fn take(&mut self, f: &Value) -> bool {
        let t = s(f, "t");
        let id = s(f, "id");
        match t.as_str() {
            "synced" => {
                self.stale = true;
                // The picker's groups, for the names on an emoji's card in the chat.
                self.want_picker = self.groups.is_empty();
                false
            }
            "event" if s(f, "kind") == "emoji.changed" => {
                self.set_own(f.get("emoji"));
                self.picker_tone = None;
                true
            }
            "core.data" if s(f, "what") == "emoji" => {
                self.set_own(f.get("items"));
                true
            }
            "core.file" if id.starts_with("app.emoji.upload:") => {
                let name = id.trim_start_matches("app.emoji.upload:").to_string();
                match f.pointer("/file/file").and_then(Value::as_str) {
                    Some(file) => {
                        let add = self.add(file, &name);
                        self.outbox.push(add);
                    }
                    None => {
                        self.uploading = None;
                        self.error = Some(
                            f.pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("The picture didn't go up.")
                                .to_string(),
                        );
                    }
                }
                true
            }
            "core.file" if id.starts_with("app.emoji.file:") => {
                let emoji = id.trim_start_matches("app.emoji.file:").to_string();
                self.fetching.remove(&emoji);
                if let Some(path) = f.get("path").and_then(Value::as_str) {
                    self.paths.insert(emoji, path.to_string());
                }
                true
            }
            "result" if id.starts_with("app.emoji.cmd:") => {
                if id == "app.emoji.cmd:add" {
                    self.uploading = None;
                }
                self.error = f
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(String::from);
                true
            }
            "core.emoji" => {
                if id.starts_with("app.emoji.typed:") {
                    // Only the answer to what was typed last.
                    if id != format!("app.emoji.typed:{}", self.asked) {
                        return true;
                    }
                    if let Some(r) = f.get("replace") {
                        self.row = None;
                        self.replace = Some(Replace {
                            suffix: s(r, "suffix"),
                            with: s(r, "with"),
                            item: r.get("item").map(Item::of),
                        });
                        return true;
                    }
                    let items: Vec<Item> = f
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(Item::of).collect())
                        .unwrap_or_default();
                    let suffix = s(f, "suffix");
                    self.row = if items.is_empty() {
                        None
                    } else {
                        // The same name typed on keeps the emoji that was lit while it still matches.
                        let lit = match &self.row {
                            Some(r) if suffix.starts_with(&r.suffix) => r
                                .items
                                .get(r.lit)
                                .and_then(|was| items.iter().position(|i| i.emoji == was.emoji))
                                .unwrap_or(0),
                            _ => 0,
                        };
                        Some(Row { suffix, items, lit })
                    };
                } else if id == "app.emoji.picker" {
                    self.groups = f
                        .get("groups")
                        .and_then(Value::as_array)
                        .map(|gs| {
                            gs.iter()
                                .map(|g| Group {
                                    id: s(g, "id"),
                                    name: s(g, "name"),
                                    emoji: g
                                        .get("emoji")
                                        .and_then(Value::as_array)
                                        .map(|a| a.iter().map(Item::of).collect())
                                        .unwrap_or_default(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    for g in &self.groups {
                        for e in &g.emoji {
                            if e.custom.is_none() {
                                self.names.insert(untoned(&e.emoji), e.names.clone());
                            }
                        }
                    }
                } else if let Some(query) = id.strip_prefix("app.emoji.find:") {
                    let items = f
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(Item::of).collect())
                        .unwrap_or_default();
                    self.found = Some((query.to_string(), items));
                }
                true
            }
            _ => false,
        }
    }

    fn set_own(&mut self, list: Option<&Value>) {
        self.own = list
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|e| Custom {
                        id: s(e, "id"),
                        name: s(e, "name"),
                        file: s(e, "file"),
                        animated: e.get("animated").and_then(Value::as_bool) == Some(true),
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
}

/// The name a picture gets as one of the person's own, from its file's: `Party Cat.png`
/// is `party_cat`, as `emoji.add` takes it (2 to 32 of `a`–`z`, `0`–`9` and `_`).
pub fn name_from_file(path: &str) -> String {
    let file = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let mut name = String::new();
    for c in file.chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '_'
        };
        if !(c == '_' && (name.is_empty() || name.ends_with('_'))) {
            name.push(c);
        }
    }
    let mut name: String = name.trim_end_matches('_').chars().take(32).collect();
    while name.len() < 2 {
        name.push_str(if name.is_empty() { "emoji" } else { "_1" });
    }
    name.trim_end_matches('_').to_string()
}

/// Whether one of the person's own that moves plays now: as Play animated emoji says
/// (always, or only under the pointer), and never while the system asks for less
/// motion (`App::reduce_motion`, which gpui-base reads from the system).
pub fn plays(always: bool, hovered: bool, reduced: bool) -> bool {
    !reduced && (always || hovered)
}

#[cfg(test)]
mod tests {
    // Not a glob: GPUI has an attribute of its own named test.
    use super::{
        as_names, code_ranges, find, jumbo, name_from_file, pictured, plays, svg, sync_own,
        text_of, tokens, untoned, wanted, Custom, Emojis,
    };
    use serde_json::json;

    #[test]
    fn the_chat_draws_emoji_but_not_in_code() {
        super::set_scale(2.0);
        let drawn = pictured("so 😭 `😭` and\n```\n😭\n```\n", 14.0);
        assert_eq!(
            drawn,
            "so [<img src=\"loom-emoji:1f62d@36\" alt=\"😭\" width=\"18\" height=\"18\" />](loom:emoji/1f62d) `😭` and\n```\n😭\n```\n"
        );
        // A few alone are big.
        assert!(pictured("😭 👍", 14.0).contains("width=\"48\""));
        // In a mention's words, only the picture.
        assert_eq!(
            pictured("[@Cat 🐱](loom:agent/a1)", 14.0),
            "[@Cat <img src=\"loom-emoji:1f431@36\" alt=\"🐱\" width=\"18\" height=\"18\" />](loom:agent/a1)"
        );
        assert_eq!(code_ranges("a `b` c"), vec![2..5]);
        assert_eq!(code_ranges("```\nx"), vec![0..5]);
        assert_eq!(text_of("1f62d"), Some("😭"));
    }

    #[test]
    fn the_persons_own_are_drawn_once_fetched() {
        let mut e = Emojis::default();
        e.own = vec![Custom {
            id: "e_3".into(),
            name: "party_cat".into(),
            file: "ab".into(),
            animated: false,
        }];
        sync_own(&e);
        // Not fetched: it stays its token, and is wanted.
        assert_eq!(pictured("hi <:party_cat:e_3>", 14.0), "hi <:party_cat:e_3>");
        assert_eq!(wanted(), vec!["e_3".to_string()]);
        e.paths.insert("e_3".into(), "/f/e_3".into());
        sync_own(&e);
        // As the person's words have it, escaped.
        assert_eq!(
            pictured("hi \\<:party_cat:e_3>", 14.0),
            "hi [<img src=\"loom-emoji-own:/f/e_3\" alt=\":party_cat:\" width=\"18\" height=\"18\" />](loom:emoji/own/e_3)"
        );
        assert!(wanted().is_empty());
    }

    #[test]
    fn a_file_names_its_emoji() {
        assert_eq!(name_from_file("C:\\x\\Party Cat!.png"), "party_cat");
        assert_eq!(name_from_file("/x/a.png"), "a_1");
        assert_eq!(name_from_file("/x/ø.png"), "emoji");
        assert_eq!(
            name_from_file(&format!("/x/{}.png", "a".repeat(40))).len(),
            32
        );
    }

    #[test]
    fn moving_emoji_follow_the_setting_and_reduce_motion() {
        assert!(plays(true, false, false));
        assert!(!plays(false, false, false));
        assert!(plays(false, true, false));
        assert!(!plays(true, true, true));
    }

    #[test]
    fn the_core_answers_the_row_and_a_replacement() {
        let mut e = Emojis::default();
        let asked = e.typed("so :so", false);
        assert_eq!(asked["t"], "emoji.typed");
        let id = asked["id"].as_str().unwrap().to_string();
        let cat = json!({"emoji": "<:sob_cat:e_1>", "custom": "e_1", "names": ["sob_cat"], "name": "sob_cat"});
        let sob = json!({"emoji": "😭", "names": ["sob", "loudly_crying_face"], "name": "sob"});
        assert!(e.take(&json!({"t": "core.emoji", "id": id, "suffix": ":so", "query": "so", "items": [cat, sob]})));
        let row = e.row.clone().unwrap();
        assert_eq!(row.suffix, ":so");
        assert_eq!(row.items[0].custom.as_deref(), Some("e_1"));
        assert_eq!(row.items[1].name, "sob");
        // An older answer is let go; the newest is a replacement.
        let old = id.clone();
        let newer = e.typed("so :sob:", false);
        assert!(e.take(&json!({"t": "core.emoji", "id": old, "items": []})));
        assert!(e.row.is_some());
        let replace =
            json!({"suffix": ":sob:", "with": "😭", "item": {"emoji": "😭", "names": ["sob"]}});
        e.take(&json!({"t": "core.emoji", "id": newer["id"], "replace": replace}));
        assert!(e.row.is_none());
        assert_eq!(e.replace.clone().unwrap().with, "😭");
    }

    #[test]
    fn the_persons_own_come_with_each_change() {
        let mut e = Emojis::default();
        e.take(&json!({"t": "synced"}));
        assert_eq!(e.fetches()[0]["what"], "emoji");
        let party = json!({"id": "e_1", "name": "party", "file": "ab", "animated": false});
        e.take(&json!({"t": "event", "kind": "emoji.changed", "emoji": [party]}));
        assert_eq!(e.own("e_1").map(|c| c.name.as_str()), Some("party"));
        let first = e.fetch("e_1");
        assert!(first.is_some() && e.fetch("e_1").is_none());
        e.take(&json!({"t": "core.file", "id": "app.emoji.file:e_1", "path": "/x/e_1"}));
        assert_eq!(e.paths.get("e_1").map(String::as_str), Some("/x/e_1"));
        // Deleting keeps it for the Undo.
        let _ = e.delete("e_1");
        assert_eq!(e.restore().unwrap()["args"]["emoji"], "e_1");
        assert!(e.restore().is_none());
    }

    #[test]
    fn finds_each_emoji_whole() {
        let text = "so 😭 and 👍🏽, 👨‍👩‍👧 #️⃣ © ©️ 1 :sob:";
        let found: Vec<(&str, &str)> = find(text)
            .iter()
            .map(|(r, f)| (&text[r.clone()], *f))
            .collect();
        assert_eq!(
            found,
            vec![
                ("😭", "1f62d"),
                ("👍🏽", "1f44d-1f3fd"),
                ("👨‍👩‍👧", "1f468-200d-1f469-200d-1f467"),
                ("#️⃣", "23-20e3"),
                ("©️", "a9"),
            ]
        );
        assert!(svg("1f62d").is_some() && svg("nothing").is_none());
        // Each way of writing an emoji has its picture, one with skin tones too.
        for line in super::SEQUENCES.lines().filter(|l| !l.starts_with("# ")) {
            let (_, file) = line.split_once('\t').unwrap();
            assert!(svg(file).is_some(), "{file} isn't found");
        }
    }

    #[test]
    fn a_few_emoji_alone_are_big() {
        assert!(jumbo("😭"));
        assert!(jumbo(" 😭 👍🏽 <:party_cat:e_3> "));
        assert!(!jumbo("😭😭😭😭"));
        assert!(!jumbo("so 😭"));
        assert!(!jumbo(""));
        assert!(jumbo("<a:dance:e_9>"));
    }

    #[test]
    fn tokens_are_the_persons_own() {
        let t = tokens("hi <:sob_cat:e_12> <a:dance:e_13> <:X:e_1> <:a:e_1>");
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].name.as_str(), t[0].id.as_str(), t[0].animated),
            ("sob_cat", "e_12", false)
        );
        assert!(t[1].animated);
        assert_eq!(as_names("so <:sob_cat:e_12>!"), "so :sob_cat:!");
    }

    #[test]
    fn untoned_finds_the_plain_one() {
        assert_eq!(untoned("👍🏽"), "👍");
        assert_eq!(untoned("❤️"), "❤");
    }
}
