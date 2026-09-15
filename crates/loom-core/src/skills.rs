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

use std::path::Path;

use serde::Serialize;

use crate::{paths, Result};

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
    let directory = paths::loom_home()?.join("skills");
    let mut skills = list_in(&directory);
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(skills)
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
}
