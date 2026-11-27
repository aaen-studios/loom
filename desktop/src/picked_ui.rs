//! Words selected in a message and the bar over them, with Copy, Quote and Ask about this
//! (LOO-200): which message the selection is in, and where the bar goes (LOO-447).

use crate::gpui::{point, px, Bounds, Pixels, Point, Size};

/// How far the bar stands from the selection, and from the chat's edges.
const GAP: f32 = 8.0;
const MARGIN: f32 = 8.0;

/// The words selected in one of the chat's texts.
pub(super) struct Piece {
    /// The message it's a piece of, or `None` for a text that's no message's words.
    pub owner: Option<String>,
    pub words: String,
    /// Where the selection was painted in it, in the window.
    pub painted: Option<Bounds<Pixels>>,
}

/// The message all of a selection is in, from the texts it took words of: one message's
/// pieces and nothing else's. With it, whether those pieces are all of `all`, the words
/// the window's selection copies, and the box around where the selection was painted in
/// them. A selection that runs past the message, into another or into words that are no
/// message's, has none.
pub(super) fn owner_of(
    pieces: &[Piece],
    all: &str,
) -> Option<(String, bool, Option<Bounds<Pixels>>)> {
    let mut owner: Option<&str> = None;
    for p in pieces.iter().filter(|p| !p.words.trim().is_empty()) {
        let of = p.owner.as_deref()?;
        if owner.is_some_and(|o| o != of) {
            return None;
        }
        owner = Some(of);
    }
    let owner = owner?;
    // The window joins its pieces with line breaks, each piece its own paragraphs.
    let bare = |s: &str| s.split_whitespace().collect::<String>();
    let mine: String = pieces.iter().map(|p| bare(&p.words)).collect();
    let whole = !mine.is_empty() && mine == bare(all);
    let painted = pieces
        .iter()
        .filter_map(|p| p.painted)
        .reduce(|a, b| a.union(&b));
    Some((owner.to_string(), whole, painted))
}

