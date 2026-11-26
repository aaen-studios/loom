//! loom: moving the moving end of the window's text selection with the keyboard, as
//! Shift with an arrow, Home or End does in a text field.
//!
//! The window selection keeps its two ends as points, which each participant
//! projects onto its text as it paints. A key here works in text offsets
//! instead: it finds the character the selection's cursor stands before in
//! the runs the participants painted last, moves that place by a character, a
//! row or to a row's end, and puts the cursor back as the point that projects
//! to the new place. The place is kept with the point, so the next key starts
//! from it rather than from the point, which can't tell the end of a wrapped
//! row from the start of the next.

use gpui::{
    App, Bounds, Entity, InputEvent as _, Pixels, Point, ScrollDelta, ScrollEvent, SharedString,
    TextLayout, WeakEntity, Window, point, px,
};

use super::{SelectableTextState, SelectionEndpoint, WindowSelectionState};

/// Where a key moves the selection's moving end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMotion {
    /// Back a character.
    Left,
    /// On a character.
    Right,
    /// To the row above, as near its place across as there is.
    Up,
    /// To the row below.
    Down,
    /// To the start of its row.
    LineStart,
    /// To the end of its row.
    LineEnd,
}

/// The place a key last put the selection's cursor, kept with the point it put
/// there so that a cursor moved since, by the pointer, is found again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct KeyCaret {
    participant: gpui::EntityId,
    /// The cursor's point in its participant's content, as the key left it.
    point: Point<Pixels>,
    /// The run among its participant's, its byte offset and the row it's on.
    run: usize,
    offset: usize,
    row: usize,
    /// Where across Up and Down aim for, kept from the first of them.
    goal: Option<Pixels>,
}

/// One laid-out row of a run: where it is, and the byte offsets it holds. A
/// wrapped row ends where the next begins.
#[derive(Clone, Copy, Debug)]
struct Row {
    top: Pixels,
    left: Pixels,
    start: usize,
    end: usize,
}

/// A run of text as its participant last painted it, with its rows. What a
/// clip that cuts text off on purpose hides, as a message's Show more does, is
/// left out (`crate::cut_text_selection`); what a scrolled view hides is in,
/// and the view scrolls to it.
struct KeyRun {
    participant: usize,
    /// Its place among its participant's runs.
    index: usize,
    text: SharedString,
    layout: TextLayout,
    rows: Vec<Row>,
    /// The offsets its rows span, as far as they aren't cut off.
    min: usize,
    max: usize,
}

/// A place between two characters: the run, its byte offset, and the row it's
/// on. The offset where a row wraps is both the end of one row and the start of
/// the next; the row says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    run: usize,
    offset: usize,
    row: usize,
}

struct Participant {
    entity: Entity<SelectableTextState>,
    origin: Point<Pixels>,
    /// What of it shows now, in the window: a scrolled view's edge cuts it.
    shown: Bounds<Pixels>,
}

/// The text of every participant in the active scope, in document order.
struct KeyDocument {
    participants: Vec<Participant>,
    runs: Vec<KeyRun>,
}

impl KeyRun {
    fn line_height(&self) -> Pixels {
        self.layout.line_height()
    }

    /// The row the offset is on: the one it starts or is inside, or else the
    /// one it ends, as at the end of a line.
    fn row_of(&self, offset: usize) -> usize {
        self.rows
            .iter()
            .position(|row| row.start <= offset && offset < row.end)
            .or_else(|| self.rows.iter().position(|row| row.end == offset))
            .unwrap_or(self.rows.len().saturating_sub(1))
    }

    /// Where the place is across its row.
    fn x(&self, place: Place) -> Pixels {
        let row = self.rows[place.row];
        if place.offset == row.start {
            return row.left;
        }
        self.layout
            .position_for_index(place.offset)
            .map_or(row.left, |p| p.x)
    }

    /// The window point that the participant projects to just this place: a
    /// selection's end there takes in every character before it and none
    /// after. The layout puts the character a row wraps at on the end of the
    /// row before, and so the point for the start of a wrapped row is too.
    fn point(&self, offset: usize) -> Option<Point<Pixels>> {
        let p = self.layout.position_for_index(offset)?;
        Some(point(p.x, p.y + self.line_height() / 2.))
    }

    fn next(&self, offset: usize) -> Option<usize> {
        self.text[offset..self.max]
            .chars()
            .next()
            .map(|c| offset + c.len_utf8())
    }

    fn previous(&self, offset: usize) -> Option<usize> {
        self.text[self.min..offset]
            .chars()
            .next_back()
            .map(|c| offset - c.len_utf8())
    }

