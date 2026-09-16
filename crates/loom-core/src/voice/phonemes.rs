//! Phoneme handling: the layer between text and the ONNX graph.
//!
//! Kokoro's graph takes phoneme **token ids**, so text must be converted to IPA
//! before inference. Two invariants from the reference implementation matter
//! here:
//!
//! 1. A phoneme string contains only symbols the graph knows. Anything else is
//!    filtered out before tokenization, not mapped to a fallback. Feeding an
//!    unknown symbol through would be a silent quality loss.
//! 2. Every window is padded with id 0 on **both** sides: `[0, ..tokens.., 0]`.
//!    The graph expects those boundary tokens; omitting them shifts the whole
//!    utterance against the style vector.
//!
//! The text-to-IPA step itself (espeak-ng) lives in `g2p`, because it needs a
//! process-global lock and therefore should not be called from here.

use super::vocab;

/// Maps a phoneme string to token ids, dropping symbols the graph does not know.
pub fn tokenize(phonemes: &str) -> Vec<i64> {
    phonemes
        .chars()
        .filter_map(vocab::token_id)
        .collect()
}

/// The phonemes that survive [`tokenize`], aligned one-to-one with it.
///
/// The reference implementation uses this to line up frame timings with the
/// text that was actually spoken: filtering changes the length, so the original
/// string cannot be used as an index.
pub fn known(phonemes: &str) -> String {
    phonemes.chars().filter(|c| vocab::contains(*c)).collect()
}

/// A token window the graph can accept: padding on both sides.
///
/// Returns the flat row; the caller supplies the batch dimension.
pub fn padded_row(tokens: &[i64]) -> Vec<i64> {
    let mut row = Vec::with_capacity(tokens.len() + 2);
    row.push(vocab::PAD);
    row.extend_from_slice(tokens);
    row.push(vocab::PAD);
    row
}

/// Collapses every whitespace run to a single space and trims the ends.
///
/// Newlines are not in the vocabulary, so without this the lines of a wrapped
/// paragraph run together with no gap for the model to pause on.
pub fn normalize_spacing(phonemes: &str) -> String {
    phonemes.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Which row of a voice's style matrix applies to a window of `length` tokens.
///
/// Voices ship a style vector per phoneme count, so `n` phonemes use row
/// `n - 1`, clamped to the matrix. Clamping rather than erroring means an
/// over-long window degrades slightly instead of failing.
pub fn style_row(length: usize, rows: usize) -> usize {
    debug_assert!(rows > 0, "a voice with no style rows cannot be used");
    length.clamp(1, rows) - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_drops_what_the_graph_cannot_pronounce() {
        // 'h' and 'ˈ' are known; '*' and 'g' (ASCII) are not.
        assert_eq!(tokenize("hˈ*g"), vec![50, 156]);
    }

    #[test]
    fn tokenize_keeps_a_real_word_intact() {
        // "hello" as espeak would give it, minus stress marks.
        let tokens = tokenize("hɛloʊ");
        assert_eq!(tokens, vec![50, 86, 54, 57, 135]);
    }

    #[test]
    fn known_matches_tokenize_one_to_one() {
        let phonemes = "hɛ*loʊ";
        assert_eq!(known(phonemes).chars().count(), tokenize(phonemes).len());
        assert_eq!(known(phonemes), "hɛloʊ");
    }

    #[test]
    fn padding_sits_on_both_sides() {
        assert_eq!(padded_row(&[1, 2, 3]), vec![0, 1, 2, 3, 0]);
        assert_eq!(padded_row(&[]), vec![0, 0]);
    }

    #[test]
    fn spacing_collapses_newlines_and_runs() {
        assert_eq!(normalize_spacing("a\n\nb\t c  "), "a b c");
        assert_eq!(normalize_spacing("   "), "");
    }

    #[test]
    fn style_row_is_one_based_and_clamped() {
        assert_eq!(style_row(1, 510), 0, "one phoneme uses row 0");
        assert_eq!(style_row(10, 510), 9);
        assert_eq!(style_row(510, 510), 509);
        // Past the matrix the reference clamps rather than failing.
        assert_eq!(style_row(900, 510), 509);
        // A zero-length window must not underflow.
        assert_eq!(style_row(0, 510), 0);
    }
}
