//! Documents: PDF, Office XML, and images.
//!
//! Every format-specific thing the file tools need lives here, so `tools.rs`
//! stays a dispatch table and this module stays readable. Four jobs:
//!
//! - **PDF reading** — page-accurate text, and the metadata `pdf_info` reports.
//! - **PDF writing** — merge, extract, split, delete, rotate, set metadata.
//! - **Office XML** — DOCX and PPTX text with headings and tables; XLSX cells.
//! - **Images** — dimensions and format, from the header alone.
//!
//! # Why `lopdf` and not `pdf-extract`
//!
//! `pdf-extract` has better font handling, and it was the obvious companion
//! crate. It was rejected for one reason that outweighs that: this workspace
//! builds release with `panic = "abort"` (see the root `Cargo.toml`), so a parser
//! that can panic on a malformed object does not fail a tool call — it takes the
//! process down. "A model reading an arbitrary PDF out of a workspace" is
//! precisely the input that finds those paths, and a crash there would take the
//! user's whole session with it. `lopdf` is `Result`-based throughout, and
//! `extract_text_with_limit` already does the font decoding that was the only
//! thing worth having from the alternative.
//!
//! # Writing through the library's own processors
//!
//! The writers call `delete_pages`, `prune_objects`, `renumber_objects` and
//! `renumber_objects_with` rather than rearranging the page tree by hand. Editing
//! a PDF means rewriting every reference to every object that moved, and one
//! missed reference produces a file that opens in one reader and not another — a
//! bug that is invisible in a test and infuriating in use. `renumber_objects_with`
//! is documented for exactly the merge case ("multiple document object insertions
//! in a single main document"), so two id spaces cannot collide.
//!
//! # The decompression bound
//!
//! A page's content stream is compressed, and a few kilobytes can inflate without
//! limit. Every read goes through `extract_text_with_limit`, so a bomb is a failed
//! tool call rather than a consumed machine.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Largest document a tool will open, checked before anything is parsed.
pub const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// Largest PDF sent to the model as a document part rather than as text.
///
/// Its own ceiling, and lower than the read limit, because a native PDF is
/// re-sent on every turn until it falls out of the newest-document window —
/// bytes here are multiplied by the rest of the conversation.
pub const MAX_NATIVE_PDF_BYTES: u64 = 32 * 1024 * 1024;

/// Longest text any of these readers will hand back, matching
/// [`crate::attachments::MAX_TEXT_CHARS`] so one document costs the same whether
/// it arrived as an attachment or was read with a tool.
pub const MAX_DOCUMENT_CHARS: usize = 60_000;

/// Per-page decompression budget. A single page's content stream is bounded to
/// this, so a whole-document cost is `pages × this` rather than unbounded.
const PAGE_CONTENT_LIMIT: usize = 8 * 1024 * 1024;

/// Most pages one call will walk. Beyond this the answer is a refusal naming the
/// limit, because a 40,000-page PDF is a data file, not something to read.
const MAX_PAGES_WALKED: usize = 2_000;

/// Refuses a file before it is read, by size.
pub fn check_size(path: &Path, size: u64, limit: u64) -> Result<()> {
    if size > limit {
        return Err(Error::Other(format!(
            "{} is {} MB, over the {} MB limit",
            name_of(path),
            size / (1024 * 1024),
            limit / (1024 * 1024),
        )));
    }
    Ok(())
}

/// A path's file name, for a tool result.
pub fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Truncates extracted text to the shared cap, saying so.
///
/// The marker matters: a model handed a silently clipped document will
/// confidently summarise the part it never saw.
pub fn clamp(text: String) -> String {
    clamp_to(text, MAX_DOCUMENT_CHARS)
}

/// [`clamp`] with an explicit budget, for a caller that asked for less.
pub fn clamp_to(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}\n\n[... truncated at {max} characters]")
}

/* ---------------------------------------------------------------------------
   Page ranges
--------------------------------------------------------------------------- */

/// Parses `"1-5,8,12-"` into page numbers, clamped to the document.
///
/// The open-ended forms are the ones worth having: "from page 12 to the end" is
/// how a person says it, and making them count the pages first is the kind of
/// friction that produces a wrong answer instead of a second call. `8-` means
/// from eight onwards; `-3` means the first three.
pub fn parse_pages(spec: &str, total: u32) -> Result<Vec<u32>> {
    let spec = spec.trim();
    if spec.is_empty() {
        if total as usize > MAX_PAGES_WALKED {
            return Err(Error::Other(format!(
                "this document has {total} pages; give a `pages` range (e.g. \"1-20\") — \
                 reading all of it at once is limited to {MAX_PAGES_WALKED} pages"
            )));
        }
        return Ok((1..=total).collect());
    }

    let mut wanted: BTreeSet<u32> = BTreeSet::new();
    for piece in spec.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match piece.split_once('-') {
            Some((low, high)) => {
                let start = parse_page(low, 1)?;
                let end = parse_page(high, total)?;
                if start > end {
                    return Err(Error::Other(format!(
                        "`{piece}` runs backwards: {start} to {end}"
                    )));
                }
                for page in start..=end {
                    if page <= total {
                        wanted.insert(page);
                    }
                }
            }
            None => {
                wanted.insert(parse_page(piece, 0)?);
            }
        }
        if wanted.len() > MAX_PAGES_WALKED {
            return Err(Error::Other(format!(
                "that selects more than {MAX_PAGES_WALKED} pages"
            )));
        }
    }

    // Out-of-range pages are dropped rather than refused: "9-99" against a
    // ten-page document plainly means "the last two", and the caller can see
    // what came back.
    let pages: Vec<u32> = wanted
        .into_iter()
        .filter(|page| *page >= 1 && *page <= total)
        .collect();
    if pages.is_empty() {
        return Err(Error::Other(format!(
            "no pages selected — this document has {total}"
        )));
    }
    Ok(pages)
}

fn parse_page(text: &str, fallback: u32) -> Result<u32> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(fallback);
    }
    let page: u32 = text
        .parse()
        .map_err(|_| Error::Other(format!("`{text}` is not a page number")))?;
    if page == 0 {
        return Err(Error::Other("page numbers start at 1".into()));
    }
    Ok(page)
}

/* ---------------------------------------------------------------------------
   PDF reading
--------------------------------------------------------------------------- */