    fn first(&self, run: usize) -> Place {
        Place {
            run,
            offset: self.min,
            row: 0,
        }
    }

    fn last(&self, run: usize) -> Place {
        Place {
            run,
            offset: self.max,
            row: self.rows.len() - 1,
        }
    }
}

/// The rows `layout` was laid out in, as [`TextLayout::position_for_index`]
/// places them.
fn rows(layout: &TextLayout) -> Vec<Row> {
    let origin = layout.bounds().origin;
    let line_height = layout.line_height();
    let mut rows = Vec::new();
    let mut top = origin.y;
    let mut line_start = 0;
    for line in layout.line_layouts() {
        let mut start = line_start;
        for boundary in line.wrap_boundaries() {
            let end = line_start
                + line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index;
            rows.push(Row {
                top,
                left: origin.x,
                start,
                end,
            });
            start = end;
            top += line_height;
        }
        rows.push(Row {
            top,
            left: origin.x,
            start,
            end: line_start + line.len(),
        });
        top += line_height;
        line_start += line.len() + 1;
    }
    rows
}

/// The rows of `layout` as far as `cut` lets them show, a row cut off across
/// ending where its first character cut off starts, or `None` when none shows.
fn rows_shown(text: &str, layout: &TextLayout, cut: Option<Bounds<Pixels>>) -> Option<Vec<Row>> {
    let all = rows(layout);
    let Some(cut) = cut else {
        return (!all.is_empty()).then_some(all);
    };
    let line_height = layout.line_height();
    let mut shown = Vec::new();
    for mut row in all {
        let middle = row.top + line_height / 2.;
        if middle < cut.top() || middle >= cut.bottom() {
            continue;
        }
        let mut offset = row.start;
        let mut cut_across = false;
        while let Some(c) = text[offset..row.end].chars().next() {
            let next = offset + c.len_utf8();
            if let Some(at) = layout.position_for_index(offset)
                && at.y == row.top
            {
                let width = layout
                    .position_for_index(next)
                    .filter(|after| after.y == at.y)
                    .map_or(line_height / 2., |after| after.x - at.x);
                if !crate::selection_cut::shows(Some(cut), at, width, line_height) {
                    cut_across = true;
                    break;
                }
            }
            offset = next;
        }
        row.end = offset;
        shown.push(row);
        if cut_across {
            break;
        }
    }
    (!shown.is_empty()).then_some(shown)
}

impl KeyDocument {
    fn gather(state: &WindowSelectionState, cx: &App) -> Self {
        let mut found = state
            .participants
            .borrow()
            .values()
            .filter(|registration| registration.registration.scope == state.active_scope)
            .filter_map(|registration| {
                let geometry = &registration.registration;
                Some((
                    registration.document_order,
                    registration.participant.upgrade()?,
                    geometry.bounds.origin + geometry.scroll_offset,
                    // What of it shows now: its hitbox, as far as its clips let it.
                    geometry
                        .hitbox
                        .bounds
                        .intersect(&geometry.hitbox.content_mask.bounds),
                    geometry.cut,
                ))
            })
            .collect::<Vec<_>>();
        found.sort_by_key(|(order, ..)| *order);
        let mut participants = Vec::new();
        let mut runs = Vec::new();
        for (_, entity, origin, shown, cut) in found {
            let mut painted = entity.read(cx).runs.borrow().clone();
            painted.sort_by_key(|run| run.document_order);
            let participant = participants.len();
            for (index, run) in painted.into_iter().enumerate() {
                if run.text.is_empty() || run.text.len() != run.layout.len() {
                    continue;
                }
                let Some(rows) = rows_shown(&run.text, &run.layout, cut) else {
                    continue;
                };
                let (min, max) = (rows[0].start, rows[rows.len() - 1].end);
                runs.push(KeyRun {
                    participant,
                    index,
                    text: run.text,
                    layout: run.layout,
                    rows,
                    min,
                    max,
                });
            }
            participants.push(Participant {
                entity,
                origin,
                shown,
            });
        }
        Self { participants, runs }
    }

