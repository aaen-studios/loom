//! How the chat's header fits its width (PLAN.md, "Design"): in a narrow window
//! its chips give up what they can, one step at a time in a fixed order, before anything
//! in it reaches the window's buttons. Once every step is taken, the head's name
//! shortens to a few letters, then the permission's words, then the rest of the name.

use crate::tokens::space;

/// What the header holds, as wide as each thing is in each of its forms. A thing that
/// isn't shown is 0 wide.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sizes {
    /// The room for what's in it: between its padding, after a Mac's traffic lights and
    /// before the window's buttons.
    pub room: f32,
    /// The agent's menu, or the group chat's head, whole.
    pub head: f32,
    /// How much of `head` its name's words are.
    pub name: f32,
    /// How much of `head` a group chat's members line is, with the gap before it.
    pub members: f32,
    pub pins: f32,
    /// The browser chip: with its word, and its globe alone.
    pub browser: (f32, f32),
    /// The brain chip: with its model and its effort, with its model alone, and its
    /// program's icon alone.
    pub brain: (f32, f32, f32),
    /// The context ring's button.
    pub context: f32,
    pub permission: f32,
    /// How much of `permission` its words are.
    pub permission_words: f32,
    /// How much the chevrons of the agent's menu and the permission chip take, with the
    /// gaps before them.
    pub chevrons: f32,
}

/// A step a narrowing header takes, in the order it takes them (`ORDER`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// The brain chip drops its effort: "Opus 5.5".
    Effort,
    /// The browser chip drops its word: its globe alone.
    BrowserWord,
    /// A group chat's head drops how many are in it.
    Members,
    /// The brain chip shows only its program's icon.
    BrainIcon,
    /// The head and the chips close up, `space::S` apart rather than `space::L`.
    Close,
    /// The agent's menu and the permission chip drop their chevrons.
    Chevrons,
}

pub const ORDER: [Step; 6] = [
    Step::Effort,
    Step::BrowserWord,
    Step::Members,
    Step::BrainIcon,
    Step::Close,
    Step::Chevrons,
];

/// What the brain chip shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Brain {
    /// Its icon, its model and its effort, and its chevron.
    Whole,
    /// Its icon, its model and its chevron.
    Model,
    /// Its icon.
    Icon,
}

/// How the header lays out in its room.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fit {
    pub brain: Brain,
    pub browser_word: bool,
    pub members: bool,
    /// How far apart the head and the chips are. The context ring sits half that
    /// closer to the chip before it.
    pub gap: f32,
    pub chevrons: bool,
    /// The most the head's name's words take, once they shorten.
    pub name: Option<f32>,
    /// The most the permission's words take, once even they shorten.
    pub permission: Option<f32>,
}

impl Fit {
    /// Everything as it is in a wide window.
    pub const WHOLE: Fit = Fit {
        brain: Brain::Whole,
        browser_word: true,
        members: true,
        gap: space::L,
        chevrons: true,
        name: None,
        permission: None,
    };

    fn take(&mut self, step: Step) {
        match step {
            Step::Effort => self.brain = Brain::Model,
            Step::BrowserWord => self.browser_word = false,
            Step::Members => self.members = false,
            Step::BrainIcon => self.brain = Brain::Icon,
            Step::Close => self.gap = space::S,
            Step::Chevrons => self.chevrons = false,
        }
    }

    /// How wide the header's things are, laid out this way.
    pub fn width(&self, s: &Sizes) -> f32 {
        let short = |words: f32, most: Option<f32>| most.map_or(0.0, |n| (words - n).max(0.0));
        let members = if self.members { 0.0 } else { s.members };
        let chevrons = if self.chevrons { 0.0 } else { s.chevrons };
        let chips = [
            s.pins,
            if self.browser_word {
                s.browser.0
            } else {
                s.browser.1
            },
            match self.brain {
                Brain::Whole => s.brain.0,
                Brain::Model => s.brain.1,
                Brain::Icon => s.brain.2,
            },
            s.context,
            s.permission - short(s.permission_words, self.permission),
        ];
        let shown = chips.iter().filter(|w| **w > 0.0).count();
        let pull = if s.context > 0.0 { self.gap / 2.0 } else { 0.0 };
        let head = s.head - short(s.name, self.name) - members;
        head + chips.iter().sum::<f32>() + self.gap * shown as f32 - pull - chevrons
    }

