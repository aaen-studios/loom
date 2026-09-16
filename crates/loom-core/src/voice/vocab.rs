//! Kokoro's phoneme vocabulary.
//!
//! Kokoro's ONNX graph consumes *phoneme token ids*, not text. This is the exact
//! vocabulary the shipped graph was exported with.
//!
//! The ids are **sparse**: 114 symbols occupying 178 slots (`n_token` in the
//! export config). The gaps are phonemes that appear in the training data but
//! were dropped from the shipped graph, so they are not "missing" and must not
//! be compacted.
//!
//! Verified symbol-for-symbol and id-for-id against `src/kokoro_onnx/config.json`
//! in the `kokoro-onnx` reference implementation, which is the authoritative
//! record of the export.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Slots the graph reserves for tokens, including the unused ones.
pub const N_TOKEN: usize = 178;

/// The id the graph uses for padding, on both sides of a token window.
pub const PAD: i64 = 0;

/// Space, which Kokoro treats as a phoneme in its own right rather than as
/// inter-token whitespace. Its presence in the vocabulary is why the phoneme
/// chunker can split on whitespace and still be splitting on tokens.
pub const SPACE: i64 = 16;

/// Every symbol the graph knows, as `(symbol, id)`, in the order the export
/// declares them. The single source of truth for this module.
pub const ALL: &[(char, i64)] = &[
    (';', 1),
    (':', 2),
    (',', 3),
    ('.', 4),
    ('!', 5),
    ('?', 6),
    ('—', 9),
    ('…', 10),
    ('"', 11),
    ('(', 12),
    (')', 13),
    ('“', 14),
    ('”', 15),
    (' ', 16),
    ('\u{0303}', 17), // combining tilde
    ('ʣ', 18),
    ('ʥ', 19),
    ('ʦ', 20),
    ('ʨ', 21),
    ('ᵝ', 22),
    ('\u{AB67}', 23),
    ('A', 24),
    ('I', 25),
    ('O', 31),
    ('Q', 33),
    ('S', 35),
    ('T', 36),
    ('W', 39),
    ('Y', 41),
    ('ᵊ', 42),
    ('a', 43),
    ('b', 44),
    ('c', 45),
    ('d', 46),
    ('e', 47),
    ('f', 48),
    // No ASCII 'g': the export uses 'ɡ' (U+0261, script g) at id 92 instead.
    ('h', 50),
    ('i', 51),
    ('j', 52),
    ('k', 53),
    ('l', 54),
    ('m', 55),
    ('n', 56),
    ('o', 57),
    ('p', 58),
    ('q', 59),
    ('r', 60),
    ('s', 61),
    ('t', 62),
    ('u', 63),
    ('v', 64),
    ('w', 65),
    ('x', 66),
    ('y', 67),
    ('z', 68),
    ('ɑ', 69),
    ('ɐ', 70),
    ('ɒ', 71),
    ('æ', 72),
    ('β', 75),
    ('ɔ', 76),
    ('ɕ', 77),
    ('ç', 78),
    ('ɖ', 80),
    ('ð', 81),
    ('ʤ', 82),
    ('ə', 83),
    ('ɚ', 85),
    ('ɛ', 86),
    ('ɜ', 87),
    ('ɟ', 90),
    ('ɡ', 92),
    ('ɥ', 99),
    ('ɨ', 101),
    ('ɪ', 102),
    ('ʝ', 103),
    ('ɯ', 110),
    ('ɰ', 111),
    ('ŋ', 112),
    ('ɳ', 113),
    ('ɲ', 114),
    ('ɴ', 115),
    ('ø', 116),
    ('ɸ', 118),
    ('θ', 119),
    ('œ', 120),
    ('ɹ', 123),
    ('ɾ', 125),
    ('ɻ', 126),
    ('ʁ', 128),
    ('ɽ', 129),
    ('ʂ', 130),
    ('ʃ', 131),
    ('ʈ', 132),
    ('ʧ', 133),
    ('ʊ', 135),
    ('ʋ', 136),
    ('ʌ', 138),
    ('ɣ', 139),
    ('ɤ', 140),
    ('χ', 142),
    ('ʎ', 143),
    ('ʒ', 147),
    ('ʔ', 148),
    ('ˈ', 156),
    ('ˌ', 157),
    ('ː', 158),
    ('ʰ', 162),
    ('ʲ', 164),
    ('↓', 169),
    ('→', 171),
    ('↗', 172),
    ('↘', 173),
    ('ᵻ', 177),
];

static LOOKUP: OnceLock<HashMap<char, i64>> = OnceLock::new();

fn lookup() -> &'static HashMap<char, i64> {
    LOOKUP.get_or_init(|| ALL.iter().copied().collect())
}

/// The token id for a phoneme, or `None` when the graph does not know it.
///
/// Unknown symbols are dropped rather than substituted: the reference
/// implementation filters them out, and inventing an id would feed the graph
/// a phoneme it was never trained on.
pub fn token_id(symbol: char) -> Option<i64> {
    lookup().get(&symbol).copied()
}

/// Whether the graph can pronounce this symbol.
pub fn contains(symbol: char) -> bool {
    lookup().contains_key(&symbol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_declares_one_hundred_and_fourteen_symbols() {
        // 114, not 115. The export's `n_token` of 178 is the slot count, not
        // the symbol count: the two differ because of the gaps.
        assert_eq!(ALL.len(), 114);
    }

    #[test]
    fn every_id_is_in_range_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for &(symbol, id) in ALL {
            assert!(id > 0, "{symbol:?} uses the padding id");
            assert!(
                (id as usize) < N_TOKEN,
                "{symbol:?} has id {id}, past the {N_TOKEN}-slot graph"
            );
            assert!(seen.insert(id), "id {id} is used twice");
        }
    }

    #[test]
    fn symbols_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for &(symbol, _) in ALL {
            assert!(seen.insert(symbol), "{symbol:?} appears twice");
        }
    }

    #[test]
    fn lookup_agrees_with_the_table() {
        for &(symbol, id) in ALL {
            assert_eq!(token_id(symbol), Some(id), "lookup disagrees for {symbol:?}");
        }
    }

    #[test]
    fn the_ids_the_graph_depends_on_are_where_we_think_they_are() {
        // Padding and space are load-bearing: the first pads every window, and
        // the second is what the phoneme chunker splits on.
        assert_eq!(token_id(' '), Some(SPACE));
        assert_eq!(SPACE, 16);
        assert_eq!(token_id('.'), Some(4));
        assert_eq!(token_id('ˈ'), Some(156));
        // The script g, not ASCII g — this is the kind of near-miss that would
        // silently drop every hard g from every utterance.
        assert_eq!(token_id('ɡ'), Some(92));
        assert_eq!(token_id('g'), None);
    }

    #[test]
    fn ascii_letters_outside_the_vocab_are_unknown() {
        for symbol in ['g', 'B', 'C', 'D', 'E', 'F', 'G', 'H', '*', '#', '[', ']'] {
            assert_eq!(token_id(symbol), None, "{symbol:?} should not be in the vocab");
        }
    }
}
