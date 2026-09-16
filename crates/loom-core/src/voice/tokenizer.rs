//! Turning Whisper's token ids back into text.
//!
//! # Byte-level BPE, decoded
//!
//! Whisper's tokenizer is GPT-2's: byte-level BPE. Every token is a string of
//! *printable* characters standing in for raw bytes, because BPE needs a
//! character alphabet and the 256 byte values are not all printable. The
//! mapping is GPT-2's `bytes_to_unicode`: ASCII-printable bytes map to
//! themselves, and the rest are shifted up into `U+0100…`.
//!
//! Decoding is therefore two steps, in this order:
//!
//! 1. Token id → the token's string.
//! 2. Each character of that string → its byte, and **the bytes accumulated
//!    across tokens** before UTF-8 decoding.
//!
//! The second step is the one that is easy to get wrong. A single character can
//! be split across two tokens — `é` is two bytes and may arrive as `Ã` then
//! `©` — so decoding token by token produces mojibake for every non-ASCII
//! character. The bytes have to be collected first and interpreted once, at the
//! end.
//!
//! # What this does not do
//!
//! **No encoding.** Speech-to-text only ever goes ids → text; the ids come from
//! the model's own greedy decoding. So there is no merging, no BPE rank
//! comparison, and no pre-tokenizer here — just the decode path, which is a
//! fraction of the code and all of what is needed.

use std::collections::HashSet;use std::path::Path;
use std::sync::OnceLock;

use crate::{Error, Result};

/// The special tokens Whisper's decoder can emit or be prompted with.
///
/// Hardcoded because they are part of the model's interface rather than of the
/// vocabulary file — `config.json` names the ids, and these are the meanings.
/// Anything not in this list is treated as text.
pub const SPECIAL_TOKENS: &[(&str, i64)] = &[
    ("<|endoftext|>", 50_256),
    ("<|startoftranscript|>", 50_257),
    ("<|en|>", 50_258),
    ("<|notimestamps|>", 50_362),
];

/// The end of a transcript.
pub const EOS: i64 = 50_256;
/// The decoder's first input: "begin".
pub const SOT: i64 = 50_257;
/// Suppresses timestamp generation, which transcription does not want.
pub const NO_TIMESTAMPS: i64 = 50_362;

/// GPT-2's byte → printable-character table, as a function.
///
/// `bs` is the set of byte values that are already printable; everything else
/// is mapped to `U+0100 + n` in increasing byte order. Reproduced exactly,
/// because a different order gives a different decoder and silently corrupts
/// every non-ASCII character.
fn bytes_to_unicode() -> [char; 256] {
    let mut printable: Vec<u32> = Vec::with_capacity(256);
    // 33..=126: '!' to '~'
    printable.extend(33..=126u32);
    // 161..=172: '¡' to '¬'
    printable.extend(161..=172u32);
    // 174..=255: '®' to 'ÿ'
    printable.extend(174..=255u32);

    let mut codes: Vec<u32> = printable.clone();
    let mut next = 0u32;
    for byte in 0..256u32 {
        if !printable.contains(&byte) {
            printable.push(byte);
            codes.push(256 + next);
            next += 1;
        }
    }

    let mut table = ['\0'; 256];
    for (byte, code) in printable.iter().zip(codes.iter()) {
        table[*byte as usize] = char::from_u32(*code).expect("valid code point");
    }
    table
}

/// The inverse of [`bytes_to_unicode`]: a character back to its byte.
///
/// Built once. Every valid character is below `U+0144`, so a flat array indexed
/// by code point beats a map in both speed and clarity.
fn char_to_byte() -> &'static [u8; 512] {
    static TABLE: OnceLock<[u8; 512]> = OnceLock::new();
    TABLE.get_or_init(|| {
        // 0xFF marks "not part of the alphabet".
        let mut table = [0xFFu8; 512];
        for (byte, symbol) in bytes_to_unicode().iter().enumerate() {
            let index = *symbol as usize;
            debug_assert!(index < 512, "a byte mapped outside the table");
            table[index] = byte as u8;
        }
        table
    })
}

