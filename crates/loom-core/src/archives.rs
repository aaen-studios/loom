//! Archives: listing and extracting zip and tar.
//!
//! Two jobs, and the second one is the dangerous one. Listing is a read.
//! Extracting writes bytes that came from somewhere else, at paths chosen by
//! whoever built the archive, so every rule about *where* a write may land is
//! enforced here rather than left to the caller.
//!
//! # The two attacks this refuses
//!
//! **Zip slip.** An entry named `../../.ssh/authorized_keys` or
//! `C:\Windows\System32\drivers\etc\hosts` is a write outside the workspace,
//! spelled as an ordinary path. Nothing is written without being checked twice:
//! `zip`'s own `enclosed_name` for zip entries, and an explicit component walk
//! for tar entries, which have no such helper. An absolute path, a drive letter,
//! or any `..` that would climb out is skipped and *reported* — silently dropping
//! it would leave a user believing they had restored a directory tree that is in
//! fact half missing.
//!
//! **Zip bombs.** A few kilobytes of compressed input can inflate to gigabytes.
//! Two caps stop that: a total decompressed byte budget and an entry count. Both
//! are checked against the *declared* size before a byte is written, so a refused
//! archive costs nothing but the parse — and the actual copy is additionally
//! bounded, because a lying header is exactly how a bomb gets in.

use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Total bytes an extraction may write.
pub const MAX_EXTRACT_BYTES: u64 = 512 * 1024 * 1024;

/// Most entries an extraction may create.
pub const MAX_EXTRACT_ENTRIES: usize = 20_000;

/// Most entries a listing will report.
pub const MAX_LISTED_ENTRIES: usize = 500;

/// One entry, as a listing shows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    /// Zero for tar, which carries no compressed size per entry.
    pub compressed: u64,
    pub is_dir: bool,
}

impl ArchiveEntry {
    fn render(&self) -> String {
        let kind = if self.is_dir { "dir " } else { "file" };
        if self.compressed > 0 && self.compressed != self.size {
            format!(
                "{kind} {:>9}  ({} compressed)  {}",
                self.size, self.compressed, self.path
            )
        } else {
            format!("{kind} {:>9}  {}", self.size, self.path)
        }
    }
}

/// What an extraction did.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractReport {
    pub written: usize,
    pub bytes: u64,
    /// Entries that were refused, with the reason. Never dropped silently.
    pub skipped: Vec<String>,
    /// The folder the files landed in, as the caller named it.
    pub destination: String,
}

/* ---------------------------------------------------------------------------
   Format detection
--------------------------------------------------------------------------- */

/// The archive kinds this understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Zip,
    Tar,
    TarGz,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Zip => "zip",
            Kind::Tar => "tar",
            Kind::TarGz => "tar.gz",
        }
    }

    /// Sniffs the container rather than trusting the extension.
    ///
    /// An extension is a claim; the magic bytes are the fact. A `.zip` that is
    /// really a tar would otherwise fail with a message about the wrong format,
    /// which is the least useful kind of error.
    pub fn detect(path: &Path, bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
            return Some(Kind::Zip);
        }
        if bytes.starts_with(&[0x1f, 0x8b]) {
            return Some(Kind::TarGz);
        }
        // `ustar` sits at offset 257 in a tar header.
        if bytes.len() > 262 && &bytes[257..262] == b"ustar" {
            return Some(Kind::Tar);
        }
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if extension == "zip" {
            return Some(Kind::Zip);
        }
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Some(Kind::TarGz);
        }
        if extension == "tar" {
            return Some(Kind::Tar);
        }
        None
    }
}

fn gzip_reader(bytes: &[u8]) -> Box<dyn Read + '_> {
    Box::new(flate2::read::GzDecoder::new(Cursor::new(bytes)))
}

/* ---------------------------------------------------------------------------
   Listing
--------------------------------------------------------------------------- */

