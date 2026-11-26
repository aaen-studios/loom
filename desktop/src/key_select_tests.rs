// Selecting a message's words with the keyboard (LOO-372): a click in them puts the keys
// there, Ctrl+A (⌘A) selects the message, Shift with the arrows, Home and End move the
// selection's end, on into the next message, and Ctrl+C (⌘C) copies it as a drag's
// selection copies, emoji and code alike. gpui-base does it (`key_selection.rs`); these
// check it in messages drawn as the chat draws them.
use gpui::{
    canvas, div, point, px, AppContext as _, Bounds, Context, Entity, IntoElement, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Render, Styled as _,
    TestAppContext, VisualTestContext, Window,
};
use gpui_kit::base::{TextSelection, TextSelectionLayer};
use gpui_kit::component::text::{TextView, TextViewState};
use std::cell::RefCell;
use std::rc::Rc;

struct Chat {
    messages: Vec<Entity<TextViewState>>,
    /// Where each message's words were drawn.
    drawn: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut chat = div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(TextSelectionLayer);
        for (i, message) in self.messages.iter().enumerate() {
            let drawn = self.drawn.clone();
            chat = chat.child(
                div()
                    .relative()
                    .child(TextView::new(message).selectable(true))
                    .child(
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
        chat
    }
}

fn open<'a>(
    messages: &[&str],
    cx: &'a mut TestAppContext,
) -> (Rc<RefCell<Vec<Bounds<Pixels>>>>, &'a mut VisualTestContext) {
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