/// A parsed tokenizer.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    /// Token string per id. Sparse: `None` for an id the file does not define.
    tokens: Vec<Option<String>>,
    /// Ids that are control tokens rather than text.
    special: HashSet<i64>,
    /// How many ids the file defines, for a sanity check on load.
    defined: usize,
}

impl Tokenizer {
    /// Reads a Hugging Face `tokenizer.json`.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Self::from_json(&text)
    }

    /// Parses `tokenizer.json` content.
    ///
    /// Only two parts of the file are read: `model.vocab`, which is the
    /// token-string-to-id map, and `added_tokens`, which carries the special
    /// ones. `merges` describes the *encoding* direction and is not needed to
    /// decode.
    pub fn from_json(text: &str) -> Result<Self> {
        let document: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| Error::other(format!("tokenizer.json is not valid JSON: {e}")))?;

        let vocab = document
            .get("model")
            .and_then(|model| model.get("vocab"))
            .and_then(|vocab| vocab.as_object())
            .ok_or_else(|| Error::other("tokenizer.json has no model.vocab"))?;

        // Two passes: the largest id decides the table size, and growing it
        // repeatedly would reallocate a few thousand times.
        let mut highest = 0i64;
        for id in vocab.values() {
            if let Some(id) = id.as_i64() {
                highest = highest.max(id);
            }
        }
        let added = document
            .get("added_tokens")
            .and_then(|tokens| tokens.as_array())
            .cloned()
            .unwrap_or_default();
        for token in &added {
            if let Some(id) = token.get("id").and_then(|id| id.as_i64()) {
                highest = highest.max(id);
            }
        }

        let mut tokens: Vec<Option<String>> = vec![None; (highest + 1) as usize];

        for (token, id) in vocab {
            if let Some(id) = id.as_i64() {
                if id >= 0 && (id as usize) < tokens.len() {
                    tokens[id as usize] = Some(token.clone());
                }
            }
        }

        let mut special = HashSet::new();
        for token in &added {
            let Some(id) = token.get("id").and_then(|id| id.as_i64()) else {
                continue;
            };
            let Some(content) = token.get("content").and_then(|c| c.as_str()) else {
                continue;
            };
            if id >= 0 && (id as usize) < tokens.len() {
                tokens[id as usize] = Some(content.to_string());
            }
            // `special: true` is the file's own marker, but Whisper's control
            // tokens are always written as `<|…|>`, so both are treated as
            // control: a transcript should never contain the literal text.
            let flagged = token
                .get("special")
                .and_then(|flag| flag.as_bool())
                .unwrap_or(false);
            if flagged || is_control_token(content) {
                special.insert(id);
            }
        }

        // The four this module names by constant must be marked, whatever the
        // file says, because the decoder loop relies on them.
        for (_, id) in SPECIAL_TOKENS {
            special.insert(*id);
        }

        // Counted from the table rather than incremented per file entry. The
        // real file lists `<|endoftext|>` in **both** `vocab` and
        // `added_tokens`, so counting entries gave 51,865 where the answer is
        // 51,864 — exactly the `vocab_size` in `config.json`. Counting the
        // slots that ended up filled cannot double-count, because a slot holds
        // one token however many times the file mentions it.
        let defined = tokens.iter().filter(|slot| slot.is_some()).count();

        if defined == 0 {
            return Err(Error::other("tokenizer.json defines no tokens"));
        }

        Ok(Self {
            tokens,
            special,
            defined,
        })
    }

    /// How many ids the file defines.
    pub fn len(&self) -> usize {
        self.defined
    }

    pub fn is_empty(&self) -> bool {
        self.defined == 0
    }

    /// Whether an id is a control token rather than text.
    pub fn is_special(&self, id: i64) -> bool {
        self.special.contains(&id)
    }

    /// The token string for an id, if the file defines it.
    pub fn token(&self, id: i64) -> Option<&str> {
        if id < 0 {
            return None;
        }
        self.tokens.get(id as usize)?.as_deref()
    }

    /// The largest id the file defines.
    pub fn capacity(&self) -> usize {
        self.tokens.len()
    }

    /// Decodes ids to text, **skipping** control tokens.
    ///
    /// This is what a transcript needs: the model's greedy loop stops at
    /// `<|endoftext|>`, and any other control token it emits is an artefact of
    /// prompting rather than something to read out.
    pub fn decode(&self, ids: &[i64]) -> String {
        self.decode_inner(ids, false)
    }

    /// Decodes ids to text, rendering control tokens as their literal names.
    ///
    /// Used for diagnostics: seeing `<|endoftext|>` in a log is more useful
    /// than seeing it silently vanish, and it makes a wrong prompt obvious.
    pub fn decode_verbose(&self, ids: &[i64]) -> String {
        self.decode_inner(ids, true)
    }

    fn decode_inner(&self, ids: &[i64], verbose: bool) -> String {
        // Bytes are accumulated across every token and interpreted once, at the
        // end. Decoding per token would break any character whose UTF-8 bytes
        // span a token boundary, which is most non-ASCII text.
        let mut bytes: Vec<u8> = Vec::with_capacity(ids.len() * 4);
        let table = char_to_byte();

        for id in ids {
            if self.is_special(*id) {
                if verbose {
                    if let Some(token) = self.token(*id) {
                        // Flush first, so the literal lands in the right place.
                        bytes.extend_from_slice(token.as_bytes());
                    }
                }
                continue;
            }

            let Some(token) = self.token(*id) else {
                // An id the file does not define. Skipping is the only safe
                // move: guessing a byte would corrupt the text around it.
                continue;
            };

            for symbol in token.chars() {
                let index = symbol as usize;
                if index < table.len() && table[index] != 0xFF {
                    bytes.push(table[index]);
                } else {
                    // Not part of the byte alphabet, so it was literal text in
                    // the token. Rare, but a tokenizer file could contain it.
                    let mut buffer = [0u8; 4];
                    bytes.extend_from_slice(symbol.encode_utf8(&mut buffer).as_bytes());
                }
            }
        }

        // Lossy at the end rather than per token: a trailing partial character
        // is replaced once, instead of mangling everything before it.
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Decodes one id, for a per-token view.
    pub fn decode_one(&self, id: i64) -> String {
        self.decode(&[id])
    }
}