/// What `pdf_info` reports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfInfo {
    pub pages: usize,
    pub encrypted: bool,
    /// Whether any *sampled* page carries extractable text at all.
    pub text_layer: bool,
    /// Characters found in the sample, so a thin layer reads as thin rather
    /// than merely present.
    pub text_chars: usize,
    /// Sampled pages that yielded no text. A scanned document is all of them; a
    /// born-digital one with a cover image is exactly one.
    pub blank_pages: Vec<u32>,
    pub pages_sampled: usize,
    /// Width and height of the first page, in points.
    pub size: Option<[f32; 2]>,
    /// The `/Info` dictionary.
    pub metadata: BTreeMap<String, String>,
}

impl PdfInfo {
    /// A one-line caveat when the text layer is missing or thin, or `None` when
    /// there is nothing to warn about.
    ///
    /// This is the difference the old hand-rolled extractor could not express:
    /// "no text layer" and "one cover page with no text" are the same empty
    /// string, and they call for entirely different next steps. A model that
    /// cannot tell them apart retries the read that already failed.
    pub fn text_caveat(&self) -> Option<String> {
        if !self.text_layer {
            return Some(
                "No sampled page carries a text layer, so this PDF is very likely scanned \
                 images. Reading it as text will produce nothing; the pages have to be looked \
                 at as images, or OCR'd."
                    .to_string(),
            );
        }
        if self.blank_pages.len() == 1 {
            return Some(format!(
                "Page {} has no text (usually a cover or a full-page image); the rest do.",
                self.blank_pages[0]
            ));
        }
        if !self.blank_pages.is_empty() {
            return Some(format!(
                "{} of the {} pages sampled carry no text: {}",
                self.blank_pages.len(),
                self.pages_sampled,
                self.blank_pages
                    .iter()
                    .take(12)
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        None
    }
}

fn load_pdf(bytes: &[u8]) -> Result<lopdf::Document> {
    lopdf::Document::load_mem(bytes)
        .map_err(|error| Error::Other(format!("could not read this as a PDF: {error}")))
}

/// Page count, metadata and text-layer state.
///
/// The text layer is *sampled* rather than exhaustive: checking every page of a
/// 900-page document costs real time, and the question being answered — "is there
/// a text layer?" — is settled by the first handful either way. `pages_sampled`
/// is reported so the caveat can be honest about what it actually looked at.
pub fn pdf_info(bytes: &[u8]) -> Result<PdfInfo> {
    let doc = load_pdf(bytes)?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();

    let mut info = PdfInfo {
        pages: pages.len(),
        encrypted: doc.is_encrypted(),
        ..Default::default()
    };

    if let Some(number) = pages.first() {
        if let Some((width, height)) = page_size(&doc, *number) {
            info.size = Some([width, height]);
        }
    }
    info.metadata = pdf_metadata(&doc);

    let sample: Vec<u32> = pages.iter().copied().take(12).collect();
    info.pages_sampled = sample.len();
    for number in sample {
        let chars = page_text(&doc, number)?.trim().chars().count();
        info.text_chars += chars;
        if chars == 0 {
            info.blank_pages.push(number);
        }
    }
    info.text_layer = info.text_chars > 0;
    Ok(info)
}

fn page_size(doc: &lopdf::Document, number: u32) -> Option<(f32, f32)> {
    let id = *doc.get_pages().get(&number)?;
    let dict = doc.get_dictionary(id).ok()?;

    // `/MediaBox` is inheritable, so a page without one takes its parent's —
    // common enough that reading only the page's own dictionary would report no
    // size on a perfectly ordinary document.
    let media = match dict.get(b"MediaBox") {
        Ok(value) => value,
        Err(_) => {
            let mut node = dict;
            let mut found = None;
            for _ in 0..8 {
                let parent = node.get(b"Parent").ok()?;
                let (_, parent) = doc.dereference(parent).ok()?;
                node = parent.as_dict().ok()?;
                if let Ok(value) = node.get(b"MediaBox") {
                    found = Some(value);
                    break;
                }
            }
            found?
        }
    };

    let (_, media) = doc.dereference(media).ok()?;
    let array = media.as_array().ok()?;
    if array.len() < 4 {
        return None;
    }
    let low_x = number_of(doc, &array[0])?;
    let low_y = number_of(doc, &array[1])?;
    let high_x = number_of(doc, &array[2])?;
    let high_y = number_of(doc, &array[3])?;
    Some(((high_x - low_x).abs(), (high_y - low_y).abs()))
}

fn number_of(doc: &lopdf::Document, object: &lopdf::Object) -> Option<f32> {
    let (_, object) = doc.dereference(object).ok()?;
    match object {
        lopdf::Object::Integer(value) => Some(*value as f32),
        lopdf::Object::Real(value) => Some(*value),
        _ => None,
    }
}

fn pdf_metadata(doc: &lopdf::Document) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(info) = doc.trailer.get(b"Info") else {
        return out;
    };
    let Ok((_, info)) = doc.dereference(info) else {
        return out;
    };
    let Ok(dict) = info.as_dict() else {
        return out;
    };
    for (key, label) in [
        (b"Title".as_slice(), "Title"),
        (b"Author".as_slice(), "Author"),
        (b"Subject".as_slice(), "Subject"),
        (b"Keywords".as_slice(), "Keywords"),
        (b"Creator".as_slice(), "Creator"),
        (b"Producer".as_slice(), "Producer"),
        (b"CreationDate".as_slice(), "Created"),
        (b"ModDate".as_slice(), "Modified"),
    ] {
        if let Ok(value) = dict.get(key) {
            if let Some(text) = object_text(doc, value) {
                if !text.trim().is_empty() {
                    out.insert(label.to_string(), text.trim().to_string());
                }
            }
        }
    }
    out
}

/// A dictionary value as a string, whichever of the shapes it is.
fn object_text(doc: &lopdf::Document, object: &lopdf::Object) -> Option<String> {
    let (_, object) = doc.dereference(object).ok()?;
    match object {
        lopdf::Object::String(bytes, _) => Some(decode_string(bytes)),
        lopdf::Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        lopdf::Object::Integer(value) => Some(value.to_string()),
        lopdf::Object::Real(value) => Some(value.to_string()),
        lopdf::Object::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

/// PDF metadata strings are UTF-16BE when they start with a BOM, and
/// PDFDocEncoding otherwise.
///
/// Each byte is taken as its own code point rather than run through UTF-8. That
/// is what the spec's default encoding means for the range that matters — the
/// Latin-1 letters — and it is the difference between "Café" and "Caf?". A
/// `from_utf8_lossy` here would be wrong for exactly that reason: a Latin-1 byte
/// above 0x7F is not a malformed UTF-8 sequence, it is the character.
///
/// PDFDocEncoding differs from Latin-1 in 0x18–0x1F and 0x80–0x9F (a few
/// accented capitals and symbols). Those are rare in a title or an author name,
/// and one wrong glyph there is a great deal better than a replacement character
/// in every accented word.
fn decode_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|byte| *byte as char).collect()
}

/// One page's text, bounded.
///
/// Never fails on a page it cannot decode — an undecodable page is an empty one,
/// because a damaged page in an otherwise good document should cost that page and
/// not the whole call. Only the decompression limit is a hard error, because that
/// is the one case where continuing would be the problem.
fn page_text(doc: &lopdf::Document, number: u32) -> Result<String> {
    match doc.extract_text_with_limit(&[number], PAGE_CONTENT_LIMIT) {
        Ok(text) => Ok(text),
        Err(error) => {
            let message = error.to_string();
            if message.to_ascii_lowercase().contains("limit") {
                Err(Error::Other(format!(
                    "page {number} decompresses to more than {} MB, which is not a page anyone \
                     means to read — refusing rather than trying",
                    PAGE_CONTENT_LIMIT / (1024 * 1024)
                )))
            } else {
                Ok(String::new())
            }
        }
    }
}

/// Text for the given pages, each labelled.
///
/// Labelled per page rather than joined, because "which page says that" is the
/// question a model is usually about to be asked, and a flat blob cannot answer
/// it.
pub fn pdf_text_pages(bytes: &[u8], pages: &[u32]) -> Result<Vec<(u32, String)>> {
    let doc = load_pdf(bytes)?;
    let mut out = Vec::new();
    for number in pages {
        out.push((*number, page_text(&doc, *number)?));
    }
    Ok(out)
}

/// Every page's text, joined. The attachment path's entry point, so a PDF the
/// user attached and a PDF the model read with a tool produce identical text.
pub fn pdf_text(bytes: &[u8]) -> Result<String> {
    let doc = load_pdf(bytes)?;
    let mut out = String::new();
    let mut pages = 0usize;
    for number in doc.get_pages().keys().copied() {
        pages += 1;
        let text = page_text(&doc, number)?;
        if !text.trim().is_empty() {
            out.push_str(&format!("--- page {number} ---\n{}\n", text.trim_end()));
        }
    }
    if out.trim().is_empty() && pages > 0 {
        return Err(Error::Other(
            "no text layer: this PDF appears to be scanned images".into(),
        ));
    }
    Ok(out)
}

/* ---------------------------------------------------------------------------
   PDF writing
--------------------------------------------------------------------------- */

/// What a write did, for the tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfWriteReport {
    pub action: String,
    pub pages: usize,
    pub bytes: usize,
    /// Anything the caller should know that did not stop the write.
    pub notes: Vec<String>,
}

/// Everything `pdf_write` can do. One function so the six actions share the
/// loading, the encryption check and the saving.
pub fn pdf_write(
    action: &str,
    sources: &[Vec<u8>],
    pages: &str,
    rotation: i64,
    metadata: &BTreeMap<String, String>,
) -> Result<(Vec<u8>, PdfWriteReport)> {
    let first = sources
        .first()
        .ok_or_else(|| Error::Other("pdf_write needs at least one file".into()))?;

    let mut doc = load_pdf(first)?;
    if doc.is_encrypted() {
        return Err(Error::Other(
            "this PDF is encrypted, so it cannot be rewritten without its password".into(),
        ));
    }
    let mut notes = Vec::new();

    match action {
        "merge" => {
            if sources.len() < 2 {
                return Err(Error::Other(
                    "merge needs at least two files: the first is the one the others are \
                     added to"
                        .into(),
                ));
            }
            for extra in &sources[1..] {
                let other = load_pdf(extra)?;
                if other.is_encrypted() {
                    return Err(Error::Other(
                        "one of the PDFs being merged is encrypted".into(),
                    ));
                }
                append_document(&mut doc, other)?;
            }
        }
        // Extract and delete are one operation from opposite ends: name the
        // pages to drop and let the library rebuild the tree.
        "extract" | "split" => {
            let total = doc.get_pages().len() as u32;
            let keep = parse_pages(pages, total)?;
            let drop: Vec<u32> = (1..=total).filter(|page| !keep.contains(page)).collect();
            if !drop.is_empty() {
                doc.delete_pages(&drop);
                doc.prune_objects();
                doc.renumber_objects();
            }
        }
        "delete_pages" => {
            let total = doc.get_pages().len() as u32;
            let drop = parse_pages(pages, total)?;
            if drop.len() as u32 >= total {
                return Err(Error::Other(
                    "that would delete every page; use a range that leaves something behind"
                        .into(),
                ));
            }
            doc.delete_pages(&drop);
            doc.prune_objects();
            doc.renumber_objects();
        }
        "rotate" => {
            if !matches!(rotation, 0 | 90 | 180 | 270) {
                return Err(Error::Other("rotation must be 0, 90, 180 or 270".into()));
            }
            let total = doc.get_pages().len() as u32;
            let targets = parse_pages(pages, total)?;
            rotate_pages(&mut doc, &targets, rotation);
        }
        "set_metadata" => {
            if metadata.is_empty() {
                return Err(Error::Other(
                    "set_metadata needs at least one key in `metadata`".into(),
                ));
            }
            set_metadata(&mut doc, metadata)?;
            notes.push(SIGNATURE_NOTE.to_string());
        }
        other => {
            return Err(Error::Other(format!(
                "`{other}` is not a pdf_write action; use merge, extract, split, \
                 delete_pages, rotate or set_metadata"
            )))
        }
    }

    if !metadata.is_empty() && action != "set_metadata" {
        set_metadata(&mut doc, metadata)?;
    }

    let page_count = doc.get_pages().len();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|error| Error::Other(format!("could not write the PDF: {error}")))?;
    // Read before the move, so the report and the bytes can both be returned.
    let written = bytes.len();

