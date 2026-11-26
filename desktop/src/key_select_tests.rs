// Selecting a message's words with the keyboard (LOO-372): a click in them puts the keys
// there, Ctrl+A (⌘A) selects the message, Shift with the arrows, Home and End move the
// selection's end, on into the next message, and Ctrl+C (⌘C) copies it as a drag's
// selection copies, emoji and code alike. gpui-base does it (`key_selection.rs`); these
// check it in messages drawn as the chat draws them.
use gpui::{
    canvas, div, point, px, AppContext as _, Bounds, Context, Entity, InteractiveElement as _,
    IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Render, ScrollHandle, StatefulInteractiveElement as _, Styled as _,
    TestAppContext, VisualTestContext, Window,
};
use gpui_kit::base::{cut_text_selection, TextSelection, TextSelectionLayer};
use gpui_kit::component::text::{TextView, TextViewState};
use std::cell::RefCell;
use std::rc::Rc;

struct Chat {
    messages: Vec<Entity<TextViewState>>,
    /// Where each message's words were drawn.
    drawn: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// The first message cut at this height, as Show more cuts one (`app::cut_long`).
    cut_first: Option<f32>,
    /// The chat scrolled in a view this tall.
    scroll: Option<(ScrollHandle, f32)>,
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut chat = div().w(px(420.0)).flex().flex_col().gap(px(12.0));
        for (i, message) in self.messages.iter().enumerate() {
            let drawn = self.drawn.clone();
            let words = TextView::new(message).selectable(true);
            let words = match self.cut_first.filter(|_| i == 0) {
                Some(height) => cut_text_selection(
                    div()
                        .flex()
                        .flex_col()
                        .max_h(px(height))
                        .overflow_hidden()
                        .child(div().flex_none().child(words)),
                )
                .into_any_element(),
                None => words.into_any_element(),
            };
            chat = chat.child(
                div().relative().child(words).child(
                    canvas(
                        move |b, _, _| {
                            let mut drawn = drawn.borrow_mut();
                            if drawn.len() <= i {
                                drawn.resize(i + 1, Bounds::default());
                            }
                            drawn[i] = b;
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                ),
            );
        }
        let root = div().child(TextSelectionLayer);
        match &self.scroll {
            Some((handle, height)) => root.child(
                div()
                    .id("chat")
                    .h(px(*height))
                    .overflow_y_scroll()
                    .track_scroll(handle)
                    .child(chat),
            ),
            None => root.child(chat),
        }
    }
}

type Drawn = Rc<RefCell<Vec<Bounds<Pixels>>>>;

fn open<'a>(messages: &[&str], cx: &'a mut TestAppContext) -> (Drawn, &'a mut VisualTestContext) {
    open_with(messages, None, None, cx)
}

