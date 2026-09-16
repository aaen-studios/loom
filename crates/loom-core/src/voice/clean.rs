//! Turning a markdown reply into something worth hearing.
//!
//! This is not cosmetic. Kokoro's vocabulary contains letters, digits, spaces
//! and punctuation, so **anything written in plain characters gets read aloud
//! verbatim** — and model replies are full of URLs, file paths, tables and
//! inline code. Most markdown syntax (`#`, `*`, `[`, `|`, `` ` ``) is dropped
//! automatically by the tokenizer because those symbols are not phonemes, but
//! the *contents* of a link or a path are ordinary words and will be read out.
//!
//! So the job here is narrow and specific: remove the text that is meant for
//! eyes, keep the text that is meant for ears.
//!
//! Fenced code blocks are handled upstream, in [`super::chunk::StreamChunker`],
//! because a fence spans streamed deltas and cannot be detected per chunk.

/// Rewrites markdown as prose suitable for synthesis.
pub fn for_speech(markdown: &str) -> String {    let mut body = String::with_capacity(markdown.len());

    for line in markdown.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            body.push('\n');
            continue;
        }
        if is_horizontal_rule(trimmed) || is_table_separator(trimmed) {
            continue;
        }
        if trimmed.starts_with('|') {
            body.push_str(&flatten_table_row(trimmed));
            body.push('\n');
            continue;
        }

        let mut content = trimmed;
        // Headings: the hashes are structural, the words are content.
        content = content.trim_start_matches('#').trim_start();
        // Block quotes can nest, each level marked with '>'.
        while let Some(rest) = content.strip_prefix('>') {
            content = rest.trim_start();
        }
        content = strip_list_marker(content);

        body.push_str(content);
        body.push('\n');
    }

    let inlined = strip_inline(&body);
    collapse(&inlined)
}

/// `---`, `***`, `___` and friends.
fn is_horizontal_rule(line: &str) -> bool {
    let mut marks = line.chars().filter(|c| !c.is_whitespace()).peekable();
    let Some(&first) = marks.peek() else {
        return false;
    };
    matches!(first, '-' | '*' | '_')
        && line.chars().filter(|c| !c.is_whitespace()).count() >= 3
        && line
            .chars()
            .filter(|c| !c.is_whitespace())
            .all(|c| c == first)
}

/// The `|---|---|` row under a markdown table header.
fn is_table_separator(line: &str) -> bool {
    line.starts_with('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// A table row read as a list, which is how a person would say it.
fn flatten_table_row(line: &str) -> String {
    let cells: Vec<&str> = line
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
        .collect();
    cells.join(", ")
}

/// Leading `- `, `* `, `+ `, `1. `, `1) `.
///
/// A bare `*` is emphasis, not a bullet, so a marker only counts when whitespace
/// follows it.
fn strip_list_marker(line: &str) -> &str {
    let mut chars = line.char_indices();
    let Some((_, first)) = chars.next() else {
        return line;
    };

    if matches!(first, '-' | '*' | '+') {
        if let Some((index, next)) = chars.next() {
            if next.is_whitespace() {
                return line[index..].trim_start();
            }
        }
        return line;
    }

    // Ordered lists: digits, then '.' or ')'.
    if first.is_ascii_digit() {
        let mut end = 0usize;
        for (index, symbol) in line.char_indices() {
            if symbol.is_ascii_digit() {
                end = index + symbol.len_utf8();
                continue;
            }
            if matches!(symbol, '.' | ')') && line[end..].starts_with(symbol) {
                let rest = &line[index + symbol.len_utf8()..];
                if rest.starts_with(char::is_whitespace) {
                    return rest.trim_start();
                }
            }
            break;
        }
    }

    line
}

/// Removes links, images, inline code, HTML tags and emphasis markers.
fn strip_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let symbol = chars[i];

        // Inline code: keep the content, drop the backticks. Code identifiers
        // are usually spoken better than the punctuation around them.
        if symbol == '`' {
            let mut end = i + 1;
            let mut ticks = 1;
            while end < chars.len() && chars[end] == '`' {
                ticks += 1;
                end += 1;
            }
            let close = find_run(&chars, end, '`', ticks);
            let stop = close.unwrap_or(chars.len());
            out.extend(chars[end..stop].iter());
            i = close.map_or(chars.len(), |c| c + ticks);
            continue;
        }

        // Images carry no spoken content at all.
        if symbol == '!' && chars.get(i + 1) == Some(&'[') {
            if let Some((_, end)) = parse_link(&chars, i + 1) {
                i = end;
                continue;
            }
        }

        // Links: say the label, not the address.
        if symbol == '[' {
            if let Some((label, end)) = parse_link(&chars, i) {
                out.push_str(&strip_inline(&label));
                i = end;
                continue;
            }
        }

        // Autolinks and HTML tags.
        if symbol == '<' {
            if let Some(close) = chars[i..].iter().position(|c| *c == '>') {
                i += close + 1;
                continue;
            }
        }

        // Emphasis and strikethrough markers carry no sound.
        if matches!(symbol, '*' | '_' | '~') {
            i += 1;
            continue;
        }

        out.push(symbol);
        i += 1;
    }

    out
}

