//! Phonemization through espeak-ng, loaded at run time.
//!
//! # Why this is hand-rolled FFI rather than the bindings crate
//!
//! `espeak-rs` wraps espeak-ng with MIT bindings, which is the obvious choice
//! until the build is attempted: `espeak-rs-sys` runs `bindgen`, which needs
//! `libclang`, and then compiles espeak-ng **from source**, which needs a full C
//! toolchain and the platform's standard headers. On Windows that is MSVC Build
//! Tools — a large, admin-only dependency for a build that calls four functions.
//!
//! So this module declares those four functions itself and loads a prebuilt
//! `espeakng.dll` with `libloading`. That is consistent with how ONNX Runtime is
//! already loaded here: nothing is linked at build time, so a missing library is
//! a reportable error rather than a binary that will not start. It also removes
//! the `espeak-rs` dependency and its build-time requirements entirely.
//!
//! # The lock, which is not optional
//!
//! espeak-ng holds **process-global** state: `espeak_SetVoiceByName` sets the
//! voice for the whole process, and `espeak_TextToPhonemes` advances an internal
//! pointer through the input. Two threads phonemizing at once interleave those
//! and produce **corrupted phonemes** — not an error, just wrong audio, which is
//! the worst kind of bug to find later.
//!
//! Everything that touches espeak-ng therefore goes through one mutex. Inference
//! stays outside it: phonemization is fast and serial, synthesis is slower and
//! should run concurrently, and holding this lock across a model call would
//! serialize the whole pipeline for no reason.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::phonemes;
use crate::{Error, Result};

// -- espeak-ng constants, from `speak_lib.h` --------------------------------

/// Return audio to the caller instead of to a sound device. Required: without
/// it espeak-ng plays through the speakers as a side effect of phonemizing.
///
/// Verified against `speak_lib.h`, where the members are implicit and the
/// ordinal *is* the value.
const AUDIO_OUTPUT_RETRIEVAL: c_int = 1;

/// Input is UTF-8. This is `textmode`, an argument separate from the phoneme
/// mode below.
const CHARS_UTF8: c_int = 1;

/// Do not call `exit()` on a fatal error. Essential in a library: an aborted
/// process inside a UI is unrecoverable.
const INITIALIZE_DONT_EXIT: c_int = 0x8000;

/// Emit IPA rather than espeak's internal mnemonic form.
///
/// **`0x0002`, not `0x0001`.** The header's comment spells out why: *"phoneme_mode
/// bit 1: 0=eSpeak's ascii phoneme names, 1= International Phonetic Alphabet"* —
/// bit 1, so the second bit. `espeakPHONEMES_IPA` is also `0x02`, which agrees.
///
/// This was written from memory as `0x0001` first and would have produced
/// noise rather than an error: espeak would have emitted its internal mnemonic
/// form, which Kokoro cannot speak, and the failure would have looked like a
/// broken model.
const INITIALIZE_PHONEME_IPA: c_int = 0x0002;

/// Success.
const EE_OK: c_int = 0;

// -- the four functions we call ---------------------------------------------

type Initialize = unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int;
type SetVoiceByName = unsafe extern "C" fn(*const c_char) -> c_int;
type TextToPhonemes =
    unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char;

/// A loaded espeak-ng, kept alive for the process's lifetime.
///
/// `espeak_Initialize` is not stored: it is called once inside [`initialize`]
/// and never again, so keeping the pointer would be a field nothing reads. The
/// other two are called per request.
struct Library {
    _lib: libloading::Library,
    set_voice_by_name: SetVoiceByName,
    text_to_phonemes: TextToPhonemes,
}

// The function pointers are plain C functions with no captured state, and the
// library is never unloaded, so sharing the handle across threads is sound.
// Every use is serialized by `ESPEAK` below.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}

/// Serialises access to espeak-ng's process-global state.
static ESPEAK: Mutex<()> = Mutex::new(());

/// Languages voice mode can phonemize.
///
/// These are espeak-ng voice names, which are not BCP-47 and are not the same
/// strings as Kokoro's voice prefixes (`af_`, `bf_`, `zf_`…), so a voice
/// carries both. Only the two languages with a confirmed mapping are listed;
/// the rest are added when each has been checked rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// `en-US` — American English.
    EnUs,
    /// `en-GB` — British English.
    EnGb,
}