/// Lists an archive's contents, capped.
pub fn list(path: &Path, bytes: &[u8], limit: usize) -> Result<(Kind, Vec<ArchiveEntry>, usize)> {
    let kind = Kind::detect(path, bytes).ok_or_else(|| {
        Error::Other(format!(
            "{} is not a zip, tar or tar.gz archive",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        ))
    })?;

    let limit = limit.clamp(1, MAX_LISTED_ENTRIES);
    let mut entries = Vec::new();
    let mut total = 0usize;

    match kind {
        Kind::Zip => {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
                .map_err(|error| Error::Other(format!("could not read the zip: {error}")))?;
            total = archive.len();
            for index in 0..total.min(limit) {
                let Ok(file) = archive.by_index(index) else {
                    continue;
                };
                entries.push(ArchiveEntry {
                    // The raw stored name, which is what a listing should show —
                    // including a hostile one, so the user can see that the
                    // archive means something hostile.
                    path: file.name().to_string(),
                    size: file.size(),
                    compressed: file.compressed_size(),
                    is_dir: file.is_dir(),
                });
            }
        }
        Kind::Tar => {
            let mut archive = tar::Archive::new(Cursor::new(bytes));
            collect_tar(&mut archive, &mut entries, &mut total, limit);
        }
        Kind::TarGz => {
            let mut archive = tar::Archive::new(gzip_reader(bytes));
            collect_tar(&mut archive, &mut entries, &mut total, limit);
        }
    }

    Ok((kind, entries, total))
}

fn collect_tar<R: Read>(
    archive: &mut tar::Archive<R>,
    entries: &mut Vec<ArchiveEntry>,
    total: &mut usize,
    limit: usize,
) {
    let Ok(iterator) = archive.entries() else {
        return;
    };
    for entry in iterator {
        let Ok(entry) = entry else {
            continue;
        };
        *total += 1;
        if entries.len() >= limit {
            continue;
        }
        let header = entry.header();
        entries.push(ArchiveEntry {
            path: entry
                .path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "(unreadable name)".to_string()),
            size: header.size().unwrap_or(0),
            compressed: 0,
            is_dir: header.entry_type().is_dir(),
        });
    }
}

/// Renders a listing for the model.
pub fn render_listing(kind: Kind, entries: &[ArchiveEntry], total: usize) -> String {
    if entries.is_empty() {
        return format!("This {} archive is empty.", kind.as_str());
    }
    let mut out = format!(
        "{} archive, {total} entr{}:\n",
        kind.as_str(),
        if total == 1 { "y" } else { "ies" }
    );
    for entry in entries {
        out.push_str("  ");
        out.push_str(&entry.render());
        out.push('\n');
    }
    if total > entries.len() {
        out.push_str(&format!(
            "  … and {} more (raise `limit` to see them)\n",
            total - entries.len()
        ));
    }
    out.trim_end().to_string()
}

/* ---------------------------------------------------------------------------
   Extraction
--------------------------------------------------------------------------- */

/// Checks an entry name and resolves it under `destination`.
///
/// Returns `None` for anything that would escape, which is the point: the caller
/// reports the skip rather than writing.
///
/// Both separator styles are handled because a zip built on Windows may carry
/// backslashes, and `Path::components` on Unix treats a backslash as an ordinary
/// character — so `..\..\evil` would sail straight through a naive check on Linux
/// while doing real damage on Windows. That asymmetry is the whole reason this
/// function exists rather than a `.starts_with()` call.
fn safe_join(destination: &Path, name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    let candidate = Path::new(&normalized);

    if candidate.is_absolute() {
        return None;
    }

    let mut out = destination.to_path_buf();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => {
                // A drive letter survives as an ordinary component on some
                // platforms, so it is refused by name as well.
                let text = part.to_string_lossy();
                if text.len() == 2 && text.ends_with(':') {
                    return None;
                }
                out.push(part);
            }
            // `..` and an absolute root are exactly the escape being prevented.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            Component::CurDir => {}
        }
    }

    out.starts_with(destination).then_some(out)
}

/// Extracts an archive into `destination`, which the caller has already resolved
/// inside the workspace.
///
/// `only` selects entries by path prefix; empty means everything.
pub fn extract(
    path: &Path,
    bytes: &[u8],
    destination: &Path,
    only: &[String],
) -> Result<ExtractReport> {
    let kind = Kind::detect(path, bytes)
        .ok_or_else(|| Error::Other("this file is not a zip, tar or tar.gz archive".into()))?;

    let mut report = ExtractReport {
        written: 0,
        bytes: 0,
        skipped: Vec::new(),
        destination: destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string()),
    };

    std::fs::create_dir_all(destination).map_err(|error| Error::io(destination, error))?;

    match kind {
        Kind::Zip => extract_zip(bytes, destination, only, &mut report)?,
        Kind::Tar => {
            let mut archive = tar::Archive::new(Cursor::new(bytes));
            extract_tar(&mut archive, destination, only, &mut report)?;
        }
        Kind::TarGz => {
            let mut archive = tar::Archive::new(gzip_reader(bytes));
            extract_tar(&mut archive, destination, only, &mut report)?;
        }
    }

    Ok(report)
}

