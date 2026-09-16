//! Reading `voices-v1.0.bin`, which is where every voice's style lives.
//!
//! # The format, established by reading the file
//!
//! Despite the `.bin` extension this is a **`np.savez` archive**: a plain ZIP
//! holding one `.npy` per voice, named after the voice (`af_heart.npy`). The
//! reference implementation reads it with `np.load` and indexes it by string
//! key, which is what a `savez` archive looks like from Python.
//!
//! This matters, because the alternative — `np.save` of a dict — would have
//! produced a **pickled** payload, and reading pickle from Rust is a real
//! project of its own. Verifying rather than assuming turned a hard problem
//! into a ZIP read, and `zip` is already a dependency of this crate.
//!
//! Each entry is `.npy` version 1.0 holding little-endian `f32` with shape
//! `(510, 1, 256)`: 510 style vectors of 256 floats, one per possible phoneme
//! count. Row `n - 1` belongs to a window of `n` phonemes, so a window that
//! reaches the context limit uses the last row.
//!
//! # Why the `.npy` header is parsed by hand
//!
//! The header is a Python dict literal:
//!
//! ```text
//! {'descr': '<f4', 'fortran_order': False, 'shape': (510, 1, 256), }
//! ```
//!
//! Only three keys matter, all with simple values, so a targeted scan is
//! smaller and more predictable than a general parser — and it cannot be
//! tricked into executing anything, unlike `eval`.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use crate::{Error, Result};

/// The magic that starts every `.npy` payload.
const NPY_MAGIC: &[u8] = b"\x93NUMPY";

/// The dtype this module accepts. Anything else is rejected rather than
/// reinterpreted, because reading `f8` bytes as `f4` would produce noise
/// rather than an error.
const EXPECTED_DESCR: &str = "<f4";

/// One voice's style matrix: `rows` vectors of `columns` floats.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    rows: usize,
    columns: usize,
    /// Row-major, `rows * columns` floats.
    data: Vec<f32>,
}

impl Style {
    /// Number of style rows, which is the largest window this voice covers.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Floats per style vector.
    pub fn columns(&self) -> usize {
        self.columns
    }

    /// Raw row-major data.
    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// The style vector for a window of `phonemes` phonemes.
    ///
    /// Clamped rather than erroring, matching the reference: a window longer
    /// than the matrix degrades slightly instead of failing.
    pub fn row(&self, phonemes: usize) -> &[f32] {
        let index = phonemes.clamp(1, self.rows) - 1;
        let start = index * self.columns;
        &self.data[start..start + self.columns]
    }
}

/// Every voice in the archive.
#[derive(Debug, Clone, PartialEq)]
pub struct Voices {
    styles: BTreeMap<String, Style>,
}

impl Voices {
    /// Reads the archive at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
        Self::from_reader(file)
    }

    /// Reads the archive from anything seekable and readable.
    pub fn from_reader<R: Read + std::io::Seek>(reader: R) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(reader)
            .map_err(|e| Error::Http(format!("voices archive is not a readable zip: {e}")))?;

        let mut styles = BTreeMap::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|e| {
                Error::Http(format!("voices archive entry {index} could not be read: {e}"))
            })?;

            let name = entry.name().to_string();
            // Directories and non-npy entries are ignored rather than treated
            // as corruption: an archive may legitimately carry extras.
            let Some(stem) = name.strip_suffix(".npy") else {
                continue;
            };
            if stem.is_empty() || stem.contains('/') {
                continue;
            }

            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| {
                Error::Http(format!("voices archive entry {name} could not be read: {e}"))
            })?;

            let style = parse_npy(&bytes).map_err(|detail| {
                Error::Http(format!("voices archive entry {name}: {detail}"))
            })?;

            styles.insert(stem.to_string(), style);
        }

        if styles.is_empty() {
            return Err(Error::Http(
                "voices archive contained no .npy entries".to_string(),
            ));
        }

        Ok(Self { styles })
    }

    /// Whether a voice exists.
    pub fn contains(&self, voice: &str) -> bool {
        self.styles.contains_key(voice)
    }

    /// A voice's style matrix.
    pub fn get(&self, voice: &str) -> Option<&Style> {
        self.styles.get(voice)
    }

    /// Every voice name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.styles.keys().map(String::as_str)
    }

    /// How many voices the archive carries.
    pub fn len(&self) -> usize {
        self.styles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.styles.is_empty()
    }
}

/// A `.npy` payload: three header fields, then raw data.
#[derive(Debug)]
struct NpyHeader {
    descr: String,
    fortran_order: bool,
    shape: Vec<usize>,
    data_offset: usize,
}

