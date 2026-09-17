//! Retention for backgrounds the user picks in Settings.
//!
//! Every pick is copied into `~/.loom/backgrounds` under a fresh uuid name, so
//! without a sweep the folder keeps every image or video ever chosen. The rule
//! is the one in use plus the two most recent picks before it — three files,
//! listed for the picker so a background you chose last week is one click away
//! rather than something you have to find on disk again.

use std::path::{Path, PathBuf};

/// How many picked backgrounds survive a sweep: the one in use plus two.
pub const KEEP: usize = 3;

/// Which kind of media a stored background is.
///
/// Separate from `config::BackgroundKind`, which also has a `Builtin` variant
/// that means nothing for a file on disk. This is derived from the file
/// extension, so it is a best-effort classification rather than a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
}

/// One file in the backgrounds folder, as the settings picker needs it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredBackground {
    /// Absolute path, which is what the config stores and the asset protocol
    /// serves.
    pub path: String,
    /// The name the user picked the file under, without the uuid prefix.
    pub name: String,
    pub kind: MediaKind,
    pub bytes: u64,
    /// Whether this is the background currently in use.
    pub in_use: bool,
}

/// The extension a stored background has, lowercased.
fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Classifies a stored file by extension.
///
/// Unknown extensions are reported as images: the picker only ever copies the
/// formats it offered, so an unrecognised one is most likely a hand-placed
/// still rather than a video, and an image thumbnail of a video is a broken
/// tile while a video element showing a still is merely odd.
pub fn kind_of(path: &Path) -> MediaKind {
    match extension_of(path).as_str() {
        "mp4" | "webm" | "mkv" | "mov" | "m4v" | "ogv" => MediaKind::Video,
        _ => MediaKind::Image,
    }
}