/// Whether an entry is wanted, by the `only` list.
fn wanted(name: &str, only: &[String]) -> bool {
    if only.is_empty() {
        return true;
    }
    let normalized = name.replace('\\', "/");
    only.iter().any(|prefix| {
        let prefix = prefix.trim().replace('\\', "/");
        let prefix = prefix.trim_end_matches('/');
        !prefix.is_empty()
            && (normalized == prefix || normalized.starts_with(&format!("{prefix}/")))
    })
}

/// Checks the two caps before an entry is written.
fn budget(report: &ExtractReport, adding: u64, name: &str) -> Result<()> {
    if report.written >= MAX_EXTRACT_ENTRIES {
        return Err(Error::Other(format!(
            "stopped after {MAX_EXTRACT_ENTRIES} entries at `{name}`: an archive this large is \
             usually a decompression bomb rather than data"
        )));
    }
    if report.bytes.saturating_add(adding) > MAX_EXTRACT_BYTES {
        return Err(Error::Other(format!(
            "stopped at `{name}`: the archive claims to decompress to more than {} MB, which \
             is refused rather than written",
            MAX_EXTRACT_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

fn extract_zip(
    bytes: &[u8],
    destination: &Path,
    only: &[String],
    report: &mut ExtractReport,
) -> Result<()> {
    use std::io::Write;

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Error::Other(format!("could not read the zip: {error}")))?;

    for index in 0..archive.len() {
        let Ok(mut file) = archive.by_index(index) else {
            continue;
        };
        let raw = file.name().to_string();
        if !wanted(&raw, only) {
            continue;
        }

        // `enclosed_name` is `zip`'s own traversal check and the one to trust for
        // zips: it knows the archive's conventions. The explicit walk still runs,
        // because it is what produces a readable reason for the report.
        let (Some(target), true) = (safe_join(destination, &raw), file.enclosed_name().is_some())
        else {
            report
                .skipped
                .push(format!("{raw} — escapes the destination; not written"));
            continue;
        };

        if file.is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| Error::io(&target, error))?;
            continue;
        }

        let declared = file.size();
        budget(report, declared, &raw)?;

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|error| Error::io(&target, error))?;

        // Read through a bounded reader rather than trusting the declared size: a
        // lying header is exactly how a bomb gets in, so the budget is enforced
        // against what is actually produced as well as what was promised.
        let remaining = MAX_EXTRACT_BYTES - report.bytes;
        let written = std::io::copy(&mut (&mut file).take(remaining), &mut out)
            .map_err(|error| Error::io(&target, error))?;
        out.flush().map_err(|error| Error::io(&target, error))?;

        report.written += 1;
        report.bytes += written;
        if written < declared {
            report.skipped.push(format!(
                "{raw} — the archive claimed {declared} bytes but produced {written}; the copy \
                 is truncated"
            ));
        }
    }
    Ok(())
}

fn extract_tar<R: Read>(
    archive: &mut tar::Archive<R>,
    destination: &Path,
    only: &[String],
    report: &mut ExtractReport,
) -> Result<()> {
    let Ok(iterator) = archive.entries() else {
        return Err(Error::Other("could not read the tar".into()));
    };

    for entry in iterator {
        let Ok(mut entry) = entry else {
            continue;
        };
        let raw = entry
            .path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if raw.is_empty() || !wanted(&raw, only) {
            continue;
        }

        // A tar's own `unpack` does its own checks, but not all of them are the
        // ones that matter here, and none of them report. So the path is resolved
        // by hand and the file copied out explicitly.
        let Some(target) = safe_join(destination, &raw) else {
            report
                .skipped
                .push(format!("{raw} — escapes the destination; not written"));
            continue;
        };

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| Error::io(&target, error))?;
            continue;
        }
        // Symlinks and hard links are refused outright: a link is a path that
        // resolves *later*, so one pointing outside the destination would defeat
        // every check above by being followed after extraction.
        if !entry.header().entry_type().is_file() {
            report.skipped.push(format!(
                "{raw} — not a regular file (links and devices are not extracted)"
            ));
            continue;
        }

        let declared = entry.header().size().unwrap_or(0);
        budget(report, declared, &raw)?;

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|error| Error::io(&target, error))?;
        let remaining = MAX_EXTRACT_BYTES - report.bytes;
        let written = std::io::copy(&mut (&mut entry).take(remaining), &mut out)
            .map_err(|error| Error::io(&target, error))?;

        report.written += 1;
        report.bytes += written;
    }
    Ok(())
}

