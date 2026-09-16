//! File attachments: images, text/code files, and text-layer PDFs.
//!
//! Files are copied into `~/.loom/attachments/<session>/` so a chat keeps
//! working even if the original moves. Text and PDF content is inlined into
//! the prompt; images go as base64 content blocks for vision models.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{paths, Error, Result};

/// Largest file we will inline as text (characters).
pub(crate) const MAX_TEXT_CHARS: usize = 60_000;
/// Largest image we will send to a provider.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Text,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub kind: AttachmentKind,
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// Absolute path inside the Loom home directory.
    pub path: String,
    /// Extracted text for `Text`/`Pdf` kinds (filled when sending).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub text: Option<String>,
    /// Automatic screenshots are context for the model, not content the user
    /// attached: the transcript shows a small tag instead of the image.
    #[serde(skip_serializing_if = "is_false", default)]
    pub hidden: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn mime_for(name: &str) -> &'static str {
    let extension = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "md" | "markdown" => "text/markdown",
        "rs" => "text/rust",
        "ts" | "tsx" => "text/typescript",
        "js" | "jsx" | "mjs" => "text/javascript",
        "py" => "text/x-python",
        "toml" => "text/toml",
        "yaml" | "yml" => "text/yaml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "sql" => "text/sql",
        _ => "text/plain",
    }
}

pub fn kind_for(mime: &str) -> AttachmentKind {
    if mime.starts_with("image/") {
        AttachmentKind::Image
    } else if mime == "application/pdf" {
        AttachmentKind::Pdf
    } else {
        AttachmentKind::Text
    }
}

/// Copies a file into the session's attachment directory and classifies it.
pub fn store(session_id: &str, source: &Path) -> Result<Attachment> {
    let root = paths::loom_home()?.join("attachments");
    store_in(&root, session_id, source)
}

/// Path-explicit variant (used by tests and portable installs).
pub fn store_in(root: &Path, session_id: &str, source: &Path) -> Result<Attachment> {
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| Error::Other("attachment has no file name".into()))?;

    let metadata = std::fs::metadata(source).map_err(|e| Error::io(source, e))?;
    if !metadata.is_file() {
        return Err(Error::Other(format!("\"{name}\" is not a file")));
    }

    let mime = mime_for(&name).to_string();
    let kind = kind_for(&mime);
    if kind == AttachmentKind::Image && metadata.len() > MAX_IMAGE_BYTES {
        return Err(Error::Other(format!(
            "\"{name}\" is larger than {} MB",
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let directory = attachments_dir_in(root, session_id);
    std::fs::create_dir_all(&directory).map_err(|e| Error::io(&directory, e))?;
    let target = directory.join(format!("{id}-{name}"));
    std::fs::copy(source, &target).map_err(|e| Error::io(source, e))?;

    Ok(Attachment {
        id,
        kind,
        name,
        mime,
        size: metadata.len(),
        path: target.to_string_lossy().into_owned(),
        text: None,
        hidden: false,
    })
}

/// Saves pasted bytes (clipboard image) as an attachment.
pub fn store_bytes(session_id: &str, name: &str, bytes: &[u8]) -> Result<Attachment> {
    let root = paths::loom_home()?.join("attachments");
    store_bytes_in(&root, session_id, name, bytes)
}

/// Path-explicit variant (used by tests and portable installs).
pub fn store_bytes_in(
    root: &Path,
    session_id: &str,
    name: &str,
    bytes: &[u8],
) -> Result<Attachment> {
    if bytes.is_empty() {
        return Err(Error::Other("pasted image was empty".into()));
    }
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(Error::Other("pasted image is too large".into()));
    }

    let mime = mime_for(name).to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let directory = attachments_dir_in(root, session_id);
    std::fs::create_dir_all(&directory).map_err(|e| Error::io(&directory, e))?;
    let target = directory.join(format!("{id}-{name}"));
    std::fs::write(&target, bytes).map_err(|e| Error::io(&target, e))?;

    Ok(Attachment {
        id,
        kind: kind_for(&mime),
        name: name.to_string(),
        mime,
        size: bytes.len() as u64,
        path: target.to_string_lossy().into_owned(),
        text: None,
        hidden: false,
    })
}

/// Saves base64 bytes received over IPC (clipboard paste).
pub fn store_base64(session_id: &str, name: &str, data: &str) -> Result<Attachment> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|e| Error::Other(format!("invalid base64 payload: {e}")))?;
    store_bytes(session_id, name, &bytes)
}

pub fn attachments_dir(session_id: &str) -> Result<PathBuf> {
    let root = paths::loom_home()?.join("attachments");
    let directory = attachments_dir_in(&root, session_id);
    std::fs::create_dir_all(&directory).map_err(|e| Error::io(&directory, e))?;
    Ok(directory)
}