    /// Which of the header's controls lost words, laid out this way, and so show them
    /// whole under the pointer.
    pub fn cut(&self, s: &Sizes) -> Cut {
        let shorter = |words: f32, most: Option<f32>| most.is_some_and(|n| n < words);
        Cut {
            brain: s.brain.0 > 0.0 && self.brain != Brain::Whole,
            browser: s.browser.0 > s.browser.1 && !self.browser_word,
            head: shorter(s.name, self.name) || (s.members > 0.0 && !self.members),
            permission: shorter(s.permission_words, self.permission),
        }
    }
}

/// The header's controls that a narrow window took words from (`Fit::cut`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Cut {
    /// The brain chip, without its effort or as its icon alone.
    pub brain: bool,
    /// The browser chip, as its globe alone.
    pub browser: bool,
    /// The agent's menu or the group chat's head: its name shortened, or the group
    /// chat's members line gone.
    pub head: bool,
    /// The permission chip, its words shortened.
    pub permission: bool,
}

/// The least of its name a shortened head keeps while the permission's words can still
/// give: about three letters and the ellipsis.
pub const NAME_LEAST: f32 = 48.0;

/// How the header fits `s.room`: whole while it can, then each step of `ORDER` in turn
/// until it fits, then the name shortened down to `NAME_LEAST`, then the permission's
/// words, and only then the rest of the name.
pub fn fit(s: &Sizes) -> Fit {
    let mut f = Fit::WHOLE;
    if f.width(s) <= s.room {
        return f;
    }
    for step in ORDER {
        f.take(step);
        if f.width(s) <= s.room {
            return f;
        }
    }
    let over = f.width(s) - s.room;
    f.name = Some((s.name - over).max(s.name.min(NAME_LEAST)));
    let over = f.width(s) - s.room;
    if over > 0.0 && s.permission_words > 0.0 {
        f.permission = Some((s.permission_words - over).max(0.0));
    }
    let over = f.width(s) - s.room;
    if over > 0.0 {
        f.name = f.name.map(|n| (n - over).max(0.0));
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An agent's header, as wide as the app measures one: "Juniper", the brain chip
    /// as "Model fake-1 · Effort medium", the ring, and "Asks first".
    fn agent() -> Sizes {
        Sizes {
            room: 1000.0,
            head: 120.0,
            name: 50.0,
            members: 0.0,
            pins: 0.0,
            browser: (0.0, 0.0),
            brain: (190.0, 110.0, 28.0),
            context: 26.0,
            permission: 90.0,
            permission_words: 58.0,
            chevrons: 36.0,
        }
    }

    /// The width at each step of `ORDER`, from whole.
    fn widths(s: &Sizes) -> Vec<f32> {
        let mut f = Fit::WHOLE;
        let mut out = vec![f.width(s)];
        for step in ORDER {
            f.take(step);
            out.push(f.width(s));
        }
        out
    }

    #[test]
    fn a_wide_header_is_whole() {
        assert_eq!(fit(&agent()), Fit::WHOLE);
        // Exactly its width is room enough.
        let s = Sizes {
            room: Fit::WHOLE.width(&agent()),
            ..agent()
        };
        assert_eq!(fit(&s), Fit::WHOLE);
    }

    #[test]
    fn its_width_is_the_things_and_the_gaps_with_the_ring_pulled_in() {
        // The head, three chips, a gap before each, less half a gap for the ring.
        assert_eq!(
            Fit::WHOLE.width(&agent()),
            120.0 + 190.0 + 26.0 + 90.0 + 3.0 * 16.0 - 8.0
        );
        // With no ring, nothing's pulled in.
        let s = Sizes {
            context: 0.0,
            ..agent()
        };
        assert_eq!(Fit::WHOLE.width(&s), 120.0 + 190.0 + 90.0 + 2.0 * 16.0);
    }

    #[test]
    fn the_brain_chip_drops_its_effort_and_then_shows_only_its_icon() {
        let whole = Fit::WHOLE.width(&agent());
        let s = Sizes {
            room: whole - 1.0,
            ..agent()
        };
        let f = fit(&s);
        assert_eq!(f.brain, Brain::Model);
        assert_eq!((f.gap, f.chevrons, f.name), (space::L, true, None));
        // Too narrow for its model too: its icon alone, the chips still apart.
        let s = Sizes {
            room: whole - 80.0 - 1.0,
            ..agent()
        };
        let f = fit(&s);
        assert_eq!(f.brain, Brain::Icon);
        assert_eq!((f.gap, f.chevrons), (space::L, true));
        assert!(f.width(&s) <= s.room);
    }

    #[test]
    fn the_steps_come_in_their_order_and_none_makes_it_wider() {
        let mut s = Sizes {
            pins: 40.0,
            browser: (80.0, 30.0),
            ..agent()
        };
        s.head += 70.0;
        s.members = 70.0;
        let w = widths(&s);
        for pair in w.windows(2) {
            assert!(pair[1] < pair[0], "{w:?}");
        }
        // At each step's width, that step is the last one taken.
        let mut f = Fit::WHOLE;
        for (k, step) in ORDER.into_iter().enumerate() {
            f.take(step);
            let at = Sizes {
                room: w[k + 1],
                ..s
            };
            assert_eq!(fit(&at), f, "at {step:?}");
        }
    }

    #[test]
    fn a_step_with_nothing_to_give_is_passed_over() {
        // An agent's head has no members line, and this one no browser: the steps for
        // them change nothing, so the next that does is taken.
        let s = agent();
        let w = widths(&s);
        assert_eq!((w[1], w[1]), (w[2], w[3]));
        let at = Sizes {
            room: w[1] - 1.0,
            ..s
        };
        assert_eq!(fit(&at).brain, Brain::Icon);
    }

    /// An agent with a long name, "Research assistant".
    fn long_name() -> Sizes {
        Sizes {
            head: 70.0 + 100.0,
            name: 100.0,
            ..agent()
        }
    }

    #[test]
    fn once_every_step_is_taken_the_name_shortens() {
        let s = long_name();
        let least = *widths(&s).last().unwrap();
        let at = Sizes {
            room: least - 20.0,
            ..s
        };
        let f = fit(&at);
        assert_eq!((f.brain, f.gap, f.chevrons), (Brain::Icon, space::S, false));
        assert_eq!((f.name, f.permission), (Some(80.0), None));
        assert_eq!(f.width(&at), at.room);
    }

    #[test]
    fn a_short_name_then_the_permissions_words_then_the_rest_of_the_name() {
        let s = long_name();
        let least = *widths(&s).last().unwrap();
        // The name down to its least, 52 off, then 18 off the permission's words.
        let at = Sizes {
            room: least - 70.0,
            ..s
        };
        let f = fit(&at);
        assert_eq!((f.name, f.permission), (Some(NAME_LEAST), Some(40.0)));
        assert_eq!(f.width(&at), at.room);
        // The permission's 58 gone too, then 10 more off the name.
        let at = Sizes {
            room: least - 52.0 - 58.0 - 10.0,
            ..s
        };
        let f = fit(&at);
        assert_eq!((f.name, f.permission), (Some(NAME_LEAST - 10.0), Some(0.0)));
        assert_eq!(f.width(&at), at.room);
        // With nothing left to give, they're as short as they go.
        let at = Sizes { room: 0.0, ..s };
        assert_eq!((fit(&at).name, fit(&at).permission), (Some(0.0), Some(0.0)));
        // A name shorter than the least is kept whole while the permission's words give.
        let s = Sizes {
            head: 70.0 + 40.0,
            name: 40.0,
            ..agent()
        };
        let least = *widths(&s).last().unwrap();
        let f = fit(&Sizes {
            room: least - 20.0,
            ..s
        });
        assert_eq!((f.name, f.permission), (Some(40.0), Some(38.0)));
    }

    /// A group chat's head, "Shop" and "2 agents", with its pins and its ring.
    fn group() -> Sizes {
        Sizes {
            room: 1000.0,
            head: 160.0,
            name: 34.0,
            members: 70.0,
            pins: 40.0,
            browser: (0.0, 0.0),
            brain: (0.0, 0.0, 0.0),
            context: 26.0,
            permission: 0.0,
            permission_words: 0.0,
            chevrons: 0.0,
        }
    }

    #[test]
    fn a_group_chat_drops_its_members_line_after_the_browser() {
        let s = group();
        let whole = Fit::WHOLE.width(&s);
        assert_eq!(whole, 160.0 + 40.0 + 26.0 + 2.0 * 16.0 - 8.0);
        let f = fit(&Sizes {
            room: whole - 1.0,
            ..s
        });
        assert!(!f.members);
        assert_eq!((f.gap, f.name), (space::L, None));
        // Then it closes up, then the name shortens.
        let f = fit(&Sizes {
            room: whole - 70.0 - 1.0,
            ..s
        });
        assert_eq!((f.members, f.gap, f.name), (false, space::S, None));
        // At its least, 168, with nothing more to give: 18 off "Shop".
        let narrow = Sizes { room: 150.0, ..s };
        let f = fit(&narrow);
        assert_eq!((f.name, f.permission), (Some(16.0), None));
        assert_eq!(f.width(&narrow), 150.0);
    }

    /// At the window's least width (the rail, 56, and the least chat, 420), less the
    /// window's buttons (122) and the header's padding (32).
    const NARROWEST: f32 = 56.0 + 420.0 - 56.0 - 122.0 - 32.0;

    #[test]
    fn the_narrowest_window_keeps_an_agents_name_whole() {
        // "Juniper", the brain chip, the ring and "Asks first": the chevrons go, and
        // nothing more.
        let s = Sizes {
            room: NARROWEST,
            ..agent()
        };
        let f = fit(&s);
        assert_eq!((f.brain, f.gap, f.chevrons), (Brain::Icon, space::S, false));
        assert_eq!((f.name, f.permission), (None, None));
        assert!(f.width(&s) <= s.room);
    }

    /// An agent's header at the narrowest window with pins, the browser, "Asks when it's
    /// risky" and a long name.
    fn every_chip() -> Sizes {
        Sizes {
            room: NARROWEST,
            head: 70.0 + 125.0,
            name: 125.0,
            pins: 40.0,
            browser: (80.0, 30.0),
            permission: 147.0,
            permission_words: 115.0,
            ..agent()
        }
    }

    #[test]
    fn the_narrowest_window_fits_an_agents_header_with_every_chip() {
        // It fits by shortening the name to its least (77 off), the permission's words to
        // nothing (115), and 8 more off the name.
        let s = every_chip();
        let f = fit(&s);
        assert_eq!((f.brain, f.browser_word), (Brain::Icon, false));
        assert_eq!((f.name, f.permission), (Some(NAME_LEAST - 8.0), Some(0.0)));
        assert_eq!(f.width(&s), s.room);
    }

    #[test]
    fn a_control_that_lost_words_is_cut() {
        // Whole, nothing is.
        assert_eq!(Fit::WHOLE.cut(&agent()), Cut::default());
        let brain = Cut {
            brain: true,
            ..Cut::default()
        };
        // The brain chip without its effort, and as its icon alone.
        let s = Sizes {
            room: Fit::WHOLE.width(&agent()) - 1.0,
            ..agent()
        };
        assert_eq!(fit(&s).brain, Brain::Model);
        assert_eq!(fit(&s).cut(&s), brain);
        // At the narrowest window the chevrons are gone too, but no more words:
        // "Juniper" and "Asks first" are whole.
        let s = Sizes {
            room: NARROWEST,
            ..agent()
        };
        assert_eq!(fit(&s).cut(&s), brain);
        // With every chip, every control lost some.
        let s = every_chip();
        let all = Cut {
            brain: true,
            browser: true,
            head: true,
            permission: true,
        };
        assert_eq!(fit(&s).cut(&s), all);
        // A name shorter than the least is kept whole while the permission's words give.
        let s = Sizes {
            head: 70.0 + 40.0,
            name: 40.0,
            ..agent()
        };
        let s = Sizes {
            room: *widths(&s).last().unwrap() - 20.0,
            ..s
        };
        let f = fit(&s);
        assert_eq!(f.name, Some(40.0));
        assert_eq!(
            f.cut(&s),
            Cut {
                permission: true,
                ..brain
            }
        );
        // With no brain chip and no browser, their steps are passed over to close up, and
        // what isn't there isn't cut.
        let s = Sizes {
            brain: (0.0, 0.0, 0.0),
            ..agent()
        };
        let s = Sizes {
            room: Fit::WHOLE.width(&s) - 1.0,
            ..s
        };
        let f = fit(&s);
        assert_eq!(
            (f.brain, f.browser_word, f.gap),
            (Brain::Icon, false, space::S)
        );
        assert_eq!(f.cut(&s), Cut::default());
    }

    #[test]
    fn a_group_chats_head_is_cut_once_its_members_line_goes() {
        let s = group();
        assert_eq!(fit(&s).cut(&s), Cut::default());
        let s = Sizes {
            room: Fit::WHOLE.width(&group()) - 1.0,
            ..group()
        };
        let head = Cut {
            head: true,
            ..Cut::default()
        };
        assert_eq!(fit(&s).cut(&s), head);
        // And while its name shortens.
        let s = Sizes {
            room: 150.0,
            ..group()
        };
        assert_eq!(fit(&s).cut(&s), head);
    }
}