/// Whether a token's content is one of Whisper's `<|…|>` control tokens.
fn is_control_token(content: &str) -> bool {
    content.starts_with("<|") && content.ends_with("|>")
}

/// The tokenizer's own vocab size, cross-checked against `config.json`.
///
/// For the English-only export this equals [`Tokenizer::len`] exactly — 51,864,
/// verified by `scripts/probe-tokenizer.py`. A multilingual export reserves
/// timestamp ids the file never fills, so the comparison there is `<=` rather
/// than `==`.
pub fn expected_vocab_size(text: &str) -> Option<usize> {
    let document: serde_json::Value = serde_json::from_str(text).ok()?;
    document
        .get("vocab_size")
        .and_then(|size| size.as_u64())
        .map(|size| size as usize)
}

/// The decode prompt's ids, read from `generation_config.json`.
///
/// A struct rather than loose values because these three only make sense
/// together: `SOT` begins the prompt, `NO_TIMESTAMPS` suppresses timing, and
/// `EOS` ends the loop. Getting one from a different file than the other two is
/// how a prompt ends up subtly wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeIds {
    pub start: i64,
    pub no_timestamps: i64,
    pub eos: i64,
}

impl DecodeIds {
    /// The defaults the tiny.en export actually names.
    pub const DEFAULT: Self = Self {
        start: SOT,
        no_timestamps: NO_TIMESTAMPS,
        eos: EOS,
    };

