use std::rc::Rc;

use gpui::{AnyElement, IntoElement, Pixels, TextStyle};

/// A GPUI element participating as one atomic object in a text line.
///
/// Use native GPUI styles, events and components on the element. TextView
/// measures its intrinsic size and wraps around the whole object. Fixed-width
/// elements keep their actual size; constrain them with GPUI styles as needed.
pub struct InlineElement {
    pub(crate) element: AnyElement,
    pub(crate) baseline: Option<Pixels>,
    /// loom: how the line cuts it when it's wider than the line (`cut_to_fit`).
    pub(crate) fit: Option<Rc<InlineFit>>,
}

/// loom: how an inline element wider than its line is cut (`InlineElement::cut_to_fit`).
pub(crate) struct InlineFit {
    /// Given its own width, the room left on the line where it would start and the
    /// line's width: the width it takes there, or None to start the next line.
    pub width: Box<dyn Fn(Pixels, Pixels, Pixels) -> Option<Pixels>>,
    /// The element again, given its own width, at the width it takes.
    pub build: Box<dyn Fn(Pixels, Pixels) -> AnyElement>,
}

impl InlineElement {
    pub fn new(element: impl IntoElement) -> Self {
        Self {
            element: element.into_any_element(),
            baseline: None,
            fit: None,
        }
    }

    /// Distance from the top edge to the alphabetic baseline, in logical pixels.
    /// By default the box aligns its bottom with the surrounding text's descent.
    pub fn with_baseline(mut self, baseline: Pixels) -> Self {
        self.baseline = Some(baseline);
        self
    }

    /// loom: lets the line cut the element when it's wider than a whole line, which
    /// would otherwise overflow it. `width` is asked, with the element's own width, the
    /// room left on the line where it would start and the line's width, what width it
    /// takes there, or None to start the next line, where it's asked again with the
    /// whole line's room; `build` makes the element again, given its own width, at the
    /// width it takes. One that fits a line keeps its size, and wraps whole.
    pub fn cut_to_fit(
        mut self,
        width: impl Fn(Pixels, Pixels, Pixels) -> Option<Pixels> + 'static,
        build: impl Fn(Pixels, Pixels) -> AnyElement + 'static,
    ) -> Self {
        self.fit = Some(Rc::new(InlineFit {
            width: Box::new(width),
            build: Box::new(build),
        }));
        self
    }
}

/// Inherited typography for a format-independent inline renderer.
///
/// TextView builds this while laying a line out; renderers only read it.
#[derive(Clone)]
pub struct InlineRenderContext {
    text_style: TextStyle,
    font_size: Pixels,
    line_height: Pixels,
    rem_size: Pixels,
}

impl InlineRenderContext {
    pub(crate) fn new(
        text_style: TextStyle,
        font_size: Pixels,
        line_height: Pixels,
        rem_size: Pixels,
    ) -> Self {
        Self {
            text_style,
            font_size,
            line_height,
            rem_size,
        }
    }

    /// Effective text style at the object's position, marks already applied.
    pub fn text_style(&self) -> &TextStyle {
        &self.text_style
    }

    /// Font size of the surrounding text, in logical pixels.
    pub fn font_size(&self) -> Pixels {
        self.font_size
    }

    /// Line height of the surrounding text, in logical pixels.
    pub fn line_height(&self) -> Pixels {
        self.line_height
    }

    /// Root font size, for resolving rem-relative lengths.
    pub fn rem_size(&self) -> Pixels {
        self.rem_size
    }
}