/// Parses a `.npy` blob into a [`Style`].
fn parse_npy(blob: &[u8]) -> std::result::Result<Style, String> {
    let header = parse_npy_header(blob)?;

    if header.descr != EXPECTED_DESCR {
        return Err(format!(
            "expected dtype {EXPECTED_DESCR:?}, found {:?}",
            header.descr
        ));
    }
    if header.fortran_order {
        // Column-major would silently transpose every style vector.
        return Err("fortran_order is true, which this reader does not handle".to_string());
    }

    let data = &blob[header.data_offset..];
    let floats = data.len() / 4;
    let expected: usize = header.shape.iter().product();

    if floats != expected {
        return Err(format!(
            "shape {:?} needs {expected} floats but the payload holds {floats}",
            header.shape
        ));
    }

    // `(rows, 1, columns)` today, but the reader accepts any shape whose last
    // dimension is the style width and whose total matches, so a future export
    // that drops the singleton axis does not need a code change.
    let (rows, columns) = match header.shape.len() {
        2 => (header.shape[0], header.shape[1]),
        3 => (header.shape[0], header.shape[2]),
        other => {
            return Err(format!(
                "expected a 2- or 3-dimensional style array, found {other} dimensions"
            ))
        }
    };
    if rows == 0 || columns == 0 {
        return Err("a style array with a zero dimension is unusable".to_string());
    }

    let mut values = Vec::with_capacity(floats);
    for chunk in data.chunks_exact(4) {
        values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    Ok(Style {
        rows,
        columns,
        data: values,
    })
}

/// Extracts the three header fields from a `.npy` blob.
fn parse_npy_header(blob: &[u8]) -> std::result::Result<NpyHeader, String> {
    if blob.len() < 10 || &blob[..6] != NPY_MAGIC {
        return Err("missing the \\x93NUMPY magic".to_string());
    }

    let major = blob[6];
    let (header_len, data_offset) = match major {
        1 => {
            let len = u16::from_le_bytes([blob[8], blob[9]]) as usize;
            (len, 10 + len)
        }
        2 | 3 => {
            if blob.len() < 12 {
                return Err("truncated version 2 header".to_string());
            }
            let len = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]) as usize;
            (len, 12 + len)
        }
        other => return Err(format!("unsupported .npy version {other}")),
    };

    if data_offset > blob.len() {
        return Err("header length runs past the end of the payload".to_string());
    }

    let raw = std::str::from_utf8(&blob[data_offset - header_len..data_offset])
        .map_err(|_| "the header is not valid UTF-8".to_string())?;
    // ASCII, space-padded, newline-terminated.
    let header = raw.trim();

    Ok(NpyHeader {
        descr: field(header, "descr").unwrap_or_default(),
        fortran_order: field(header, "fortran_order").as_deref() == Some("True"),
        shape: field(header, "shape")
            .ok_or_else(|| "the header has no 'shape'".to_string())
            .and_then(|value| parse_shape(&value))?,
        data_offset,
    })
}

/// The text following `'key':` in a `.npy` header dict literal.
///
/// The three values have three different terminators, and getting this wrong is
/// silent: `'shape': (510, 1, 256)` contains commas, so trimming a tuple at the
/// first comma yields `(510` and the shape never parses.
fn field(header: &str, key: &str) -> Option<String> {
    let needle = format!("'{key}'");
    let at = header.find(&needle)? + needle.len();
    let rest = &header[at..];
    let colon = rest.find(':')?;
    let value = rest[colon + 1..].trim_start();

    // Quoted string: ends at the closing quote.
    if let Some(inner) = value.strip_prefix('\'') {
        let end = inner.find('\'')?;
        return Some(inner[..end].to_string());
    }

    // Tuple: ends at the matching paren, commas inside notwithstanding.
    if value.starts_with('(') {
        let end = value.find(')')?;
        return Some(value[..=end].to_string());
    }

    // Anything else (True/False/a number): ends at the next comma or brace.
    let end = value
        .find(',')
        .or_else(|| value.find('}'))
        .unwrap_or(value.len());
    Some(value[..end].trim().to_string())
}

