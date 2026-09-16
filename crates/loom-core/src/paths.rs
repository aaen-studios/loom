//! Canonical filesystem locations.
//!
//! Everything lives in a single home directory (`~/.loom` by default) so a
//! future CLI can share config, history, and assets with the desktop app.
//! `LOOM_HOME` overrides the location, which tests and portable installs use.

use std::path::PathBuf;

use crate::{Error, Result};

/// The Loom home directory. Does not touch the filesystem.
pub fn loom_home() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("LOOM_HOME") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    Ok(home.join(".loom"))
}

/// `~/.loom/config.json`
pub fn config_path() -> Result<PathBuf> {
    Ok(loom_home()?.join("config.json"))
}

/// `~/.loom/loom.db` (SQLite, populated from M1 onwards)
pub fn database_path() -> Result<PathBuf> {
    Ok(loom_home()?.join("loom.db"))
}

/// `~/.loom/logs/`
pub fn logs_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("logs"))
}

/// `~/.loom/backgrounds/` (user-picked in-app backgrounds)
pub fn backgrounds_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("backgrounds"))
}

/// `~/.loom/skills/` (markdown skills, one file per skill)
pub fn skills_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("skills"))
}

/// `~/.loom/backups/` (pre-write snapshots of config.json, and overwritten
/// skill files)
pub fn backups_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("backups"))
}

/// `~/.loom/cache/`
pub fn cache_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("cache"))
}

/// `~/.loom/scratch/` — the stand-in workspace for chats that have not picked a
/// folder. Disposable: the model is told not to use it unless explicitly asked.
pub fn scratch_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("scratch"))
}

/// Creates the home directory and its standard subdirectories.
pub fn ensure_home() -> Result<PathBuf> {
    let home = loom_home()?;
    for dir in [
        home.clone(),
        home.join("logs"),
        home.join("backgrounds"),
        home.join("cache"),
        home.join("skills"),
        home.join("backups"),
        home.join("scratch"),
    ] {
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    }
    Ok(home)
}

/// Serialises tests that point `LOOM_HOME` at a temporary directory. One lock
/// for the whole crate: module-local locks cannot see each other, and the test
/// runner is parallel.
#[cfg(test)]
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
