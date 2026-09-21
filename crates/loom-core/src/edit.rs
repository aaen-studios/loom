//! Text file IO for the editor panel.
//!
//! Separate from `tools.rs`, which has its own `read_file`/`write_file` for the
//! *model*. The two have genuinely different requirements and sharing one
//! implementation would mean each getting the other's compromises:
//!
//! * The model's reader returns a numbered, line-ranged, size-capped **view** of
//!   a file for a prompt. This one returns the **whole** file plus the metadata
//!   an editor needs to write it back faithfully — the line ending, the byte
//!   order mark, and a hash of what was loaded.
//! * The model's writer is unconditional. This one refuses if the file moved on
//!   since it was read, because the editor holds unsaved keystrokes and the agent
//!   may be writing the same path at the same time.
//!
//! ## Why the hash guard exists
//!
//! Autosave writes without being asked, which is only acceptable if it can never
//! discard work. So a save carries the hash of the bytes that were loaded, and a
//! mismatch is refused rather than written. The UI turns that refusal into a
//! conflict the user resolves — reload, overwrite, or compare. Without it,
//! autosave is a mechanism for losing exactly the edits it was meant to protect.
//!
//! ## Faithfulness
//!
//! An editor that rewrites a file's line endings or drops its BOM has corrupted
//! it, quietly, in a way a diff will show as every line changed. Both are
//! detected on read and re-applied on write, so a CRLF file with a BOM comes back
//! byte-identical when nothing is edited.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{fsutil, Error, Result};

/// Largest file the editor will open.
///
/// A cap rather than a stream, because the editor holds the whole buffer: Monaco
/// can technically open far more, but a monolithic bundle or a minified blob at
/// that size makes the panel unusable rather than merely slow, and refusing with
/// a number is more honest than degrading silently.
pub const MAX_EDIT_BYTES: u64 = 4 * 1024 * 1024;

/// Bytes inspected for a NUL when deciding whether a file is text.
const SNIFF_BYTES: usize = 8 * 1024;

/// How a file's line endings are written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    /// What a file's bytes are dominated by.
    ///
    /// Dominant rather than "contains", because a file with one stray CRLF among
    /// a thousand LF lines is an LF file — and picking the minority ending would
    /// rewrite the whole file.
    fn detect(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        if crlf == 0 {
            return Self::Lf;
        }
        let lf = text.matches('\n').count();
        if crlf * 2 >= lf {
            Self::Crlf
        } else {
            Self::Lf
        }
    }
}

/// A file, as the editor loads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFile {
    /// Path relative to the workspace root, `/`-separated.
    pub path: String,
    /// Absolute path, for the tab's tooltip.
    pub absolute: String,
    /// The text with a BOM stripped and line endings **normalised to `\n`**.
    ///
    /// Normalised because that is what an editor wants to hold, and re-applying
    /// the original ending on save is exact — see [`write_text`].
    pub text: String,
    /// SHA-256 of the *raw bytes on disk*, which is what a save checks against.
    /// Hashing the normalised text instead would make every CRLF file conflict
    /// with itself.
    pub hash: String,
    pub bytes: u64,
    pub lines: usize,
    pub eol: LineEnding,
    /// The file began with a UTF-8 byte order mark, which is put back on save.
    pub bom: bool,
    /// Set when the file is not valid UTF-8. Read as lossy text so it can still
    /// be looked at, but the editor will not write it back.
    pub lossy: bool,
    pub read_only: bool,
    /// The freshness hint for this version, from size and mtime together.
    ///
    /// Carried on the load so the editor's later `file_stat_many` can compare
    /// like for like. See [`FileStat::hash_hint`].
    pub hash_hint: String,
}