impl Lang {
    /// The espeak-ng voice name to phonemize with.
    ///
    /// **British English is `"en"`, not `"en-gb"`.** Verified by probing the
    /// build: `en-gb`, `en-rp` and `en-sc` all fail with code 2, while `en`
    /// succeeds — and `en` produces `həlˈəʊ ðˈeə` against `en-us`'s
    /// `həlˈoʊ ðˈɛɹ`, which is RP against General American. The voice set the
    /// `espeakng-loader` wheel ships is trimmed, and `en` is the British one.
    ///
    /// Lowercase for the American voice, matching the reference
    /// implementation. `en-US` also resolves, but there is no reason to rely on
    /// case-insensitive matching when the canonical form is known.
    pub fn espeak_name(self) -> &'static str {
        match self {
            Lang::EnUs => "en-us",
            Lang::EnGb => "en",
        }
    }

    /// Whether espeak-ng is known to provide this voice in a trimmed build.
    ///
    /// The wheels ship a subset. A voice that is absent gets an explicit error
    /// rather than silently falling back to a different accent, which would be
    /// worse than saying so.
    pub fn is_bundled(self) -> bool {
        true
    }

    /// Infers the language from a Kokoro voice id's prefix.
    ///
    /// Kokoro's convention is `<language><gender>_<name>`: `af_heart` is
    /// American female, `bm_george` is British male.
    pub fn from_voice_id(voice: &str) -> Option<Self> {
        match voice.as_bytes().first().copied() {
            Some(b'a') => Some(Lang::EnUs),
            Some(b'b') => Some(Lang::EnGb),
            _ => None,
        }
    }
}

/// Where espeak-ng lives.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The shared library. `espeakng.dll` on Windows, `libespeak-ng.so`
    /// elsewhere — whichever of the candidates exists is used.
    pub library: PathBuf,
    /// The directory *containing* `espeak-ng-data`. Passing the data directory
    /// itself is a common mistake that produces a bare initialisation failure.
    pub data_root: PathBuf,
}

impl Paths {
    /// `~/.loom/espeak/`, which is where `scripts/fetch-espeak.py` puts things.
    pub fn from_home(home: &Path) -> Self {
        let root = home.join("espeak");
        Self {
            library: pick_library(&root),
            data_root: root,
        }
    }

    /// The paths this build will use.
    pub fn resolve() -> Result<Self> {
        Ok(Self::from_home(&crate::paths::loom_home()?))
    }

    /// Whether both the library and its data directory are present.
    pub fn present(&self) -> bool {
        self.library.exists() && self.data_root.join("espeak-ng-data").is_dir()
    }

    /// The paths for a specific home directory, bypassing the environment.
    ///
    /// Tests use this rather than [`Paths::resolve`]: the latter reads the
    /// process-global `LOOM_HOME`, which other tests in this crate mutate, so a
    /// concurrently running test can point it at a temporary directory and make
    /// these fail for reasons that have nothing to do with espeak-ng.
    pub fn for_home(home: &Path) -> Self {
        Self::from_home(home)
    }
}

/// The first candidate library name that exists, or the expected default.
///
/// `espeak-ng.dll` is what the `espeakng-loader` wheel ships, which is what
/// `scripts/fetch-espeak.py` unpacks, so it leads the list. The others cover
/// system installs and the other platforms.
fn pick_library(root: &Path) -> PathBuf {
    let candidates = [
        "espeak-ng.dll",
        "libespeak-ng.so",
        "libespeak-ng.dylib",
        "espeakng.dll",
    ];
    for name in candidates {
        let candidate = root.join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    // Nothing present: report the expected name so the message is actionable.
    root.join(candidates[0])
}

/// A phonemization failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error_ {
    /// The library could not be loaded or initialised.
    Library(String),
    /// The text or language name could not be passed to espeak-ng.
    Input(String),
}

impl std::fmt::Display for Error_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error_::Library(detail) => write!(f, "espeak-ng: {detail}"),
            Error_::Input(detail) => write!(f, "espeak-ng rejected the input: {detail}"),
        }
    }
}