    fn runs_of(&self, participant: gpui::EntityId) -> impl Iterator<Item = usize> + '_ {
        self.runs.iter().enumerate().filter_map(move |(i, run)| {
            (self.participants[run.participant].entity.entity_id() == participant).then_some(i)
        })
    }

    /// The place `caret` keeps, if its participant still has that run.
    fn kept(&self, caret: &KeyCaret) -> Option<Place> {
        let run = self
            .runs_of(caret.participant)
            .find(|&i| self.runs[i].index == caret.run)?;
        let key_run = &self.runs[run];
        ((key_run.min..=key_run.max).contains(&caret.offset)
            && key_run.text.is_char_boundary(caret.offset)
            && caret.row < key_run.rows.len())
        .then_some(Place {
            run,
            offset: caret.offset,
            row: caret.row,
        })
    }

    /// The place a selection end at the window point `at`, in `participant`,
    /// stands before: past every character it takes in and before every one it
    /// leaves out, as the participant projects it.
    fn place_at(&self, participant: gpui::EntityId, at: Point<Pixels>) -> Option<Place> {
        let runs = self.runs_of(participant).collect::<Vec<_>>();
        // The run with a row across `at`, nearest it across.
        let across = runs
            .iter()
            .filter_map(|&i| {
                let run = &self.runs[i];
                let row = run
                    .rows
                    .iter()
                    .position(|row| at.y >= row.top && at.y < row.top + run.line_height())?;
                let bounds = run.layout.bounds();
                let distance = if at.x < bounds.left() {
                    bounds.left() - at.x
                } else if at.x > bounds.right() {
                    at.x - bounds.right()
                } else {
                    Pixels::ZERO
                };
                Some((distance, i, row))
            })
            .min_by(|a, b| a.0.as_f32().total_cmp(&b.0.as_f32()));
        if let Some((_, i, row)) = across {
            return Some(self.place_in_row(i, row, at.x));
        }
        // Between rows: before the first row below it, or after them all.
        for &i in &runs {
            if let Some(row) = self.runs[i].rows.iter().position(|row| row.top > at.y) {
                return Some(Place {
                    run: i,
                    offset: self.runs[i].rows[row].start,
                    row,
                });
            }
        }
        runs.last().map(|&i| self.runs[i].last(i))
    }

    /// The place in `row` of `run` that a selection end at `x` across it
    /// stands before: before the first character on the row whose middle is
    /// past `x`.
    fn place_in_row(&self, run: usize, row: usize, x: Pixels) -> Place {
        let key_run = &self.runs[run];
        let r = key_run.rows[row];
        let mut offset = r.start;
        while offset < r.end {
            let Some(next) = key_run.next(offset) else {
                break;
            };
            if let Some(at) = key_run.layout.position_for_index(offset)
                // The character a row wraps at belongs, to the layout, to the
                // row before: an end anywhere on this row takes it in.
                && at.y == r.top
            {
                let width = key_run
                    .layout
                    .position_for_index(next)
                    .filter(|after| after.y == at.y)
                    .map_or(key_run.line_height() / 2., |after| after.x - at.x);
                if at.x + width / 2. > x {
                    break;
                }
            }
            offset = next;
        }
        Place { run, offset, row }
    }

    /// Whether two rows are one row on screen, as the pieces of a line around
    /// a picture are.
    fn same_row(&self, a: (usize, usize), b: (usize, usize)) -> bool {
        let (run_a, run_b) = (&self.runs[a.0], &self.runs[b.0]);
        let height = run_a.line_height().min(run_b.line_height());
        (run_a.rows[a.1].top - run_b.rows[b.1].top).abs() < height / 2.
    }

    fn right(&self, place: Place) -> Place {
        let run = &self.runs[place.run];
        if let Some(offset) = run.next(place.offset) {
            return Place {
                run: place.run,
                offset,
                row: run.row_of(offset),
            };
        }
        let Some(next) = (place.run + 1 < self.runs.len()).then_some(place.run + 1) else {
            return place;
        };
        let first = self.runs[next].first(next);
        // The start of a run on the row this one ends on is a step on, over
        // what's drawn between them; the start of one below takes in no more
        // than this end does, so the step goes on past its first character.
        if self.same_row((place.run, place.row), (next, 0)) {
            first
        } else {
            self.right(first)
        }
    }

    fn left(&self, place: Place) -> Place {
        let run = &self.runs[place.run];
        if let Some(offset) = run.previous(place.offset) {
            return Place {
                run: place.run,
                offset,
                row: run.row_of(offset),
            };
        }
        let Some(previous) = place.run.checked_sub(1) else {
            return place;
        };
        let last = self.runs[previous].last(previous);
        if self.same_row((place.run, place.row), (previous, last.row)) {
            last
        } else {
            self.left(last)
        }
    }

    /// The rows on screen, top to bottom, each the run rows it's made of.
    fn screen_rows(&self) -> Vec<Vec<(usize, usize)>> {
        let mut out: Vec<Vec<(usize, usize)>> = Vec::new();
        for (run, key_run) in self.runs.iter().enumerate() {
            for row in 0..key_run.rows.len() {
                match out.last_mut() {
                    Some(last) if self.same_row(last[0], (run, row)) => last.push((run, row)),
                    _ => out.push(vec![(run, row)]),
                }
            }
        }
        out
    }

    fn screen_row_of(rows: &[Vec<(usize, usize)>], place: Place) -> Option<usize> {
        rows.iter()
            .position(|row| row.contains(&(place.run, place.row)))
    }

    /// The place on screen row `row` nearest `goal` across.
    fn nearest(&self, row: &[(usize, usize)], goal: Pixels) -> Option<Place> {
        let mut best: Option<(Pixels, Place)> = None;
        for &(run, r) in row {
            let key_run = &self.runs[run];
            let span = key_run.rows[r];
            let mut offset = span.start;
            loop {
                let place = Place {
                    run,
                    offset,
                    row: r,
                };
                let distance = (key_run.x(place) - goal).abs();
                if best.is_none_or(|(d, _)| distance < d) {
                    best = Some((distance, place));
                }
                match key_run.next(offset).filter(|&next| next <= span.end) {
                    Some(next) => offset = next,
                    None => break,
                }
            }
        }
        best.map(|(_, place)| place)
    }

    /// `place` moved by `motion`, and where across Up and Down aim for after.
    fn moved(
        &self,
        place: Place,
        motion: SelectionMotion,
        goal: Option<Pixels>,
    ) -> (Place, Option<Pixels>) {
        match motion {
            SelectionMotion::Left => (self.left(place), None),
            SelectionMotion::Right => (self.right(place), None),
            SelectionMotion::Up | SelectionMotion::Down => {
                let goal = goal.unwrap_or_else(|| self.runs[place.run].x(place));
                let rows = self.screen_rows();
                let Some(at) = Self::screen_row_of(&rows, place) else {
                    return (place, Some(goal));
                };
                let to = if motion == SelectionMotion::Up {
                    at.checked_sub(1)
                } else {
                    Some(at + 1).filter(|&to| to < rows.len())
                };
                let moved = match to {
                    Some(to) => self.nearest(&rows[to], goal).unwrap_or(place),
                    // Past the first row, the start; past the last, the end.
                    None if motion == SelectionMotion::Up => self.runs[0].first(0),
                    None => {
                        let last = self.runs.len() - 1;
                        self.runs[last].last(last)
                    }
                };
                (moved, Some(goal))
            }
            SelectionMotion::LineStart | SelectionMotion::LineEnd => {
                let rows = self.screen_rows();
                let Some(at) = Self::screen_row_of(&rows, place) else {
                    return (place, None);
                };
                let row = &rows[at];
                let (run, r) = if motion == SelectionMotion::LineStart {
                    row[0]
                } else {
                    row[row.len() - 1]
                };
                let span = self.runs[run].rows[r];
                let offset = if motion == SelectionMotion::LineStart {
                    span.start
                } else {
                    span.end
                };
                (
                    Place {
                        run,
                        offset,
                        row: r,
                    },
                    None,
                )
            }
        }
    }

    /// How far the view `place` is in has to scroll down (or up, less than
    /// nothing) for its row to show, and where in the view to scroll it, or
    /// `None` when it shows. A participant out of view altogether measures
    /// against what shows of the others.
    fn scroll_for(&self, place: Place) -> Option<(Point<Pixels>, Pixels)> {
        const INSET: Pixels = px(1.);
        let run = &self.runs[place.run];
        let usable = |shown: &Bounds<Pixels>| {
            shown.size.width > INSET * 2. && shown.size.height > INSET * 2.
        };
        let own = self.participants[run.participant].shown;
        let shown = if usable(&own) {
            own
        } else {
            // The nearest that shows, before or after it.
            let order = run.participant;
            self.participants
                .iter()
                .enumerate()
                .filter(|(_, p)| usable(&p.shown))
                .min_by_key(|(i, _)| i.abs_diff(order))
                .map(|(_, p)| p.shown)?
        };
        let line_height = run.line_height();
        let top = run.rows[place.row].top;
        let bottom = top + line_height;
        let by = if top < shown.top() {
            top - shown.top()
        } else if bottom > shown.bottom() {
            bottom - shown.bottom()
        } else {
            return None;
        };
        let x = run
            .x(place)
            .clamp(shown.left() + INSET, shown.right() - INSET);
        let y = (top + line_height / 2.).clamp(shown.top() + INSET, shown.bottom() - INSET);
        Some((point(x, y), by))
    }

    /// A selection end at `place`, and the caret that keeps it.
    fn endpoint(
        &self,
        place: Place,
        goal: Option<Pixels>,
        cx: &App,
    ) -> Option<(SelectionEndpoint, KeyCaret)> {
        let run = &self.runs[place.run];
        let participant = &self.participants[run.participant];
        let content_point = run.point(place.offset)? - participant.origin;
        let content_key_resolver = participant.entity.read(cx).content_key_resolver.clone();
        let weak: WeakEntity<SelectableTextState> = participant.entity.downgrade();
        Some((
            SelectionEndpoint {
                participant: Some(weak),
                point: content_point,
                inside: true,
                inside_text: true,
                content_key: None,
                content_key_resolver: content_key_resolver
                    .map(|resolver| (resolver, content_point)),
            },
            KeyCaret {
                participant: participant.entity.entity_id(),
                point: content_point,
                run: run.index,
                offset: place.offset,
                row: place.row,
                goal,
            },
        ))
    }
}

