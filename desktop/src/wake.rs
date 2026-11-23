//! Wakes the window's task when something comes for it: a core's event, a device
//! answer, a page's message, a tray click or a notification's. Between those it looks
//! a few times a second, for what nothing announces, instead of 125 times a second.

use std::sync::OnceLock;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use gpui_kit::BackgroundExecutor;

fn pair() -> &'static (Sender<()>, Receiver<()>) {
    static PAIR: OnceLock<(Sender<()>, Receiver<()>)> = OnceLock::new();
    // One waiting wake is enough: wakes that come while one waits are the same wake.
    PAIR.get_or_init(|| async_channel::bounded(1))
}

/// Something came for the window's task. Any thread can say so.
pub fn wake() {
    let _ = pair().0.try_send(());
}

/// Waits for a wake, or `most` at the longest.
pub async fn wait(executor: &BackgroundExecutor, most: Duration) {
    let woken = async {
        let _ = pair().1.recv().await;
    };
    futures_lite::future::or(woken, executor.timer(most)).await;
}
