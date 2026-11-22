//! Drawing by hand: loom's colours and sizes from design/tokens.toml, text in Geist
//! (fontdue), the knot and the wordmark from design/logo, and the thread, which moves
//! as desktop/src/thread.rs moves it.

// Drawing takes a place, a size, a colour and how much shows, so its calls are long.
#![allow(clippy::too_many_arguments)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

#[allow(dead_code)]
#[path = "../../../design/generated/tokens.rs"]
pub mod tokens;
use tokens::*;

static GEIST: &[u8] = include_bytes!("../../assets/fonts/Geist-Regular.ttf");
static GEIST_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Geist-Medium.ttf");
pub static FONT_LICENCE: &str = include_str!("../../assets/fonts/OFL.txt");
static KNOT: &str = include_str!("../../../design/logo/knot.svg");
static WORDMARK: &str = include_str!("../../../design/logo/wordmark.svg");

/// The colours of one theme.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub hover: u32,
    pub active: u32,
    pub line: u32,
    pub ink: u32,
    pub ink_2: u32,
    pub ink_3: u32,
    pub button: u32,
    pub button_ink: u32,
    pub bad: u32,
    pub thread: u32,
}

impl Palette {
    pub fn new(dark: bool) -> Palette {
        if dark {
            Palette {
                bg: color_dark::BG,
                hover: color_dark::HOVER,
                active: color_dark::ACTIVE,
                line: color_dark::LINE_STRONG,
                ink: color_dark::INK,
                ink_2: color_dark::INK_2,
                ink_3: color_dark::INK_3,
                button: color_dark::BUTTON,
                button_ink: color_dark::BUTTON_INK,
                bad: color_dark::BAD,
                thread: thread_color_dark::PERIWINKLE,
            }
        } else {
            Palette {
                bg: color_light::BG,
                hover: color_light::HOVER,
                active: color_light::ACTIVE,
                line: color_light::LINE_STRONG,
                ink: color_light::INK,
                ink_2: color_light::INK_2,
                ink_3: color_light::INK_3,
                button: color_light::BUTTON,
                button_ink: color_light::BUTTON_INK,
                bad: color_light::BAD,
                thread: thread_color_light::PERIWINKLE,
            }
        }
    }
}

pub fn mix(a: u32, b: u32, k: f32) -> u32 {
    let k = k.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 255) as f32, ((b >> s) & 255) as f32);
        ((x + (y - x) * k).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

fn color(c: u32, alpha: f32) -> Color {
    Color::from_rgba8(
        (c >> 16) as u8,
        (c >> 8) as u8,
        c as u8,
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn paint(c: u32, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color(c, alpha));
    p.anti_alias = true;
    p
}

/// Slow at both ends.
pub fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Fast, then settling.
pub fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Medium,
}

struct Fonts {
    regular: fontdue::Font,
    medium: fontdue::Font,
    glyphs: HashMap<(Face, char, u32), (fontdue::Metrics, Vec<u8>)>,
}

thread_local! {
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
}

fn with_fonts<R>(f: impl FnOnce(&mut Fonts) -> R) -> R {
    FONTS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let fonts = cell.get_or_insert_with(|| Fonts {
            regular: fontdue::Font::from_bytes(GEIST, Default::default()).expect("Geist"),
            medium: fontdue::Font::from_bytes(GEIST_MEDIUM, Default::default())
                .expect("Geist Medium"),
            glyphs: HashMap::new(),
        });
        f(fonts)
    })
}

/// A pixmap to draw on, `scale` device pixels to each of loom's logical pixels.
pub struct Canvas {
    pub pix: Pixmap,
    pub scale: f32,
}

impl Canvas {
    pub fn new(w: u32, h: u32, scale: f32) -> Canvas {
        Canvas {
            pix: Pixmap::new(w.max(1), h.max(1)).unwrap(),
            scale,
        }
    }

    fn t(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
    }