/// Where the bar `bar` big goes for a selection painted in `sel`, in the chat's list
/// `column`: centred over the selection, above its top line, or below its bottom line
/// when there's no room above, so it never covers the words it's for, and always inside
/// the column. Only a selection taller than the column leaves no room either side: then
/// it's at the column's top.
pub(super) fn bar_at(
    sel: Bounds<Pixels>,
    bar: Size<Pixels>,
    column: Bounds<Pixels>,
) -> Point<Pixels> {
    let (gap, margin) = (px(GAP), px(MARGIN));
    let left = column.left() + margin;
    let right = (column.right() - margin - bar.width).max(left);
    let x = (sel.center().x - bar.width / 2.0).clamp(left, right);
    let top = column.top() + margin;
    let bottom = (column.bottom() - margin - bar.height).max(top);
    let above = sel.top() - gap - bar.height;
    let below = sel.bottom() + gap;
    let y = if above >= top {
        above
    } else if below <= bottom {
        below
    } else {
        top
    };
    point(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpui::size;

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn piece(owner: Option<&str>, words: &str, painted: Option<Bounds<Pixels>>) -> Piece {
        Piece {
            owner: owner.map(str::to_string),
            words: words.to_string(),
            painted,
        }
    }

    const BAR: Size<Pixels> = Size {
        width: px(240.0),
        height: px(32.0),
    };

    fn column() -> Bounds<Pixels> {
        b(260.0, 48.0, 844.0, 620.0)
    }

    #[test]
    fn the_bar_sits_above_the_selection_centred_on_it() {
        let sel = b(364.0, 404.0, 582.0, 85.0);
        let at = bar_at(sel, BAR, column());
        assert_eq!(at, point(px(364.0 + 291.0 - 120.0), px(404.0 - 8.0 - 32.0)));
        // Clear of the selection's top line.
        assert!(at.y + BAR.height < sel.top());
    }

    #[test]
    fn with_no_room_above_it_goes_below_the_bottom_line() {
        let sel = b(364.0, 60.0, 300.0, 46.0);
        let at = bar_at(sel, BAR, column());
        assert_eq!(at.y, px(60.0 + 46.0 + 8.0));
        assert!(at.y > sel.bottom());
    }

    #[test]
    fn it_stays_inside_the_column() {
        // A selection at the column's right edge: the bar is pulled in from it.
        let sel = b(1040.0, 400.0, 50.0, 23.0);
        let at = bar_at(sel, BAR, column());
        assert_eq!(at.x, px(260.0 + 844.0 - 8.0 - 240.0));
        // And at its left edge.
        let sel = b(262.0, 400.0, 20.0, 23.0);
        assert_eq!(bar_at(sel, BAR, column()).x, px(268.0));
        // A column narrower than the bar: from its left edge.
        let narrow = b(260.0, 48.0, 200.0, 620.0);
        assert_eq!(bar_at(sel, BAR, narrow).x, px(268.0));
    }

    #[test]
    fn a_selection_taller_than_the_column_has_the_bar_at_its_top() {
        let sel = b(364.0, 20.0, 582.0, 700.0);
        assert_eq!(bar_at(sel, BAR, column()).y, px(56.0));
    }

    #[test]
    fn a_selection_in_one_message_is_that_messages() {
        let pieces = [
            piece(
                Some("m1"),
                "Here is where things stand this week.\nUsage limits: ",
                Some(b(344.0, 370.0, 300.0, 23.0)),
            ),
            piece(
                Some("m1"),
                "LOO-206 is nearly done.\n",
                Some(b(364.0, 404.0, 582.0, 46.0)),
            ),
        ];
        let all = "Here is where things stand this week.\nUsage limits: LOO-206 is nearly done.";
        let (owner, whole, painted) = owner_of(&pieces, all).unwrap();
        assert_eq!(owner, "m1");
        assert!(whole);
        assert_eq!(painted, Some(b(344.0, 370.0, 602.0, 80.0)));
    }

    #[test]
    fn a_selection_past_its_message_is_no_ones() {
        let into_next = [
            piece(Some("m1"), "the end of one", None),
            piece(Some("m2"), "the start of the next", None),
        ];
        assert!(owner_of(&into_next, "the end of one\n\nthe start of the next").is_none());
        let into_other_words = [
            piece(Some("m1"), "words", None),
            piece(None, "Planner", None),
        ];
        assert!(owner_of(&into_other_words, "words Planner").is_none());
        assert!(owner_of(&[], "").is_none());
    }

    #[test]
    fn words_the_window_has_beyond_the_messages_pieces_arent_whole() {
        let pieces = [piece(Some("m1"), "words", None)];
        let (_, whole, _) = owner_of(&pieces, "words and a label").unwrap();
        assert!(!whole);
    }
}

// A message's selection as gpui-base paints it (LOO-447): one even band over its rows,
// across paragraphs, list items, bold words and a pill, the gaps between them filled,
// and the box around it what the bar is placed by.
#[cfg(test)]
mod drawn_tests {
    use gpui::{
        canvas, div, point, px, App, AppContext as _, Bounds, Context, Entity, IntoElement,
        Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
        Pixels, Render, Styled as _, TestAppContext, Window,
    };
    use gpui_kit::base::{TextSelection, TextSelectionLayer};
    use gpui_kit::component::text::{
        markdown_ast, InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode,
        MarkdownParseContext, MarkdownPlugin, TextView, TextViewState,
    };
    use std::{cell::Cell, rc::Rc};

    /// A link to `chip:ID` drawn as a pill taller than the words beside it, with a
    /// background of its own, as a card's is.
    struct Chip;

    impl MarkdownPlugin for Chip {
        fn name(&self) -> &str {
            "test-chip"
        }

        fn parse(
            &self,
            node: &markdown_ast::Node,
            cx: &MarkdownParseContext<'_>,
        ) -> Option<MarkdownNode> {
            let markdown_ast::Node::Link(link) = node else {
                return None;
            };
            let id = link.url.strip_prefix("chip:")?.to_string();
            Some(
                MarkdownNode::new("test-chip", id.clone())
                    .text(id)
                    .markdown(cx.node_source(node).unwrap_or_default()),
            )
        }

        fn render_inline(
            &self,
            _: &MarkdownNode,
            _: &InlineRenderContext,
            _: &mut Window,
            _: &mut App,
        ) -> Option<InlineElement> {
            let pill = div().w(px(80.0)).h(px(26.0)).bg(gpui::white());
            Some(InlineElement::new(pill).with_baseline(px(18.0)))
        }
    }

    struct Message {
        words: Entity<TextViewState>,
        drawn: Rc<Cell<Bounds<Pixels>>>,
    }

    impl Render for Message {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.drawn.clone();
            let words = TextView::new(&self.words)
                .selectable(true)
                .markdown_extensions(MarkdownExtensions::default().plugin(Chip));
            div().child(TextSelectionLayer).child(
                div().w(px(420.0)).relative().child(words).child(
                    canvas(move |b, _, _| drawn.set(b), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                ),
            )
        }
    }

    const WORDS: &str = "Where things stand.\n\n- **Usage:** [LOO-206](chip:LOO-206) is nearly done.\n- **Group chats:** in the sidebar now.\n\nThe rest of the week.";

    #[gpui::test]
    fn a_drag_over_list_items_bold_words_and_a_pill_paints_one_band(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let drawn = Rc::new(Cell::new(Bounds::default()));
        let (view, cx) = cx.add_window_view({
            let drawn = drawn.clone();
            move |_, cx| Message {
                words: cx.new(|cx| TextViewState::markdown(WORDS, cx)),
                drawn,
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let at = drawn.get();
        let (start, end) = (
            at.origin + point(px(1.0), px(8.0)),
            point(at.right() - px(1.0), at.bottom() - px(8.0)),
        );
        cx.simulate_event(MouseDownEvent {
            position: start,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.simulate_event(MouseUpEvent {
            position: end,
            modifiers: Modifiers::default(),
            button: MouseButton::Left,
            click_count: 1,
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let words = cx.update(TextSelection::selected_text);
        assert!(words.contains("LOO-206"), "{words}");
        let (boxes, around) = cx.update(|_, cx| {
            let s = view.read(cx).words.read(cx);
            (s.painted_selection_boxes(), s.painted_selection())
        });
        // The box around it is the words', from the first row's top to the last's bottom.
        let around = around.expect("the selection was painted");
        assert!(
            (around.top() - at.top()).abs() < px(1.0),
            "{around:?} in {at:?}"
        );
        assert!(
            (around.bottom() - at.bottom()).abs() < px(1.0),
            "{around:?} in {at:?}"
        );
        // From top to bottom, no line, list item or paragraph leaves a gap in it.
        let mut spans: Vec<(f32, f32)> = boxes
            .iter()
            .map(|b| (b.top().as_f32(), b.bottom().as_f32()))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut reach = spans[0].1;
        for (top, bottom) in &spans[1..] {
            assert!(
                *top <= reach + 0.01,
                "a gap from {reach} to {top}: {boxes:?}"
            );
            reach = reach.max(*bottom);
        }
        // The pill's row is one band: the words beside it fill its whole height.
        let pill_row = boxes
            .iter()
            .find(|b| b.size.height >= px(26.0))
            .expect("the pill's row");
        for b in boxes.iter().filter(|b| {
            b.top() < pill_row.bottom() && b.bottom() > pill_row.top() && b.size.height > px(4.0)
        }) {
            assert_eq!(
                (b.top(), b.bottom()),
                (pill_row.top(), pill_row.bottom()),
                "{boxes:?}"
            );
        }
    }
}