impl std::error::Error for Error_ {}

/// Loads espeak-ng, once per process, **without caching a failure**.
///
/// Caching a negative result is a real bug, not a tidiness question: it was
/// caught by the test suite. A caller that asks for a path where the library is
/// missing would otherwise poison the process permanently, so every later
/// request — including ones pointed at a correct installation — would report
/// the original failure.
///
/// Only success is memoized, which is what espeak-ng actually requires:
/// `espeak_Initialize` may be called once. A failed attempt never initialised
/// anything, so retrying is safe.
fn library(paths: &Paths) -> std::result::Result<&'static Library, Error_> {
    static CACHE: Mutex<Option<(PathBuf, &'static Library)>> = Mutex::new(None);

    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // The cache is keyed on the library path, and that is not tidiness.
    //
    // It was previously keyed on nothing, which meant the first successful load
    // answered every later request — so a call pointed at an installation that
    // does not exist got back a library from a *different* directory and
    // phonemised successfully. A missing install reported no error at all. The
    // test that should have caught it could not, because until the suite had
    // loaded espeak once in the same process the cache was always empty.
    if let Some((cached_path, loaded)) = cache.as_ref() {
        if *cached_path == paths.library {
            return Ok(loaded);
        }

        // A different path. `espeak_Initialize` may only be called once per
        // process, so loading a second copy is not an option — and using the
        // first one would be the silent wrong-installation bug above. Saying so
        // is the only honest answer.
        if paths.library.exists() {
            return Err(Error_::Library(format!(
                "espeak-ng is already loaded from {} and cannot be re-initialised \
                 from {}. espeak_Initialize may only be called once per process.",
                cached_path.display(),
                paths.library.display()
            )));
        }

        // The requested library is absent. Fall through rather than return the
        // cached one, so the normal "not found" message is produced.
    }

    if !paths.library.exists() {
        return Err(Error_::Library(format!(
            "library not found at {}. Run scripts/fetch-espeak.py.",
            paths.library.display()
        )));
    }

    let loaded: &'static Library =
        Box::leak(Box::new(initialize(paths).map_err(Error_::Library)?));
    *cache = Some((paths.library.clone(), loaded));
    Ok(loaded)
}

/// The actual load-and-initialize.
fn initialize(paths: &Paths) -> std::result::Result<Library, String> {
    if !paths.library.exists() {
        return Err(format!(
            "library not found at {}. Run scripts/fetch-espeak.py.",
            paths.library.display()
        ));
    }

    // SAFETY: the path is a file that was checked to exist. Loading a shared
    // library runs its initialisers, which is the point.
    let lib = unsafe { libloading::Library::new(&paths.library) }
        .map_err(|e| format!("could not load {}: {e}", paths.library.display()))?;

    // SAFETY: these names and signatures are espeak-ng's documented C API, read
    // from `speak_lib.h`. The symbols are resolved once and are never unloaded
    // because the `Library` is stored in the cell above.
    let (initialize, set_voice_by_name, text_to_phonemes) = unsafe {
        let initialize: Initialize = *lib
            .get(b"espeak_Initialize\0")
            .map_err(|e| format!("espeak_Initialize is missing: {e}"))?;
        let set_voice_by_name: SetVoiceByName = *lib
            .get(b"espeak_SetVoiceByName\0")
            .map_err(|e| format!("espeak_SetVoiceByName is missing: {e}"))?;
        let text_to_phonemes: TextToPhonemes = *lib
            .get(b"espeak_TextToPhonemes\0")
            .map_err(|e| format!("espeak_TextToPhonemes is missing: {e}"))?;
        (initialize, set_voice_by_name, text_to_phonemes)
    };

    // The data directory must exist before initialization. espeak-ng copies the
    // path, so the `CString` only has to outlive the call below.
    let data_path = paths
        .data_root
        .to_str()
        .and_then(|s| CString::new(s).ok())
        .ok_or_else(|| {
            format!("the data path {} is not valid UTF-8", paths.data_root.display())
        })?;
    let data_ptr = data_path.as_ptr();

    // SAFETY: audio is retrieved rather than played, the buffer length is 0
    // (espeak-ng chooses), the path is a valid NUL-terminated string that
    // outlives this call, and DONT_EXIT keeps a fatal error from aborting.
    let sample_rate = unsafe {
        initialize(
            AUDIO_OUTPUT_RETRIEVAL,
            0,
            data_ptr,
            INITIALIZE_DONT_EXIT,
        )
    };

    if sample_rate <= 0 {
        return Err(format!(
            "initialisation failed with code {sample_rate}. This usually means \
             espeak-ng-data was not found: expected it under {}.",
            paths.data_root.display()
        ));
    }

    // `data_path` goes out of scope here. That is safe: espeak-ng copies the
    // path during initialization and does not retain the pointer, which is why
    // the `CString` only had to outlive the call above.
    Ok(Library {
        _lib: lib,
        set_voice_by_name,
        text_to_phonemes,
    })
}