    pub fn clear(&mut self, c: u32) {
        self.pix.fill(color(c, 1.0));
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: u32, alpha: f32) {
        if let Some(r) = Rect::from_xywh(x, y, w, h) {
            self.pix.fill_rect(r, &paint(c, alpha), self.t(), None);
        }
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: u32, alpha: f32) {
        if let Some(p) = round_rect_path(x, y, w, h, r) {
            self.pix
                .fill_path(&p, &paint(c, alpha), FillRule::Winding, self.t(), None);
        }
    }

    pub fn line(&mut self, pts: &[(f32, f32)], width: f32, c: u32, alpha: f32) {
        let mut pb = PathBuilder::new();
        for (i, &(x, y)) in pts.iter().enumerate() {
            if i == 0 {
                pb.move_to(x, y)
            } else {
                pb.line_to(x, y)
            }
        }
        if let Some(p) = pb.finish() {
            let s = Stroke {
                width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            self.pix
                .stroke_path(&p, &paint(c, alpha), &s, self.t(), None);
        }
    }

    /// Text in Geist, its top left at (`x`, `y`). Gives how wide it was.
    pub fn text(
        &mut self,
        s: &str,
        x: f32,
        y: f32,
        size: f32,
        face: Face,
        c: u32,
        alpha: f32,
    ) -> f32 {
        let scale = self.scale;
        let px = size * scale;
        let key = (px * 4.0).round() as u32;
        let (w, h) = (self.pix.width() as i32, self.pix.height() as i32);
        let ink = [(c >> 16) & 255, (c >> 8) & 255, c & 255];
        let alpha = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
        let data = self.pix.data_mut();
        with_fonts(|f| {
            let ascent = font_of(f, face)
                .horizontal_line_metrics(px)
                .map(|m| m.ascent)
                .unwrap_or(px * 0.8);
            let base = (y * scale + ascent).round();
            let mut pen = x * scale;
            let mut prev: Option<char> = None;
            for ch in s.chars() {
                if let Some(p) = prev {
                    pen += font_of(f, face).horizontal_kern(p, ch, px).unwrap_or(0.0);
                }
                prev = Some(ch);
                let (m, cov) = f
                    .glyphs
                    .entry((face, ch, key))
                    .or_insert_with(|| match face {
                        Face::Regular => f.regular.rasterize(ch, px),
                        Face::Medium => f.medium.rasterize(ch, px),
                    })
                    .clone();
                let gx = pen.round() as i32 + m.xmin;
                let gy = base as i32 - m.height as i32 - m.ymin;
                for row in 0..m.height {
                    let py = gy + row as i32;
                    if py < 0 || py >= h {
                        continue;
                    }
                    for col in 0..m.width {
                        let a = cov[row * m.width + col] as u32 * alpha / 255;
                        let pxx = gx + col as i32;
                        if a == 0 || pxx < 0 || pxx >= w {
                            continue;
                        }
                        let i = ((py * w + pxx) * 4) as usize;
                        for k in 0..3 {
                            data[i + k] =
                                ((data[i + k] as u32 * (255 - a) + ink[k] * a) / 255) as u8;
                        }
                    }
                }
                pen += m.advance_width;
            }
            pen / scale - x
        })
    }

    pub fn text_width(&self, s: &str, size: f32, face: Face) -> f32 {
        let px = size * self.scale;
        with_fonts(|f| {
            let font = font_of(f, face);
            let mut w = 0.0;
            let mut prev: Option<char> = None;
            for ch in s.chars() {
                if let Some(p) = prev {
                    w += font.horizontal_kern(p, ch, px).unwrap_or(0.0);
                }
                prev = Some(ch);
                w += font.metrics(ch, px).advance_width;
            }
            w / self.scale
        })
    }

    /// Text centred on `cx`, wrapped to `max` wide. Gives the lines' total height.
    pub fn text_centered(
        &mut self,
        s: &str,
        cx: f32,
        y: f32,
        max: f32,
        size: f32,
        lh: f32,
        face: Face,
        c: u32,
        alpha: f32,
    ) -> f32 {
        let lines = self.wrap(s, max, size, face);
        for (i, l) in lines.iter().enumerate() {
            let w = self.text_width(l, size, face);
            self.text(
                l,
                cx - w / 2.0,
                y + i as f32 * size * lh,
                size,
                face,
                c,
                alpha,
            );
        }
        lines.len() as f32 * size * lh
    }

    pub fn wrap(&self, s: &str, max: f32, size: f32, face: Face) -> Vec<String> {
        let mut lines = Vec::new();
        for para in s.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                // A word wider than a line, like a path, is broken where it has to be.
                if self.text_width(word, size, face) > max {
                    if !line.is_empty() {
                        lines.push(std::mem::take(&mut line));
                    }
                    for ch in word.chars() {
                        let tried = format!("{line}{ch}");
                        if self.text_width(&tried, size, face) > max && !line.is_empty() {
                            lines.push(std::mem::replace(&mut line, ch.to_string()));
                        } else {
                            line = tried;
                        }
                    }
                    continue;
                }
                let tried = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if self.text_width(&tried, size, face) > max && !line.is_empty() {
                    lines.push(std::mem::replace(&mut line, word.to_string()));
                } else {
                    line = tried;
                }
            }
            lines.push(line);
        }
        lines
    }

    /// loom's knot and wordmark, as the app draws them side by side, the knot `text * 1.2`
    /// high, centred on `cx` with its middle at `cy`.
    pub fn wordmark(&mut self, cx: f32, cy: f32, text: f32, bg: u32, ink: u32, alpha: f32) {
        let knot = text * 1.2;
        let gap = text * 0.16;
        // The outlined word is 2122 by 722 in its own units, at 1000 to the em.
        let word_w = 2122.0 * text / 1000.0;
        let word_h = 722.0 * text / 1000.0;
        let left = cx - (knot + gap + word_w) / 2.0;
        // The knot, on its 32 grid, with the gap where the falling stroke passes over.
        let ks = knot / 32.0;
        let kt = Transform::from_scale(ks, ks)
            .post_translate(left, cy - knot / 2.0)
            .post_scale(self.scale, self.scale);
        let paths = svg_paths(KNOT);
        let stroke = |w: f32| Stroke {
            width: w,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        let solid = |c: u32| paint(c, alpha);
        // In the file's order: the mask's stroke, the stroke it cuts, and the one on top.
        if let [gap, under, over] = &paths[..] {
            self.pix
                .stroke_path(under, &solid(ink), &stroke(2.6), kt, None);
            self.pix
                .stroke_path(gap, &solid(bg), &stroke(5.6), kt, None);
            self.pix
                .stroke_path(over, &solid(ink), &stroke(2.6), kt, None);
        }
        // The word, its middle on the knot's.
        let ws = text / 1000.0;
        let wt = Transform::from_translate(-70.0, -295.0)
            .post_scale(ws, ws)
            .post_translate(left + knot + gap, cy - word_h / 2.0 + text * 0.02)
            .post_scale(self.scale, self.scale);
        for p in svg_paths(WORDMARK) {
            self.pix
                .fill_path(&p, &solid(ink), FillRule::Winding, wt, None);
        }
    }
}

