//! The mic and the speaker for loom's voice (PLAN.md, "Voice"). The app only records
//! and plays: what the mic hears goes to the core as 16-bit PCM about every 100 ms
//! (`voice.audio`), which hears it with the engine the person picked, and the speech
//! the core writes (`core.speak`) is played here. The core decides everything else.
//! It records from the mic picked on Settings › Voice, or the system's default.

use std::sync::mpsc::{channel, Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// One of the system's inputs: cpal's id for it, which stays the same across restarts
/// and is kept in prefs.json, and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub id: String,
    pub name: String,
}

/// The system's inputs as they are now: the default's name, and each input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub default: Option<String>,
    pub all: Vec<Input>,
}

impl Inputs {
    /// Whether the input `id` is plugged in.
    pub fn has(&self, id: &str) -> bool {
        self.all.iter().any(|i| i.id == id)
    }

    /// The name of the mic loom records from with `picked`: the picked one while it's
    /// plugged in, otherwise the default's.
    pub fn in_use(&self, picked: Option<&Input>) -> Option<String> {
        match picked {
            Some(i) if self.has(&i.id) => Some(i.name.clone()),
            _ => self.default.clone(),
        }
    }
}

fn input_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

/// The system's inputs. It can take a moment, so it's called on a thread of its own.
pub fn inputs() -> Inputs {
    let host = cpal::default_host();
    let default = host.default_input_device().as_ref().and_then(input_name);
    let all = host
        .input_devices()
        .map(|ds| {
            ds.filter_map(|d| {
                Some(Input {
                    id: d.id().ok()?.to_string(),
                    name: input_name(&d)?,
                })
            })
            .collect()
        })
        .unwrap_or_default();
    Inputs { default, all }
}

/// The inputs, listed on a thread of their own, out of what it gives once they're there.
pub fn list_inputs() -> Receiver<Inputs> {
    let (to, from) = channel();
    let _ = std::thread::Builder::new()
        .name("loom-inputs".into())
        .spawn(move || {
            let _ = to.send(inputs());
            crate::wake::wake();
        });
    from
}

/// How loud a piece of 16-bit PCM is, from 0 for silence to 1, by its RMS from -60 dBFS
/// to 0: what Settings › Voice's meter shows.
pub fn level(pcm: &[u8]) -> f32 {
    let n = pcm.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f64::from(i16::from_le_bytes(*b)).powi(2))
        .sum();
    let rms = (sum / n as f64).sqrt() / 32768.0;
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 60.0) / 60.0).clamp(0.0, 1.0) as f32
}

/// The mic while it records: what it heard comes out of `chunks`, each 16-bit
/// little-endian PCM at `rate`, `channels` interleaved. Dropping it stops it.
pub struct Mic {
    stop: Sender<()>,
    pub chunks: Receiver<Vec<u8>>,
    pub rate: u32,
    pub channels: u16,
    /// The mic's name.
    pub name: String,
    /// Whether the mic picked wasn't plugged in, so this is the default.
    pub fell_back: bool,
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
    /// Starts the mic `picked` (an `Input`'s id), or the default with none or while
    /// that one isn't plugged in, or says why it can't.
    pub fn start(picked: Option<&str>) -> Result<Mic, String> {
        let picked = picked.map(String::from);
        let (stop, stopped) = channel::<()>();
        let (to, chunks) = channel::<Vec<u8>>();
        type Opened = (u32, u16, String, bool);
        let (said, opened) = channel::<Result<Opened, String>>();
        // The stream stays on a thread of its own, which owns it until it's stopped.
        std::thread::Builder::new()
            .name("loom-mic".into())
            .spawn(move || {
                let host = cpal::default_host();
                let chosen = picked
                    .as_deref()
                    .and_then(|id| id.parse::<cpal::DeviceId>().ok())
                    .and_then(|id| host.device_by_id(&id))
                    .filter(DeviceTrait::supports_input);
                let fell_back = picked.is_some() && chosen.is_none();
                let Some(device) = chosen.or_else(|| host.default_input_device()) else {
                    let _ = said.send(Err("There's no mic.".into()));
                    return;
                };
                let name = input_name(&device).unwrap_or_else(|| "Mic".into());
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
                let _ = said.send(Ok((rate, channels, name, fell_back)));
                // Until it's stopped, or the Mic is dropped.
                let _ = stopped.recv();
                drop(stream);
            })
            .map_err(|e| format!("The mic didn't start: {e}"))?;
        let (rate, channels, name, fell_back) = opened
            .recv()
            .map_err(|_| "The mic didn't start.".to_string())??;
        Ok(Mic {
            stop,
            chunks,
            rate,
            channels,
            name,
            fell_back,
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
    use super::{f32_to_i16, level, Gather, Input, Inputs};

    #[test]
    fn the_meter_reads_by_loudness() {
        assert_eq!(level(&[]), 0.0);
        assert_eq!(level(&[0, 0, 0, 0]), 0.0);
        let full: Vec<u8> = [32767i16, -32767]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert!(level(&full) > 0.99);
        // -30 dBFS is halfway.
        let s = (32768.0 * 10f64.powf(-1.5)).round() as i16;
        let half: Vec<u8> = [s, -s].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert!((level(&half) - 0.5).abs() < 0.01);
    }

    #[test]
    fn the_mic_in_use_is_the_picked_one_while_it_is_there() {
        let usb = Input {
            id: "wasapi:usb".into(),
            name: "USB Mic".into(),
        };
        let inputs = Inputs {
            default: Some("Laptop Mic".into()),
            all: vec![usb.clone()],
        };
        assert_eq!(inputs.in_use(Some(&usb)).as_deref(), Some("USB Mic"));
        assert_eq!(inputs.in_use(None).as_deref(), Some("Laptop Mic"));
        let gone = Input {
            id: "wasapi:gone".into(),
            name: "Headset".into(),
        };
        assert_eq!(inputs.in_use(Some(&gone)).as_deref(), Some("Laptop Mic"));
    }

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