/// What a save did.
///
/// A conflict is a *value* rather than an error because it is an expected
/// outcome of autosave, not a failure — the UI branches on it to show the
/// reload/overwrite choice, and an error string would have to be parsed to
/// recover that distinction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SaveOutcome {
    #[serde(rename_all = "camelCase")]
    Written {
        hash: String,
        bytes: u64,
        /// The freshness hint for the bytes just written.
        ///
        /// Returned rather than left for the caller to derive, and that is a
        /// correctness fix rather than a convenience. The editor used to build
        /// this itself as `format!("{bytes}:{}", Date::now())` — the *client*
        /// clock, against a value [`stat`] computes from the *filesystem's*
        /// mtime. The two disagree by however long the write took, so every save
        /// left a buffer whose recorded hint did not match the file's, and the
        /// next `recheck` read that as "the file changed underneath you". On a
        /// clean buffer that meant a silent re-read after every save; on a dirty
        /// one — the ordinary autosave case, since autosave fires while you are
        /// still typing — it raised a conflict banner on the file Loom had just
        /// written itself.
        hash_hint: String,
    },
    /// The file changed underneath. Nothing was written.
    #[serde(rename_all = "camelCase")]
    Conflict {
        /// The hash of what is on disk now.
        current_hash: String,
        /// The mtime of what is on disk now, in unix milliseconds, so the UI can
        /// say when it changed.
        modified: i64,
    },
}

/// One entry in a directory listing, for the file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    pub name: String,
    /// Relative to the workspace root, `/`-separated.
    pub path: String,
    pub is_dir: bool,
}

/// A file's state on disk, for the freshness check and the conflict banner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    /// Unix milliseconds, or 0 when the file is gone.
    pub modified: i64,
    /// A cheap freshness hint: size and mtime together, as a short string.
    ///
    /// The *exact* check is a SHA-256 of the whole file, and the editor cannot
    /// afford that on every tool call and every window focus — an open buffer is
    /// a few hundred kilobytes, and hashing a dozen of them is real work.
    ///
    /// So this is the gate: it changes whenever the bytes plausibly changed, and
    /// only a file whose hint differs from the loaded one pays for the hash. It
    /// is deliberately **not** a claim about content — two edits inside the same
    /// millisecond with the same size would collide — which is exactly why it
    /// only ever decides whether to look closer, never whether to act.
    pub hash_hint: String,
}

/// SHA-256 of some bytes, as lowercase hex.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A file's contents as raw bytes, or `None` when it does not exist.
pub fn read_bytes_or_none(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io(path, error)),
    }
}

/// Resolves a workspace-relative path and refuses anything that escapes.
///
/// The same rule as the agent's `resolve`, restated rather than shared because
/// this one has to handle a path that does not exist yet — a "new file" in the
/// tree — where a canonicalising check would have nothing to canonicalise.
fn resolve(root: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(Error::Other("absolute paths are not allowed".into()));
    }
    let joined = root.join(candidate);

    // Lexical normalisation, so `..`s are collapsed *before* the containment
    // check. `a/../../b` has to be judged as the path it means, not the string
    // it is — otherwise the escape is checked for and then performed.
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if !normalized.starts_with(root) {
        return Err(Error::Other(format!(
            "\"{relative}\" is outside the workspace folder"
        )));
    }
    Ok(normalized)
}

/// The workspace root for a call, or a plain error.
fn workspace_root(workdir: Option<&str>) -> Result<PathBuf> {
    let path = workdir
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Other("this chat has no workspace folder set".into()))?;
    Ok(PathBuf::from(path))
}

/// Reads a file for editing.
pub fn read_text(workdir: Option<&str>, relative: &str) -> Result<TextFile> {
    let root = workspace_root(workdir)?;
    let path = resolve(&root, relative)?;

    let metadata = std::fs::metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::Other(format!("\"{relative}\" does not exist"))
        } else {
            Error::io(&path, error)
        }
    })?;
    if metadata.is_dir() {
        return Err(Error::Other(format!("\"{relative}\" is a folder")));
    }
    if metadata.len() > MAX_EDIT_BYTES {
        return Err(Error::Other(format!(
            "this file is {} KB, over the {} KB editing limit",
            metadata.len() / 1024,
            MAX_EDIT_BYTES / 1024
        )));
    }

    let raw = std::fs::read(&path).map_err(|error| Error::io(&path, error))?;
    let hash = hash_bytes(&raw);

    // A NUL in the first few kilobytes is the same heuristic git uses, and it is
    // right in practice: a file with a NUL is not text in any encoding the
    // editor can round-trip.
    if raw.iter().take(SNIFF_BYTES).any(|byte| *byte == 0) {
        return Err(Error::Other(format!(
            "\"{relative}\" is not a text file"
        )));
    }

    let bom = raw.starts_with(&[0xEF, 0xBB, 0xBF]);
    let body = if bom { &raw[3..] } else { &raw[..] };
    let (text, lossy) = match std::str::from_utf8(body) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (String::from_utf8_lossy(body).into_owned(), true),
    };

    let eol = LineEnding::detect(&text);
    let normalized = text.replace("\r\n", "\n");
    let read_only = metadata.permissions().readonly();
    let modified = modified_ms(&path);

    Ok(TextFile {
        path: relative.replace('\\', "/"),
        absolute: path.to_string_lossy().into_owned(),
        lines: normalized.lines().count(),
        text: normalized,
        hash,
        bytes: metadata.len(),
        eol,
        bom,
        lossy,
        read_only,
        hash_hint: hint_for(metadata.len(), modified),
    })
}