fn font_of(f: &Fonts, face: Face) -> &fontdue::Font {
    match face {
        Face::Regular => &f.regular,
        Face::Medium => &f.medium,
    }
}

fn round_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = 0.5523 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Every `d="…"` in an SVG, in order, as paths. Enough of SVG's path language for loom's
/// logo files: M, L, H, V, C, Q and Z, absolute.
pub fn svg_paths(svg: &str) -> Vec<tiny_skia::Path> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find(" d=\"") {
        let body = &rest[at + 4..];
        let end = body.find('"').unwrap_or(body.len());
        if let Some(p) = path_from(&body[..end]) {
            out.push(p);
        }
        rest = &body[end..];
    }
    out
}

fn path_from(d: &str) -> Option<tiny_skia::Path> {
    let mut toks: Vec<String> = Vec::new();
    let mut num = String::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            if !num.is_empty() {
                toks.push(std::mem::take(&mut num));
            }
            toks.push(ch.to_string());
        } else if ch == ',' || ch.is_whitespace() {
            if !num.is_empty() {
                toks.push(std::mem::take(&mut num));
            }
        } else if ch == '-' && !num.is_empty() {
            toks.push(std::mem::take(&mut num));
            num.push(ch);
        } else {
            num.push(ch);
        }
    }
    if !num.is_empty() {
        toks.push(num);
    }
    let mut pb = PathBuilder::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut cmd = 'M';
    let mut i = 0;
    let n = |i: &mut usize| -> f32 {
        let v = toks.get(*i).and_then(|t| t.parse().ok()).unwrap_or(0.0);
        *i += 1;
        v
    };
    while i < toks.len() {
        if let Some(c) = toks[i].chars().next().filter(|c| c.is_ascii_alphabetic()) {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                pb.close();
                continue;
            }
        }
        match cmd {
            'M' => {
                x = n(&mut i);
                y = n(&mut i);
                pb.move_to(x, y);
                cmd = 'L';
            }
            'L' => {
                x = n(&mut i);
                y = n(&mut i);
                pb.line_to(x, y);
            }
            'H' => {
                x = n(&mut i);
                pb.line_to(x, y);
            }
            'V' => {
                y = n(&mut i);
                pb.line_to(x, y);
            }
            'C' => {
                let (a, b, c, d) = (n(&mut i), n(&mut i), n(&mut i), n(&mut i));
                x = n(&mut i);
                y = n(&mut i);
                pb.cubic_to(a, b, c, d, x, y);
            }
            'Q' => {
                let (a, b) = (n(&mut i), n(&mut i));
                x = n(&mut i);
                y = n(&mut i);
                pb.quad_to(a, b, x, y);
            }
            _ => i += 1,
        }
    }
    pb.finish()
}

