//! Skills: markdown files in `~/.loom/skills/` that expand into prompts.
//!
//! Each file may start with a tiny frontmatter block:
//!
//! ```text
//! ---
//! name: Code review
//! description: Review a diff for bugs
//! ---
//! <prompt body>
//! ```
//!
//! Writing is validated here so the model-driven `write_skill` tool and the
//! Settings editor share one implementation.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{fsutil, paths, Error, Result};

/// Largest skill body accepted (64 KiB). The frontmatter is extra.
pub const MAX_BODY: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// File stem, used for invocation (`/review`).
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub path: String,
}

/// Lists skills from the standard directory (missing directory → empty list).
pub fn list() -> Result<Vec<Skill>> {
    let directory = paths::skills_dir()?;
    let mut skills = list_in(&directory);
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(skills)
}

/// The one gate that keeps a skill id from escaping the skills folder. Ids
/// become file names, so anything but lowercase letters, digits and dashes is
/// refused (and `..`, `/`, `\` and leading dots all fail by construction).
pub fn validate_id(id: &str) -> Result<()> {
    let mut chars = id.chars();
    let valid = match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {
            id.len() <= 48
                && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        }
        _ => false,
    };
    if valid {
        return Ok(());
    }
    Err(Error::other(format!(
        "invalid skill id \"{id}\": use lowercase letters, digits and dashes, start \
         with a letter or digit, and keep it at most 48 characters"
    )))
}

/// `<directory>/<id>.md`, after validating the id.
pub fn path_for(directory: &Path, id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    Ok(directory.join(format!("{id}.md")))
}

/// Writes `~/.loom/skills/<id>.md`, copying an existing file to
/// `~/.loom/backups/skills/` first. Returns the path written.
pub fn write(id: &str, name: &str, description: &str, body: &str) -> Result<String> {
    let directory = paths::skills_dir()?;
    write_in(&directory, id, name, description, body)
}

/// Path-explicit variant (tests, portable installs).
pub fn write_in(
    directory: &Path,
    id: &str,
    name: &str,
    description: &str,
    body: &str,
) -> Result<String> {
    if body.len() > MAX_BODY {
        return Err(Error::other(format!(
            "skill body is {} bytes, larger than the {MAX_BODY} byte limit",
            body.len()
        )));
    }

    let path = path_for(directory, id)?;
    if path.exists() {
        backup_existing(&path, id);
    }

    // An empty name would parse back as blank rather than falling back to the
    // id, so default it here.
    let name = if name.trim().is_empty() { id } else { name };

    let mut file = String::new();
    file.push_str("---\n");
    file.push_str(&format!("name: {}\n", sanitize_meta(name)));
    file.push_str(&format!("description: {}\n", sanitize_meta(description)));
    file.push_str("---\n");
    file.push_str(body);
    if !file.ends_with('\n') {
        file.push('\n');
    }
    fsutil::atomic_write(&path, file.as_bytes())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Reads one skill file from the standard directory.
pub fn read(id: &str) -> Result<Skill> {
    read_in(&paths::skills_dir()?, id)
}

/// Path-explicit variant (tests, portable installs).
pub fn read_in(directory: &Path, id: &str) -> Result<Skill> {
    let path = path_for(directory, id)?;
    let raw = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
    let (meta, body) = split_frontmatter(&raw);
    Ok(Skill {
        id: id.to_string(),
        name: lookup(&meta, "name").unwrap_or_else(|| id.to_string()),
        description: lookup(&meta, "description").unwrap_or_default(),
        prompt: body.trim().to_string(),
        path: path.to_string_lossy().into_owned(),
    })
}

/// Deletes `~/.loom/skills/<id>.md`. A missing file is an error the caller
/// can read and act on.
pub fn delete(id: &str) -> Result<()> {
    delete_in(&paths::skills_dir()?, id)
}

/// Path-explicit variant (tests, portable installs).
pub fn delete_in(directory: &Path, id: &str) -> Result<()> {
    let path = path_for(directory, id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(Error::other(format!("no skill named \"{id}\"")))
        }
        Err(e) => Err(Error::io(&path, e)),
    }
}

fn lookup(meta: &[(String, String)], key: &str) -> Option<String> {
    meta.iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

/// Frontmatter values are single-line; newlines would break the block.
fn sanitize_meta(value: &str) -> String {
    value.replace(['\n', '\r'], " ").trim().to_string()
}

/// Best effort: a failed backup never blocks the write.
fn backup_existing(path: &Path, id: &str) {
    let Ok(directory) = paths::backups_dir() else {
        return;
    };
    let directory = directory.join("skills");
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let target = directory.join(format!("{id}-{}.md", fsutil::utc_stamp()));
    let _ = std::fs::copy(path, target);
}

/// Path-explicit variant (tests, portable installs).
pub fn list_in(directory: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    let Ok(reader) = std::fs::read_dir(directory) else {
        return skills;
    };

    for entry in reader.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (meta, body) = split_frontmatter(&raw);

        let name = meta
            .iter()
            .find(|(key, _)| key == "name")
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| id.clone());
        let description = meta
            .iter()
            .find(|(key, _)| key == "description")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();

        skills.push(Skill {
            id,
            name,
            description,
            prompt: body.trim().to_string(),
            path: path.to_string_lossy().into_owned(),
        });
    }

    skills
}

