//! Small filesystem helpers shared by config and (later) storage.

use std::path::Path;

use crate::{Error, Result};

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
        assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
    }
}
