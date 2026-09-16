//! Cutting speech into pieces.
//!
//! Two different jobs live here, because "chunking" means two different things
//! in a voice pipeline and conflating them is how you end up either truncating
//! long text or waiting two seconds for the first word.
//!
//! * [`split_phonemes`] — a faithful port of the reference chunker. Kokoro's
//!   context is fixed at [`MAX_PHONEME_LENGTH`] phonemes, so long phoneme
//!   strings are cut at the least disruptive boundary available: sentence,
//!   then clause, then word, and only as a last resort mid-word. It also
//!   *balances* the batches, because a short trailing batch is spoken at a
//!   different rate and loudness than its neighbours.
//!
//! * [`StreamChunker`] — the latency lever. This runs on **text** as the model
//!   streams it, so sentence one can be synthesised while sentence three is
//!   still being written. Time to first audio comes from here, not from a
//!   faster model.
//!
//! The two are complementary: the stream chunker decides *when* to speak, the
//! phoneme splitter decides *how much* fits in one pass.

/// Marks that end a sentence. Punctuation stays with the text before it, which
/// is how the model was trained.
pub const SENTENCE_MARKS: &[char] = &['.', '!', '?', '…'];

/// Marks that end a clause: a weaker place to cut than a full stop.
pub const CLAUSE_MARKS: &[char] = &[',', ';', ':'];

/// The CommonMark fence: three or more backticks.
const FENCE_TICKS: usize = 3;

// ---------------------------------------------------------------------------
// Phoneme batching (model context)
// ---------------------------------------------------------------------------

/// Splits `text` after any character in `marks` that is followed by whitespace.
///
/// Equivalent to `re.split(r"(?<=[.!?…])\s+", text)`, hand-rolled because the
/// `regex` crate has no lookbehind. Whitespace at the seam is dropped.
fn split_after_marks(text: &str, marks: &[char]) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i].is_whitespace() {
            let mut end = i;
            while end < chars.len() && chars[end].is_whitespace() {
                end += 1;
            }
            if i > 0 && marks.contains(&chars[i - 1]) {
                let piece: String = chars[start..i].iter().collect();
                if !piece.is_empty() {
                    out.push(piece);
                }
                start = end;
            }
            i = end;
        } else {
            i += 1;
        }
    }

    let tail: String = chars[start..].iter().collect();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// Splits on any run of whitespace. The third and last boundary, and the one
/// that catches text with no punctuation at all.
///
/// Equivalent to `re.split(r"\s+", text)`.
fn split_on_whitespace(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// Recursively breaks `phonemes` into pieces no longer than `max_length`.
fn atoms_into(phonemes: &str, max_length: usize, level: usize, out: &mut Vec<String>) {
    if phonemes.chars().count() <= max_length {
        if !phonemes.is_empty() {
            out.push(phonemes.to_string());
        }
        return;
    }

    for index in level..3 {
        let pieces = match index {
            0 => split_after_marks(phonemes, SENTENCE_MARKS),
            1 => split_after_marks(phonemes, CLAUSE_MARKS),
            _ => split_on_whitespace(phonemes),
        };
        if pieces.len() > 1 {
            for piece in pieces {
                atoms_into(piece.trim(), max_length, index + 1, out);
            }
            return;
        }
    }

    // A single unbroken run longer than the context. Slice it rather than
    // dropping the tail.
    let chars: Vec<char> = phonemes.chars().collect();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + max_length).min(chars.len());
        out.push(chars[start..end].iter().collect());
        start = end;
    }
}

/// Groups consecutive atoms into batches within `limit`, as index ranges.
fn pack(lengths: &[usize], limit: usize) -> Vec<(usize, usize)> {
    let mut batches = Vec::new();
    let mut start = 0usize;
    let mut size = 0usize;

    for (index, &length) in lengths.iter().enumerate() {
        // Joining atoms costs one character for the space between them.
        let candidate = if index == start {
            length
        } else {
            size + 1 + length
        };
        if candidate > limit && index > start {
            batches.push((start, index));
            start = index;
            size = length;
        } else {
            size = candidate;
        }
    }

    if !lengths.is_empty() {
        batches.push((start, lengths.len()));
    }
    batches
}