    Ok((
        bytes,
        PdfWriteReport {
            action: action.to_string(),
            pages: page_count,
            bytes: written,
            notes,
        },
    ))
}

/// Said whenever a document is rewritten. A PDF's signature covers its bytes, so
/// any edit — even adding a title — invalidates it, and a user who signed a
/// contract should be told rather than find out later.
const SIGNATURE_NOTE: &str = "Rewriting a PDF invalidates any digital signature it carried.";

/// Merges `other` into `target`.
///
/// The second document's object ids are moved above the first's and every
/// reference remapped, which is what `renumber_objects_with` is documented for.
/// Doing it by hand would mean rewriting the reference graph myself, and a single
/// missed reference is a file that opens in one reader and not in another.
fn append_document(target: &mut lopdf::Document, mut other: lopdf::Document) -> Result<()> {
    let offset = target.max_id.saturating_add(1);
    other.renumber_objects_with(offset);

    let added: Vec<lopdf::ObjectId> = other.get_pages().values().copied().collect();
    if added.is_empty() {
        return Ok(());
    }

    for (id, object) in other.objects {
        target.objects.insert(id, object);
    }
    target.max_id = target.objects.keys().map(|id| id.0).max().unwrap_or(0);

    let pages_id = pages_id_of(target)?;

    // Append rather than rebuild: every page already in the document keeps the
    // object number and the `/Parent` it had, so nothing that referred to them
    // needs touching.
    let mut kids = target
        .get_dictionary(pages_id)
        .ok()
        .and_then(|dict| dict.get(b"Kids").ok().and_then(|kids| kids.as_array().ok()))
        .cloned()
        .unwrap_or_default();

    for id in &added {
        if let Ok(dict) = target.get_dictionary_mut(*id) {
            dict.set("Parent", lopdf::Object::Reference(pages_id));
        }
        kids.push(lopdf::Object::Reference(*id));
    }

    let count = kids.len() as i64;
    let dict = target
        .get_dictionary_mut(pages_id)
        .map_err(|_| Error::Other("this PDF's page tree is unreadable".into()))?;
    dict.set("Kids", lopdf::Object::Array(kids));
    dict.set("Count", lopdf::Object::Integer(count));
    Ok(())
}