    /// Reads them from `generation_config.json`, falling back to `config.json`.
    ///
    /// `config.json` carries `start` as `decoder_start_token_id` and `eos` as
    /// `eos_token_id`, and names `no_timestamps` only indirectly — through
    /// `forced_decoder_ids`, whose second entry is the token appended after
    /// `start`. `generation_config.json` names all three directly, so it is
    /// preferred when present.
    pub fn from_configs(generation: &str, config: &str) -> Self {
        let generation: Option<serde_json::Value> = serde_json::from_str(generation).ok();
        let config: Option<serde_json::Value> = serde_json::from_str(config).ok();

        let get = |key: &str| -> Option<i64> {
            generation
                .as_ref()
                .and_then(|value| value.get(key))
                .and_then(|value| value.as_i64())
                .or_else(|| {
                    config
                        .as_ref()
                        .and_then(|value| value.get(key))
                        .and_then(|value| value.as_i64())
                })
        };

        // `no_timestamps` is the one that needs the fallback path, because
        // config.json does not name it.
        let no_timestamps = get("no_timestamps_token_id")
            .or_else(|| {
                config
                    .as_ref()?
                    .get("forced_decoder_ids")?
                    .as_array()?
                    .iter()
                    .find(|pair| {
                        pair.as_array()
                            .and_then(|parts| parts.first())
                            .and_then(|first| first.as_i64())
                            == Some(1)
                    })
                    .and_then(|pair| pair.as_array())
                    .and_then(|parts| parts.get(1))
                    .and_then(|value| value.as_i64())
            })
            .unwrap_or(NO_TIMESTAMPS);

        Self {
            start: get("decoder_start_token_id").unwrap_or(SOT),
            no_timestamps,
            eos: get("eos_token_id").unwrap_or(EOS),
        }
    }

