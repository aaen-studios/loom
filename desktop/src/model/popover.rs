//! Where a popover that opens from a button in the header goes: centred under the button,
//! as close as the window lets it, with its nub on the button (PLAN.md, "Pinned messages").

/// A popover's place across the window, in logical pixels from the window's left: its left
/// edge and width, and its nub's middle, from the popover's own left edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopUnder {
    pub left: f32,
    pub width: f32,
    pub nub: f32,
}

/// Where a popover `width` wide goes under a button from `button.0` to `button.1` in a
/// window `window` wide: centred under it, at least `margin` inside both of the window's
/// edges, and no wider than the window less a margin each side, so near an edge it slides
/// in and stays as centred as it can. Its nub points at the button's middle, wherever the
/// popover went, but stays `inset` from its ends, clear of its rounded corners.
pub fn pop_under(button: (f32, f32), window: f32, width: f32, margin: f32, inset: f32) -> PopUnder {
    let middle = (button.0 + button.1) / 2.0;
    let width = width.min(window - 2.0 * margin).max(0.0);
    let most = (window - margin - width).max(margin);
    let left = (middle - width / 2.0).clamp(margin, most);
    let nub = if width > 2.0 * inset {
        (middle - left).clamp(inset, width - inset)
    } else {
        width / 2.0
    };
    PopUnder { left, width, nub }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 460.0;
    const MARGIN: f32 = 12.0;
    const INSET: f32 = 20.0;

    #[test]
    fn in_a_wide_window_it_is_centred_under_its_button() {
        // The pin from 780 to 820 in a window 1400 wide: its middle is 800.
        let at = pop_under((780.0, 820.0), 1400.0, W, MARGIN, INSET);
        assert_eq!(
            at,
            PopUnder {
                left: 570.0,
                width: W,
                nub: 230.0,
            }
        );
        assert_eq!(at.left + at.nub, 800.0);
    }

    #[test]
    fn by_the_right_edge_it_slides_left_and_its_nub_stays_on_the_button() {
        // The pin's middle at 960 in a window 1000 wide: centred, it would end at 1190.
        let at = pop_under((940.0, 980.0), 1000.0, W, MARGIN, INSET);
        assert_eq!(at.left, 1000.0 - MARGIN - W);
        assert_eq!(at.width, W);
        assert_eq!(at.left + at.nub, 960.0);
        // Past where its corner rounds, the nub stops short of the end.
        let at = pop_under((975.0, 999.0), 1000.0, W, MARGIN, INSET);
        assert_eq!(at.left, 1000.0 - MARGIN - W);
        assert_eq!(at.nub, W - INSET);
    }

    #[test]
    fn by_the_left_edge_it_slides_right() {
        let at = pop_under((20.0, 60.0), 1000.0, W, MARGIN, INSET);
        assert_eq!(at.left, MARGIN);
        assert_eq!(at.left + at.nub, 40.0);
    }

    #[test]
    fn in_a_window_narrower_than_it_it_takes_the_window_less_its_margins() {
        let at = pop_under((300.0, 340.0), 400.0, W, MARGIN, INSET);
        assert_eq!(
            at,
            PopUnder {
                left: MARGIN,
                width: 400.0 - 2.0 * MARGIN,
                nub: 320.0 - MARGIN,
            }
        );
    }

    #[test]
    fn a_window_too_small_for_it_never_breaks_it() {
        let at = pop_under((10.0, 20.0), 30.0, W, MARGIN, INSET);
        assert_eq!(at.left, MARGIN);
        assert_eq!(at.width, 6.0);
        assert_eq!(at.nub, 3.0);
        let at = pop_under((0.0, 10.0), 0.0, W, MARGIN, INSET);
        assert_eq!(at.width, 0.0);
        assert!(at.left.is_finite() && at.nub.is_finite());
    }
}