/// Converts text to a phoneme string the graph can tokenize.
///
/// The result is normalized (whitespace collapsed) and still carries the
/// punctuation espeak-ng emits, which Kokoro treats as tokens and which the
/// chunker uses to decide where pauses belong.
///
/// Symbols outside Kokoro's vocabulary are dropped later by
/// [`phonemes::tokenize`], not here: keeping them makes a mismatch visible while
/// debugging instead of silent.
pub fn phonemize(text: &str, lang: Lang, paths: &Paths) -> std::result::Result<String, Error_> {
    let sentences = phonemize_sentences(text, lang, paths)?;
    // Join with a space: espeak-ng emits sentence-final punctuation but no
    // trailing space, so concatenating directly would fuse one sentence's end
    // to the next one's start.
    Ok(phonemes::normalize_spacing(&sentences.join(" ")))
}

/// As [`phonemize`], but keeping espeak-ng's own sentence split.
///
/// # Re-attaching the punctuation
///
/// espeak-ng splits at sentence and clause punctuation but **does not return the
/// marks themselves**. Probed directly: `"One. Two! Three?"` comes back as
/// three clauses, `["wˌʌn", "tˈuː", "θɹˈiː"]`, with no punctuation anywhere.
///
/// That matters twice over. Kokoro was trained on text containing punctuation,
/// which it treats as tokens that carry intonation — a sentence ending in `.`
/// falls, one ending in `?` rises — so losing every mark flattens the delivery.
/// It also means the marks cannot be used to find sentence boundaries, because
/// none arrive.
///
/// So they are recovered from the input: espeak emits exactly one clause per
/// punctuation mark, so the marks are read from the source text in order and
/// attached positionally. Where the counts disagree — an abbreviation like
/// "Dr." that espeak does not split on — the extra marks are dropped and the
/// remaining clauses stay bare, which degrades to slightly flatter prosody
/// rather than to wrong words.
pub fn phonemize_sentences(
    text: &str,
    lang: Lang,
    paths: &Paths,
) -> std::result::Result<Vec<String>, Error_> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let clauses = raw_clauses(text, lang, paths)?;
    if clauses.is_empty() {
        return Ok(Vec::new());
    }

    // Recover the punctuation espeak withheld.
    let marks: Vec<char> = text
        .chars()
        .filter(|c| is_clause_mark(*c))
        .collect();
    let mut punctuated: Vec<String> = clauses
        .into_iter()
        .enumerate()
        .map(|(index, mut clause)| {
            if let Some(mark) = marks.get(index) {
                clause.push(*mark);
            }
            clause
        })
        .collect();

    // A sentence-final mark closes a sentence; a comma does not, so clauses are
    // accumulated until one arrives.
    let mut sentences = Vec::new();
    let mut current = String::new();
    for clause in punctuated.drain(..) {
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&clause);
        if matches!(current.trim_end().chars().next_back(), Some(c) if is_sentence_mark(c)) {
            sentences.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        sentences.push(current);
    }

    Ok(sentences)
}

/// Punctuation that espeak-ng splits on, and that Kokoro knows as a token.
fn is_clause_mark(symbol: char) -> bool {
    matches!(symbol, '.' | ',' | '!' | '?' | ';' | ':' | '…')
}