/// The `/Pages` object id, from the trailer's catalog.
fn pages_id_of(doc: &lopdf::Document) -> Result<lopdf::ObjectId> {
    let root = doc
        .trailer
        .get(b"Root")
        .map_err(|_| Error::Other("this PDF has no catalog".into()))?;
    let (_, root) = doc
        .dereference(root)
        .map_err(|_| Error::Other("this PDF's catalog is unreadable".into()))?;
    let root = root
        .as_dict()
        .map_err(|_| Error::Other("this PDF's catalog is not a dictionary".into()))?;
    match root.get(b"Pages") {
        Ok(lopdf::Object::Reference(id)) => Ok(*id),
        _ => Err(Error::Other("this PDF has no page tree".into())),
    }
}

fn rotate_pages(doc: &mut lopdf::Document, pages: &[u32], rotation: i64) {
    let ids: Vec<lopdf::ObjectId> = doc
        .get_pages()
        .into_iter()
        .filter(|(number, _)| pages.contains(number))
        .map(|(_, id)| id)
        .collect();
    for id in ids {
        if let Ok(dict) = doc.get_dictionary_mut(id) {
            dict.set("Rotate", lopdf::Object::Integer(rotation));
        }
    }
}

fn set_metadata(doc: &mut lopdf::Document, metadata: &BTreeMap<String, String>) -> Result<()> {
    // Write into the existing `/Info` when there is one, so the fields the caller
    // did not name survive the edit.
    let info_id = match doc.trailer.get(b"Info") {
        Ok(lopdf::Object::Reference(id)) => *id,
        _ => {
            let id = (doc.max_id.saturating_add(1), 0);
            doc.objects
                .insert(id, lopdf::Object::Dictionary(lopdf::Dictionary::new()));
            doc.trailer.set("Info", lopdf::Object::Reference(id));
            doc.max_id = id.0;
            id
        }
    };

    let dict = doc
        .get_dictionary_mut(info_id)
        .map_err(|_| Error::Other("this PDF's info dictionary is unreadable".into()))?;
    for (key, value) in metadata {
        let canonical = match key.to_ascii_lowercase().as_str() {
            "title" => "Title",
            "author" => "Author",
            "subject" => "Subject",
            "keywords" => "Keywords",
            "creator" => "Creator",
            "producer" => "Producer",
            other => {
                return Err(Error::Other(format!(
                    "`{other}` is not a metadata field; use title, author, subject, \
                     keywords, creator or producer"
                )))
            }
        };
        // UTF-16BE with a BOM is how a PDF carries a non-Latin-1 title without
        // guessing an encoding.
        let mut bytes = vec![0xFE, 0xFF];
        for unit in value.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        dict.set(
            canonical.as_bytes().to_vec(),
            lopdf::Object::String(bytes, lopdf::StringFormat::Literal),
        );
    }
    Ok(())
}

/* ---------------------------------------------------------------------------
   Office XML
--------------------------------------------------------------------------- */

/// A document as blocks, so a report reads as a document rather than as a word
/// salad.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentText {
    pub format: String,
    pub blocks: Vec<Block>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// `heading`, `paragraph`, `table` or `sheet`.
    pub kind: String,
    pub text: String,
    /// Heading level, for `heading`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
}

impl DocumentText {
    /// Markdown-ish, which is what a model reads best and what the transcript
    /// renders.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            match block.kind.as_str() {
                "heading" => {
                    let hashes = "#".repeat(block.level.unwrap_or(1).clamp(1, 6) as usize);
                    out.push_str(&format!("{hashes} {}\n\n", block.text));
                }
                _ => out.push_str(&format!("{}\n\n", block.text)),
            }
        }
        for note in &self.notes {
            out.push_str(&format!("[{note}]\n"));
        }
        out.trim_end().to_string()
    }
}

/// Reads a DOCX, PPTX or XLSX.
pub fn ooxml_text(bytes: &[u8], format: &str) -> Result<DocumentText> {
    match format {
        "xlsx" | "xls" | "xlsm" | "ods" => spreadsheet_text(bytes),
        "docx" => word_text(bytes),
        "pptx" => slides_text(bytes),
        other => Err(Error::Other(format!(
            "`{other}` is not a format read_document knows; use docx, pptx or xlsx"
        ))),
    }
}

/// The format a file name implies, when the caller did not say.
pub fn ooxml_format(name: &str) -> Option<&'static str> {
    let extension = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "docx" => Some("docx"),
        "pptx" => Some("pptx"),
        "xlsx" | "xlsm" | "xls" | "ods" => Some("xlsx"),
        _ => None,
    }
}

