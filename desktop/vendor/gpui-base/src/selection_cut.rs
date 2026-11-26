//! loom: a clip that cuts text off on purpose, as a long message's Show more
//! does. Text it hides is no part of a selection: a drag or a key leaves it
//! out, and so does copying. It lies over whatever is drawn there, and a
//! selection end over that would otherwise take it in. Text a scrolled view
//! hides is still part of it, as before: that's only out of view.

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, Window,
};

use crate::GlobalState;

/// `element`, whose bounds cut the text in it off on purpose: what of the text
/// lies outside them is no part of a selection. Put it around the element that
/// clips, with the same bounds.
pub fn cut_text_selection(element: impl IntoElement) -> impl IntoElement {
    SelectionCut(element.into_any_element())
}

struct SelectionCut(AnyElement);

impl IntoElement for SelectionCut {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectionCut {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.0.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        GlobalState::global(cx).push_text_cut(bounds);
        self.0.prepaint(window, cx);
        GlobalState::global(cx).pop_text_cut();
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        GlobalState::global(cx).push_text_cut(bounds);
        self.0.paint(window, cx);
        GlobalState::global(cx).pop_text_cut();
    }
}

/// Whether a character laid out at `at`, `width` wide on a row `height` tall,
/// shows inside `cut`: whether its middle does.
pub(crate) fn shows(
    cut: Option<Bounds<Pixels>>,
    at: Point<Pixels>,
    width: Pixels,
    height: Pixels,
) -> bool {
    cut.is_none_or(|cut| {
        let middle = Point::new(at.x + width / 2., at.y + height / 2.);
        middle.x >= cut.left()
            && middle.x < cut.right()
            && middle.y >= cut.top()
            && middle.y < cut.bottom()
    })
}
