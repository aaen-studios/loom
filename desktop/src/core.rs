//! The Calx core, through its C boundary: JSON in with `send`, JSON out
//! through a callback on the core's own thread, parsed there and handed to the UI
//! thread over
//! a channel, so a burst of frames isn't parsed on the UI thread.

use std::sync::mpsc::{channel, Receiver, Sender};

extern "C" {
    fn calx_init();
    fn loom_core_start(
        config: *const u8,
        len: usize,
        on_event: extern "C" fn(*const u8, usize, *mut u8),
        ctx: *mut u8,
    ) -> *mut u8;
    fn loom_core_send(core: *mut u8, json: *const u8, len: usize);
    fn loom_core_stop(core: *mut u8);
    fn loom_mcp() -> i32;
}

extern "C" fn on_event(json: *const u8, len: usize, ctx: *mut u8) {
    // SAFETY: the core passes `len` bytes at `json`, and the ctx we gave it,
    // which points at a Sender that lives as long as the Core.
    let bytes = unsafe { std::slice::from_raw_parts(json, len) };
    let tx = unsafe { &*(ctx as *const Sender<serde_json::Value>) };
    if let Ok(v) = serde_json::from_slice(bytes) {
        let _ = tx.send(v);
    }
    crate::wake::wake();
}

pub struct Core {
    handle: *mut u8,
    _tx: Box<Sender<serde_json::Value>>,
}

// The handle is only used through the core's functions, which take any thread.
unsafe impl Send for Core {}

impl Core {
    /// Starts the core. Its events come out of the receiver.
    pub fn start(config: &str) -> (Core, Receiver<serde_json::Value>) {
        let (tx, rx) = channel();
        let tx = Box::new(tx);
        let ctx = &*tx as *const Sender<serde_json::Value> as *mut u8;
        // The Calx runtime starts once, however many cores (one for each server) start.
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| unsafe { calx_init() });
        let handle = unsafe { loom_core_start(config.as_ptr(), config.len(), on_event, ctx) };
        (Core { handle, _tx: tx }, rx)
    }

    pub fn send(&self, json: &str) {
        unsafe { loom_core_send(self.handle, json.as_ptr(), json.len()) }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe { loom_core_stop(self.handle) }
    }
}

/// Runs `loom mcp`, loom's own tools for an agent, on standard input and output,
/// as the instance in this app starts it. Gives the exit status.
pub fn mcp() -> i32 {
    // SAFETY: the Calx runtime is started once, before the core is called, and
    // nothing else in this process uses it.
    unsafe {
        calx_init();
        loom_mcp()
    }
}