fn open_zip(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>> {
    zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Error::Other(format!("could not open this as an Office file: {error}")))
}

fn read_entry(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String> {
    use std::io::Read;
    let mut file = archive
        .by_name(name)
        .map_err(|_| Error::Other(format!("this document has no {name}")))?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| Error::Other(format!("{name} is not readable text: {error}")))?;
    Ok(text)
}

/// Empties the buffer into the document, as a heading when a style said so.
fn flush_paragraph(out: &mut DocumentText, buffer: &mut String, level: Option<u8>) {
    let text = buffer.trim().to_string();
    buffer.clear();
    if text.is_empty() {
        return;
    }
    out.blocks.push(match level {
        Some(level) => Block {
            kind: "heading".into(),
            text,
            level: Some(level),
        },
        None => Block {
            kind: "paragraph".into(),
            text,
            level: None,
        },
    });
}

/// DOCX: `word/document.xml`, walked for paragraphs, headings and tables.
fn word_text(bytes: &[u8]) -> Result<DocumentText> {
    let mut archive = open_zip(bytes)?;
    let xml = read_entry(&mut archive, "word/document.xml")?;

    let mut out = DocumentText {
        format: "docx".into(),
        blocks: Vec::new(),
        notes: Vec::new(),
    };

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(false);

    // One buffer, because a Word run, a paragraph and a table cell are nested
    // rather than separate streams of text: what `</w:p>` does with the buffer
    // depends on whether a cell is open around it.
    let mut buffer = String::new();
    let mut level: Option<u8> = None;
    let mut in_text = false;
    let mut table_rows: Vec<String> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut in_table = false;

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(element)) => {
                // `LocalName` derefs to `str` in quick-xml 0.42, so these arms
                // are string literals rather than byte strings.
                match element.local_name().as_ref() {
                    "t" => in_text = true,
                    "tbl" => {
                        in_table = true;
                        table_rows.clear();
                    }
                    "tr" => row.clear(),
                    // A paragraph style is where Word records "this is Heading 2".
                    "pStyle" => {
                        if let Some(style) = attribute(&element, "val") {
                            level = heading_level(&style);
                        }
                    }
                    _ => {}
                }
            }
            // A self-closing tag is `Empty`, not `Start`, and `<w:pStyle
            // w:val="Heading1"/>` is exactly that. Missing this arm is why a
            // heading used to read as an ordinary paragraph.
            Ok(quick_xml::events::Event::Empty(element)) => {
                if element.local_name().as_ref() == "pStyle" {
                    if let Some(style) = attribute(&element, "val") {
                        level = heading_level(&style);
                    }
                }
            }
            Ok(quick_xml::events::Event::End(element)) => match element.local_name().as_ref() {
                "t" => in_text = false,
                "p" => {
                    if in_table {
                        // A cell can hold several paragraphs; they run together
                        // with a space rather than losing all but the last.
                        if !buffer.trim().is_empty() {
                            buffer.push(' ');
                        }
                    } else {
                        flush_paragraph(&mut out, &mut buffer, level);
                        level = None;
                    }
                }
                "tc" => {
                    row.push(buffer.trim().to_string());
                    buffer.clear();
                    level = None;
                }
                "tr" => {
                    if !row.is_empty() {
                        table_rows.push(format!("| {} |", row.join(" | ")));
                    }
                    row.clear();
                }
                "tbl" => {
                    if !table_rows.is_empty() {
                        out.blocks.push(Block {
                            kind: "table".into(),
                            text: table_rows.join("\n"),
                            level: None,
                        });
                    }
                    in_table = false;
                    table_rows.clear();
                    buffer.clear();
                }
                _ => {}
            },
            Ok(quick_xml::events::Event::Text(text)) => {
                if in_text {
                    // `xml10_content` is the 0.42 replacement for `unescape`: it
                    // resolves entities and applies XML 1.0 end-of-line
                    // normalisation. 1.0 is right here because both DOCX and
                    // PPTX are XML 1.0 documents.
                    buffer.push_str(&text.xml10_content());
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => {
                // Reported, not fatal: half a contract is worth more than none.
                out.notes
                    .push(format!("stopped early: the document XML is malformed ({error})"));
                break;
            }
            _ => {}
        }
    }

    if out.blocks.is_empty() {
        out.notes.push("no text found in this document".into());
    }
    Ok(out)
}

/// PPTX: one block per slide, in slide order rather than archive order.
fn slides_text(bytes: &[u8]) -> Result<DocumentText> {
    let mut archive = open_zip(bytes)?;
    let names: Vec<String> = (0..archive.len())
        .filter_map(|index| archive.by_index(index).ok().map(|f| f.name().to_string()))
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .collect();

    let mut ordered: Vec<(u32, String)> = names
        .into_iter()
        .filter_map(|name| {
            let number: u32 = name
                .trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse()
                .ok()?;
            Some((number, name))
        })
        .collect();
    // Zip entries arrive in whatever order the writer chose, which is not slide
    // order and frequently not even numeric order.
    ordered.sort_by_key(|(number, _)| *number);

    let mut out = DocumentText {
        format: "pptx".into(),
        blocks: Vec::new(),
        notes: Vec::new(),
    };
    if ordered.is_empty() {
        out.notes.push("this presentation has no slides".into());
        return Ok(out);
    }

    for (number, name) in ordered {
        let xml = read_entry(&mut archive, &name)?;
        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut text = String::new();
        let mut in_text = false;
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(element)) => {
                    if element.local_name().as_ref() == "t" {
                        in_text = true;
                    }
                }
                Ok(quick_xml::events::Event::End(element)) => {
                    if element.local_name().as_ref() == "t" {
                        in_text = false;
                        // A run boundary is a word boundary in a text box.
                        text.push(' ');
                    }
                }
                Ok(quick_xml::events::Event::Text(chunk)) => {
                    if in_text {
                        text.push_str(&chunk.xml10_content());
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }
        out.blocks.push(Block {
            kind: "paragraph".into(),
            text: format!(
                "Slide {number}: {}",
                text.split_whitespace().collect::<Vec<_>>().join(" ")
            ),
            level: None,
        });
    }
    Ok(out)
}