    /// The prompt the decoder starts from: `[SOT, NO_TIMESTAMPS]`.
    ///
    /// Not `[SOT]` alone. Without `NO_TIMESTAMPS` the model emits timestamp
    /// tokens interleaved with the text, and the transcript comes out as
    /// `<|0.00|> hello <|0.52|> world` rather than `hello world`.
    pub fn prompt(&self) -> [i64; 2] {
        [self.start, self.no_timestamps]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Whisper directory, when it has been downloaded.
    fn whisper_dir() -> Option<std::path::PathBuf> {
        let dir = dirs::home_dir()?.join(".loom/voice/whisper");
        dir.exists().then_some(dir)
    }

    fn real_tokenizer() -> Option<Tokenizer> {
        let dir = whisper_dir()?;
        let path = dir.join("tokenizer.json");
        path.exists().then(|| Tokenizer::load(&path).ok())?
    }

    // -- the byte alphabet --------------------------------------------------

    #[test]
    fn the_byte_alphabet_has_exactly_two_hundred_and_fifty_six_entries() {
        // Every byte must map somewhere, and no two bytes may share a
        // character, or the decoder is ambiguous.
        let table = bytes_to_unicode();
        let mut seen = HashSet::new();
        for symbol in table {
            assert!(seen.insert(symbol), "{symbol:?} is used twice");
        }
        assert_eq!(seen.len(), 256);
    }

    #[test]
    fn printable_bytes_map_to_themselves() {
        // '!' is 33 and should stay '!'. This is what makes ordinary English
        // text decode without any transformation at all.
        let table = bytes_to_unicode();
        for byte in 33u8..=126 {
            assert_eq!(
                table[byte as usize] as u32, byte as u32,
                "byte {byte} should map to itself"
            );
        }
    }

    #[test]
    fn the_shifted_bytes_start_at_u0100() {
        // 0x00 is the first byte not in the printable set, so it takes 256.
        let table = bytes_to_unicode();
        assert_eq!(table[0] as u32, 256);
        // And 0xAD (soft hyphen), the last, takes 323.
        let highest = table.iter().map(|c| *c as u32).max().expect("non-empty");
        assert_eq!(highest, 323, "the shifted range is 256..=323");
    }

    #[test]
    fn the_char_table_is_the_exact_inverse() {
        let forward = bytes_to_unicode();
        let reverse = char_to_byte();
        for byte in 0..256usize {
            let symbol = forward[byte];
            let index = symbol as usize;
            assert!(index < reverse.len(), "{symbol:?} is outside the table");
            assert_eq!(
                reverse[index], byte as u8,
                "byte {byte} did not round-trip through {symbol:?}"
            );
        }
    }

    #[test]
    fn unflagged_characters_are_marked_invalid() {
        let reverse = char_to_byte();
        // 'A' is not in the alphabet as a *character* to decode — it is only
        // reached via byte 65, which maps to 'A'. So `reverse['A']` is valid.
        assert_eq!(reverse['A' as usize], 65);
        // But a character outside the alphabet is a sentinel.
        assert_eq!(reverse['\u{4e00}' as usize % 512], 0xFF);
    }

    // -- a synthetic tokenizer ----------------------------------------------

    /// A minimal file with the same shape as the real one.
    fn synthetic() -> String {
        // "hello" as two tokens, then a multi-byte one, then a special.
        serde_json::json!({
            "model": {
                "type": "BPE",
                "vocab": {
                    "he": 0,
                    "llo": 1,
                    // U+00C3 U+00A9 is the byte-level form of "é" (0xC3 0xA9).
                    "\u{c3}\u{a9}": 2,
                    " ": 3,
                    "world": 4
                },
                "merges": []
            },
            "added_tokens": [
                {"id": 50256, "content": "<|endoftext|>", "special": true},
                {"id": 50257, "content": "<|startoftranscript|>", "special": true},
                {"id": 50362, "content": "<|notimestamps|>", "special": true}
            ]
        })
        .to_string()
    }

    #[test]
    fn a_synthetic_tokenizer_parses() {
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        assert_eq!(tokenizer.token(0), Some("he"));
        assert_eq!(tokenizer.token(1), Some("llo"));
        assert_eq!(tokenizer.token(50256), Some("<|endoftext|>"));
        assert_eq!(tokenizer.len(), 8);
    }

    #[test]
    fn decoding_joins_tokens_without_spaces() {
        // The space is a token, not a separator. Inserting one would corrupt
        // every word.
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        assert_eq!(tokenizer.decode(&[0, 1]), "hello");
        assert_eq!(tokenizer.decode(&[0, 1, 3, 4]), "hello world");
    }

    #[test]
    fn a_character_split_across_tokens_is_reassembled() {
        // The whole reason bytes are accumulated: 0xC3 0xA9 is "é", and no
        // single token holds it.
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        assert_eq!(tokenizer.decode(&[2]), "é");
        assert_eq!(tokenizer.decode(&[0, 2, 1]), "heél lo".replace(' ', ""));
    }

    #[test]
    fn special_tokens_are_skipped_by_default() {
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        assert_eq!(tokenizer.decode(&[50257, 0, 1, 50256]), "hello");
        // And shown when asked for, which is what makes a bad prompt visible.
        let verbose = tokenizer.decode_verbose(&[50257, 0, 1, 50256]);
        assert!(verbose.contains("<|startoftranscript|>"), "{verbose}");
        assert!(verbose.contains("hello"), "{verbose}");
        assert!(verbose.contains("<|endoftext|>"), "{verbose}");
    }

    #[test]
    fn an_unknown_id_is_skipped_rather_than_guessed() {
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        // 40000 is inside the table but undefined, and 99999 is past the end.
        assert_eq!(tokenizer.decode(&[0, 40000, 1]), "hello");
        assert_eq!(tokenizer.decode(&[0, 99999, 1]), "hello");
        assert_eq!(tokenizer.decode(&[-5, 0, 1]), "hello");
    }

    #[test]
    fn decoding_nothing_gives_nothing() {
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        assert_eq!(tokenizer.decode(&[]), "");
        assert_eq!(tokenizer.decode(&[50256]), "");
    }

    #[test]
    fn the_named_constants_are_marked_special() {
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        for (name, id) in SPECIAL_TOKENS {
            assert!(tokenizer.is_special(*id), "{name} ({id}) is not special");
        }
        // An ordinary token is not.
        assert!(!tokenizer.is_special(0));
    }

    #[test]
    fn control_tokens_are_detected_by_shape_as_well_as_by_flag() {
        // A file that forgets `special: true` should still not leak control
        // tokens into a transcript.
        let text = serde_json::json!({
            "model": {"type": "BPE", "vocab": {"a": 0}, "merges": []},
            "added_tokens": [
                {"id": 10, "content": "<|notimestamps|>"}
            ]
        })
        .to_string();
        let tokenizer = Tokenizer::from_json(&text).expect("valid");
        assert!(tokenizer.is_special(10));
        assert_eq!(tokenizer.decode(&[0, 10, 0]), "aa");
    }

    #[test]
    fn a_file_with_no_vocab_is_an_error() {
        let error = Tokenizer::from_json("{}").unwrap_err();
        assert!(error.to_string().contains("model.vocab"), "{error}");
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        let error = Tokenizer::from_json("not json at all").unwrap_err();
        assert!(error.to_string().contains("valid JSON"), "{error}");
    }

    #[test]
    fn an_empty_vocab_is_an_error() {
        let text = serde_json::json!({
            "model": {"type": "BPE", "vocab": {}, "merges": []}
        })
        .to_string();
        assert!(Tokenizer::from_json(&text).is_err());
    }

    #[test]
    fn capacity_covers_the_highest_id() {
        let tokenizer = Tokenizer::from_json(&synthetic()).expect("valid");
        assert!(tokenizer.capacity() > 50362);
    }

    // -- the real file ------------------------------------------------------

    #[test]
    fn the_real_tokenizer_loads_with_the_ids_the_configs_name() {
        let Some(tokenizer) = real_tokenizer() else {
            return;
        };
        let Some(dir) = whisper_dir() else {
            return;
        };

        // Read from the configs rather than hardcoding, so a different export
        // is caught rather than silently mistranscribed.
        let config_text = std::fs::read_to_string(dir.join("config.json")).unwrap_or_default();
        let generation_text =
            std::fs::read_to_string(dir.join("generation_config.json")).unwrap_or_default();
        let ids = DecodeIds::from_configs(&generation_text, &config_text);

        assert_eq!(ids.eos, EOS, "the configs and this module disagree about EOS");
        assert_eq!(ids.start, SOT, "the configs and this module disagree about SOT");
        assert_eq!(
            ids.no_timestamps, NO_TIMESTAMPS,
            "the configs and this module disagree about notimestamps"
        );

        assert!(tokenizer.is_special(ids.eos));
        assert!(tokenizer.is_special(ids.start));
        assert!(tokenizer.is_special(ids.no_timestamps));
        assert!(tokenizer.len() > 50_000, "only {} tokens", tokenizer.len());

        // The prompt is two tokens, in this order: SOT then NO_TIMESTAMPS.
        assert_eq!(ids.prompt(), [SOT, NO_TIMESTAMPS]);
    }

    #[test]
    fn the_real_vocab_size_matches_what_the_file_defines() {
        let Some(tokenizer) = real_tokenizer() else {
            return;
        };
        let Some(dir) = whisper_dir() else {
            return;
        };
        let Ok(config_text) = std::fs::read_to_string(dir.join("config.json")) else {
            return;
        };
        let Some(vocab_size) = expected_vocab_size(&config_text) else {
            return;
        };

        // For the English-only export these are equal, exactly: 51,864. The
        // file lists `<|endoftext|>` in both `vocab` and `added_tokens`, so an
        // entry-counting parser reports 51,865 — one too many. Asserting
        // equality rather than `<=` is what catches that.
        assert_eq!(
            tokenizer.len(),
            vocab_size,
            "the tokenizer defines {} ids but config says {vocab_size}",
            tokenizer.len()
        );
        assert!(tokenizer.capacity() <= vocab_size);
    }

    #[test]
    fn the_decode_ids_survive_a_missing_generation_config() {
        // Regression: `no_timestamps_token_id` is not in `config.json`, so a
        // loader that only reads that file has to recover it from
        // `forced_decoder_ids` — whose second entry is the token appended after
        // the start token.
        let config = r#"{
            "eos_token_id": 50256,
            "decoder_start_token_id": 50257,
            "forced_decoder_ids": [[1, 50362]]
        }"#;
        let ids = DecodeIds::from_configs("", config);
        assert_eq!(ids.eos, EOS);
        assert_eq!(ids.start, SOT);
        assert_eq!(ids.no_timestamps, NO_TIMESTAMPS);
    }