// --- The thread ---

/// What the thread is doing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mood {
    /// Barely moving: waiting for the person.
    Idle,
    /// Working, with a loop that travels along it as far as the work has got.
    Working,
    /// Just finished: a twang that dies away, then idle.
    Done,
    /// Slack, sagging at its far end.
    Failed,
}

/// How long the thread takes to ease from one mood to the next, in seconds.
const EASE: f32 = 0.7;

/// A thread that eases between moods and colours, as the app's do.
pub struct Thread {
    from: Mood,
    to: Mood,
    at: f32,
    tint_from: u32,
    tint_to: u32,
    tint_at: f32,
    done_at: f32,
    /// Where the loop is along it while working, eased towards the work's progress.
    pub head: f32,
}

impl Thread {
    pub fn new(tint: u32) -> Thread {
        Thread {
            from: Mood::Idle,
            to: Mood::Idle,
            at: -EASE,
            tint_from: tint,
            tint_to: tint,
            tint_at: -EASE,
            done_at: -10.0,
            head: 0.0,
        }
    }

    pub fn set(&mut self, mood: Mood, tint: u32, t: f32) {
        if self.to != mood {
            let k = (t - self.at) / EASE;
            self.from = if k < 0.5 { self.from } else { self.to };
            self.to = mood;
            self.at = t;
            if mood == Mood::Done {
                self.done_at = t;
            }
        }
        if self.tint_to != tint {
            self.tint_from = mix(
                self.tint_from,
                self.tint_to,
                smooth((t - self.tint_at) / EASE),
            );
            self.tint_to = tint;
            self.tint_at = t;
        }
    }