/// Writes text back, refusing if the file moved on.
///
/// * `expected_hash` is the hash of what was loaded. `None` means "write
///   unconditionally", which is what an explicit Save As or a conflict's
///   Overwrite passes.
/// * The text arrives with `\n` endings and is converted to the file's own
///   `eol`, and a BOM is restored. Round-tripping is exact.
/// * The write is atomic: [`fsutil::atomic_write`] writes a sibling temp file and
///   renames over the destination, so a crash mid-save cannot leave a truncated
///   source file.
pub fn write_text(
    workdir: Option<&str>,
    relative: &str,
    text: &str,
    expected_hash: Option<&str>,
    eol: LineEnding,
    bom: bool,
) -> Result<SaveOutcome> {
    let root = workspace_root(workdir)?;
    let path = resolve(&root, relative)?;

    let existing = read_bytes_or_none(&path)?;
    let current_hash = existing.as_deref().map(hash_bytes);

    // The guard. Checked before anything is written, so a conflicting save has no
    // side effect at all — not even a temp file.
    if let (Some(expected), Some(current)) = (expected_hash, current_hash.as_deref()) {
        if expected != current {
            return Ok(SaveOutcome::Conflict {
                current_hash: current.to_string(),
                modified: modified_ms(&path),
            });
        }
    }
    // A file that did not exist when it was loaded and does now is also a
    // conflict: something created it while the buffer was open.
    if let (Some(expected), None) = (expected_hash, current_hash.as_deref()) {
        if !expected.is_empty() {
            return Ok(SaveOutcome::Conflict {
                current_hash: String::new(),
                modified: modified_ms(&path),
            });
        }
    }

    let body = if eol == LineEnding::Crlf {
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        text.replace("\r\n", "\n")
    };
    let mut bytes = Vec::with_capacity(body.len() + 3);
    if bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(body.as_bytes());

    fsutil::atomic_write(&path, &bytes)?;

    // Stat the file we just wrote rather than inventing a timestamp: the hint
    // has to be the one `stat` will compute later, or every save looks like an
    // external change. See the note on `SaveOutcome::Written::hash_hint`.
    let written = bytes.len() as u64;
    Ok(SaveOutcome::Written {
        hash: hash_bytes(&bytes),
        bytes: written,
        hash_hint: hint_for(written, modified_ms(&path)),
    })
}

/// A file's mtime in unix milliseconds, or 0 when it cannot be read.
///
/// Milliseconds rather than seconds because `git status` inside the same second
/// as an edit is a real thing, and a conflict banner that says "changed 0 seconds
/// ago" for a change from last week is worse than no timestamp.
pub fn modified_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// A file's state on disk.
pub fn stat(workdir: Option<&str>, relative: &str) -> Result<FileStat> {
    let root = workspace_root(workdir)?;
    let path = resolve(&root, relative)?;
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Ok(FileStat {
            path: relative.replace('\\', "/"),
            exists: false,
            bytes: 0,
            modified: 0,
            hash_hint: "gone".to_string(),
        });
    };
    let bytes = metadata.len();
    let modified = modified_ms(&path);
    Ok(FileStat {
        path: relative.replace('\\', "/"),
        exists: true,
        bytes,
        modified,
        // The hint a buffer's own load records is `edit::hint_for(bytes, mtime)`,
        // and the two must be computed the same way or every clean buffer would
        // look changed on the first check.
        hash_hint: hint_for(bytes, modified),
    })
}