fn attachments_dir_in(root: &Path, session_id: &str) -> PathBuf {
    let safe: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    root.join(safe)
}

/// Parses the `extra` JSON column into attachments (empty on any problem).
pub fn parse_extra(extra: Option<&str>) -> Vec<Attachment> {
    extra
        .and_then(|raw| serde_json::from_str::<Vec<Attachment>>(raw).ok())
        .unwrap_or_default()
}

/// Serialises attachments for the `extra` column; `None` when empty.
pub fn serialize_extra(attachments: &[Attachment]) -> Option<String> {
    if attachments.is_empty() {
        return None;
    }
    serde_json::to_string(attachments).ok()
}

/// Guards against reading arbitrary paths: attachments must live inside the
/// Loom home directory.
pub fn is_managed(attachment: &Attachment) -> bool {
    match paths::loom_home() {
        Ok(home) => Path::new(&attachment.path).starts_with(home),
        Err(_) => false,
    }
}

/// Reads an attachment's bytes (images for the wire, text for inlining).
pub fn read_bytes(attachment: &Attachment) -> Result<Vec<u8>> {
    let path = Path::new(&attachment.path);
    std::fs::read(path).map_err(|e| Error::io(path, e))
}

/// Extracts prompt-ready text for a text or PDF attachment.
pub fn extract_text(attachment: &Attachment) -> Result<String> {
    match attachment.kind {
        AttachmentKind::Image => Err(Error::Other("images are not inlined as text".into())),
        AttachmentKind::Text => {
            let path = Path::new(&attachment.path);
            let raw = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
            Ok(clamp_text(raw))
        }
        AttachmentKind::Pdf => {
            let path = Path::new(&attachment.path);
            let raw = std::fs::read(path).map_err(|e| Error::io(path, e))?;
            let text = pdf_text(&raw)?;
            if text.trim().is_empty() {
                return Err(Error::Other(format!(
                    "\"{}\" has no text layer (scanned PDFs are not supported yet)",
                    attachment.name
                )));
            }
            Ok(clamp_text(text))
        }
    }
}

fn clamp_text(text: String) -> String {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return text;
    }
    let truncated: String = text.chars().take(MAX_TEXT_CHARS).collect();
    format!("{truncated}\n[... truncated]")
}

/// Minimal PDF text extraction: walks `stream ... endstream` blocks, inflates
/// flate streams, and concatenates the string operands of text-showing
/// operators.
///
/// Deliberately dependency-light and conservative: a scanned PDF yields an
/// empty string, which the caller reports as "no text layer".
pub fn pdf_text(bytes: &[u8]) -> Result<String> {
    use std::io::Read;

    let text = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut cursor = 0;

    while let Some(start) = text[cursor..].find("stream") {
        let stream_start = cursor + start + "stream".len();
        let Some(end) = text[stream_start..].find("endstream") else {
            break;
        };
        let end_index = stream_start + end;
        let raw = &bytes[stream_start.min(bytes.len())..end_index.min(bytes.len())];
        let raw = raw
            .strip_prefix(b"\r\n")
            .or_else(|| raw.strip_prefix(b"\n"))
            .unwrap_or(raw);

        let mut decoded = String::new();
        let inflated = match flate2_decode(raw) {
            Ok(mut decoder) => decoder.read_to_string(&mut decoded).is_ok(),
            Err(_) => false,
        };
        if !inflated {
            decoded = String::from_utf8_lossy(raw).into_owned();
        }
        out.push_str(&extract_operands(&decoded));

        cursor = end_index + "endstream".len();
        if cursor >= text.len() {
            break;
        }
    }

    if out.trim().is_empty() {
        out = extract_operands(&text);
    }

    Ok(out)
}

/// Extracts the string operands of text-showing operators, which is where the
/// characters live in a PDF content stream.
fn extract_operands(stream: &str) -> String {
    let mut out = String::new();
    let bytes = stream.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'(' {
            let (literal, next) = read_literal(stream, index);
            let after = stream[next..].trim_start();
            let shows_text = after.starts_with("Tj")
                || after.starts_with("TJ")
                || after.starts_with('\'')
                || after.starts_with('"');
            if shows_text {
                out.push_str(&literal);
                out.push(' ');
            }
            index = next.max(index + 1);
            continue;
        }
        if bytes[index] == b'<' && bytes.get(index + 1) != Some(&b'<') {
            let (hex, next) = read_hex(stream, index);
            let after = stream[next..].trim_start();
            if !hex.is_empty() && (after.starts_with("Tj") || after.starts_with("TJ")) {
                out.push_str(&hex);
                out.push(' ');
            }
            index = next.max(index + 1);
            continue;
        }
        index += 1;
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads a `(...)` PDF string literal starting at `start`, returning the
/// decoded text and the index just past the closing parenthesis.
fn read_literal(stream: &str, start: usize) -> (String, usize) {
    let bytes = stream.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut index = start + 1; // skip the opening parenthesis

    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                index += 1;
                if index >= bytes.len() {
                    break;
                }
                let escaped = match bytes[index] {
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'(' => b'(',
                    b')' => b')',
                    b'\\' => b'\\',
                    other => other,
                };
                out.push(escaped);
            }
            b'(' => {
                depth += 1;
                out.push(b'(');
            }
            b')' => {
                if depth == 0 {
                    return (String::from_utf8_lossy(&out).into_owned(), index + 1);
                }
                depth -= 1;
                out.push(b')');
            }
            other => out.push(other),
        }
        index += 1;
    }

    (String::from_utf8_lossy(&out).into_owned(), index)
}