    #[test]
    fn the_decode_ids_prefer_the_generation_config() {
        let generation = r#"{"eos_token_id": 1, "decoder_start_token_id": 2, "no_timestamps_token_id": 3}"#;
        let config = r#"{"eos_token_id": 50256, "decoder_start_token_id": 50257}"#;
        let ids = DecodeIds::from_configs(generation, config);
        assert_eq!(ids, DecodeIds { start: 2, no_timestamps: 3, eos: 1 });
    }

    #[test]
    fn the_decode_ids_fall_back_to_the_defaults_when_nothing_parses() {
        let ids = DecodeIds::from_configs("", "");
        assert_eq!(ids, DecodeIds::DEFAULT);
        // And a forced_decoder_ids that is absent or malformed does not panic.
        let ids = DecodeIds::from_configs("{}", r#"{"forced_decoder_ids": "nonsense"}"#);
        assert_eq!(ids.no_timestamps, NO_TIMESTAMPS);
        let ids = DecodeIds::from_configs("{}", r#"{"forced_decoder_ids": [[9, 9]]}"#);
        assert_eq!(ids.no_timestamps, NO_TIMESTAMPS, "step 0 is not the one");
    }

    #[test]
    fn every_defined_token_decodes_without_panicking() {
        let Some(tokenizer) = real_tokenizer() else {
            return;
        };
        for id in 0..tokenizer.capacity() as i64 {
            let _ = tokenizer.decode(&[id]);
        }
    }

    #[test]
    fn the_real_tokenizer_decodes_the_word_hello() {
        let Some(tokenizer) = real_tokenizer() else {
            return;
        };
        // "hello" is a single token in GPT-2's vocabulary. Found by searching
        // rather than hardcoded, so this does not depend on remembering an id.
        let id = (0..tokenizer.capacity() as i64)
            .find(|id| !tokenizer.is_special(*id) && tokenizer.token(*id) == Some("hello"));

        if let Some(id) = id {
            assert_eq!(tokenizer.decode(&[id]), "hello");
        }
    }

    #[test]
    fn most_tokens_decode_to_printable_text_on_their_own() {
        let Some(tokenizer) = real_tokenizer() else {
            return;
        };
        // A token holding the *first half* of a multi-byte character decodes to
        // a replacement character on its own, and is correct: the other half is
        // in the next token. The real file has 344 such tokens — 0.68% — which
        // `scripts/probe-tokenizer.py` computes independently from the byte
        // table. So this is a ratio with a documented cause, not a count chosen
        // because it passed.
        let mut partial = 0usize;
        let mut ordinary = 0usize;

        for id in 0..tokenizer.capacity() as i64 {
            if tokenizer.is_special(id) || tokenizer.token(id).is_none() {
                continue;
            }
            ordinary += 1;
            if tokenizer.decode(&[id]).contains('\u{fffd}') {
                partial += 1;
            }
        }

        assert!(ordinary > 50_000, "only {ordinary} ordinary tokens");
        let ratio = partial as f64 / ordinary as f64;
        assert!(
            ratio < 0.02,
            "{partial} of {ordinary} tokens decode to replacement characters \
             ({:.2}%), which is too many to be partial characters",
            ratio * 100.0
        );
        // And it should not be zero either: a byte table that decoded every
        // token standalone would not be GPT-2's.
        assert!(partial > 100, "only {partial} partial tokens, which is suspicious");
    }

    #[test]
    fn the_partial_tokens_pair_up_into_valid_text() {
        let Some(tokenizer) = real_tokenizer() else {
            return;
        };
        // The point of accumulating bytes: two tokens that are each invalid
        // alone become one valid character together. Found by searching for a
        // pair rather than hardcoding an id.
        let mut found = None;
        let leading: Vec<i64> = (0..tokenizer.capacity() as i64)
            .filter(|id| !tokenizer.is_special(*id) && tokenizer.token(*id).is_some())
            .filter(|id| {
                // A token whose byte form starts a multi-byte sequence.
                let bytes = tokenizer.decode_one(*id).into_bytes();
                !bytes.is_empty() && bytes[0] >= 0xC0
            })
            .take(50)
            .collect();

        for first in &leading {
            for second in 0..tokenizer.capacity() as i64 {
                if tokenizer.is_special(second) || tokenizer.token(second).is_none() {
                    continue;
                }
                let pair = tokenizer.decode(&[*first, second]);
                if !pair.contains('\u{fffd}') && pair.chars().count() < 8 {
                    found = Some((*first, second, pair));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }

        let (first, second, text) = found.expect("some pair should form valid text");
        assert!(!text.is_empty());
        // And each half alone is *not* valid, which is what makes the pair
        // meaningful as a test.
        assert!(
            tokenizer.decode(&[first]).contains('\u{fffd}')
                || tokenizer.decode(&[second]).contains('\u{fffd}'),
            "the pair {first}+{second} decoded cleanly on its own, so it proves nothing"
        );
    }
}
