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

/// `~/.loom/cache/`
pub fn cache_dir() -> Result<PathBuf> {
    Ok(loom_home()?.join("cache"))
}

/// Creates the home directory and its standard subdirectories.
pub fn ensure_home() -> Result<PathBuf> {
    let home = loom_home()?;
    for dir in [
        home.clone(),
        home.join("logs"),
        home.join("backgrounds"),
        home.join("cache"),
    ] {
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    }
    Ok(home)
}