/// The original name, with the uuid prefix this module adds removed.
///
/// Files are stored as `{uuid}-{original}`. The prefix is checked to be a real
/// uuid before slicing rather than trusting the shape, so a file dropped into
/// the folder by hand keeps its whole name instead of losing 37 characters to a
/// guess.
fn display_name(file_name: &str) -> String {
    const UUID_LEN: usize = 36;
    if file_name.len() > UUID_LEN + 1 {
        let prefix = &file_name[..UUID_LEN];
        if uuid::Uuid::parse_str(prefix).is_ok() && file_name.as_bytes()[UUID_LEN] == b'-' {
            let rest = &file_name[UUID_LEN + 1..];
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    file_name.to_string()
}

/// Every stored background, newest first, with `in_use` set for `current`.
///
/// Newest-first is the same order `prune` keeps in, so the list reads as "the
/// one in use, then the ones it will retire after" rather than as an arbitrary
/// folder listing.
pub fn list(dir: &Path, current: Option<&Path>) -> Vec<StoredBackground> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .map(|entry| {
            let meta = entry.metadata().ok();
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            (modified, entry.path(), bytes)
        })
        .collect();

    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    files
        .into_iter()
        .map(|(_, path, bytes)| StoredBackground {
            in_use: current == Some(path.as_path()),
            name: path
                .file_name()
                .map(|n| display_name(&n.to_string_lossy()))
                .unwrap_or_default(),
            kind: kind_of(&path),
            bytes,
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// Whether `candidate` is a file directly inside `dir`.
///
/// Guards the "use this stored background" command. The path arrives from the
/// frontend, and without a containment check a stale or edited entry could name
/// any file on the machine — which the asset protocol would then happily serve
/// into the window. Both sides are canonicalised, so symlinks and `..` are
/// resolved before the comparison rather than papered over with a string prefix
/// check.
pub fn is_managed(dir: &Path, candidate: &Path) -> bool {
    let (Ok(dir), Ok(candidate)) = (dir.canonicalize(), candidate.canonicalize()) else {
        return false;
    };
    candidate.parent() == Some(dir.as_path())
}

/// Delete all but the newest `keep` files in `dir`, always sparing `protect`.
///
/// `protect` is the file the config points at — the pick that triggered the
/// sweep. It is normally the newest by modification time anyway, but naming it
/// makes the invariant explicit: a filesystem that carries the source's mtime
/// through a copy must not be able to cost us the background in use. When
/// `protect` names a file recency would have deleted, it survives and the
/// budget for the others shrinks by one, so the folder still lands at `keep`
/// files rather than `keep` + 1.
///
/// Newest means most recently modified, which is when the pick was copied.
/// Name breaks ties, so the outcome never depends on enumeration order.
/// Directories are left alone. Removal is best-effort: a file that resists
/// deletion — the webview may still hold the outgoing background open — is
/// kept rather than failing the pick that triggered the sweep. Returns the
/// paths that were deleted.
pub fn prune(dir: &Path, keep: usize, protect: Option<&Path>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (modified, entry.path())
        })
        .collect();

    // Newest first.
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let pinned = protect.filter(|wanted| files.iter().any(|(_, path)| path.as_path() == *wanted));
    let budget = if pinned.is_some() {
        keep.saturating_sub(1)
    } else {
        keep
    };

    let mut removed = Vec::new();
    let mut held = 0usize;
    for (_, path) in &files {
        if Some(path.as_path()) == pinned {
            continue;
        }
        if held < budget {
            held += 1;
            continue;
        }
        if std::fs::remove_file(path).is_ok() {
            removed.push(path.clone());
        }
    }
    removed
}

/// Stamp `path` as modified now, so "the newest file" means "the most recent
/// pick" everywhere.
///
/// `fs::copy` carries the source's timestamps across on Windows, so a freshly
/// copied background can look older than a file picked days ago and be swept
/// out from under the config that points at it.
pub fn mark_picked(path: &Path) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("loom-bg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Write `name` into `dir` and stamp its mtime `age` seconds in the past, so
    /// "newest" is exact rather than an artefact of clock resolution.
    fn pick(dir: &Path, name: &str, age: u64) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"x").expect("write");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open");
        let when = SystemTime::now() - Duration::from_secs(age);
        file.set_times(std::fs::FileTimes::new().set_modified(when))
            .expect("set mtime");
        path
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn keeps_the_newest_three_picks() {
        let dir = scratch("keeps_three");
        let current = pick(&dir, "d.png", 0);
        pick(&dir, "c.png", 10);
        pick(&dir, "b.png", 20);
        pick(&dir, "a.png", 30);
        pick(&dir, "oldest.png", 40);

        let removed = prune(&dir, KEEP, Some(current.as_path()));

        assert_eq!(removed.len(), 2);
        assert_eq!(names(&dir), vec!["b.png", "c.png", "d.png"]);
    }

    /// The file the config points at survives even when recency would have
    /// deleted it, and the folder still ends up at three files, not four.
    #[test]
    fn never_deletes_the_protected_file() {
        let dir = scratch("protects");
        pick(&dir, "newer.png", 0);
        pick(&dir, "middle.png", 10);
        pick(&dir, "third.png", 20);
        pick(&dir, "fourth.png", 30);
        let in_use = pick(&dir, "in-use.png", 40);

        let removed = prune(&dir, KEEP, Some(in_use.as_path()));

        assert_eq!(removed.len(), 2);
        assert_eq!(names(&dir), vec!["in-use.png", "middle.png", "newer.png"]);
    }

    /// With nothing in use the rule falls back to the three newest.
    #[test]
    fn keeps_three_picks_when_nothing_is_in_use() {
        let dir = scratch("unpinned");
        pick(&dir, "c.png", 0);
        pick(&dir, "b.png", 10);
        pick(&dir, "a.png", 20);
        pick(&dir, "gone.png", 30);

        prune(&dir, KEEP, None);

        assert_eq!(names(&dir), vec!["a.png", "b.png", "c.png"]);
    }

    /// A copy that inherits an old mtime must still count as the newest pick.
    #[test]
    fn mark_picked_makes_a_stale_copy_the_newest() {
        let dir = scratch("mark");
        let stale = pick(&dir, "copied.png", 3600);
        pick(&dir, "older.png", 7200);
        pick(&dir, "oldest.png", 10800);

        mark_picked(&stale).expect("stamp");
        prune(&dir, KEEP, Some(stale.as_path()));

        assert_eq!(names(&dir), vec!["copied.png", "older.png", "oldest.png"]);
    }

    #[test]
    fn lists_newest_first_and_marks_the_one_in_use() {
        let dir = scratch("lists");
        // Named the way the picker stores them: `{uuid}-{original}`. The uuid
        // must be first, which is what this test caught in its first version —
        // the fixture had an extra prefix, so `display_name` correctly refused
        // to strip anything and the assertion failed on the fixture, not the
        // code.
        pick(&dir, "11111111-1111-1111-1111-111111111111-old.png", 30);
        pick(&dir, "22222222-2222-2222-2222-222222222222-newer.jpg", 0);
        let middle = pick(&dir, "33333333-3333-3333-3333-333333333333-mid.webp", 10);

        let listed = list(&dir, Some(middle.as_path()));

        assert_eq!(listed.len(), 3);
        // Newest first, and the uuid prefix is not part of the name.
        let names: Vec<&str> = listed.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["newer.jpg", "mid.webp", "old.png"]);
        // Exactly one is marked, and it is the one named.
        assert_eq!(listed.iter().filter(|b| b.in_use).count(), 1);
        assert!(listed.iter().find(|b| b.in_use).unwrap().name == "mid.webp");
        assert_eq!(listed[1].kind, MediaKind::Image);
        assert!(listed[1].bytes > 0);
    }

    #[test]
    fn classifies_video_by_extension() {
        assert_eq!(kind_of(Path::new("a.mp4")), MediaKind::Video);
        assert_eq!(kind_of(Path::new("a.WEBM")), MediaKind::Video);
        assert_eq!(kind_of(Path::new("a.png")), MediaKind::Image);
        // Not a format the picker offers, so it is treated as a still rather
        // than guessed at as a video.
        assert_eq!(kind_of(Path::new("a.xyz")), MediaKind::Image);
        assert_eq!(kind_of(Path::new("noext")), MediaKind::Image);
    }

    #[test]
    fn reads_a_name_without_a_uuid_prefix() {
        // The real shape.
        assert_eq!(
            display_name("6f1c1f9e-63a3-4f0e-8a4a-0f2f8a5f1c2b-sunset over water.png"),
            "sunset over water.png"
        );
        // A file dropped in by hand keeps its whole name.
        assert_eq!(display_name("sunset.png"), "sunset.png");
        // A name that merely *looks* prefixed is not sliced.
        assert_eq!(display_name("not-a-uuid-really-here-at-all.png"), "not-a-uuid-really-here-at-all.png");
        assert_eq!(display_name("short.png"), "short.png");
    }

    #[test]
    fn only_treats_files_inside_the_folder_as_managed() {
        let dir = scratch("managed");
        let inside = pick(&dir, "a.png", 0);
        let outside = std::env::temp_dir().join(format!("loom-bg-outside-{}.png", std::process::id()));
        std::fs::write(&outside, b"x").expect("write outside");

        assert!(is_managed(&dir, &inside));
        assert!(!is_managed(&dir, &outside));
        // A traversal attempt resolves to the parent, not the folder.
        assert!(!is_managed(&dir, &dir.join("..").join("elsewhere.png")));
        // A directory is not a background.
        assert!(!is_managed(&dir, &dir));

        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn ignores_directories_and_a_missing_folder() {
        let dir = scratch("dirs");
        pick(&dir, "a.png", 0);
        std::fs::create_dir_all(dir.join("nested")).expect("nested dir");

        prune(&dir, KEEP, None);

        assert!(dir.join("nested").is_dir());
        assert_eq!(names(&dir), vec!["a.png", "nested"]);

        let missing = dir.join("nowhere");
        assert!(prune(&missing, KEEP, None).is_empty());
    }
}
