//! Loom engine.
//!
//! Owns everything the app does beyond drawing pixels: configuration, storage,
//! provider adapters, streaming sessions, tools, and the updater. The Tauri
//! shell in `src-tauri` is a thin command layer over this crate so a future
//! CLI or headless mode can reuse it unchanged.

pub mod attachments;
pub mod backgrounds;
pub mod browser;
pub mod catalog;
pub mod computer;
pub mod condense;
pub mod config;
pub mod context;
pub mod db;
mod db_reentry;
pub mod dock;
pub mod edit;
pub mod embeddings;
pub mod engine;
pub mod error;
pub mod export;
pub mod external;
pub mod fsutil;
pub mod git;
pub mod harness;
pub mod images;
pub mod index;
pub mod jobs;
pub mod mcp;
pub mod memory;
pub mod paths;
pub mod persona;
pub mod process;
pub mod provider;
pub mod providers;
pub mod pty;
pub mod screen;
pub mod secrets;
pub mod skills;
pub mod tools;
pub mod ui_guide;
pub mod updater;
pub mod usage;
pub mod voice;
pub mod web;
pub mod workspace;

pub use error::{Error, Result};