/// Renders an extraction report for the model.
pub fn render_report(report: &ExtractReport) -> String {
    let mut out = format!(
        "extracted {} file{} ({} bytes) into `{}`",
        report.written,
        if report.written == 1 { "" } else { "s" },
        report.bytes,
        report.destination,
    );
    if !report.skipped.is_empty() {
        out.push_str(&format!(
            "\n\n{} entr{} not written:\n",
            report.skipped.len(),
            if report.skipped.len() == 1 {
                "y was"
            } else {
                "ies were"
            }
        ));
        for line in &report.skipped {
            out.push_str(&format!("  {line}\n"));
        }
        out.push_str(
            "\nEntries that escape the destination are refused on purpose — that is how an \
             archive writes outside the folder it was extracted into.",
        );
    }
    out
}

/* ---------------------------------------------------------------------------
   Fixtures, shared with the tool layer's tests
--------------------------------------------------------------------------- */

#[cfg(test)]
pub(crate) mod test_support {
    use std::io::{Cursor, Write};

    /// A zip built here rather than fixtured, so a test can create entries with
    /// names no archiver would ever write — which is the whole point of the
    /// traversal tests.
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
}

#[cfg(test)]
mod tests {
    use super::test_support::zip_bytes;
    use super::*;
    use std::io::Write;

    fn workspace() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn the_container_decides_the_format_not_the_extension() {
        let path = Path::new("archive.zip");
        // A zip's magic bytes win over a `.tar` name.
        assert_eq!(Kind::detect(path, b"PK\x03\x04rest"), Some(Kind::Zip));
        assert_eq!(
            Kind::detect(Path::new("x.tar.gz"), &[0x1f, 0x8b, 0x08]),
            Some(Kind::TarGz)
        );
        // With no usable magic, the extension is the fallback.
        assert_eq!(Kind::detect(path, b"nothing"), Some(Kind::Zip));
        assert_eq!(Kind::detect(Path::new("x.tar"), b"nothing"), Some(Kind::Tar));
        assert_eq!(
            Kind::detect(Path::new("x.tgz"), b"nothing"),
            Some(Kind::TarGz)
        );
        assert_eq!(Kind::detect(Path::new("x.txt"), b"nothing"), None);
    }

