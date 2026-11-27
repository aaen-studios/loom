//! Playing loom's sounds (design/sounds), which the app carries. The core says which to
//! play and how loud (`core.sound`, and a notification's `sound`); this only plays it.
//! On Windows the app plays it itself, and not while Focus Assist or Do Not Disturb is
//! on; on a Mac a sound with no notification plays through NSSound.

/// The WAV of one of loom's sounds, by its file name without `.wav`.
pub fn bytes(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "loom-reply" => include_bytes!("../../design/sounds/loom-reply.wav"),
        "loom-waiting" => include_bytes!("../../design/sounds/loom-waiting.wav"),
        "loom-routine" => include_bytes!("../../design/sounds/loom-routine.wav"),
        "loom-failed" => include_bytes!("../../design/sounds/loom-failed.wav"),
        "loom-sent" => include_bytes!("../../design/sounds/loom-sent.wav"),
        "loom-dictation-on" => include_bytes!("../../design/sounds/loom-dictation-on.wav"),
        "loom-dictation-off" => include_bytes!("../../design/sounds/loom-dictation-off.wav"),
        _ => return None,
    })
}

/// A 16-bit PCM WAV with its samples scaled to `volume`, 0 to 1. The header is the
/// 44 bytes `std.wave` writes, so the samples start after it.
#[cfg_attr(not(windows), allow(dead_code))]
fn at_volume(wav: &[u8], volume: f32) -> Vec<u8> {
    let v = volume.clamp(0.0, 1.0);
    let mut out = wav.to_vec();
    if out.len() <= 44 {
        return out;
    }
    for s in out[44..].as_chunks_mut::<2>().0 {
        let x = f32::from(i16::from_le_bytes(*s)) * v;
        s.copy_from_slice(&(x.round() as i16).to_le_bytes());
    }
    out
}

/// Whether Windows says the person shouldn't be disturbed now: Focus Assist or Do Not
/// Disturb, quiet time, a presentation, or a game or video full screen.
#[cfg(windows)]
pub fn do_not_disturb() -> bool {
    use windows_sys::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME,
        QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    let mut state = 0;
    // SAFETY: the call writes one QUERY_USER_NOTIFICATION_STATE to the pointer it's given.
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    hr >= 0
        && matches!(
            state,
            QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE | QUNS_QUIET_TIME
        )
}

/// The sound playing now, kept until the next one takes its place, since Windows reads
/// it from memory while it plays.
#[cfg(windows)]
static PLAYING: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// Plays one of loom's sounds at `volume`, unless the person isn't to be disturbed.
#[cfg(windows)]
pub fn play(file: &str, volume: f32) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    let Some(wav) = bytes(file) else {
        return;
    };
    if do_not_disturb() {
        return;
    }
    let scaled = at_volume(wav, volume);
    let Ok(mut playing) = PLAYING.lock() else {
        return;
    };
    // SAFETY: with SND_MEMORY the first argument points at a whole WAV in memory, which
    // stays alive in PLAYING until a later call has started another in its place.
    unsafe {
        PlaySoundW(
            scaled.as_ptr() as *const u16,
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
    *playing = scaled;
}

/// Plays one of loom's sounds at `volume` through NSSound.
#[cfg(target_os = "macos")]
pub fn play(file: &str, volume: f32) {
    if let Some(wav) = bytes(file) {
        crate::mac::play_sound(wav, volume);
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn play(_file: &str, _volume: f32) {}

#[cfg(test)]
mod tests {
    use super::{at_volume, bytes};

    #[test]
    fn every_sound_loom_names_is_carried() {
        for f in [
            "loom-reply",
            "loom-waiting",
            "loom-routine",
            "loom-failed",
            "loom-sent",
            "loom-dictation-on",
            "loom-dictation-off",
        ] {
            let wav = bytes(f).expect(f);
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[36..40], b"data");
        }
        assert!(bytes("loom-nothing").is_none());
    }

    #[test]
    fn a_sound_plays_at_its_volume() {
        let wav = bytes("loom-sent").unwrap();
        let half = at_volume(wav, 0.5);
        assert_eq!(half.len(), wav.len());
        assert_eq!(&half[..44], &wav[..44]);
        let peak = |w: &[u8]| {
            w[44..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|s| i16::from_le_bytes(*s).unsigned_abs())
                .max()
                .unwrap()
        };
        let (full, halved) = (peak(wav), peak(&half));
        assert!(halved.abs_diff(full / 2) <= 1, "{full} {halved}");
        assert_eq!(peak(&at_volume(wav, 0.0)), 0);
    }
}
