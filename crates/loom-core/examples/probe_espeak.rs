//! Probes espeak-ng directly: which voice names it accepts, and whether it
//! emits punctuation.
//!
//! This exists because two facts the phonemizer depends on could not be
//! established by reading, and guessing either would have produced plausible
//! but wrong audio:
//!
//! 1. **`en-GB` does not exist in the bundled build.** It fails with code 2,
//!    along with `en-rp` and `en-sc`, while `en` succeeds — and `en` gives
//!    `həlˈəʊ ðˈeə` against `en-us`'s `həlˈoʊ ðˈɛɹ`. That is RP against General
//!    American, so `en` *is* the British voice.
//! 2. **espeak-ng withholds punctuation.** `"One. Two! Three?"` returns three
//!    clauses with no marks anywhere. Kokoro was trained with punctuation
//!    tokens, so they have to be re-attached from the input — see
//!    [`espeak::phonemize_sentences`].
//!
//! Worth re-running when adding a language: the voice set a wheel ships is
//! trimmed, so a name that looks canonical may be absent.
//!
//! ```text
//! cargo run -p loom-core --example probe_espeak
//! ```

use std::ffi::{c_char, c_int, c_void, CStr, CString};

use loom_core::voice::espeak;

type Initialize = unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int;
type SetVoiceByName = unsafe extern "C" fn(*const c_char) -> c_int;
type TextToPhonemes = unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char;

const AUDIO_OUTPUT_RETRIEVAL: c_int = 1;
const CHARS_UTF8: c_int = 1;
const DONT_EXIT: c_int = 0x8000;
const IPA: c_int = 0x0002;

/// Voice names worth trying, including the exact casing variants.
const CANDIDATES: &[&str] = &[
    "en", "en-us", "en-US", "en-gb", "en-GB", "en-029", "en-sc", "en-rp", "gmw/en",
];

/// What to phonemize. The punctuation is the point.
const SAMPLES: &[&str] = &[
    "One. Two! Three?",
    "Hello there, friend.",
    "test",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = loom_core::paths::loom_home()?;
    let paths = espeak::Paths::from_home(&home);
    println!("library : {}", paths.library.display());
    println!("exists  : {}", paths.library.exists());
    println!("data    : {}", paths.data_root.join("espeak-ng-data").display());
    println!("data ok : {}", paths.data_root.join("espeak-ng-data").is_dir());
    println!();

    if !paths.present() {
        eprintln!("espeak-ng is not installed. Run: python scripts/setup-voice.py");
        return Ok(());
    }

    // SAFETY: only exercised by this probe, with the documented C signatures.
    let lib = unsafe { libloading::Library::new(&paths.library)? };
    let initialize: Initialize = unsafe { *lib.get(b"espeak_Initialize\0")? };
    let set_voice: SetVoiceByName = unsafe { *lib.get(b"espeak_SetVoiceByName\0")? };
    let text_to_phonemes: TextToPhonemes = unsafe { *lib.get(b"espeak_TextToPhonemes\0")? };

    let data = CString::new(paths.data_root.to_string_lossy().as_ref())?;
    let rate = unsafe {
        initialize(
            AUDIO_OUTPUT_RETRIEVAL,
            0,
            data.as_ptr(),
            DONT_EXIT,
        )
    };
    println!("initialize -> sample rate {rate}");
    if rate <= 0 {
        eprintln!("initialization failed");
        return Ok(());
    }
    println!();

    // Which voice names exist.
    println!("=== voice names ===");
    let mut working = Vec::new();
    for name in CANDIDATES {
        let c = CString::new(*name)?;
        let status = unsafe { set_voice(c.as_ptr()) };
        let verdict = if status == 0 { "OK" } else { "FAILED" };
        println!("  {name:<10} {verdict} (code {status})");
        if status == 0 {
            working.push(*name);
        }
    }
    println!();

    // Whether punctuation survives, for each working voice.
    for name in working.iter().take(3) {
        let c = CString::new(*name)?;
        unsafe { set_voice(c.as_ptr()) };

        println!("=== {name} ===");
        for sample in SAMPLES {
            let clauses = clauses(text_to_phonemes, sample, IPA);
            let joined = clauses.join("");
            let marks: String = joined
                .chars()
                .filter(|c| ".!?,;:".contains(*c))
                .collect();
            println!("  in    : {sample:?}");
            println!("  clauses: {clauses:?}");
            println!("  marks  : {marks:?}");
            println!();
        }
    }

    // And once with the IPA flag off, to see what changes.
    let c = CString::new(working.first().copied().unwrap_or("en"))?;
    unsafe { set_voice(c.as_ptr()) };
    println!("=== phonememode 0 (mnemonic, not IPA) ===");
    for sample in SAMPLES {
        println!("  {sample:?} -> {:?}", clauses(text_to_phonemes, sample, 0));
    }
    println!();

    println!("=== phonememode 0x0a (IPA | trace) ===");
    for sample in SAMPLES {
        println!("  {sample:?} -> {:?}", clauses(text_to_phonemes, sample, 0x0a));
    }

    Ok(())
}

/// Runs the clause loop for one string, the same way `phonemize_sentences` does.
fn clauses(
    text_to_phonemes: TextToPhonemes,
    text: &str,
    mode: c_int,
) -> Vec<String> {
    let Ok(line) = CString::new(text) else {
        return Vec::new();
    };
    let mut cursor: *const c_char = line.as_ptr();
    let mut out = Vec::new();

    while !cursor.is_null() {
        // SAFETY: the same call the module makes, with a live buffer.
        let raw = unsafe {
            let result = text_to_phonemes(
                &mut cursor as *mut *const c_char as *mut *const c_void,
                CHARS_UTF8,
                mode,
            );
            if result.is_null() {
                break;
            }
            CStr::from_ptr(result).to_string_lossy().into_owned()
        };
        out.push(raw);
    }
    out
}