    #[test]
    fn listing_reports_names_and_sizes() {
        let bytes = zip_bytes(&[("a.txt", "hello"), ("dir/b.txt", "world!")]);
        let (kind, entries, total) = list(Path::new("x.zip"), &bytes, 50).unwrap();
        assert_eq!(kind, Kind::Zip);
        assert_eq!(total, 2);
        let names: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "dir/b.txt"]);
        assert_eq!(entries[0].size, 5);

        let rendered = render_listing(kind, &entries, total);
        assert!(rendered.contains("a.txt"), "{rendered}");
        assert!(rendered.contains("dir/b.txt"), "{rendered}");
    }

    #[test]
    fn listing_says_when_it_truncated() {
        let bytes = zip_bytes(&[("a", "1"), ("b", "2"), ("c", "3")]);
        let (kind, entries, total) = list(Path::new("x.zip"), &bytes, 2).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(total, 3);
        let rendered = render_listing(kind, &entries, total);
        assert!(rendered.contains("1 more"), "{rendered}");
    }

    #[test]
    fn extraction_writes_inside_the_destination() {
        let dir = workspace();
        let bytes = zip_bytes(&[("a.txt", "hello"), ("nested/b.txt", "world")]);
        let report = extract(Path::new("x.zip"), &bytes, dir.path(), &[]).unwrap();

        assert_eq!(report.written, 2);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("nested/b.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn a_traversing_entry_is_skipped_and_said_so() {
        let dir = workspace();
        let bytes = zip_bytes(&[("../../escaped.txt", "nope"), ("fine.txt", "yes")]);
        let report = extract(Path::new("x.zip"), &bytes, dir.path(), &[]).unwrap();

        assert_eq!(report.written, 1, "only the safe entry is written");
        assert_eq!(report.skipped.len(), 1);
        assert!(
            report.skipped[0].contains("escaped.txt"),
            "{:?}",
            report.skipped
        );
        // And nothing landed outside.
        assert!(!dir.path().parent().unwrap().join("escaped.txt").exists());

        let rendered = render_report(&report);
        assert!(rendered.contains("escapes the destination"), "{rendered}");
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let dir = workspace();
        assert!(safe_join(dir.path(), "/etc/passwd").is_none());
        assert!(safe_join(dir.path(), "C:\\Windows\\system32\\evil.dll").is_none());
        assert!(safe_join(dir.path(), "\\\\server\\share\\evil").is_none());
    }

    #[test]
    fn backslash_traversal_is_caught_on_every_platform() {
        // The trap this guards: on Unix a backslash is an ordinary filename
        // character, so `..\..\x` is one long name rather than a traversal — and
        // the same archive extracted on Windows does real damage.
        let dir = workspace();
        assert!(safe_join(dir.path(), "..\\..\\evil.txt").is_none());
        assert!(safe_join(dir.path(), "a\\..\\..\\evil.txt").is_none());
        // And the forward-slash form, which is the obvious one.
        assert!(safe_join(dir.path(), "a/../../evil.txt").is_none());
        // A name that merely *contains* dots is fine.
        assert!(safe_join(dir.path(), "a..b/c.txt").is_some());
    }

    #[test]
    fn only_selects_by_prefix() {
        let bytes = zip_bytes(&[("src/a.rs", "1"), ("src/b.rs", "2"), ("docs/c.md", "3")]);
        let dir = workspace();
        let report = extract(Path::new("x.zip"), &bytes, dir.path(), &["src".into()]).unwrap();
        assert_eq!(report.written, 2);
        assert!(dir.path().join("src/a.rs").exists());
        assert!(!dir.path().join("docs/c.md").exists());
    }

    #[test]
    fn a_bomb_is_refused_by_the_declared_size_before_writing() {
        // Declared rather than produced: a 600 MB entry is refused without
        // inflating it, which is the point of checking the header first.
        let mut report = ExtractReport {
            written: 0,
            bytes: 0,
            skipped: Vec::new(),
            destination: "out".into(),
        };
        let error = budget(&report, MAX_EXTRACT_BYTES + 1, "huge.bin")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("decompression bomb") || error.contains("refused"),
            "{error}"
        );

        // And the entry count has its own ceiling.
        report.written = MAX_EXTRACT_ENTRIES;
        let error = budget(&report, 1, "many.bin").unwrap_err().to_string();
        assert!(error.contains("entries"), "{error}");
    }

    #[test]
    fn a_tar_round_trips_and_reports_its_entries() {
        let mut buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut buffer);
            let mut header = tar::Header::new_gnu();
            let body = b"tar contents";
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "inside.txt", &body[..])
                .unwrap();
            builder.finish().unwrap();
        }

        let (kind, entries, total) = list(Path::new("x.tar"), &buffer, 50).unwrap();
        assert_eq!(kind, Kind::Tar);
        assert_eq!(total, 1);
        assert_eq!(entries[0].path, "inside.txt");
        assert_eq!(entries[0].size, 12);
        // A tar entry has no per-entry compressed size, and says so with 0.
        assert_eq!(entries[0].compressed, 0);

        let dir = workspace();
        let report = extract(Path::new("x.tar"), &buffer, dir.path(), &[]).unwrap();
        assert_eq!(report.written, 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("inside.txt")).unwrap(),
            "tar contents"
        );
    }

    #[test]
    fn gzipped_tar_is_detected_and_read() {
        let mut raw = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut raw);
            let mut header = tar::Header::new_gnu();
            let body = b"gz body";
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, "gz.txt", &body[..]).unwrap();
            builder.finish().unwrap();
        }

        let mut gz = Vec::new();
        {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
            encoder.write_all(&raw).unwrap();
            encoder.finish().unwrap();
        }

        let dir = workspace();
        let report = extract(Path::new("x.tar.gz"), &gz, dir.path(), &[]).unwrap();
        assert_eq!(report.written, 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("gz.txt")).unwrap(),
            "gz body"
        );
    }

    #[test]
    fn a_file_that_is_not_an_archive_says_so() {
        let error = list(Path::new("notes.txt"), b"just some text", 50)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a zip"), "{error}");
        assert!(error.contains("notes.txt"), "{error}");
    }
}