/// Marks that end a sentence rather than merely pausing inside one.
fn is_sentence_mark(symbol: char) -> bool {
    matches!(symbol, '.' | '!' | '?' | '…')
}

/// The clauses espeak-ng returns, before punctuation is recovered.
fn raw_clauses(
    text: &str,
    lang: Lang,
    paths: &Paths,
) -> std::result::Result<Vec<String>, Error_> {
    let library = library(paths)?;

    // Poisoning is recoverable: the guarded value is `()`, so a panic in
    // another thread cannot have left it inconsistent.
    let _guard = ESPEAK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let language = CString::new(lang.espeak_name())
        .map_err(|_| Error_::Input("the language name contains a null byte".into()))?;

    // SAFETY: the string is valid and NUL-terminated, and the lock above
    // guarantees no other thread is inside espeak-ng.
    let status = unsafe { (library.set_voice_by_name)(language.as_ptr()) };
    if status != EE_OK {
        return Err(Error_::Input(format!(
            "voice {:?} is not available in this espeak-ng build (code {status}). \
             The bundled wheels ship a trimmed voice set.",
            lang.espeak_name()
        )));
    }

    // The phoneme mode is *only* the IPA flag. `espeak_TextToPhonemes` takes
    // `(textptr, textmode, phonememode)`, so the UTF-8 character code belongs in
    // `textmode` below — not OR'd into the high bits of the phoneme mode, where
    // bits 8-23 mean "separator character between phoneme names" and a value of
    // 1 would insert U+0001 between every phoneme.
    let mode = INITIALIZE_PHONEME_IPA;

    let mut clauses: Vec<String> = Vec::new();

    for line in text.lines() {
        let Ok(line) = CString::new(line) else {
            // A null byte is not worth failing a whole reply for.
            continue;
        };

        let mut cursor: *const c_char = line.as_ptr();
        // espeak_TextToPhonemes returns one clause at a time, advancing
        // `cursor`, until it is null.
        while !cursor.is_null() {
            // SAFETY: `cursor` either points into `line`, which outlives the
            // loop, or is null. The cast is what the C signature requires.
            let clause = unsafe {
                let result = (library.text_to_phonemes)(
                    &mut cursor as *mut *const c_char as *mut *const c_void,
                    CHARS_UTF8,
                    mode,
                );
                if result.is_null() {
                    break;
                }
                CStr::from_ptr(result).to_string_lossy().into_owned()
            };

            let clause = strip_language_switches(&clause);
            let clause = clause.trim();
            if !clause.is_empty() {
                clauses.push(clause.to_string());
            }
        }
    }

    Ok(clauses)
}

/// Removes the `(en)` / `(ar)` markers espeak-ng inserts when a text contains
/// words from another language. They are not phonemes and must not be spoken.
fn strip_language_switches(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut depth = 0usize;
    for symbol in input.chars() {
        match symbol {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(symbol),
            _ => {}
        }
    }
    out
}

/// Whether this build can phonemize at all.
pub const fn compiled_in() -> bool {
    true
}

/// Checks that espeak-ng loads and works, for the settings screen.
///
/// Phonemizes a word rather than only loading the library: espeak-ng reports a
/// missing data directory only when it is asked to do real work, so a load
/// check alone would pass and then fail on the first sentence.
pub fn available(paths: &Paths) -> std::result::Result<(), Error_> {
    match phonemize("test", Lang::EnUs, paths) {
        Ok(out) if !out.is_empty() => Ok(()),
        Ok(_) => Err(Error_::Library(
            "espeak-ng produced no phonemes for a known word".into(),
        )),
        Err(error) => Err(error),
    }
}