fn open_with<'a>(
    messages: &[&str],
    cut_first: Option<f32>,
    scroll: Option<(ScrollHandle, f32)>,
    cx: &'a mut TestAppContext,
) -> (Drawn, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let drawn = Rc::new(RefCell::new(Vec::new()));
    let words: Vec<String> = messages.iter().map(|m| m.to_string()).collect();
    let (_, cx) = cx.add_window_view({
        let drawn = drawn.clone();
        move |_, cx| Chat {
            messages: words
                .iter()
                .map(|w| cx.new(|cx| TextViewState::markdown(w, cx)))
                .collect(),
            drawn,
            cut_first,
            scroll,
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (drawn, cx)
}

/// A click just inside the first character of message `i`.
fn click_start(drawn: &Rc<RefCell<Vec<Bounds<Pixels>>>>, i: usize, cx: &mut VisualTestContext) {
    let at = drawn.borrow()[i].origin + point(px(1.0), px(8.0));
    cx.simulate_event(MouseDownEvent {
        position: at,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position: at,
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 1,
    });
    cx.run_until_parked();
}

/// What's selected after `keys`, trimmed, as Ctrl+C copies it.
fn after(keys: &str, cx: &mut VisualTestContext) -> String {
    cx.simulate_keystrokes(keys);
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        TextSelection::selected_text(window, cx).trim().to_string()
    })
}

fn copied(cx: &mut VisualTestContext) -> Option<String> {
    cx.simulate_keystrokes("secondary-c");
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

#[gpui::test]
fn shift_and_the_arrows_select_from_a_click(cx: &mut TestAppContext) {
    let (drawn, cx) = open(&["alpha beta gamma"], cx);
    click_start(&drawn, 0, cx);
    assert_eq!(after("", cx), "");
    let five = "shift-right shift-right shift-right shift-right shift-right";
    assert_eq!(after(five, cx), "alpha");
    assert_eq!(after("shift-end", cx), "alpha beta gamma");
    assert_eq!(after("shift-left", cx), "alpha beta gamm");
    assert_eq!(after("shift-home", cx), "");
    assert_eq!(after("shift-end shift-left", cx), "alpha beta gamm");
    assert_eq!(copied(cx).as_deref(), Some("alpha beta gamm"));
}

#[gpui::test]
fn select_all_takes_the_message_and_shift_then_moves_its_end(cx: &mut TestAppContext) {
    let (drawn, cx) = open(&["alpha beta gamma", "another message"], cx);
    click_start(&drawn, 0, cx);
    assert_eq!(after("secondary-a", cx), "alpha beta gamma");
    assert_eq!(after("shift-left shift-left", cx), "alpha beta gam");
    assert_eq!(copied(cx).as_deref(), Some("alpha beta gam"));
}

#[gpui::test]
fn shift_down_goes_on_into_the_next_message(cx: &mut TestAppContext) {
    let (drawn, cx) = open(&["first message", "second message"], cx);
    click_start(&drawn, 0, cx);
    assert_eq!(after("shift-down", cx), "first message");
    // Two messages copy with a line between them, as a drag over both does.
    assert_eq!(after("shift-right", cx), "first message\n\ns");
    // Back up, to as far across as it had gone: past the "f" the click was in.
    assert_eq!(after("shift-up", cx), "f");
    let both = "first message\n\nsecond message";
    assert_eq!(after("shift-down shift-end", cx), both);
    assert_eq!(copied(cx).as_deref(), Some(both));
}

#[gpui::test]
fn copying_keeps_emoji_and_code_as_a_drag_does(cx: &mut TestAppContext) {
    let words = crate::emoji::pictured("Sad 😭 then `code 😭` done", 14.0);
    assert!(words.contains("<img"), "{words}");
    let (drawn, cx) = open(&[&words], cx);
    click_start(&drawn, 0, cx);
    assert_eq!(after("shift-end", cx), "Sad 😭 then code 😭 done");
    assert_eq!(copied(cx).as_deref(), Some("Sad 😭 then code 😭 done"));
}

#[gpui::test]
fn shift_and_an_arrow_with_nothing_clicked_leave_the_keys_alone(cx: &mut TestAppContext) {
    let (_, cx) = open(&["alpha beta"], cx);
    assert_eq!(after("shift-right shift-end", cx), "");
}

// What Show more hides is no part of a selection (LOO-412): it lies over the message
// under it, and a drag or a key into that message took it in.
#[gpui::test]
fn a_drag_into_the_message_under_a_cut_one_leaves_out_what_the_cut_hides(cx: &mut TestAppContext) {
    let (drawn, cx) = open_with(&["one\n\ntwo\n\nthree", "under it"], Some(30.0), None, cx);
    let start = drawn.borrow()[0].origin + point(px(1.0), px(8.0));
    let end = drawn.borrow()[1].origin + point(px(400.0), px(8.0));
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
    assert_eq!(after("", cx), "one\n\nunder it");
    assert_eq!(copied(cx).as_deref(), Some("one\n\nunder it"));
    // Back with the keys, and on again: past "one", the message under it.
    assert_eq!(after("shift-up shift-end", cx), "one");
    assert_eq!(after("shift-right", cx), "one\n\nu");
}

// Past the chat's edge, the chat scrolls to the selection's end, as under a drag
// (LOO-411).
#[gpui::test]
fn shift_down_past_the_chats_edge_scrolls_it(cx: &mut TestAppContext) {
    let scroll = ScrollHandle::new();
    let messages = ["one", "two", "three", "four", "five"];
    let (drawn, cx) = open_with(&messages, None, Some((scroll.clone(), 60.0)), cx);
    click_start(&drawn, 0, cx);
    for _ in 0..4 {
        cx.simulate_keystrokes("shift-down");
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(
        after("shift-end", cx),
        "one\n\ntwo\n\nthree\n\nfour\n\nfive"
    );
    assert!(scroll.offset().y < px(-60.0), "{:?}", scroll.offset());
    // The end's row shows.
    let five = drawn.borrow()[4];
    assert!(five.bottom() <= px(61.0), "{five:?}");
}