/// XLSX and friends, through `calamine`.
///
/// Shared strings, cell typing, number formats and merged ranges are the hard
/// half of OOXML, and none of it is worth re-implementing — it is the same class
/// of work that got the old hand-rolled PDF extractor into trouble, on input that
/// is merely unforgiving rather than adversarial.
fn spreadsheet_text(bytes: &[u8]) -> Result<DocumentText> {
    use calamine::Reader;

    let mut workbook = calamine::open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|error| Error::Other(format!("could not read this spreadsheet: {error}")))?;

    let mut out = DocumentText {
        format: "xlsx".into(),
        blocks: Vec::new(),
        notes: Vec::new(),
    };

    let names = workbook.sheet_names().to_vec();
    if names.is_empty() {
        out.notes.push("this workbook has no sheets".into());
        return Ok(out);
    }

    for name in names {
        let Ok(range) = workbook.worksheet_range(&name) else {
            out.notes.push(format!("sheet `{name}` could not be read"));
            continue;
        };
        let mut lines: Vec<String> = Vec::new();
        for row in range.rows() {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| cell.to_string().trim().to_string())
                .collect();
            // A spreadsheet usually stores a used range wider than its content,
            // so trailing empty cells are dropped: the grid stays honest about
            // where the data actually ends.
            let Some(width) = cells.iter().rposition(|cell| !cell.is_empty()) else {
                continue;
            };
            lines.push(format!("| {} |", cells[..=width].join(" | ")));
        }
        if lines.is_empty() {
            out.notes.push(format!("sheet `{name}` is empty"));
            continue;
        }
        out.blocks.push(Block {
            kind: "sheet".into(),
            text: format!("Sheet: {name}\n{}", lines.join("\n")),
            level: None,
        });
    }
    Ok(out)
}

/// One attribute's value, by local name.
///
/// Namespaced keys carry a prefix the caller never wrote, so matching the local
/// name is what finds `w:val` from `val`.
fn attribute(element: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == name)
        // `normalized_value` is the current spelling of the deprecated
        // `unescape_value`. The version argument only affects end-of-line
        // handling *inside* an attribute, and `Implicit1_0` is the spec's own
        // default for a document whose declaration has not been read.
        .and_then(|attribute| {
            attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned())
        })
}

/// `Heading1` -> 1, `heading 2` -> 2, `Title` -> 1, anything else -> None.
fn heading_level(style: &str) -> Option<u8> {
    let lower = style.to_ascii_lowercase();
    if lower == "title" {
        return Some(1);
    }
    let digits: String = lower
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse::<u8>()
        .ok()
        .map(|level| level.clamp(1, 6))
        .or_else(|| lower.starts_with("heading").then_some(1))
}

/* ---------------------------------------------------------------------------
   Images
--------------------------------------------------------------------------- */

/// What `image_info` reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMeta {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// Whether the format can carry transparency. A property of the format, not
    /// of this file — reading the actual channel count would mean decoding it.
    pub can_have_alpha: bool,
}

pub fn image_meta(path: &Path, bytes: &[u8]) -> Result<ImageMeta> {
    let format = image::guess_format(bytes)
        .map_err(|error| Error::Other(format!("not a recognisable image: {error}")))?;

    // The file first, then the bytes: `image_dimensions` is the cheap path and
    // works for anything on disk, and the reader covers a header that needs the
    // full stream (a progressive JPEG, anything with a trailing directory).
    let dimensions = match image::image_dimensions(path) {
        Ok(size) => Some(size),
        Err(_) => image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|reader| reader.into_dimensions().ok()),
    };
    let (width, height) = dimensions.ok_or_else(|| {
        Error::Other(format!("could not read the image header of {}", name_of(path)))
    })?;

    Ok(ImageMeta {
        format: format!("{format:?}").to_ascii_lowercase(),
        width,
        height,
        bytes: bytes.len() as u64,
        can_have_alpha: matches!(
            format,
            image::ImageFormat::Png
                | image::ImageFormat::Gif
                | image::ImageFormat::WebP
                | image::ImageFormat::Tiff
        ),
    })
}

/* ---------------------------------------------------------------------------
   Fixtures, shared with the tool layer's tests
--------------------------------------------------------------------------- */

/// Constructing a valid PDF or zip by hand is the only way to test a reader
/// against bytes it did not itself produce, and two modules need to do it.
#[cfg(test)]
pub(crate) mod test_support {
    use std::io::{Cursor, Write};