/// Converts a phonemization failure into the crate's error type.
impl From<Error_> for Error {
    fn from(value: Error_) -> Self {
        Error::Http(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real files, when `scripts/fetch-espeak.py` has run.
    ///
    /// Deliberately **not** `Paths::resolve()`: that reads the process-global
    /// `LOOM_HOME`, which other tests in this crate point at temporary
    /// directories, so these tests would fail with a path like
    /// `/nonexistent/loom/espeak/...` for reasons that have nothing to do with
    /// espeak-ng. Holding the crate's environment lock is not enough, because a
    /// test that writes the variable without taking the lock can still race.
    ///
    /// The real installation is always under the user's home directory, so it is
    /// read directly.
    fn ready() -> Option<Paths> {
        let home = dirs::home_dir()?.join(".loom");
        let paths = Paths::from_home(&home);
        paths.present().then_some(paths)
    }

    // -- pure helpers -------------------------------------------------------

    #[test]
    fn language_names_are_espeak_voice_names() {
        // British English is "en", not "en-gb": the latter is absent from the
        // bundled build, and "en" is the voice that produces RP vowels.
        assert_eq!(Lang::EnUs.espeak_name(), "en-us");
        assert_eq!(Lang::EnGb.espeak_name(), "en");
    }

    #[test]
    fn clause_and_sentence_marks_are_distinct() {
        // A comma pauses inside a sentence; a full stop ends one. Conflating
        // them would either merge sentences or split them at every comma.
        assert!(is_clause_mark(','));
        assert!(is_clause_mark(';'));
        assert!(is_sentence_mark('.'));
        assert!(is_sentence_mark('?'));
        assert!(is_sentence_mark('!'));
        // Every sentence mark is also a clause mark, but not the reverse.
        assert!(is_clause_mark('.'));
        assert!(!is_sentence_mark(','));
        assert!(!is_sentence_mark(';'));
        assert!(!is_clause_mark('x'));
    }

    #[test]
    fn language_is_inferred_from_kokoro_voice_prefixes() {
        assert_eq!(Lang::from_voice_id("af_heart"), Some(Lang::EnUs));
        assert_eq!(Lang::from_voice_id("am_michael"), Some(Lang::EnUs));
        assert_eq!(Lang::from_voice_id("bf_emma"), Some(Lang::EnGb));
        assert_eq!(Lang::from_voice_id("bm_george"), Some(Lang::EnGb));
        // Languages with no confirmed espeak voice name yet.
        assert_eq!(Lang::from_voice_id("zf_xiaobei"), None);
        assert_eq!(Lang::from_voice_id("jf_alpha"), None);
        assert_eq!(Lang::from_voice_id(""), None);
    }

    #[test]
    fn language_switch_markers_are_stripped() {
        assert_eq!(strip_language_switches("hˈɛloʊ(en)"), "hˈɛloʊ");
        assert_eq!(strip_language_switches("a(ar)b(ar)c"), "abc");
        // An unbalanced marker must not eat the rest of the string.
        assert_eq!(strip_language_switches("abc(def"), "abc");
        assert_eq!(strip_language_switches("plain"), "plain");
    }

    #[test]
    fn empty_text_produces_nothing_without_touching_the_library() {
        // Must not require espeak-ng to be present, or CI would fail here.
        let paths = Paths::from_home(Path::new("/nonexistent"));
        assert_eq!(phonemize("", Lang::EnUs, &paths).unwrap(), "");
        assert!(phonemize_sentences("   ", Lang::EnUs, &paths).unwrap().is_empty());
    }

    #[test]
    fn errors_name_the_component() {
        assert!(Error_::Library("x".into()).to_string().contains("espeak-ng"));
        assert!(Error_::Input("y".into()).to_string().contains("rejected"));
    }

    #[test]
    fn a_missing_library_is_reported_with_the_fix() {
        let paths = Paths::from_home(Path::new("/nonexistent/loom"));
        assert!(!paths.present());
        let error = phonemize("hello", Lang::EnUs, &paths).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("fetch-espeak"), "unhelpful error: {text}");
    }

    #[test]
    fn the_library_name_is_chosen_by_platform() {
        let paths = Paths::from_home(Path::new("/nonexistent"));
        let name = paths.library.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("espeak") || name.starts_with("libespeak"),
            "unexpected library name {name}"
        );
    }

    #[test]
    fn this_build_always_can_phonemize() {
        // The hand-rolled FFI has no optional feature: if the crate builds, the
        // code path exists, and a missing *library* is a runtime matter.
        assert!(compiled_in());
    }

    // -- against the real library ------------------------------------------