impl WindowSelectionState {
    /// Moves the selection's cursor by `motion`, keeping its anchor. A word or
    /// line a double or triple click selected is the selection it moves.
    /// Returns whether there was a selection, or a place clicked, to move.
    pub(super) fn move_by_key(
        &mut self,
        motion: SelectionMotion,
        window: &Window,
        cx: &mut App,
    ) -> bool {
        if self.snapshot().is_none()
            && let Some(unit) = self.unit.clone()
        {
            self.anchor = Some(unit.start);
            self.cursor = Some(unit.end);
            self.did_hit_text = true;
        }
        if !self.did_hit_text || self.anchor.is_none() {
            return false;
        }
        let Some(cursor) = self.cursor.clone() else {
            return false;
        };
        let Some(participant) = cursor.entity_id() else {
            return false;
        };
        let document = KeyDocument::gather(self, cx);
        let kept = self
            .key_caret
            .filter(|caret| caret.participant == participant && caret.point == cursor.point);
        let place = kept.and_then(|caret| document.kept(&caret)).or_else(|| {
            let at = cursor.resolve(&self.participants.borrow())?;
            document.place_at(participant, at)
        });
        let Some(place) = place else {
            return false;
        };
        let goal = kept.and_then(|caret| caret.goal);
        let (place, goal) = document.moved(place, motion, goal);
        let Some((endpoint, caret)) = document.endpoint(place, goal, cx) else {
            return false;
        };
        self.cursor = Some(endpoint);
        self.key_caret = Some(caret);
        self.unit = None;
        self.is_selecting = false;
        self.pending_extension_anchor = None;
        self.publish_snapshots(cx);
        // A row past a scrolled view's edge: the view scrolls to show it, as
        // it does under a drag, by a wheel's worth of pixels at it.
        if let Some((position, by)) = document.scroll_for(place) {
            let window = window.window_handle();
            cx.defer(move |cx| {
                _ = window.update(cx, |_, window, cx| {
                    window.dispatch_event(
                        ScrollEvent {
                            position,
                            delta: ScrollDelta::Pixels(point(px(0.), -by)),
                            ..Default::default()
                        }
                        .to_platform_input(),
                        cx,
                    );
                });
            });
        }
        true
    }

    /// Selects all of `participant`'s text through the window selection, from
    /// the start of its first run to the end of its last, so that a key can move
    /// the end. Returns whether it has text.
    pub(super) fn select_whole(
        &mut self,
        participant: &Entity<SelectableTextState>,
        cx: &mut App,
    ) -> bool {
        let document = KeyDocument::gather(self, cx);
        let runs = document
            .runs_of(participant.entity_id())
            .collect::<Vec<_>>();
        let (Some(&first), Some(&last)) = (runs.first(), runs.last()) else {
            return false;
        };
        let start = document.runs[first].first(first);
        let end = document.runs[last].last(last);
        let (Some((anchor, _)), Some((cursor, caret))) = (
            document.endpoint(start, None, cx),
            document.endpoint(end, None, cx),
        ) else {
            return false;
        };
        self.table_cell_anchor.set(None);
        self.anchor = Some(anchor);
        self.cursor = Some(cursor);
        self.key_caret = Some(caret);
        self.unit = None;
        self.did_hit_text = true;
        self.is_selecting = false;
        self.pending_extension_anchor = None;
        self.publish_snapshots(cx);
        true
    }
}