/// The freshness hint for a version of a file, from its size and mtime.
///
/// Public because `read_text` records the same value on the tab it opens, and
/// the two have to agree exactly: a hint computed two different ways would make
/// every freshly-opened buffer look stale, and the editor would reload the file
/// you are typing into on the first tool call.
pub fn hint_for(bytes: u64, modified: i64) -> String {
    format!("{bytes}:{modified}")
}

/// One directory's entries, for the lazily-expanded file tree.
///
/// A single level at a time, deliberately. The composer's `@` picker walks the
/// whole tree because it needs to *search* it; a tree that can be expanded needs
/// only what is on screen, so a collapsed folder costs nothing and a huge
/// repository opens instantly.
pub fn list_dir(workdir: Option<&str>, relative: &str) -> Result<Vec<TreeEntry>> {
    let root = workspace_root(workdir)?;
    let directory = if relative.trim().is_empty() {
        root.clone()
    } else {
        resolve(&root, relative)?
    };

    let reader = std::fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))?;
    let mut entries = Vec::new();
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // One hidden folder that is never worth showing in a source tree, and it
        // is the one that would make every listing start with a wall of internals.
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(TreeEntry {
            name,
            path: relative_path,
            is_dir,
        });
    }

    // Folders first, then files, each alphabetically and case-insensitively: the
    // order every file tree in every editor uses, because the alternative makes
    // you hunt.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// Creates an empty file, or a folder. Refuses to overwrite.
pub fn create_entry(workdir: Option<&str>, relative: &str, is_dir: bool) -> Result<TreeEntry> {
    let root = workspace_root(workdir)?;
    let path = resolve(&root, relative)?;
    if path.exists() {
        return Err(Error::Other(format!("\"{relative}\" already exists")));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    }
    if is_dir {
        std::fs::create_dir(&path).map_err(|error| Error::io(&path, error))?;
    } else {
        // `create_new` rather than a write: a file created empty and then
        // written would be briefly visible as a zero-byte file to anything
        // watching, and `create_new` also makes "already exists" atomic.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| Error::io(&path, error))?;
    }
    Ok(TreeEntry {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: relative.replace('\\', "/"),
        is_dir,
    })
}

/// Renames or moves a path inside the workspace.
pub fn rename_entry(workdir: Option<&str>, from: &str, to: &str) -> Result<TreeEntry> {
    let root = workspace_root(workdir)?;
    let source = resolve(&root, from)?;
    let destination = resolve(&root, to)?;
    if !source.exists() {
        return Err(Error::Other(format!("\"{from}\" does not exist")));
    }
    if destination.exists() {
        return Err(Error::Other(format!("\"{to}\" already exists")));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    }
    // `rename` with a copy fallback: a move across a volume boundary fails on
    // Windows, and the workspace can span a junction.
    if std::fs::rename(&source, &destination).is_err() {
        if source.is_dir() {
            return Err(Error::Other(
                "folders can only be renamed within one drive".into(),
            ));
        }
        std::fs::copy(&source, &destination).map_err(|error| Error::io(&destination, error))?;
        std::fs::remove_file(&source).map_err(|error| Error::io(&source, error))?;
    }
    Ok(TreeEntry {
        name: destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: to.replace('\\', "/"),
        is_dir: destination.is_dir(),
    })
}

/// Whether a path exists, for the tree's refresh after a delete.
pub fn exists(workdir: Option<&str>, relative: &str) -> Result<bool> {
    let root = workspace_root(workdir)?;
    Ok(resolve(&root, relative)?.exists())
}