    #[test]
    fn phonemizes_a_word_into_the_kokoro_vocabulary() {
        let Some(paths) = ready() else {
            return;
        };
        let out = phonemize("test", Lang::EnUs, &paths).unwrap();
        assert!(!out.is_empty(), "espeak-ng produced nothing");
        for symbol in out.chars() {
            assert!(
                symbol.is_whitespace() || super::super::vocab::contains(symbol),
                "espeak produced {symbol:?}, which Kokoro cannot speak, in {out:?}"
            );
        }
    }

    #[test]
    fn phonemizes_a_sentence_with_punctuation_reattached() {
        let Some(paths) = ready() else {
            return;
        };
        let out = phonemize("Hello there, friend.", Lang::EnUs, &paths).unwrap();
        assert!(out.contains(' '), "words were run together: {out:?}");
        assert!(!out.contains("  "), "spacing was not normalized: {out:?}");
        assert!(!out.contains('\n'), "a newline survived: {out:?}");
        // espeak withholds the marks; they are recovered from the input, and
        // without them Kokoro loses the intonation the punctuation carries.
        assert!(out.contains('.'), "the full stop was lost: {out:?}");
        assert!(out.contains(','), "the comma was lost: {out:?}");
    }

    #[test]
    fn sentence_splitting_returns_more_than_one_piece() {
        let Some(paths) = ready() else {
            return;
        };
        let parts = phonemize_sentences("One. Two! Three?", Lang::EnUs, &paths).unwrap();
        assert_eq!(parts.len(), 3, "expected three sentences, got {parts:?}");
        for part in &parts {
            assert!(
                part.ends_with(['.', '!', '?']),
                "a sentence lost its terminator: {part:?}"
            );
        }
    }

    #[test]
    fn a_comma_does_not_end_a_sentence() {
        let Some(paths) = ready() else {
            return;
        };
        // "Hello there, friend." is one sentence containing a clause break, not
        // two sentences.
        let parts = phonemize_sentences("Hello there, friend.", Lang::EnUs, &paths).unwrap();
        assert_eq!(parts.len(), 1, "the comma split a sentence: {parts:?}");
        assert!(parts[0].contains(','), "the comma was lost: {parts:?}");
    }

    #[test]
    fn the_two_english_voices_differ() {
        let Some(paths) = ready() else {
            return;
        };
        // American and British English disagree on enough words that identical
        // output would suggest the voice is not being switched at all. On this
        // espeak build the difference shows in the vowels of "hello there":
        // həlˈoʊ ðˈɛɹ against həlˈəʊ ðˈeə.
        let us = phonemize("hello there", Lang::EnUs, &paths).unwrap();
        let gb = phonemize("hello there", Lang::EnGb, &paths).unwrap();
        assert_ne!(us, gb, "en-us and en produced identical phonemes");
        assert!(
            us.contains('o') && gb.contains('ə'),
            "the American voice should give a diphthong and the British one a schwa: {us:?} vs {gb:?}"
        );
    }

    #[test]
    fn concurrent_calls_do_not_corrupt_each_other() {
        let Some(paths) = ready() else {
            return;
        };
        // The reason the lock exists. Without it, espeak-ng's process-global
        // voice would be set by one thread while another was mid-phonemize, and
        // the results would be silently wrong.
        let us = phonemize("hello there", Lang::EnUs, &paths).unwrap();
        let gb = phonemize("hello there", Lang::EnGb, &paths).unwrap();
        assert_ne!(us, gb, "the two voices must differ for this test to mean anything");

        let handles: Vec<_> = (0..8)
            .map(|index| {
                let paths = paths.clone();
                std::thread::spawn(move || {
                    let lang = if index % 2 == 0 { Lang::EnUs } else { Lang::EnGb };
                    phonemize("hello there", lang, &paths)
                })
            })
            .collect();

        for (index, handle) in handles.into_iter().enumerate() {
            let result = handle.join().expect("a thread panicked").unwrap();
            let expected = if index % 2 == 0 { &us } else { &gb };
            assert_eq!(
                &result, expected,
                "thread {index} got another thread's voice — the lock is not working"
            );
        }
    }

    #[test]
    fn availability_is_accurate_when_the_files_are_present() {
        let Some(paths) = ready() else {
            return;
        };
        assert!(available(&paths).is_ok());
    }
}