    /// Builds a small but structurally real PDF with one text run per page.
    ///
    /// Hand-written rather than produced by `lopdf` so the fixture exercises the
    /// *reader* against bytes it did not itself generate: a document built and
    /// read by the same library would pass even if the object layout were wrong.
    pub(crate) fn sample_pdf(pages: &[&str]) -> Vec<u8> {
        let mut objects: Vec<String> = Vec::new();
        let count = pages.len();
        let font_id = 3 + count * 2;

        objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
        let kids: Vec<String> = (0..count)
            .map(|index| format!("{} 0 R", 3 + index * 2))
            .collect();
        objects.push(format!(
            "<< /Type /Pages /Kids [{}] /Count {count} >>",
            kids.join(" ")
        ));

        for (index, text) in pages.iter().enumerate() {
            let content_id = 4 + index * 2;
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
                 /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {content_id} 0 R >>"
            ));
            let stream = if text.is_empty() {
                String::new()
            } else {
                format!("BT /F1 12 Tf 20 100 Td ({text}) Tj ET")
            };
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ));
        }
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());

        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (index, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", index + 1));
        }
        let xref = out.len();
        out.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
        out.push_str("0000000000 65535 f \n");
        for offset in &offsets {
            out.push_str(&format!("{offset:010} 00000 n \n"));
        }
        out.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));
        out.into_bytes()
    }

    /// A minimal zipped container holding just the parts named.
    pub(crate) fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for (name, body) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(body.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    /// The smallest thing that reads as a DOCX.
    pub(crate) fn docx_bytes(document_xml: &str) -> Vec<u8> {
        zip_bytes(&[("word/document.xml", document_xml)])
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    #[test]
    fn page_ranges_parse_the_ways_people_write_them() {
        assert_eq!(parse_pages("", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_pages("2", 5).unwrap(), vec![2]);
        assert_eq!(parse_pages("1-3", 10).unwrap(), vec![1, 2, 3]);
        // A trailing dash means "to the end", which is how anyone would say it.
        assert_eq!(parse_pages("8-", 10).unwrap(), vec![8, 9, 10]);
        assert_eq!(parse_pages("-3", 10).unwrap(), vec![1, 2, 3]);
        // Overlaps collapse, and the order is the document's rather than the
        // caller's.
        assert_eq!(parse_pages("3,1-2,2", 10).unwrap(), vec![1, 2, 3]);
        // Out-of-range pages are dropped rather than refused.
        assert_eq!(parse_pages("9-99", 10).unwrap(), vec![9, 10]);
    }

    #[test]
    fn page_ranges_refuse_the_shapes_that_are_a_mistake() {
        assert!(parse_pages("0", 5).is_err());
        assert!(parse_pages("5-2", 5).is_err());
        assert!(parse_pages("abc", 5).is_err());
        assert!(parse_pages("90-99", 5).is_err());
        // An unbounded read of a huge document is refused, with advice.
        let error = parse_pages("", 5000).unwrap_err().to_string();
        assert!(error.contains("pages"), "{error}");
    }

    #[test]
    fn heading_levels_read_the_styles_word_actually_writes() {
        assert_eq!(heading_level("Heading1"), Some(1));
        assert_eq!(heading_level("heading 3"), Some(3));
        assert_eq!(heading_level("Title"), Some(1));
        assert_eq!(heading_level("Normal"), None);
        assert_eq!(heading_level("Quote"), None);
        // A wild level is clamped rather than trusted.
        assert_eq!(heading_level("Heading99"), Some(6));
    }

    #[test]
    fn metadata_strings_decode_both_encodings() {
        // PDFDocEncoding, which is Latin-1 for everything we care about.
        assert_eq!(decode_string(b"Caf\xe9"), "Café");
        // UTF-16BE with a BOM, which is how a PDF carries a non-Latin-1 title.
        assert_eq!(decode_string(&[0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42]), "AB");
    }

    #[test]
    fn a_pdf_round_trips_through_the_reader() {
        let bytes = sample_pdf(&["First page", "Second page"]);
        let info = pdf_info(&bytes).unwrap();
        assert_eq!(info.pages, 2);
        assert!(info.text_layer, "the sample has a text layer");
        assert!(info.blank_pages.is_empty());
        assert!(info.text_caveat().is_none());

        let text = pdf_text(&bytes).unwrap();
        assert!(text.contains("First page"), "{text}");
        assert!(text.contains("page 2"), "{text}");
    }

    #[test]
    fn a_blank_pdf_says_it_is_scanned_rather_than_returning_nothing() {
        let bytes = sample_pdf(&["", ""]);
        let info = pdf_info(&bytes).unwrap();
        assert!(!info.text_layer);
        let caveat = info.text_caveat().unwrap();
        assert!(caveat.contains("scanned"), "{caveat}");

        // And the text reader says the same thing in its own words.
        let error = pdf_text(&bytes).unwrap_err().to_string();
        assert!(error.contains("scanned"), "{error}");
    }

    #[test]
    fn extract_keeps_the_pages_it_was_asked_for() {
        let bytes = sample_pdf(&["Alpha", "Beta", "Gamma"]);
        let (out, report) = pdf_write("extract", &[bytes], "2", 0, &BTreeMap::new()).unwrap();
        assert_eq!(report.pages, 1);

        let doc = load_pdf(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        // Renumbered, so the surviving page is page 1 of the new document.
        let text = page_text(&doc, 1).unwrap();
        assert!(text.contains("Beta"), "{text}");
        assert!(!text.contains("Alpha"), "{text}");
    }

    #[test]
    fn a_range_extracts_several_pages_in_document_order() {
        let bytes = sample_pdf(&["Alpha", "Beta", "Gamma", "Delta"]);
        let (out, report) = pdf_write("extract", &[bytes], "1,4", 0, &BTreeMap::new()).unwrap();
        assert_eq!(report.pages, 2);
        let text = pdf_text(&out).unwrap();
        assert!(text.contains("Alpha"), "{text}");
        assert!(text.contains("Delta"), "{text}");
    }

    #[test]
    fn delete_removes_pages_and_refuses_to_remove_all_of_them() {
        let bytes = sample_pdf(&["Alpha", "Beta", "Gamma"]);
        let (out, report) =
            pdf_write("delete_pages", &[bytes.clone()], "2", 0, &BTreeMap::new()).unwrap();
        assert_eq!(report.pages, 2);

        let doc = load_pdf(&out).unwrap();
        assert!(page_text(&doc, 1).unwrap().contains("Alpha"));
        assert!(page_text(&doc, 2).unwrap().contains("Gamma"));

        let error = pdf_write("delete_pages", &[bytes], "1-3", 0, &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("every page"), "{error}");
    }

    #[test]
    fn merge_puts_both_documents_in_one_page_tree() {
        let first = sample_pdf(&["Alpha", "Beta"]);
        let second = sample_pdf(&["Gamma"]);
        let (out, report) =
            pdf_write("merge", &[first, second], "", 0, &BTreeMap::new()).unwrap();
        assert_eq!(report.pages, 3);

        // Every page is reachable from the catalog, which is exactly what a
        // mishandled object-id collision breaks.
        let doc = load_pdf(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
        let text = pdf_text(&out).unwrap();
        for word in ["Alpha", "Beta", "Gamma"] {
            assert!(text.contains(word), "{word} missing from {text}");
        }
    }

    #[test]
    fn merge_refuses_a_single_file() {
        let bytes = sample_pdf(&["Alpha"]);
        let error = pdf_write("merge", &[bytes], "", 0, &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("at least two"), "{error}");
    }

    #[test]
    fn rotate_sets_the_angle_on_the_pages_named() {
        let bytes = sample_pdf(&["Alpha", "Beta"]);
        let (out, _) = pdf_write("rotate", &[bytes], "1", 90, &BTreeMap::new()).unwrap();
        let doc = load_pdf(&out).unwrap();
        let pages = doc.get_pages();
        let first = doc.get_dictionary(*pages.get(&1).unwrap()).unwrap();
        assert_eq!(first.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
        let second = doc.get_dictionary(*pages.get(&2).unwrap()).unwrap();
        assert!(second.get(b"Rotate").is_err(), "the second page was not rotated");
    }

    #[test]
    fn metadata_is_written_where_pdf_info_reads_it_back() {
        let bytes = sample_pdf(&["Alpha"]);
        let mut meta = BTreeMap::new();
        meta.insert("title".to_string(), "A Title — with punctuation".to_string());
        meta.insert("author".to_string(), "Elliot".to_string());

        let (out, _) = pdf_write("set_metadata", &[bytes], "", 0, &meta).unwrap();
        let info = pdf_info(&out).unwrap();
        assert_eq!(
            info.metadata.get("Title").unwrap(),
            "A Title — with punctuation"
        );
        assert_eq!(info.metadata.get("Author").unwrap(), "Elliot");
    }

    #[test]
    fn set_metadata_warns_that_it_breaks_a_signature() {
        let bytes = sample_pdf(&["Alpha"]);
        let mut meta = BTreeMap::new();
        meta.insert("title".to_string(), "x".to_string());
        let (_, report) = pdf_write("set_metadata", &[bytes], "", 0, &meta).unwrap();
        assert!(
            report.notes.iter().any(|note| note.contains("signature")),
            "{:?}",
            report.notes
        );
    }

    #[test]
    fn an_unknown_metadata_field_names_the_real_ones() {
        let bytes = sample_pdf(&["Alpha"]);
        let mut meta = BTreeMap::new();
        meta.insert("colour".to_string(), "blue".to_string());
        let error = pdf_write("set_metadata", &[bytes], "", 0, &meta)
            .unwrap_err()
            .to_string();
        assert!(error.contains("title"), "{error}");
        assert!(error.contains("author"), "{error}");
    }

    #[test]
    fn an_unknown_action_names_the_real_ones() {
        let bytes = sample_pdf(&["Alpha"]);
        let error = pdf_write("staple", &[bytes], "", 0, &BTreeMap::new())
            .unwrap_err()
            .to_string();
        for action in ["merge", "extract", "delete_pages", "rotate", "set_metadata"] {
            assert!(error.contains(action), "{error} should mention {action}");
        }
    }

    #[test]
    fn bad_rotation_is_refused_rather_than_written() {
        let bytes = sample_pdf(&["Alpha"]);
        let error = pdf_write("rotate", &[bytes], "1", 45, &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("0, 90, 180 or 270"), "{error}");
    }

    #[test]
    fn a_pdf_that_is_not_a_pdf_fails_with_a_readable_message() {
        let error = pdf_info(b"this is not a PDF at all").unwrap_err().to_string();
        assert!(error.contains("PDF"), "{error}");
    }

    #[test]
    fn size_checks_refuse_before_reading() {
        assert!(check_size(Path::new("big.pdf"), 100, MAX_DOCUMENT_BYTES).is_ok());
        let error = check_size(
            Path::new("big.pdf"),
            MAX_DOCUMENT_BYTES + 1,
            MAX_DOCUMENT_BYTES,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("big.pdf"), "{error}");
        assert!(error.contains("limit"), "{error}");
    }

    #[test]
    fn clamping_says_that_it_clamped() {
        let short = "hello".to_string();
        assert_eq!(clamp(short.clone()), short);

        let clamped = clamp("x".repeat(MAX_DOCUMENT_CHARS + 10));
        assert!(clamped.contains("truncated"), "a silent clip is a lie");
        assert!(clamped.chars().count() > MAX_DOCUMENT_CHARS);
    }

    #[test]
    fn ooxml_formats_come_from_the_extension() {
        assert_eq!(ooxml_format("report.docx"), Some("docx"));
        assert_eq!(ooxml_format("deck.PPTX"), Some("pptx"));
        assert_eq!(ooxml_format("sheet.xlsx"), Some("xlsx"));
        // `.xls` and `.ods` are read by the same path, since calamine takes both.
        assert_eq!(ooxml_format("old.xls"), Some("xlsx"));
        assert_eq!(ooxml_format("notes.md"), None);
    }

    #[test]
    fn a_docx_with_headings_and_a_table_reads_as_blocks() {
        let bytes = docx_bytes(
            r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title Here</w:t></w:r></w:p>
    <w:p><w:r><w:t>An ordinary paragraph.</w:t></w:r></w:p>
    <w:tbl>
      <w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr>
    </w:tbl>
  </w:body>
</w:document>"#,
        );

        let doc = ooxml_text(&bytes, "docx").unwrap();
        let rendered = doc.render();
        assert!(rendered.contains("# Title Here"), "{rendered}");
        assert!(rendered.contains("An ordinary paragraph."), "{rendered}");
        assert!(rendered.contains("| A | B |"), "{rendered}");
        assert!(
            doc.blocks.iter().any(|b| b.kind == "table"),
            "{:?}",
            doc.blocks
        );
    }

    #[test]
    fn a_pptx_reads_its_slides_in_slide_order() {
        let slide = |text: &str| {
            format!(
                r#"<?xml version="1.0"?>
<a:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <a:cSld><a:spTree><a:sp><a:txBody><a:p>
    <a:r><a:t>{text}</a:t></a:r>
  </a:p></a:txBody></a:sp></a:spTree></a:cSld>
</a:sld>"#
            )
        };
        // Deliberately written out of order, because zip entries are stored in
        // whatever order the writer felt like and slides must still come back in
        // slide order.
        let bytes = zip_bytes(&[
            ("ppt/slides/slide2.xml", &slide("Second slide")),
            ("ppt/slides/slide1.xml", &slide("First slide")),
        ]);

        let doc = ooxml_text(&bytes, "pptx").unwrap();
        let rendered = doc.render();
        let first = rendered.find("First slide").expect("slide one present");
        let second = rendered.find("Second slide").expect("slide two present");
        assert!(first < second, "slides came back out of order: {rendered}");
        assert!(rendered.contains("Slide 1"), "{rendered}");
    }

    #[test]
    fn a_docx_that_is_not_a_zip_says_so() {
        let error = ooxml_text(b"not a zip", "docx").unwrap_err().to_string();
        assert!(error.contains("Office file"), "{error}");
    }

    #[test]
    fn an_unknown_document_format_names_the_real_ones() {
        let error = ooxml_text(b"", "rtf").unwrap_err().to_string();
        assert!(error.contains("docx"), "{error}");
        assert!(error.contains("xlsx"), "{error}");
    }

    #[test]
    fn image_metadata_reads_a_png_that_was_just_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shot.png");
        image::RgbaImage::new(37, 19).save(&path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let meta = image_meta(&path, &bytes).unwrap();
        assert_eq!(meta.width, 37);
        assert_eq!(meta.height, 19);
        assert_eq!(meta.format, "png");
        assert!(meta.can_have_alpha, "png carries an alpha channel");
    }

    #[test]
    fn image_metadata_refuses_something_that_is_not_an_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "just text").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let error = image_meta(&path, &bytes).unwrap_err().to_string();
        assert!(error.contains("image"), "{error}");
    }
}
