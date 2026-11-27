//! The Calx core, through its C boundary: JSON in with `send`, JSON out
//! through a callback on the core's own thread, parsed there and handed to the UI
//! thread over
//! a channel, so a burst of frames isn't parsed on the UI thread. A browser's frame
//! comes as its JSON head, a newline and the JPEG, and goes over a channel of its own.

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

/// A picture of an agent's browser, as the instance sent it: its head
/// (`browser.frame`'s `agent`, `width` and `height`) and the JPEG.
pub struct Page {
    pub head: serde_json::Value,
    pub jpeg: Vec<u8>,
}

/// Where the core's callback hands what it says.
struct Sinks {
    events: Sender<serde_json::Value>,
    pages: Sender<Page>,
}

/// A binary message from the core, its JSON head and what follows its newline, or
/// None if it isn't one.
pub fn page_of(bytes: &[u8]) -> Option<Page> {
    let nl = bytes.iter().position(|&b| b == b'\n')?;
    let head: serde_json::Value = serde_json::from_slice(&bytes[..nl]).ok()?;
    head.is_object().then(|| Page {
        head,
        jpeg: bytes[nl + 1..].to_vec(),
    })
}

extern "C" fn on_event(json: *const u8, len: usize, ctx: *mut u8) {
    // SAFETY: the core passes `len` bytes at `json`, and the ctx we gave it,
    // which points at the Sinks, which live as long as the Core.
    let bytes = unsafe { std::slice::from_raw_parts(json, len) };
    let sinks = unsafe { &*(ctx as *const Sinks) };
    match serde_json::from_slice(bytes) {
        Ok(v) => {
            let _ = sinks.events.send(v);
        }
        Err(_) => {
            if let Some(page) = page_of(bytes) {
                let _ = sinks.pages.send(page);
            }
        }
    }
    crate::wake::wake();
}

pub struct Core {
    handle: *mut u8,
    _sinks: Box<Sinks>,
    /// The browser frames that came, the newest last.
    pub pages: Receiver<Page>,
}

// The handle is only used through the core's functions, which take any thread.
unsafe impl Send for Core {}

/// The frame each core is given as it starts (`set_first_frame`).
static FIRST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Gives every core started from now `frame` first: the app's sound prefs, which a
/// server added later has from its start.
pub fn set_first_frame(frame: &str) {
    if let Ok(mut first) = FIRST.lock() {
        *first = frame.to_string();
    }
}

impl Core {
    /// Starts the core. Its events come out of the receiver, and its browser frames
    /// out of `pages`.
    pub fn start(config: &str) -> (Core, Receiver<serde_json::Value>) {
        let (tx, rx) = channel();
        let (pages_tx, pages) = channel();
        let sinks = Box::new(Sinks {
            events: tx,
            pages: pages_tx,
        });
        let ctx = &*sinks as *const Sinks as *mut u8;
        // The Calx runtime starts once, however many cores (one for each server) start.
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| unsafe { calx_init() });
        let handle = unsafe { loom_core_start(config.as_ptr(), config.len(), on_event, ctx) };
        // What every core is told as it starts: the sound prefs.
        if let Ok(first) = FIRST.lock() {
            if !first.is_empty() {
                unsafe { loom_core_send(handle, first.as_ptr(), first.len()) }
            }
        }
        (
            Core {
                handle,
                _sinks: sinks,
                pages,
            },
            rx,
        )
    }

    pub fn send(&self, json: &str) {
        unsafe { loom_core_send(self.handle, json.as_ptr(), json.len()) }
    }

    /// The newest browser frame that came since the last call, if any: the ones
    /// before it are no longer the page.
    pub fn newest_page(&self) -> Option<Page> {
        std::iter::from_fn(|| self.pages.try_recv().ok()).last()
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

#[cfg(test)]
mod tests {
    use super::page_of;

    #[test]
    fn a_binary_message_is_its_head_and_its_jpeg() {
        let mut m = br#"{"t":"browser.frame","agent":"a_1","width":1264,"height":705}"#.to_vec();
        m.push(b'\n');
        m.extend_from_slice(&[0xFF, 0xD8, b'\n', 0xFF, 0xD9]);
        let page = page_of(&m).expect("a page");
        assert_eq!(page.head["agent"], "a_1");
        assert_eq!(page.head["width"], 1264);
        assert_eq!(page.jpeg, [0xFF, 0xD8, b'\n', 0xFF, 0xD9]);
        assert!(page_of(b"not a frame").is_none());
        assert!(page_of(b"[1]\n\xFF").is_none());
    }
}
