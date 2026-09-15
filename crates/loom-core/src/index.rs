//! Workspace indexing: file collection, chunking, and similarity search.
//!
//! The index is per chat: chunks (relative path + text + embedding) live in
//! SQLite, and `search_workspace` embeds the query and ranks by cosine
//! similarity in-process.

use std::path::{Path, PathBuf};

use crate::db::Chunk;

/// Files larger than this are skipped (they are usually generated or binary).
const MAX_FILE_BYTES: u64 = 200_000;
const MAX_FILES: usize = 400;
const CHUNK_CHARS: usize = 1_600;
const CHUNK_OVERLAP: usize = 200;
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

const TEXT_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "rb", "php", "cs",
    "c", "h", "cpp", "hpp", "swift", "lua", "sh", "ps1", "bat", "sql", "toml", "yaml", "yml",
    "json", "md", "markdown", "txt", "css", "scss", "html", "htm", "xml", "vue", "svelte", "astro",
    "ex", "exs", "erl", "hs", "ml", "scala", "dart", "r", "jl", "tf", "dockerfile", "env", "cfg",
    "ini", "conf",
];

pub fn is_text_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        name.as_str(),
        "dockerfile" | "makefile" | "justfile" | ".gitignore" | ".env.example"
    ) {
        return true;
    }
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    TEXT_EXTENSIONS.contains(&extension.as_str())
}

/// Collects indexable files under `root`, skipping junk directories and
/// oversized files. Returns `(relative path, contents)`.
pub fn collect_files(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files
}

fn walk(root: &Path, directory: &Path, files: &mut Vec<(String, String)>) {
    if files.len() >= MAX_FILES {
        return;
    }
    let Ok(reader) = std::fs::read_dir(directory) else {
        return;
    };

    for entry in reader.flatten() {
        if files.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();

        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk(root, &path, files);
            continue;
        }

        if !is_text_file(&path) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        if contents.trim().is_empty() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        files.push((relative, contents));
    }
}

/// Splits text into chunks of roughly `target` characters, carrying the last
/// `overlap` characters of each chunk into the next one so context is not lost
/// at the boundary.
pub fn chunk_text(text: &str, target: usize, overlap: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if !current.is_empty() && current.len() + line.len() + 1 > target {
            chunks.push(std::mem::take(&mut current));

            if overlap > 0 {
                if let Some(previous) = chunks.last() {
                    let mut start = previous.len().saturating_sub(overlap);
                    while start < previous.len() && !previous.is_char_boundary(start) {
                        start += 1;
                    }
                    current.push_str(&previous[start..]);
                }
            }
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        chunks.push(current);
    }

    chunks
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .collect()
}

/// Cosine similarity; zero vectors score zero.
pub fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

/// Ranked chunks for a query embedding.
pub fn rank<'a>(chunks: &'a [Chunk], query: &[f32], limit: usize) -> Vec<(&'a Chunk, f32)> {
    let mut scored: Vec<(&Chunk, f32)> = chunks
        .iter()
        .map(|chunk| {
            let vector = crate::embeddings::decode(&chunk.embedding);
            (chunk, cosine(&vector, query))
        })
        .filter(|(_, score)| *score > 0.2)
        .collect();

    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    scored
}

/// Chunk + file budget report for the UI.
pub fn plan(root: &Path) -> (usize, usize) {
    let files = collect_files(root);
    let chunks = files
        .iter()
        .map(|(_, content)| chunk_text(content, CHUNK_CHARS, CHUNK_OVERLAP).len())
        .sum();
    (files.len(), chunks)
}

pub fn chunk_settings() -> (usize, usize) {
    (CHUNK_CHARS, CHUNK_OVERLAP)
}

pub fn root_of(workdir: &str) -> PathBuf {
    PathBuf::from(workdir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_overlap_and_stay_bounded() {
        let text = (0..200)
            .map(|index| format!("line {index} with some content"))
            .collect::<Vec<_>>()
            .join("\n");

        let chunks = chunk_text(&text, 400, 50);
        assert!(chunks.len() > 5);
        assert!(chunks.iter().all(|chunk| chunk.len() < 900));
        // The overlap is carried verbatim: the next chunk starts with the tail
        // of the previous one.
        for pair in chunks.windows(2) {
            let previous = &pair[0];
            let start = previous.len() - 50;
            assert!(pair[1].starts_with(&previous[start..]));
        }
    }

    #[test]
    fn short_text_is_one_chunk() {
        let chunks = chunk_text("just one line", 400, 50);
        assert_eq!(chunks, vec!["just one line\n"]);
        assert!(chunk_text("", 400, 50).is_empty());
    }

    #[test]
    fn cosine_scores_are_sane() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn collects_text_files_and_skips_junk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("notes.md"), "# notes").unwrap();
        std::fs::write(dir.path().join("image.png"), "not really").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/dep.js"), "module").unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/config"), "x").unwrap();

        let files = collect_files(dir.path());
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();

        assert!(names.contains(&"main.rs"));
        assert!(names.contains(&"notes.md"));
        assert!(!names.iter().any(|name| name.contains("node_modules")));
        assert!(!names.iter().any(|name| name.contains(".git")));
        assert!(!names.contains(&"image.png"));
    }

    #[test]
    fn ranking_prefers_closer_vectors_and_thresholds_noise() {
        let chunks = vec![
            Chunk {
                id: "1".into(),
                path: "a.rs".into(),
                content: "close".into(),
                embedding: crate::embeddings::encode(&[1.0, 0.0]),
            },
            Chunk {
                id: "2".into(),
                path: "b.rs".into(),
                content: "far".into(),
                embedding: crate::embeddings::encode(&[0.0, 1.0]),
            },
        ];

        let ranked = rank(&chunks, &[1.0, 0.2], 5);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].0.path, "a.rs");
        assert!(ranked[0].1 > 0.9);
    }
}