/// Splits optional `---` frontmatter from the body. Also accepts the whole
/// file as the prompt when there is no frontmatter.
pub fn split_frontmatter(raw: &str) -> (Vec<(String, String)>, String) {
    let trimmed = raw.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (Vec::new(), raw.to_string());
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), raw.to_string());
    };

    let header = &rest[..end];
    let body = rest[end + 4..].to_string();

    let mut meta = Vec::new();
    for line in header.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            meta.push((
                key.trim().to_ascii_lowercase(),
                value.trim().trim_matches('"').to_string(),
            ));
        }
    }

    (meta, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points `LOOM_HOME` at a temp dir for the duration of `run`.
    fn home<T>(run: impl FnOnce(&Path) -> T) -> T {
        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());
        let outcome = run(dir.path());
        std::env::remove_var("LOOM_HOME");
        outcome
    }

    #[test]
    fn frontmatter_is_parsed() {
        let raw = "---\nname: Review\ndescription: Check a diff\n---\nLook for bugs.";
        let (meta, body) = split_frontmatter(raw);
        assert_eq!(meta[0], ("name".to_string(), "Review".to_string()));
        assert_eq!(meta[1].1, "Check a diff");
        assert_eq!(body.trim(), "Look for bugs.");
    }

    #[test]
    fn files_without_frontmatter_still_work() {
        let (meta, body) = split_frontmatter("Just a prompt.");
        assert!(meta.is_empty());
        assert_eq!(body, "Just a prompt.");
    }

    #[test]
    fn broken_frontmatter_falls_back_to_body() {
        let (meta, body) = split_frontmatter("---\nname: nope");
        assert!(meta.is_empty());
        assert!(body.contains("name: nope"));
    }

    #[test]
    fn listing_reads_markdown_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("review.md"),
            "---\nname: Review\ndescription: d\n---\nBody",
        )
        .unwrap();
        std::fs::write(dir.path().join("ignored.txt"), "x").unwrap();

        let skills = list_in(dir.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "review");
        assert_eq!(skills[0].name, "Review");
        assert_eq!(skills[0].prompt, "Body");
    }

    #[test]
    fn write_read_delete_round_trip_under_loom_home() {
        home(|_| {
            write("review", "Review", "Check a diff", "Look for bugs.").unwrap();

            let skill = read("review").unwrap();
            assert_eq!(skill.id, "review");
            assert_eq!(skill.name, "Review");
            assert_eq!(skill.description, "Check a diff");
            assert_eq!(skill.prompt, "Look for bugs.");
            // The frontmatter is the shape the existing parser reads.
            assert!(list().unwrap().iter().any(|entry| entry.id == "review"));

            delete("review").unwrap();
            assert!(read("review").is_err());
            // A missing file is a readable error, not a silent success.
            assert!(delete("review").is_err());
        });
    }

    #[test]
    fn invalid_ids_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        for id in [
            "../evil",
            "Foo Bar",
            "a/b",
            ".hidden",
            "-leading",
            "trailing-",
            "",
        ] {
            assert!(validate_id(id).is_err(), "{id} should be refused");
            assert!(write_in(dir.path(), id, "n", "", "body").is_err(), "{id}");
            assert!(read_in(dir.path(), id).is_err(), "{id}");
            assert!(delete_in(dir.path(), id).is_err(), "{id}");
        }
        // Nothing escaped the directory.
        assert!(!dir.path().parent().unwrap().join("evil").exists());
    }

    #[test]
    fn oversized_bodies_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let body = "x".repeat(1024 * 1024);

        let error = write_in(dir.path(), "big", "Big", "", &body)
            .unwrap_err()
            .to_string();
        assert!(error.contains("64"), "{error}");
        assert!(!dir.path().join("big.md").exists());
    }

    #[test]
    fn overwriting_a_skill_lands_a_backup() {
        home(|home_dir| {
            write("review", "Review", "", "one").unwrap();
            write("review", "Review", "", "two").unwrap();
            assert_eq!(read("review").unwrap().prompt, "two");

            let backups = home_dir.join("backups").join("skills");
            let entries: Vec<String> = std::fs::read_dir(&backups)
                .unwrap()
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(entries.len(), 1, "{entries:?}");
            assert!(
                entries[0].starts_with("review-") && entries[0].ends_with(".md"),
                "{entries:?}"
            );
        });
    }

    #[test]
    fn a_name_less_write_falls_back_to_the_id() {
        let dir = tempfile::tempdir().unwrap();
        write_in(dir.path(), "review", "  ", "", "body").unwrap();
        assert_eq!(read_in(dir.path(), "review").unwrap().name, "review");
    }
}
