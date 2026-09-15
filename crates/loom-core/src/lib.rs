//! Loom engine.
//!
//! Owns everything the app does beyond drawing pixels: configuration, storage,
//! provider adapters, streaming sessions, tools, and the updater. The Tauri
//! shell in `src-tauri` is a thin command layer over this crate so a future
//! CLI or headless mode can reuse it unchanged.

pub mod attachments;
pub mod catalog;
pub mod config;
pub mod db;
pub mod engine;
pub mod embeddings;
pub mod error;
pub mod export;
pub mod fsutil;
pub mod images;
pub mod index;
pub mod mcp;
pub mod paths;
pub mod persona;
pub mod provider;
pub mod providers;
pub mod secrets;
pub mod skills;
pub mod tools;
pub mod updater;
pub mod web;
pub mod workspace;

pub use error::{Error, Result};