/// Splits `phonemes` into batches of at most `max_length`, as evenly as the
/// atom boundaries allow.
///
/// Filling each batch to the limit leaves a short remainder, and a short batch
/// is spoken at a different rate and loudness than its neighbours. The batches
/// are therefore balanced by binary-searching for the *tightest* limit that
/// still needs no extra pass over the text.
///
/// Balance is bounded by where the atoms fall. Four equal atoms re-cut evenly,
/// but lopsided atoms cannot be levelled without inventing cuts the model was
/// not trained on — so this reduces lopsidedness, it does not guarantee
/// equality.
pub fn split_phonemes(phonemes: &str, max_length: usize) -> Vec<String> {
    let mut atoms = Vec::new();
    atoms_into(phonemes.trim(), max_length, 0, &mut atoms);
    if atoms.is_empty() {
        return Vec::new();
    }

    let lengths: Vec<usize> = atoms.iter().map(|a| a.chars().count()).collect();
    let fewest = pack(&lengths, max_length).len();

    let mut low = *lengths.iter().max().unwrap_or(&0);
    let mut high = max_length;
    while low < high {
        let middle = (low + high) / 2;
        if pack(&lengths, middle).len() <= fewest {
            high = middle;
        } else {
            low = middle + 1;
        }
    }

    pack(&lengths, low)
        .into_iter()
        .map(|(start, end)| atoms[start..end].join(" "))
        .collect()
}