/// Parses `[label](target)` starting at `open`, returning the label and the
/// index just past the closing parenthesis.
fn parse_link(chars: &[char], open: usize) -> Option<(String, usize)> {
    if chars.get(open) != Some(&'[') {
        return None;
    }
    let label_end = chars[open..].iter().position(|c| *c == ']')? + open;
    if chars.get(label_end + 1) != Some(&'(') {
        return None;
    }
    let target_end = chars[label_end..].iter().position(|c| *c == ')')? + label_end;
    let label: String = chars[open + 1..label_end].iter().collect();
    Some((label, target_end + 1))
}

/// Finds the start of the next run of `ticks` copies of `marker`.
fn find_run(chars: &[char], from: usize, marker: char, ticks: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] != marker {
            i += 1;
            continue;
        }
        let mut run = 0usize;
        while i + run < chars.len() && chars[i + run] == marker {
            run += 1;
        }
        if run >= ticks {
            return Some(i);
        }
        i += run;
    }
    None
}

/// Collapses every whitespace run to a single space and trims the ends.
///
/// Public because a transcript needs it: Whisper's output is punctuation-driven
/// and the byte-level decoder can leave doubled spaces where a token boundary
/// fell between them.
pub fn collapse_whitespace(text: &str) -> String {
    collapse(text)
}

/// Collapses every whitespace run to a single space and trims the ends.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_prose_survives_intact() {
        assert_eq!(for_speech("Hello there, friend."), "Hello there, friend.");
    }

    #[test]
    fn a_link_is_spoken_but_its_address_is_not() {
        let out = for_speech("See [the docs](https://example.com/a/b?c=d) for more.");
        assert_eq!(out, "See the docs for more.");
        assert!(!out.contains("example.com"), "a URL was read aloud: {out}");
    }

    #[test]
    fn images_disappear_entirely() {
        assert_eq!(for_speech("Before ![alt text](pic.png) after"), "Before after");
    }

    #[test]
    fn headings_and_bullets_lose_their_syntax() {
        assert_eq!(for_speech("## A heading"), "A heading");
        assert_eq!(for_speech("- first\n- second"), "first second");
        assert_eq!(for_speech("1. one\n2. two"), "one two");
    }

    #[test]
    fn a_bullet_marker_is_not_confused_with_emphasis() {
        assert_eq!(for_speech("*emphasis* here"), "emphasis here");
        assert_eq!(for_speech("- item here"), "item here");
    }

    #[test]
    fn block_quotes_lose_their_arrows() {
        assert_eq!(for_speech("> quoted text"), "quoted text");
        assert_eq!(for_speech(">> nested quote"), "nested quote");
    }

    #[test]
    fn horizontal_rules_are_dropped() {
        assert_eq!(for_speech("Above\n\n---\n\nBelow"), "Above Below");
        assert_eq!(for_speech("Above\n***\nBelow"), "Above Below");
    }

    #[test]
    fn inline_code_keeps_its_identifier_but_not_its_ticks() {
        assert_eq!(for_speech("Call `persona.rs` first."), "Call persona.rs first.");
    }

    #[test]
    fn a_table_becomes_a_list() {
        let table = "| a | b |\n|---|---|\n| 1 | 2 |";
        assert_eq!(for_speech(table), "a, b 1, 2");
    }

    #[test]
    fn html_tags_are_removed() {
        assert_eq!(for_speech("A <b>bold</b> move"), "A bold move");
    }

    #[test]
    fn an_autolink_does_not_get_read_out() {
        let out = for_speech("Go to <https://example.com> now");
        assert_eq!(out, "Go to now");
    }

    #[test]
    fn emphasis_markers_are_stripped() {
        assert_eq!(for_speech("**very** _important_ ~text~"), "very important text");
    }

    #[test]
    fn file_paths_that_are_not_links_still_get_spoken() {
        // A known limitation, asserted so it stays visible rather than being
        // discovered during a demo. The spoken-style instruction is the
        // defence: the model is told not to emit bare paths in voice mode.
        let out = for_speech("Edit crates/loom-core/src/paths.rs");
        assert!(out.contains("paths.rs"));
    }

    #[test]
    fn nested_markdown_inside_a_link_label_is_flattened() {
        assert_eq!(for_speech("[`code` label](u)"), "code label");
    }

    #[test]
    fn an_unclosed_bracket_is_left_alone_rather_than_eaten() {
        let out = for_speech("An array [1, 2, 3 and more");
        assert!(out.contains("1, 2, 3"), "content was lost: {out}");
    }

    #[test]
    fn whitespace_is_normalised() {
        assert_eq!(for_speech("a\n\n\n   b"), "a b");
    }

    #[test]
    fn an_empty_document_produces_nothing() {
        assert_eq!(for_speech(""), "");
        assert_eq!(for_speech("   \n  "), "");
    }

    #[test]
    fn a_realistic_reply_loses_its_markup() {
        let reply = "## Answer\n\nUse **two** steps:\n\n1. Open `config.json`\n\
                     2. Set the [flag](https://x.dev/y)\n\n> Note: it is safe.\n\n\
                     | key | value |\n|---|---|\n| a | 1 |";
        let out = for_speech(reply);
        for unwanted in ["##", "**", "](http", "|", ">"] {
            assert!(!out.contains(unwanted), "{unwanted:?} survived in {out}");
        }
        assert!(out.contains("two steps"));
        assert!(out.contains("config.json"));
        assert!(out.contains("flag"));
    }
}
