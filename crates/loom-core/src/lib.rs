//! Loom engine.
//!
//! Owns everything the app does beyond drawing pixels: configuration, storage,
//! provider adapters, streaming sessions, tools, and the updater. The Tauri
//! shell in `src-tauri` is a thin command layer over this crate so a future
//! CLI or headless mode can reuse it unchanged.

pub mod config;
pub mod error;
pub mod fsutil;
pub mod paths;

pub use error::{Error, Result};
