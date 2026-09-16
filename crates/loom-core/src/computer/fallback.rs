//! Non-Windows stub: computer use is Windows-only in this build.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use super::{Capture, CaptureResult};
use crate::{Error, Result};

const UNSUPPORTED: &str = "computer use is Windows-only in this build";

fn unsupported<T>() -> Result<T> {
    Err(Error::Other(UNSUPPORTED.into()))
}

/// There are no hooks off Windows, so there is never a trip to consume.
pub struct TakeoverWatch;

impl TakeoverWatch {
    pub fn start() -> Result<Self> {
        unsupported()
    }
    pub fn take_trip(&self) -> bool {
        false
    }
    pub fn clear_trip(&self) {}
    pub fn idle_ms(&self) -> u64 {
        0
    }
    pub fn mark_input_now(&self) {}
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    pub fn stop(&self) {}
}

pub fn set_ignored_window(_hwnd: isize) {}

pub fn make_window_non_activating(_hwnd: isize) {}

pub fn trip_takeover_for_debug() {}

pub fn capture(_request: &Capture) -> Result<CaptureResult> {
    unsupported()
}

pub fn mouse(
    _action: &str,
    _x: i32,
    _y: i32,
    _to: Option<(i32, i32)>,
    _button: &str,
    _count: u32,
    _modifiers: &[String],
    _amount: i32,
    _horizontal: bool,
    _duration_ms: u32,
    _steps: u32,
) -> Result<String> {
    unsupported()
}

pub fn cursor_pos() -> Result<(i32, i32)> {
    unsupported()
}

pub fn check_input_target() -> Result<()> {
    unsupported()
}

pub fn named_key_code(_name: &str) -> Option<u16> {
    None
}

pub fn release(_buttons: [bool; 3], _keys: &[u16]) {}

pub fn key(_action: &str, _name: &str, _repeat: u32) -> Result<String> {
    unsupported()
}

pub fn combo(_keys: &[String], _repeat: u32) -> Result<String> {
    unsupported()
}

pub fn type_unicode(_text: &str) -> Result<String> {
    unsupported()
}

pub fn type_via_clipboard(_text: &str) -> Result<String> {
    unsupported()
}

pub fn clipboard(_arguments: &Value, _read_chars: usize) -> Result<String> {
    unsupported()
}

pub fn list_windows(_arguments: &Value) -> Result<String> {
    unsupported()
}

pub fn window(_arguments: &Value) -> Result<String> {
    unsupported()
}

pub fn focus_window_handle(_hwnd: isize) -> Result<String> {
    unsupported()
}

pub fn list_processes(_arguments: &Value) -> Result<String> {
    unsupported()
}

pub fn launch_app(_arguments: &Value) -> Result<String> {
    unsupported()
}

pub fn kill_process(_arguments: &Value) -> Result<String> {
    unsupported()
}

pub fn find_window(_title: &str) -> Result<Option<isize>> {
    unsupported()
}

pub fn ui_tree(
    _window: Option<&str>,
    _depth: u32,
    _max_nodes: usize,
) -> Result<(isize, Vec<super::UiNode>)> {
    unsupported()
}

pub fn ui_act(
    _hwnd: isize,
    _path: &[u32],
    _action: &str,
    _value: Option<&str>,
    _expected: Option<&str>,
) -> Result<String> {
    unsupported()
}

#[allow(dead_code)]
fn ordering_marker() -> Ordering {
    Ordering::Relaxed
}