/// Every path under a directory, relative to the workspace root.
///
/// Used once, when the editor is asked to "save all" or when a rename must
/// re-point open tabs. Depth-capped by the caller's trust in `fsutil`.
pub fn walk(workdir: Option<&str>, relative: &str, limit: usize) -> Result<Vec<String>> {
    let root = workspace_root(workdir)?;
    let directory = if relative.trim().is_empty() {
        root.clone()
    } else {
        resolve(&root, relative)?
    };
    Ok(fsutil::walk_files(&directory, limit)
        .into_iter()
        .map(|path| {
            if relative.trim().is_empty() {
                path
            } else {
                format!("{}/{}", relative.trim_end_matches('/'), path)
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn workdir(dir: &tempfile::TempDir) -> String {
        dir.path().to_string_lossy().into_owned()
    }

    fn write(root: &Path, name: &str, body: &str) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn reads_a_file_with_its_hash_and_line_count() {
        let dir = workspace();
        write(dir.path(), "a.txt", "one\ntwo\nthree\n");

        let file = read_text(Some(&workdir(&dir)), "a.txt").unwrap();
        assert_eq!(file.text, "one\ntwo\nthree\n");
        assert_eq!(file.lines, 3);
        assert_eq!(file.bytes, 14);
        assert!(!file.lossy);
        assert_eq!(file.eol, LineEnding::Lf);
        // The hash is of the raw bytes, so it matches what is on disk.
        assert_eq!(
            file.hash,
            hash_bytes(&std::fs::read(dir.path().join("a.txt")).unwrap())
        );
    }

    #[test]
    fn a_missing_file_is_a_clear_error() {
        let dir = workspace();
        let error = read_text(Some(&workdir(&dir)), "nope.txt").unwrap_err();
        assert!(error.to_string().contains("does not exist"), "{error}");
    }

    #[test]
    fn a_folder_is_refused() {
        let dir = workspace();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let error = read_text(Some(&workdir(&dir)), "src").unwrap_err();
        assert!(error.to_string().contains("is a folder"), "{error}");
    }

    #[test]
    fn a_binary_file_is_refused() {
        let dir = workspace();
        std::fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        let error = read_text(Some(&workdir(&dir)), "blob.bin").unwrap_err();
        assert!(error.to_string().contains("not a text file"), "{error}");
    }

    #[test]
    fn an_oversized_file_is_refused_with_its_size() {
        let dir = workspace();
        let big = "x".repeat((MAX_EDIT_BYTES + 1) as usize);
        std::fs::write(dir.path().join("big.txt"), big).unwrap();
        let error = read_text(Some(&workdir(&dir)), "big.txt").unwrap_err();
        assert!(error.to_string().contains("editing limit"), "{error}");
    }

    #[test]
    fn an_escape_attempt_is_refused() {
        let dir = workspace();
        let outside = dir.path().parent().unwrap().join("outside.txt");
        std::fs::write(&outside, "secret").unwrap();

        let error = read_text(Some(&workdir(&dir)), "../outside.txt").unwrap_err();
        assert!(error.to_string().contains("outside the workspace"), "{error}");
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let dir = workspace();
        let error = read_text(Some(&workdir(&dir)), "C:/Windows/system.ini").unwrap_err();
        assert!(error.to_string().contains("absolute"), "{error}");
    }

    #[test]
    fn a_doubly_nested_escape_is_refused() {
        // `a/../../b` normalises to `../b`, so the check has to run on the
        // collapsed path rather than the string.
        let dir = workspace();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        let error = read_text(Some(&workdir(&dir)), "a/../../outside.txt").unwrap_err();
        assert!(error.to_string().contains("outside the workspace"), "{error}");
    }

    #[test]
    fn a_round_trip_writes_the_same_bytes() {
        let dir = workspace();
        write(dir.path(), "a.txt", "hello\n");

        let file = read_text(Some(&workdir(&dir)), "a.txt").unwrap();
        let outcome = write_text(
            Some(&workdir(&dir)),
            "a.txt",
            &file.text,
            Some(&file.hash),
            file.eol,
            file.bom,
        )
        .unwrap();
        match outcome {
            SaveOutcome::Written { hash, .. } => assert_eq!(hash, file.hash),
            other => panic!("expected a write, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(dir.path().join("a.txt")).unwrap(),
            b"hello\n"
        );
    }

    #[test]
    fn crlf_endings_survive_a_round_trip() {
        // The failure this prevents is every line of a file showing as changed.
        let dir = workspace();
        std::fs::write(dir.path().join("win.txt"), b"one\r\ntwo\r\n").unwrap();

        let file = read_text(Some(&workdir(&dir)), "win.txt").unwrap();
        assert_eq!(file.eol, LineEnding::Crlf);
        // The editor sees plain newlines.
        assert_eq!(file.text, "one\ntwo\n");

        let before = std::fs::read(dir.path().join("win.txt")).unwrap();
        write_text(
            Some(&workdir(&dir)),
            "win.txt",
            &file.text,
            Some(&file.hash),
            file.eol,
            file.bom,
        )
        .unwrap();
        assert_eq!(std::fs::read(dir.path().join("win.txt")).unwrap(), before);
    }

    #[test]
    fn a_byte_order_mark_survives_a_round_trip() {
        let dir = workspace();
        let original = [&[0xEF, 0xBB, 0xBF][..], b"hello\n"].concat();
        std::fs::write(dir.path().join("bom.txt"), &original).unwrap();

        let file = read_text(Some(&workdir(&dir)), "bom.txt").unwrap();
        assert!(file.bom);
        // The editor must not show the mark as text.
        assert_eq!(file.text, "hello\n");
        assert!(!file.text.starts_with('\u{feff}'));

        write_text(
            Some(&workdir(&dir)),
            "bom.txt",
            &file.text,
            Some(&file.hash),
            file.eol,
            file.bom,
        )
        .unwrap();
        assert_eq!(std::fs::read(dir.path().join("bom.txt")).unwrap(), original);
    }

    #[test]
    fn a_changed_file_conflicts_rather_than_being_overwritten() {
        // The whole reason autosave is safe.
        let dir = workspace();
        write(dir.path(), "a.txt", "mine\n");
        let file = read_text(Some(&workdir(&dir)), "a.txt").unwrap();

        // The agent writes the same path.
        write(dir.path(), "a.txt", "theirs\n");

        let outcome = write_text(
            Some(&workdir(&dir)),
            "a.txt",
            "my unsaved edit\n",
            Some(&file.hash),
            file.eol,
            file.bom,
        )
        .unwrap();
        match outcome {
            SaveOutcome::Conflict { current_hash, .. } => {
                assert_eq!(current_hash, hash_bytes(b"theirs\n"));
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        // And nothing was written: the agent's version is intact.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "theirs\n"
        );
    }

    #[test]
    fn a_forced_write_ignores_the_guard() {
        // What the conflict banner's Overwrite does.
        let dir = workspace();
        write(dir.path(), "a.txt", "mine\n");
        let file = read_text(Some(&workdir(&dir)), "a.txt").unwrap();
        write(dir.path(), "a.txt", "theirs\n");

        let outcome = write_text(
            Some(&workdir(&dir)),
            "a.txt",
            "mine\n",
            None, // unconditional
            file.eol,
            file.bom,
        )
        .unwrap();
        assert!(matches!(outcome, SaveOutcome::Written { .. }));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "mine\n"
        );
    }

    #[test]
    fn an_unchanged_file_saves_cleanly_twice() {
        // Autosave fires repeatedly on an idle buffer; the second write must not
        // conflict with the first.
        let dir = workspace();
        write(dir.path(), "a.txt", "one\n");
        let file = read_text(Some(&workdir(&dir)), "a.txt").unwrap();

        let first = write_text(
            Some(&workdir(&dir)),
            "a.txt",
            "two\n",
            Some(&file.hash),
            file.eol,
            file.bom,
        )
        .unwrap();
        let SaveOutcome::Written { hash, .. } = first else {
            panic!("first write should succeed");
        };
        let second = write_text(
            Some(&workdir(&dir)),
            "a.txt",
            "three\n",
            Some(&hash),
            file.eol,
            file.bom,
        )
        .unwrap();
        assert!(matches!(second, SaveOutcome::Written { .. }));
    }

    #[test]
    fn a_file_that_appeared_underneath_a_new_buffer_conflicts() {
        // A buffer for a file that did not exist gave an empty expected hash, so
        // a file appearing at that path is someone else's.
        let dir = workspace();
        let outcome = write_text(
            Some(&workdir(&dir)),
            "new.txt",
            "mine\n",
            Some("placeholder-hash"),
            LineEnding::Lf,
            false,
        )
        .unwrap();
        assert!(matches!(outcome, SaveOutcome::Conflict { .. }));
    }

    #[test]
    fn writing_a_new_file_with_no_expectation_works() {
        let dir = workspace();
        let outcome = write_text(
            Some(&workdir(&dir)),
            "nested/new.txt",
            "created\n",
            None,
            LineEnding::Lf,
            false,
        )
        .unwrap();
        assert!(matches!(outcome, SaveOutcome::Written { .. }));
        // Parent folders are created, so a save cannot fail on a missing path.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("nested/new.txt")).unwrap(),
            "created\n"
        );
    }

    #[test]
    fn invalid_utf8_is_read_lossily_and_flagged() {
        let dir = workspace();
        std::fs::write(dir.path().join("latin.txt"), b"caf\xe9\n").unwrap();
        let file = read_text(Some(&workdir(&dir)), "latin.txt").unwrap();
        assert!(file.lossy);
        assert!(file.text.contains("caf"));
    }

    #[test]
    fn no_temp_file_is_left_behind_by_a_write() {
        let dir = workspace();
        write(dir.path(), "a.txt", "one\n");
        let file = read_text(Some(&workdir(&dir)), "a.txt").unwrap();
        write_text(
            Some(&workdir(&dir)),
            "a.txt",
            "two\n",
            Some(&file.hash),
            file.eol,
            file.bom,
        )
        .unwrap();

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "a.txt")
            .collect();
        assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
    }

    #[test]
    fn a_directory_listing_puts_folders_first() {
        let dir = workspace();
        write(dir.path(), "zebra.txt", "x");
        write(dir.path(), "apple.txt", "x");
        std::fs::create_dir(dir.path().join("src")).unwrap();
        // `.git` is the one thing a source tree should never show.
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let entries = list_dir(Some(&workdir(&dir)), "").unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["src", "apple.txt", "zebra.txt"]);
        assert!(entries[0].is_dir);
    }

    #[test]
    fn a_nested_listing_reports_paths_relative_to_the_root() {
        let dir = workspace();
        write(dir.path(), "src/deep/thing.ts", "x");
        let entries = list_dir(Some(&workdir(&dir)), "src").unwrap();
        assert_eq!(entries[0].path, "src/deep");
    }

    #[test]
    fn a_listing_outside_the_workspace_is_refused() {
        let dir = workspace();
        let error = list_dir(Some(&workdir(&dir)), "../").unwrap_err();
        assert!(error.to_string().contains("outside the workspace"), "{error}");
    }

    #[test]
    fn creating_an_entry_refuses_to_clobber() {
        let dir = workspace();
        create_entry(Some(&workdir(&dir)), "new.txt", false).unwrap();
        let error = create_entry(Some(&workdir(&dir)), "new.txt", false).unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
    }

    #[test]
    fn creating_a_nested_file_makes_its_folders() {
        let dir = workspace();
        create_entry(Some(&workdir(&dir)), "a/b/c.txt", false).unwrap();
        assert!(dir.path().join("a/b/c.txt").exists());
    }

    #[test]
    fn renaming_moves_the_file() {
        let dir = workspace();
        write(dir.path(), "before.txt", "x");
        rename_entry(Some(&workdir(&dir)), "before.txt", "after.txt").unwrap();
        assert!(!dir.path().join("before.txt").exists());
        assert!(dir.path().join("after.txt").exists());
    }

    #[test]
    fn renaming_onto_an_existing_path_is_refused() {
        let dir = workspace();
        write(dir.path(), "a.txt", "x");
        write(dir.path(), "b.txt", "y");
        let error = rename_entry(Some(&workdir(&dir)), "a.txt", "b.txt").unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
        // And neither file was damaged.
        assert_eq!(std::fs::read_to_string(dir.path().join("b.txt")).unwrap(), "y");
    }

    #[test]
    fn stat_reports_a_missing_file_rather_than_failing() {
        let dir = workspace();
        let stat = stat(Some(&workdir(&dir)), "nope.txt").unwrap();
        assert!(!stat.exists);
        assert_eq!(stat.modified, 0);
    }

    #[test]
    fn stat_reports_a_real_file() {
        let dir = workspace();
        write(dir.path(), "a.txt", "x");
        let stat = stat(Some(&workdir(&dir)), "a.txt").unwrap();
        assert!(stat.exists);
        assert_eq!(stat.bytes, 1);
        assert!(stat.modified > 0);
    }

    #[test]
    fn a_workdir_of_none_is_a_clear_error() {
        let error = read_text(None, "a.txt").unwrap_err();
        assert!(error.to_string().contains("no workspace folder"), "{error}");
    }

    #[test]
    fn line_ending_detection_prefers_the_majority() {
        assert_eq!(LineEnding::detect("no newline"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\nb\nc\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\r\nb\r\nc\r\n"), LineEnding::Crlf);
        // One stray CRLF in an LF file must not flip the whole file.
        assert_eq!(LineEnding::detect("a\nb\r\nc\nd\ne\n"), LineEnding::Lf);
    }
}