    /// Draws it across `x0..x0+width`, its middle at `mid`, `room` pixels above and below,
    /// `reveal` of it drawn from the left. While it works, what's ahead of the loop is
    /// still `rest`, and what's behind it has taken the thread's colour: the thread is
    /// the progress.
    pub fn draw(
        &self,
        c: &mut Canvas,
        x0: f32,
        mid: f32,
        width: f32,
        room: f32,
        t: f32,
        reveal: f32,
        stroke: f32,
        rest: u32,
    ) {
        // Not yet drawn, or drawn all the way back: nothing, not even its round end.
        if reveal * width < 1.0 {
            return;
        }
        let k = smooth((t - self.at) / EASE);
        let tint = mix(
            self.tint_from,
            self.tint_to,
            smooth((t - self.tint_at) / EASE),
        );
        let since_done = t - self.done_at;
        let steps = (width / 0.75).max(48.0) as usize;
        let failed = weight(Mood::Failed, self.from, self.to, k);
        let working = weight(Mood::Working, self.from, self.to, k);
        let end = (1.0 - 0.2 * failed) * reveal.clamp(0.0, 1.0);
        let mut pts = Vec::with_capacity(steps + 1);
        let mut split = steps + 1;
        for i in 0..=steps {
            let u = end * i as f32 / steps as f32;
            if split > steps && u >= loop_at(self.head) {
                split = i;
            }
            let (ax, ay) = bend(self.from, u, width, room, t, since_done, self.head);
            let (bx, by) = bend(self.to, u, width, room, t, since_done, self.head);
            let (dx, up) = (ax + (bx - ax) * k, ay + (by - ay) * k);
            pts.push((x0 + u * width + dx, mid - up));
        }
        let ahead = mix(tint, rest, working);
        if split <= steps && working > 0.0 {
            c.line(&pts[split..], stroke, ahead, 1.0);
            c.line(&pts[..=split.min(steps)], stroke, tint, 1.0);
        } else {
            c.line(&pts, stroke, tint, 1.0);
        }
    }
}

/// Where along the thread the working loop is, for work `head` of the way done.
fn loop_at(head: f32) -> f32 {
    0.04 + 0.92 * head.clamp(0.0, 1.0)
}

fn weight(mood: Mood, from: Mood, to: Mood, k: f32) -> f32 {
    (if from == mood { 1.0 - k } else { 0.0 }) + if to == mood { k } else { 0.0 }
}

/// The loop a thread curls into, as desktop/src/thread.rs draws it.
fn curl(u: f32, c: f32, r: f32, width: f32) -> (f32, f32) {
    if r <= 0.0 || width <= 0.0 {
        return (0.0, 0.0);
    }
    let span = (r * 2.2 / width).min(0.9);
    let s = (u - c) / span + 0.5;
    if !(0.0..=1.0).contains(&s) {
        return (0.0, 0.0);
    }
    let e = (PI * s).sin();
    (
        r * (TAU * s).sin() * e,
        r * (1.0 - (TAU * s).cos()) * e * 0.5,
    )
}

fn bend(
    mood: Mood,
    u: f32,
    width: f32,
    room: f32,
    t: f32,
    since_done: f32,
    head: f32,
) -> (f32, f32) {
    let loop_r = (room * 0.9).min(width * 0.18);
    // Waiting for the person: the slow loose wave of the app's opening weave, calm at
    // the ends so the thread comes in from nowhere.
    let edge = (PI * u).sin().max(0.0).powf(0.6);
    let idle = (TAU * (u * 1.1 - t * thread_motion_idle::SPEED)).sin() * 0.16 * edge
        + (TAU * (u * 1.3 + t * thread_motion_idle::SPEED)).sin()
            * thread_motion_idle::AMPLITUDE
            * 2.0;
    let up = match mood {
        Mood::Idle => idle,
        Mood::Done => {
            let snap = if since_done < 1.5 {
                let a = thread_motion_snap::AMPLITUDE
                    * 2.0
                    * (-thread_motion_snap::DAMPING * since_done).exp();
                a * (TAU * (thread_motion_snap::FREQUENCY / 10.0) * since_done).sin()
                    * (PI * u).sin()
            } else {
                0.0
            };
            idle + snap
        }
        Mood::Working => {
            // The thinking loop, but where the work has got to rather than sliding on its own.
            let (back, lift) = curl(u, loop_at(head), loop_r, width);
            let wave =
                (TAU * (u * 2.0 - t * 0.35)).sin() * thread_motion_thinking::AMPLITUDE * 2.0 * edge;
            return (back, wave * room + lift);
        }
        Mood::Failed => -0.55 * u * u,
    };
    (0.0, up * room)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_files_parse() {
        assert_eq!(svg_paths(KNOT).len(), 3);
        assert_eq!(svg_paths(WORDMARK).len(), 1);
    }

    #[test]
    fn colours_mix() {
        assert_eq!(mix(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(mix(0x000000, 0xffffff, 1.0), 0xffffff);
        assert_eq!(mix(0x102030, 0x102030, 0.5), 0x102030);
    }
}