/// Reads a `<hex>` PDF string.
fn read_hex(stream: &str, start: usize) -> (String, usize) {
    let bytes = stream.as_bytes();
    let mut hex = String::new();
    let mut index = start + 1;
    while index < bytes.len() && bytes[index] != b'>' {
        hex.push(bytes[index] as char);
        index += 1;
    }
    (decode_hex_text(&hex), (index + 1).min(bytes.len()))
}

/// Hex strings are usually UTF-16BE in PDFs, so decode both shapes.
fn decode_hex_text(hex: &str) -> String {
    let cleaned: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut bytes = Vec::new();
    let mut chars = cleaned.chars();
    while let (Some(high), Some(low)) = (chars.next(), chars.next()) {
        if let Ok(byte) = u8::from_str_radix(&format!("{high}{low}"), 16) {
            bytes.push(byte);
        }
    }

    if bytes.len() % 2 == 0 && bytes.iter().step_by(2).all(|b| *b == 0) {
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

fn flate2_decode(raw: &[u8]) -> Result<Box<dyn std::io::Read>> {
    use std::io::Cursor;
    let cursor = Cursor::new(raw.to_vec());
    Ok(Box::new(flate2::read::ZlibDecoder::new(cursor)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension() {
        assert_eq!(kind_for(mime_for("shot.PNG")), AttachmentKind::Image);
        assert_eq!(kind_for(mime_for("notes.md")), AttachmentKind::Text);
        assert_eq!(kind_for(mime_for("paper.pdf")), AttachmentKind::Pdf);
        assert_eq!(kind_for(mime_for("weird")), AttachmentKind::Text);
    }

    #[test]
    fn extracts_text_from_a_simple_content_stream() {
        let content = "BT /F1 12 Tf (Hello ) Tj (world) Tj ET";
        assert_eq!(extract_operands(content), "Hello world");
    }

    #[test]
    fn handles_escapes_and_hex_strings() {
        assert_eq!(extract_operands(r"(a \(b\) c) Tj"), "a (b) c");
        assert_eq!(extract_operands("<48656C6C6F> Tj"), "Hello");
    }

    #[test]
    fn ignores_strings_that_are_not_text_operands() {
        assert_eq!(extract_operands("(not shown) 0 0 1 RG"), "");
    }

    #[test]
    fn extracts_from_a_stream_with_binary_envelope() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 34 >>\nstream\nBT (Hi there) Tj ET\nendstream\nendobj\n";
        assert_eq!(pdf_text(pdf).unwrap(), "Hi there");
    }

    #[test]
    fn stores_and_reads_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("hello.txt");
        std::fs::write(&source, "hello loom").unwrap();

        let attachment = store_in(dir.path(), "session-1", &source).unwrap();
        assert_eq!(attachment.kind, AttachmentKind::Text);
        assert_eq!(attachment.name, "hello.txt");
        assert!(Path::new(&attachment.path).exists());
        assert_eq!(extract_text(&attachment).unwrap(), "hello loom");
    }

    #[test]
    fn extras_round_trip() {
        let attachment = Attachment {
            id: "a1".into(),
            kind: AttachmentKind::Image,
            name: "x.png".into(),
            mime: "image/png".into(),
            size: 4,
            path: "C:/tmp/x.png".into(),
            text: None,
            hidden: true,
        };
        let encoded = serialize_extra(std::slice::from_ref(&attachment)).unwrap();
        assert_eq!(parse_extra(Some(&encoded)), vec![attachment]);
        assert!(serialize_extra(&[]).is_none());
    }

    #[test]
    fn rejects_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let error = store_in(dir.path(), "s", &dir.path().join("nope.txt")).unwrap_err();
        assert!(error.to_string().contains("io") || error.to_string().contains("nope"));
    }

    #[test]
    fn pasted_bytes_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let attachment =
            store_bytes_in(dir.path(), "s", "paste.png", &[0x89, 0x50, 0x4E, 0x47]).unwrap();
        assert_eq!(attachment.kind, AttachmentKind::Image);
        assert_eq!(read_bytes(&attachment).unwrap().len(), 4);
    }
}
