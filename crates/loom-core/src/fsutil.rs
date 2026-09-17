//! Small filesystem helpers shared by config and (later) storage.

use std::path::Path;

use crate::{Error, Result};

/// Folders a file picker never wants to walk: build output and dependency
/// trees, all of which are large, generated, and identical to something the
/// user already has.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "out",
    "vendor",
    "__pycache__",
    ".venv",
];

/// How deep the walk will go. A guard rather than a policy: a directory tree
/// that loops through a junction has no bottom, and a picker must not hang.
const MAX_DEPTH: usize = 24;

/// Every file under `root`, as `/`-separated paths relative to it.
///
/// Paths only — nothing is opened, so this is cheap enough to run when a picker
/// opens rather than needing an index. Sorted, so the list does not depend on
/// the order the filesystem happens to hand directories back in, and capped at
/// `limit` so a huge repository cannot push a megabyte of strings through the
/// IPC boundary.
pub fn walk_files(root: &Path, limit: usize) -> Vec<String> {
    let mut files = Vec::new();
    walk_into(root, root, 0, limit, &mut files);
    files.sort();
    files
}

fn walk_into(
    root: &Path,
    directory: &Path,
    depth: usize,
    limit: usize,
    files: &mut Vec<String>,
) {
    if depth >= MAX_DEPTH || files.len() >= limit {
        return;
    }
    let Ok(reader) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in reader.flatten() {
        if files.len() >= limit {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // A dot-directory is configuration or a checkout; either way it is not
        // something to offer as "@" in a prompt.
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk_into(root, &path, depth + 1, limit, files);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        files.push(relative);
    }
}

/// UTC timestamp for file names, e.g. `20260915-123456Z`. Fixed width, so
/// names sort chronologically with a plain string compare.
pub fn utc_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}Z",
        time / 3_600,
        (time % 3_600) / 60,
        time % 60
    )
}

/// Howard Hinnant's civil-from-days algorithm (proleptic Gregorian).
pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The inverse of [`civil_from_days`]: days since 1970-01-01 for a civil date.
/// Only the scheduler tests need it, to name fixed dates as epoch days.
#[cfg(test)]
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Writes `bytes` to `path` atomically: a sibling temp file is written and
/// flushed first, then renamed over the destination. Prevents torn config
/// files if the process dies mid-write.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let mut tmp = path.to_path_buf();
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    tmp.set_file_name(format!(".{file_name}.tmp"));

    std::fs::write(&tmp, bytes).map_err(|e| Error::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(path, e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamps_are_fixed_width() {
        let stamp = super::utc_stamp();
        assert_eq!(stamp.len(), 16, "{stamp}");
        assert!(stamp.ends_with('Z'), "{stamp}");
    }

    #[test]
    fn creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/file.txt");

        atomic_write(&path, b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn overwrites_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");

        atomic_write(&path, b"one").unwrap();
        atomic_write(&path, b"two").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "file.txt")
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }
}