/// Parses the `(510, 1, 256)` form, including the one-element `(256,)` form.
fn parse_shape(value: &str) -> std::result::Result<Vec<usize>, String> {
    let start = value.find('(').ok_or("shape is not a tuple")? + 1;
    let end = value.find(')').ok_or("shape tuple is not closed")?;
    let inner = &value[start..end];

    let mut dims = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            // The trailing comma of a one-element tuple.
            continue;
        }
        dims.push(
            part.parse::<usize>()
                .map_err(|_| format!("shape dimension {part:?} is not a number"))?,
        );
    }
    if dims.is_empty() {
        return Err("shape tuple is empty".to_string());
    }
    Ok(dims)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal `.npy` blob for testing the parser without a real file.
    fn npy(values: &[f32], dims: &[usize]) -> Vec<u8> {
        let inner = dims
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let shape = if dims.len() == 1 {
            format!("({inner},)")
        } else {
            format!("({inner})")
        };
        let mut header = format!(
            "{{'descr': '<f4', 'fortran_order': False, 'shape': {shape}, }}"
        );
        // The format pads the header to a 64-byte boundary with spaces and
        // terminates it with a newline. The outer `% 64` matters: without it an
        // already-aligned header gets a spurious extra 64 bytes of padding.
        let padding = (64 - ((10 + header.len() + 1) % 64)) % 64;
        header.push_str(&" ".repeat(padding));
        header.push('\n');

        let mut blob = Vec::from(NPY_MAGIC);
        blob.extend_from_slice(&[1u8, 0u8]);
        blob.extend_from_slice(&(header.len() as u16).to_le_bytes());
        blob.extend_from_slice(header.as_bytes());
        for value in values {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        blob
    }

    /// The real archive, when it has been downloaded.
    fn real_archive() -> Option<std::path::PathBuf> {
        let path = crate::paths::voice_dir().ok()?.join("voices-v1.0.bin");
        path.exists().then_some(path)
    }

    // -- header parsing -----------------------------------------------------

    #[test]
    fn parses_a_three_dimensional_header() {
        let blob = npy(&[0.0; 6], &[2, 1, 3]);
        let header = parse_npy_header(&blob).unwrap();
        assert_eq!(header.descr, "<f4");
        assert!(!header.fortran_order);
        assert_eq!(header.shape, vec![2, 1, 3]);
    }

    #[test]
    fn parses_a_one_dimensional_header() {
        let blob = npy(&[0.0; 4], &[4]);
        assert_eq!(parse_npy_header(&blob).unwrap().shape, vec![4]);
    }

    #[test]
    fn rejects_a_missing_magic() {
        let blob = vec![0u8; 32];
        assert!(parse_npy_header(&blob).unwrap_err().contains("magic"));
    }

    #[test]
    fn rejects_an_unsupported_version() {
        let mut blob = npy(&[0.0; 4], &[4]);
        blob[6] = 9;
        assert!(parse_npy_header(&blob).unwrap_err().contains("version"));
    }

    #[test]
    fn rejects_a_header_that_runs_off_the_end() {
        let mut blob = npy(&[0.0; 4], &[4]);
        blob[8] = 0xff;
        blob[9] = 0xff;
        assert!(parse_npy_header(&blob).unwrap_err().contains("past the end"));
    }

    #[test]
    fn a_wrong_dtype_is_rejected_rather_than_reinterpreted() {
        // f8 read as f4 would be noise, not an error.
        let mut blob = npy(&[0.0; 4], &[4]);
        let at = blob
            .windows(3)
            .position(|w| w == b"<f4")
            .expect("the dtype is in the header");
        blob[at + 2] = b'8';
        assert!(parse_npy(&blob).unwrap_err().contains("dtype"));
    }

    #[test]
    fn fortran_order_is_rejected() {
        let mut blob = npy(&[0.0; 4], &[4]);
        let at = blob
            .windows(16)
            .position(|w| w.starts_with(b"False, 'shape'"))
            .expect("fortran_order is in the header");
        // "False" -> "True" plus the two spaces that vanish.
        blob[at..at + 5].copy_from_slice(b"True ");
        assert!(parse_npy(&blob).unwrap_err().contains("fortran_order"));
    }

    #[test]
    fn a_shape_and_payload_disagreement_is_caught() {
        // Claims 100 floats, carries 4.
        let blob = npy(&[0.0; 4], &[100]);
        assert!(parse_npy(&blob).unwrap_err().contains("floats"));
    }

    // -- style access -------------------------------------------------------

    #[test]
    fn reads_values_in_row_major_order() {
        let blob = npy(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 1, 3]);
        let style = parse_npy(&blob).unwrap();
        assert_eq!(style.rows(), 2);
        assert_eq!(style.columns(), 3);
        assert_eq!(style.row(1), &[1.0, 2.0, 3.0]);
        assert_eq!(style.row(2), &[4.0, 5.0, 6.0]);
    }

    #[test]
    fn row_selection_is_one_based_and_clamped() {
        let blob = npy(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 1, 3]);
        let style = parse_npy(&blob).unwrap();
        assert_eq!(style.row(1), &[1.0, 2.0, 3.0]);
        // Past the matrix the last row is reused rather than panicking.
        assert_eq!(style.row(99), &[4.0, 5.0, 6.0]);
        // A zero-length window must not underflow.
        assert_eq!(style.row(0), &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_two_dimensional_shape_is_accepted() {
        // A future export might drop the singleton batch axis.
        let blob = npy(&[1.0, 2.0, 3.0, 4.0], &[2, 2]);
        let style = parse_npy(&blob).unwrap();
        assert_eq!(style.rows(), 2);
        assert_eq!(style.columns(), 2);
    }

    #[test]
    fn a_one_dimensional_shape_is_rejected_as_a_style() {
        // Two and three dimensions both describe a style matrix. One does not:
        // there is no row to select for a window of n phonemes.
        let blob = npy(&[0.0; 4], &[4]);
        assert!(parse_npy(&blob).unwrap_err().contains("dimensions"));
    }

    #[test]
    fn a_zero_dimension_is_rejected() {
        let blob = npy(&[], &[0, 4]);
        assert!(parse_npy(&blob).unwrap_err().contains("zero"));
    }

    // -- archive ------------------------------------------------------------

    #[test]
    fn a_non_zip_payload_is_reported_clearly() {
        let bytes = std::io::Cursor::new(b"not a zip at all".to_vec());
        let error = Voices::from_reader(bytes).unwrap_err();
        assert!(error.to_string().contains("zip"), "{error}");
    }

    // -- against the real archive -------------------------------------------

    #[test]
    fn the_real_archive_holds_fifty_four_voices() {
        let Some(path) = real_archive() else {
            return;
        };
        let voices = Voices::load(&path).unwrap();
        assert_eq!(voices.len(), 54, "expected 54 voices");

        // The roster, grouped by Kokoro's two-letter prefix. These counts were
        // read from the shipped archive and are asserted so that a swapped or
        // truncated file cannot pass unnoticed.
        let expected = [
            ("af", 11),
            ("am", 9),
            ("bf", 4),
            ("bm", 4),
            ("ef", 1),
            ("em", 2),
            ("ff", 1),
            ("hf", 2),
            ("hm", 2),
            ("if", 1),
            ("im", 1),
            ("jf", 4),
            ("jm", 1),
            ("pf", 1),
            ("pm", 2),
            ("zf", 4),
            ("zm", 4),
        ];

        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for name in voices.names() {
            let prefix = name.split('_').next().unwrap_or("").to_string();
            *counts.entry(prefix).or_default() += 1;
        }

        for (prefix, count) in expected {
            assert_eq!(
                counts.get(prefix).copied().unwrap_or(0),
                count,
                "voice prefix {prefix:?} has the wrong count"
            );
        }
        assert_eq!(counts.len(), expected.len(), "an unknown prefix appeared");
    }

    #[test]
    fn the_real_archive_has_the_shape_the_model_expects() {
        let Some(path) = real_archive() else {
            return;
        };
        let voices = Voices::load(&path).unwrap();
        let heart = voices.get("af_heart").expect("af_heart is a documented voice");

        // 510 = the model's phoneme context limit; 256 = the style input width.
        assert_eq!(heart.rows(), 510);
        assert_eq!(heart.columns(), 256);
        assert_eq!(heart.data().len(), 510 * 256);
        assert_eq!(heart.row(1).len(), 256);
        assert_eq!(heart.row(510).len(), 256);
    }

    #[test]
    fn every_voice_has_the_same_shape() {
        let Some(path) = real_archive() else {
            return;
        };
        let voices = Voices::load(&path).unwrap();
        for name in voices.names() {
            let style = voices.get(name).unwrap();
            assert_eq!(
                (style.rows(), style.columns()),
                (510, 256),
                "{name} has an unexpected shape"
            );
        }
    }

    #[test]
    fn style_values_are_finite_and_not_all_zero() {
        let Some(path) = real_archive() else {
            return;
        };
        let voices = Voices::load(&path).unwrap();
        for name in voices.names() {
            let style = voices.get(name).unwrap();
            assert!(
                style.data().iter().all(|v| v.is_finite()),
                "{name} contains a non-finite value, so the bytes were misread"
            );
            assert!(
                style.data().iter().any(|v| *v != 0.0),
                "{name} is all zeroes, which suggests the payload was not read"
            );
        }
    }

    #[test]
    fn the_documented_voice_names_are_all_present() {
        let Some(path) = real_archive() else {
            return;
        };
        let voices = Voices::load(&path).unwrap();
        for name in [
            "af_heart",
            "af_bella",
            "am_michael",
            "bf_emma",
            "bm_george",
            "zf_xiaobei",
            "jm_kumo",
            "ff_siwis",
        ] {
            assert!(voices.contains(name), "{name} is missing from the archive");
        }
        assert!(!voices.contains("af_nonexistent"));
    }
}
