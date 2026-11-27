//! The mic and the speaker for loom's voice (PLAN.md, "Voice"). The app only records
//! and plays: what the mic hears goes to the core as 16-bit PCM about every 100 ms
//! (`voice.audio`), which hears it with the engine the person picked, and the speech
//! the core writes (`core.speak`) is played here. The core decides everything else.

use std::sync::mpsc::{channel, Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// The mic while it records: what it heard comes out of `chunks`, each 16-bit
/// little-endian PCM at `rate`, `channels` interleaved. Dropping it stops it.
pub struct Mic {
    stop: Sender<()>,
    pub chunks: Receiver<Vec<u8>>,
    pub rate: u32,
    pub channels: u16,
}

/// Samples as 16-bit little-endian bytes, from any of the mic's formats.
fn push_i16(out: &mut Vec<u8>, s: i16) {
    out.extend_from_slice(&s.to_le_bytes());
}

fn f32_to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Gathers what the mic hears into pieces of about 100 ms, each sent as it fills.
struct Gather {
    piece: Vec<u8>,
    bytes: usize,
    to: Sender<Vec<u8>>,
}

impl Gather {
    fn take(&mut self, more: impl Iterator<Item = i16>) {
        for s in more {
            push_i16(&mut self.piece, s);
        }
        if self.piece.len() >= self.bytes {
            let _ = self.to.send(std::mem::take(&mut self.piece));
            crate::wake::wake();
        }
    }
}

impl Drop for Gather {
    fn drop(&mut self) {
        if !self.piece.is_empty() {
            let _ = self.to.send(std::mem::take(&mut self.piece));
            crate::wake::wake();
        }
    }
}

impl Mic {
    /// Starts the default mic, or says why it can't.
    pub fn start() -> Result<Mic, String> {
        let (stop, stopped) = channel::<()>();
        let (to, chunks) = channel::<Vec<u8>>();
        let (said, opened) = channel::<Result<(u32, u16), String>>();
        // The stream stays on a thread of its own, which owns it until it's stopped.
        std::thread::Builder::new()
            .name("loom-mic".into())
            .spawn(move || {
                let host = cpal::default_host();
                let Some(device) = host.default_input_device() else {
                    let _ = said.send(Err("There's no mic.".into()));
                    return;
                };
                let config = match device.default_input_config() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = said.send(Err(format!("The mic didn't start: {e}")));
                        return;
                    }
                };
                let rate = config.sample_rate();
                let channels = config.channels();
                let bytes = (rate as usize / 10) * channels as usize * 2;
                let mut gather = Gather {
                    piece: Vec::with_capacity(bytes),
                    bytes,
                    to,
                };
                let on_error = |e| eprintln!("loom: the mic: {e}");
                let stream = match config.sample_format() {
                    cpal::SampleFormat::I16 => device.build_input_stream(
                        config.into(),
                        move |data: &[i16], _: &_| gather.take(data.iter().copied()),
                        on_error,
                        None,
                    ),
                    cpal::SampleFormat::U16 => device.build_input_stream(
                        config.into(),
                        move |data: &[u16], _: &_| {
                            gather.take(data.iter().map(|&s| (s as i32 - 32768) as i16))
                        },
                        on_error,
                        None,
                    ),
                    cpal::SampleFormat::I32 => device.build_input_stream(
                        config.into(),
                        move |data: &[i32], _: &_| {
                            gather.take(data.iter().map(|&s| (s >> 16) as i16))
                        },
                        on_error,
                        None,
                    ),
                    cpal::SampleFormat::F32 => device.build_input_stream(
                        config.into(),
                        move |data: &[f32], _: &_| gather.take(data.iter().map(|&s| f32_to_i16(s))),
                        on_error,
                        None,
                    ),
                    other => {
                        let _ = said.send(Err(format!(
                            "The mic gives {other} samples, which loom can't read."
                        )));
                        return;
                    }
                };
                let stream = match stream {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = said.send(Err(format!("The mic didn't start: {e}")));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = said.send(Err(format!("The mic didn't start: {e}")));
                    return;
                }
                let _ = said.send(Ok((rate, channels)));
                // Until it's stopped, or the Mic is dropped.
                let _ = stopped.recv();
                drop(stream);
            })
            .map_err(|e| format!("The mic didn't start: {e}"))?;
        let (rate, channels) = opened
            .recv()
            .map_err(|_| "The mic didn't start.".to_string())??;
        Ok(Mic {
            stop,
            chunks,
            rate,
            channels,
        })
    }

    /// The `voice.audio` frames for what the mic heard since the last call.
    pub fn frames(&self) -> Vec<serde_json::Value> {
        use base64::Engine;
        self.chunks
            .try_iter()
            .map(|pcm| {
                serde_json::json!({
                    "t": "voice.audio",
                    "pcm": base64::engine::general_purpose::STANDARD.encode(pcm),
                    "rate": self.rate,
                    "channels": self.channels,
                })
            })
            .collect()
    }

    /// Stops the mic, and gives the last of what it heard.
    pub fn stop(self) -> Vec<serde_json::Value> {
        let _ = self.stop.send(());
        // The stream's thread drops its gatherer, which sends the rest, once it ends.
        let mut out = Vec::new();
        while let Ok(pcm) = self
            .chunks
            .recv_timeout(std::time::Duration::from_millis(300))
        {
            use base64::Engine;
            out.push(serde_json::json!({
                "t": "voice.audio",
                "pcm": base64::engine::general_purpose::STANDARD.encode(pcm),
                "rate": self.rate,
                "channels": self.channels,
            }));
        }
        out
    }
}

/// Plays speech the core wrote (`core.speak`), in place of what's playing.
pub fn speak(file: &str) {
    match std::fs::read(file) {
        Ok(wav) => crate::sound::play_speech(wav),
        Err(e) => eprintln!("loom: can't read {file}: {e}"),
    }
}

/// Stops the speech playing.
pub fn hush() {
    crate::sound::stop_speech();
}

#[cfg(test)]
mod tests {
    use super::{f32_to_i16, Gather};

    #[test]
    fn floats_become_16_bit() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), 32767);
        assert_eq!(f32_to_i16(-2.0), -32767);
        assert_eq!(f32_to_i16(0.5), 16384);
    }

    #[test]
    fn what_the_mic_hears_goes_in_pieces() {
        let (to, from) = std::sync::mpsc::channel();
        let mut g = Gather {
            piece: Vec::new(),
            bytes: 8,
            to,
        };
        g.take([1i16, 2, 3].into_iter());
        assert!(from.try_recv().is_err());
        g.take([4i16].into_iter());
        assert_eq!(from.try_recv().unwrap(), vec![1, 0, 2, 0, 3, 0, 4, 0]);
        g.take([-1i16].into_iter());
        drop(g);
        assert_eq!(from.try_recv().unwrap(), vec![255, 255]);
    }
}