/// Seconds of silence a batch ending with this text should be followed by.
///
/// Trimming removes the silence the model leaves at a batch end, so the pause
/// the punctuation calls for has to be added back when joining.
pub fn pause_after(phonemes: &str, sentence: f32, clause: f32) -> f32 {
    match phonemes.trim_end().chars().next_back() {
        Some(mark) if SENTENCE_MARKS.contains(&mark) => sentence,
        Some(mark) if CLAUSE_MARKS.contains(&mark) => clause,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Streaming text chunking (latency)
// ---------------------------------------------------------------------------

/// Accumulates streamed reply text and hands back sentences as soon as they are
/// safely complete.
///
/// Model replies are full of fenced code, and reading one aloud is worse than
/// silence, so fences are tracked and their contents discarded. The subtle part
/// is that a fence marker routinely arrives **split across deltas** — `"``"`
/// then `` "`rust" `` — so runs of backticks are counted across calls rather
/// than matched per delta. Matching per delta would miss the opening fence and
/// read the whole block out loud.
///
/// A sentence mark only counts as a boundary when whitespace follows it. That
/// single rule handles decimals (`3.5`), ellipses (`...`) and abbreviations
/// without special cases, at the cost of holding a sentence until the next
/// token arrives — tens of milliseconds.
#[derive(Debug, Default)]
pub struct StreamChunker {
    prose: String,
    in_fence: bool,
    /// Backticks seen since the last non-backtick character. Persists across
    /// deltas, which is what makes a split fence detectable.
    ticks: usize,
}

/// Flush at a clause mark once the buffer is this long without a full stop.
const CLAUSE_THRESHOLD: usize = 80;

/// Never let the buffer grow past this; cut at the last space instead.
const HARD_CAP: usize = 200;

impl StreamChunker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a delta and returns every chunk that is now complete.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(before_fence) = self.absorb(delta) {
            let trimmed = before_fence.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out.extend(self.drain());
        out
    }

    /// Ends the stream, returning whatever prose is left.
    pub fn finish(&mut self) -> Option<String> {
        self.in_fence = false;
        self.ticks = 0;
        let rest = std::mem::take(&mut self.prose);
        let trimmed = rest.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    /// Appends `delta` to the prose buffer, dropping anything inside a fence.
    ///
    /// Returns prose that must be spoken *immediately*, which happens when a
    /// fence opens: without this, an introduction like "Here you go:" would sit
    /// in the buffer for the entire duration of the code block that follows.
    fn absorb(&mut self, delta: &str) -> Option<String> {
        let mut flush_now = None;

        for symbol in delta.chars() {
            if symbol == '`' {
                self.ticks += 1;
                continue;
            }

            if self.ticks > 0 {
                let opening = self.ticks >= FENCE_TICKS && !self.in_fence;
                if self.ticks >= FENCE_TICKS {
                    self.in_fence = !self.in_fence;
                }
                // A run shorter than a fence is inline code; drop the markers
                // and keep the content.
                self.ticks = 0;
                if opening {
                    let pending = std::mem::take(&mut self.prose);
                    if !pending.trim().is_empty() {
                        flush_now = Some(pending);
                    }
                }
            }

            if self.in_fence {
                continue;
            }
            self.prose.push(symbol);
        }

        flush_now
    }

    /// Repeatedly cuts complete chunks off the front of the buffer.
    fn drain(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(end) = self.find_cut() {
            let rest = self.prose.split_off(end);
            let chunk = std::mem::replace(&mut self.prose, rest);
            let trimmed = chunk.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    /// The byte index to cut at, or `None` if more input is needed.
    fn find_cut(&self) -> Option<usize> {
        if let Some(end) = self.mark_boundary(SENTENCE_MARKS) {
            return Some(self.skip_whitespace(end));
        }
        let length = self.prose.chars().count();
        if length >= CLAUSE_THRESHOLD {
            if let Some(end) = self.mark_boundary(CLAUSE_MARKS) {
                return Some(self.skip_whitespace(end));
            }
        }
        if length >= HARD_CAP {
            return Some(self.hard_cut());
        }
        None
    }

    /// The end offset of the first mark in `marks` that is followed by
    /// whitespace. A mark at the very end of the buffer does not count: the
    /// next delta may turn `3.` into `3.5`.
    fn mark_boundary(&self, marks: &[char]) -> Option<usize> {
        for (index, symbol) in self.prose.char_indices() {
            if !marks.contains(&symbol) {
                continue;
            }
            let end = index + symbol.len_utf8();
            match self.prose[end..].chars().next() {
                Some(next) if next.is_whitespace() => return Some(end),
                None => return None,
                _ => continue,
            }
        }
        None
    }

    fn skip_whitespace(&self, mut index: usize) -> usize {
        while let Some(next) = self.prose[index..].chars().next() {
            if !next.is_whitespace() {
                break;
            }
            index += next.len_utf8();
        }
        index
    }

    /// A cut for text that has run long without any punctuation: the last space
    /// before the cap, so a word is not split in half.
    fn hard_cut(&self) -> usize {
        let mut seen = 0usize;
        let mut last_space = None;
        for (index, symbol) in self.prose.char_indices() {
            if seen >= HARD_CAP {
                break;
            }
            if symbol.is_whitespace() {
                last_space = Some(index);
            }
            seen += 1;
        }
        last_space.map(|i| i + 1).unwrap_or_else(|| self.prose.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `text` in one go and closes the stream.
    fn chunk(text: &str) -> Vec<String> {
        let mut chunker = StreamChunker::new();
        let mut out = chunker.push(text);
        if let Some(tail) = chunker.finish() {
            out.push(tail);
        }
        out
    }

    // -- phoneme batching ---------------------------------------------------

    #[test]
    fn short_phonemes_pass_through_untouched() {
        assert_eq!(split_phonemes("hɛloʊ", 510), vec!["hɛloʊ"]);
    }

    #[test]
    fn empty_input_produces_no_batches() {
        assert!(split_phonemes("", 510).is_empty());
        assert!(split_phonemes("   ", 510).is_empty());
    }

    #[test]
    fn every_batch_respects_the_limit() {
        let phonemes =
            "ˈhɛloʊ wˈɜːld. ˈðɪs ɪz ə tˈɛst, wɪð klˈɔːzɪz; ənd sˈɛntənsɪz! ".repeat(12);
        let batches = split_phonemes(&phonemes, 60);
        assert!(batches.len() > 1, "expected the text to be split");
        for batch in &batches {
            assert!(
                batch.chars().count() <= 60,
                "batch of {} chars exceeds the limit: {batch:?}",
                batch.chars().count()
            );
            assert!(!batch.trim().is_empty());
        }
    }

    #[test]
    fn batches_are_balanced_rather_than_filled_to_the_limit() {
        // Four equal atoms. Filling greedily to the limit gives [92, 30]; the
        // same text re-cut at a tighter limit gives [61, 61].
        let phonemes = format!(
            "{} {} {} {}",
            "a".repeat(30),
            "b".repeat(30),
            "c".repeat(30),
            "d".repeat(30)
        );
        let batches = split_phonemes(&phonemes, 100);
        assert_eq!(batches.len(), 2, "expected two batches: {batches:?}");

        let lengths: Vec<usize> = batches.iter().map(|b| b.chars().count()).collect();
        let spread = lengths.iter().max().unwrap() - lengths.iter().min().unwrap();
        assert!(spread <= 2, "batches are lopsided: {lengths:?}");
    }

    #[test]
    fn an_unbroken_run_is_sliced_not_dropped() {
        let run = "a".repeat(100);
        let batches = split_phonemes(&run, 30);
        let rejoined: String = batches.join("");
        assert_eq!(rejoined.chars().count(), 100, "characters were lost");
        assert!(batches.iter().all(|b| b.chars().count() <= 30));
    }

    #[test]
    fn sentences_that_fit_are_used_as_the_boundary() {
        // Two complete sentences, each inside the limit: the cut belongs at the
        // full stop, not wherever the character count happens to land.
        let phonemes = format!("{}. {}.", "a".repeat(60), "b".repeat(60));
        let batches = split_phonemes(&phonemes, 70);

        assert_eq!(batches.len(), 2, "expected one batch per sentence");
        for batch in &batches {
            assert!(
                batch.ends_with('.'),
                "a sentence was cut somewhere other than its end: {batch:?}"
            );
        }
    }

    #[test]
    fn words_are_never_cut_in_half() {
        // The whole point of the whitespace boundary: when text has to be
        // split, it splits between words rather than through one.
        let words = [
            "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
        ];
        let phonemes = words.join(" ");
        let batches = split_phonemes(&phonemes, 15);

        assert!(batches.len() > 1, "expected the text to be split");
        for batch in &batches {
            for token in batch.split(' ') {
                assert!(
                    words.contains(&token),
                    "batch cut through a word: {token:?} in {batch:?}"
                );
            }
        }
    }

    #[test]
    fn pause_follows_the_last_mark() {
        assert_eq!(pause_after("hɛloʊ.", 0.25, 0.1), 0.25);
        assert_eq!(pause_after("hɛloʊ!", 0.25, 0.1), 0.25);
        assert_eq!(pause_after("hɛloʊ,", 0.25, 0.1), 0.1);
        assert_eq!(pause_after("hɛloʊ ", 0.25, 0.1), 0.0);
        assert_eq!(pause_after("hɛloʊ", 0.25, 0.1), 0.0);
    }

    #[test]
    fn splitting_after_marks_matches_the_regex_it_replaces() {
        assert_eq!(split_after_marks("a. b", SENTENCE_MARKS), vec!["a.", "b"]);
        assert_eq!(
            split_after_marks("a. b. c", SENTENCE_MARKS),
            vec!["a.", "b.", "c"]
        );
        // No mark, no split.
        assert_eq!(split_after_marks("a b", SENTENCE_MARKS), vec!["a b"]);
        // A mark not followed by whitespace is not a boundary.
        assert_eq!(split_after_marks("3.5 kg", SENTENCE_MARKS), vec!["3.5 kg"]);
        // Multiple spaces collapse at the seam.
        assert_eq!(split_after_marks("a.   b", SENTENCE_MARKS), vec!["a.", "b"]);
    }

    // -- stream chunking ----------------------------------------------------

    #[test]
    fn a_complete_sentence_comes_back_at_once() {
        assert_eq!(
            chunk("Hello there. How are you?"),
            vec!["Hello there.", "How are you?"]
        );
    }

    #[test]
    fn a_sentence_is_held_until_its_end_is_known() {
        let mut chunker = StreamChunker::new();
        // The dot is at the end of what we have; it could still become "3.5".
        assert!(chunker.push("The value is 3.").is_empty());
        let rest = chunker.push("5 exactly. Next one. ");
        assert_eq!(rest, vec!["The value is 3.5 exactly.", "Next one."]);
    }

    #[test]
    fn decimals_do_not_end_sentences() {
        assert_eq!(
            chunk("Pi is 3.14 and that is that."),
            vec!["Pi is 3.14 and that is that."]
        );
    }

    #[test]
    fn text_is_released_across_deltas() {
        let mut chunker = StreamChunker::new();
        assert!(chunker.push("The first sentence ").is_empty());
        assert_eq!(
            chunker.push("is now done. "),
            vec!["The first sentence is now done."]
        );
        assert!(chunker.finish().is_none(), "everything should already be out");
    }

    #[test]
    fn fenced_code_never_reaches_the_speaker() {
        assert_eq!(
            chunk("Here you go:\n```python\nprint('hi')\n```\nDone. "),
            vec!["Here you go:", "Done."]
        );
    }

    #[test]
    fn a_fence_split_across_deltas_is_still_dropped() {
        // The bug this guards against: "``" and "`rust" are each too short to
        // be a fence, so per-delta matching misses the opening marker.
        let mut chunker = StreamChunker::new();
        let mut out = chunker.push("Try this. ``");
        out.extend(chunker.push("`rust\nlet x = 1;\n"));
        out.extend(chunker.push("``` And that works. "));

        assert_eq!(out, vec!["Try this.", "And that works."]);
        assert!(
            !out.join(" ").contains("let x"),
            "code was read aloud: {out:?}"
        );
        assert!(chunker.finish().is_none());
    }

    #[test]
    fn an_introduction_is_spoken_before_a_long_code_block() {
        // Without a forced flush on fence-open, "Here it is:" would wait for
        // the whole code block to finish streaming.
        let mut chunker = StreamChunker::new();
        let out = chunker.push("Here it is: ```rust\nfn main() {}\n");
        assert_eq!(out, vec!["Here it is:"]);
    }

    #[test]
    fn inline_code_keeps_its_text_and_loses_its_backticks() {
        assert_eq!(
            chunk("Call `persona.rs` first. "),
            vec!["Call persona.rs first."]
        );
    }

    #[test]
    fn long_unpunctuated_text_is_cut_at_a_word_boundary() {
        let long = "word ".repeat(60);
        let chunks = chunk(&long);
        assert!(chunks.len() > 1);
        for piece in &chunks {
            assert!(
                piece.chars().count() <= HARD_CAP + 1,
                "piece too long: {}",
                piece.len()
            );
            assert!(!piece.starts_with(' ') && !piece.ends_with(' '));
        }
    }

    #[test]
    fn a_clause_mark_releases_long_text_early() {
        let text = format!("{}, and then it continues without stopping", "w".repeat(90));
        let chunks = chunk(&text);
        assert!(
            chunks.len() >= 2,
            "the clause mark should have released a chunk"
        );
    }

    #[test]
    fn empty_input_yields_nothing() {
        let mut chunker = StreamChunker::new();
        assert!(chunker.push("").is_empty());
        assert!(chunker.finish().is_none());
    }

    #[test]
    fn whitespace_only_input_yields_nothing() {
        assert!(chunk("   \n\n  ").is_empty());
    }
}
