//! Voice mode's Tauri layer: two services, their commands, and their events.
//!
//! # Why a dedicated thread each
//!
//! `ort::Session::run` takes `&mut self`, and loading the 325 MB graph takes
//! over a second. Both facts point the same way: one long-lived thread owns the
//! model, jobs arrive over a channel, and nothing on the UI thread ever waits
//! for inference.
//!
//! That also keeps the promise the design made — synthesis can never stall
//! token generation — and gives cancellation somewhere to happen: the worker
//! checks a flag between sentences, so barge-in stops within one sentence
//! rather than waiting for a whole reply.
//!
//! # Why recognition has its own thread rather than sharing this one
//!
//! [`DictationService`] is deliberately *not* a job on [`VoiceService`]'s
//! thread. Recognition takes about a third of the audio's length, and the speak
//! worker holds a 325 MB model and a synchronous inference loop — so audio
//! queued behind it would be recognised after the reply finished, which is
//! exactly backwards when the user interrupted to say something. Two threads
//! means speaking never delays hearing, which is the whole point of barge-in.
//!
//! # Streaming, honestly
//!
//! Synthesis runs **one sentence at a time** and emits each as it completes, so
//! playback starts while later sentences are still being made and the gaps
//! between them are hidden by the queue rather than heard as silence.
//!
//! What this is *not* is token-level streaming: the text arrives whole rather
//! than as deltas from the engine. Feeding it deltas is a small change to
//! [`Job::Speak`]'s payload and a real change to the caller, so it is left for
//! when the composer has something to feed it from.
//!
//! Recognition is not streaming either, and cannot be: a transcript needs the
//! whole utterance. Partial results would need a second thread and a policy for
//! what to do with a half-finished hypothesis — retracting a word already shown
//! is worse than showing it a moment later.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use loom_core::voice::config::VoiceConfig;
use loom_core::voice::espeak;
use loom_core::voice::install::{self, Component, Step};
use loom_core::voice::tts::{Availability, Audio, Kokoro, Paths};

use crate::commands::AppState;

/// Where voice events are emitted.
const VOICE_EVENT: &str = "loom://voice";
/// Where install progress is emitted. Separate from playback: the two have
/// different payload shapes and different lifetimes.
const INSTALL_EVENT: &str = "loom://voice-install";

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Something the voice service did.
///
/// Tagged by `type`, camelCase fields, matching the engine's own event style so
/// the frontend's listener code looks like the rest of the app.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VoiceEvent {
    /// The model is being loaded. Only on the first utterance after a start.
    Loading { utterance: u64 },
    /// Synthesis has begun.
    ///
    /// `lines` are the sentences that will be spoken, in order, and they are
    /// sent rather than merely counted because the UI highlights the sentence
    /// being played. Re-splitting the text on the frontend would be a second
    /// implementation of the sentence rule, and the two would eventually
    /// disagree about where a boundary is — which shows up as the highlight
    /// drifting away from the voice.
    Started {
        utterance: u64,
        voice: String,
        lines: Vec<String>,
    },
    /// One sentence, ready to play.
    Chunk {
        utterance: u64,
        index: usize,
        /// Base64 WAV, so the webview can play it without a file or a decoder.
        audio: String,
        seconds: f32,
    },
    /// Everything was spoken.
    Done {
        utterance: u64,
        chunks: usize,
        seconds: f32,
        real_time_factor: f32,
    },
    /// Cancelled before finishing, which is what barge-in does.
    Cancelled { utterance: u64 },
    /// Something went wrong, phrased for a person.
    Error { utterance: u64, message: String },
}

/// Install progress, for the settings screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallEvent {
    pub component: String,
    pub label: String,
    pub step: String,
    pub detail: String,
    pub done: bool,
    /// Overall progress, 0–100, weighted by expected download size.
    pub percent: u8,
}

// ---------------------------------------------------------------------------
// The service
// ---------------------------------------------------------------------------

/// Jobs the worker thread accepts.
enum Job {
    Speak {
        utterance: u64,
        text: String,
        voice: String,
        speed: f32,
    },
    /// Synthesises one short phrase, for the voice picker's preview button.
    Preview { utterance: u64, voice: String },
}

/// Owns the voice worker thread.
///
/// The thread is started lazily, on the first utterance, because it needs an
/// `AppHandle` to emit events from and the app does not have one at the moment
/// state is constructed.
pub struct VoiceService {
    /// `None` until the first job. Guarded because two commands could race.
    sender: Mutex<Option<Sender<Job>>>,
    /// Set to stop the current utterance.
    cancel: Arc<AtomicBool>,
    /// Source of utterance ids, so the UI can ignore events from a superseded
    /// request — a second click, or a reply that was interrupted.
    next_utterance: AtomicU64,
}

impl Default for VoiceService {
    fn default() -> Self {
        Self {
            sender: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
            next_utterance: AtomicU64::new(1),
        }
    }
}

impl VoiceService {
    pub fn new() -> Self {
        Self::default()
    }

    /// The id for a new request, and the flag that will cancel it.
    fn begin(&self) -> (u64, Arc<AtomicBool>) {
        // Clearing before issuing means a cancel that arrives fractionally
        // early still applies to this utterance rather than the last one.
        self.cancel.store(false, Ordering::SeqCst);
        let id = self.next_utterance.fetch_add(1, Ordering::SeqCst);
        (id, Arc::clone(&self.cancel))
    }

    /// Queues a job, starting the worker if it is not running.
    fn submit(&self, app: &AppHandle, job: Job) -> Result<(), String> {
        let mut slot = self.sender.lock().map_err(|_| "voice service poisoned")?;

        if slot.is_none() {
            let (tx, rx) = mpsc::channel::<Job>();
            let cancel = Arc::clone(&self.cancel);
            let handle = app.clone();

            std::thread::Builder::new()
                .name("loom-voice".to_string())
                .spawn(move || worker(rx, handle, cancel))
                .map_err(|e| format!("could not start the voice thread: {e}"))?;

            *slot = Some(tx);
        }

        slot.as_ref()
            .expect("just installed")
            .send(job)
            .map_err(|_| "the voice thread has stopped".to_string())
    }

    /// Stops the current utterance. Returns `false` if nothing was speaking.
    pub fn cancel(&self) -> bool {
        self.cancel.swap(true, Ordering::SeqCst)
    }
}

/// The worker loop. Owns the model for the thread's lifetime.
fn worker(rx: mpsc::Receiver<Job>, app: AppHandle, cancel: Arc<AtomicBool>) {
    // Loaded once, on first use. Kept across jobs so the 325 MB graph is read
    // from disk once per session rather than once per sentence.
    let mut engine: Option<Kokoro> = None;

    while let Ok(job) = rx.recv() {
        match job {
            Job::Speak {
                utterance,
                text,
                voice,
                speed,
            } => {
                speak(&app, &mut engine, &cancel, utterance, &text, &voice, speed);
            }
            Job::Preview { utterance, voice } => {
                // A fixed phrase, so a preview is comparable between voices
                // rather than flattering whichever one got a longer sample.
                const SAMPLE: &str = "This is how I sound.";
                speak(&app, &mut engine, &cancel, utterance, SAMPLE, &voice, 1.0);
            }
        }
    }
}

/// Loads the model if needed, then speaks `text` sentence by sentence.
fn speak(
    app: &AppHandle,
    engine: &mut Option<Kokoro>,
    cancel: &AtomicBool,
    utterance: u64,
    text: &str,
    voice: &str,
    speed: f32,
) {
    if let Err(message) = ensure_loaded(app, engine, utterance) {
        emit(app, &VoiceEvent::Error { utterance, message });
        return;
    }
    let Some(kokoro) = engine.as_mut() else {
        emit(
            app,
            &VoiceEvent::Error {
                utterance,
                message: "the voice model is not loaded".to_string(),
            },
        );
        return;
    };

    // Check the voice before doing any work, so a bad id is an immediate error
    // rather than an empty result the UI has to interpret.
    if !kokoro.has_voice(voice) {
        emit(
            app,
            &VoiceEvent::Error {
                utterance,
                message: format!("there is no voice called {voice:?}"),
            },
        );
        return;
    }

    let lang = match Kokoro::language_of(voice) {
        Some(lang) => lang,
        None => {
            emit(
                app,
                &VoiceEvent::Error {
                    utterance,
                    message: format!("no phonemizer is mapped to the voice {voice:?}"),
                },
            );
            return;
        }
    };

    // Markdown out, then sentences. Doing this here rather than in the UI keeps
    // the guarantee in one place: nothing downstream can accidentally speak a
    // code fence or a URL.
    let prose = loom_core::voice::clean::for_speech(text);
    if prose.trim().is_empty() {
        emit(
            app,
            &VoiceEvent::Done {
                utterance,
                chunks: 0,
                seconds: 0.0,
                real_time_factor: 0.0,
            },
        );
        return;
    }

    let sentences = match espeak::phonemize_sentences(&prose, lang, &espeak_paths()) {
        Ok(sentences) => sentences,
        Err(error) => {
            emit(
                app,
                &VoiceEvent::Error {
                    utterance,
                    message: error.to_string(),
                },
            );
            return;
        }
    };

    let sentences: Vec<String> = sentences
        .into_iter()
        .filter(|sentence| !sentence.trim().is_empty())
        .collect();

    if sentences.is_empty() {
        emit(
            app,
            &VoiceEvent::Done {
                utterance,
                chunks: 0,
                seconds: 0.0,
                real_time_factor: 0.0,
            },
        );
        return;
    }

    emit(
        app,
        &VoiceEvent::Started {
            utterance,
            voice: voice.to_string(),
            lines: sentences.clone(),
        },
    );

    let mut spoken = 0usize;
    let mut total_seconds = 0f32;
    let mut total_compute = 0f64;

    for (index, sentence) in sentences.iter().enumerate() {
        // Cancellation is checked between sentences, which bounds how long a
        // barge-in takes to take effect: one sentence, not one reply.
        if cancel.load(Ordering::SeqCst) {
            emit(app, &VoiceEvent::Cancelled { utterance });
            return;
        }

        // Each sentence is synthesised on its own, so the punctuation that ends
        // it is what decides the pause and the intonation.
        let audio = match synthesize_sentence(kokoro, sentence, voice, speed) {
            Ok(Some((audio, compute))) => {
                total_compute += compute;
                audio
            }
            Ok(None) => continue,
            Err(error) => {
                emit(
                    app,
                    &VoiceEvent::Error {
                        utterance,
                        message: error,
                    },
                );
                return;
            }
        };

        total_seconds += audio.duration();
        spoken += 1;

        emit(
            app,
            &VoiceEvent::Chunk {
                utterance,
                index,
                audio: base64_wav(&audio),
                seconds: audio.duration(),
            },
        );
    }

    let real_time_factor = if total_seconds > 0.0 {
        total_compute as f32 / total_seconds
    } else {
        0.0
    };

    emit(
        app,
        &VoiceEvent::Done {
            utterance,
            chunks: spoken,
            seconds: total_seconds,
            real_time_factor,
        },
    );
}

/// Loads the model if it is not already in memory.
fn ensure_loaded(
    app: &AppHandle,
    engine: &mut Option<Kokoro>,
    utterance: u64,
) -> Result<(), String> {
    if engine.is_some() {
        return Ok(());
    }

    emit(app, &VoiceEvent::Loading { utterance });

    let home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;
    let paths = Paths::from_home(&home);
    let espeak_paths = espeak::Paths::from_home(&home);

    let availability = Availability::probe(&paths, &espeak_paths);
    if let Some(blocker) = availability.blocking() {
        return Err(format!("{blocker}. Install it from Settings → Voice."));
    }

    let loaded = Kokoro::load(&paths, &espeak_paths).map_err(|e| e.to_string())?;
    *engine = Some(loaded);
    Ok(())
}

/// Synthesises one sentence, or `None` when it has nothing speakable.
fn synthesize_sentence(
    engine: &mut Kokoro,
    sentence: &str,
    voice: &str,
    speed: f32,
) -> Result<Option<(Audio, f64)>, String> {
    let synthesis = engine
        .speak(sentence, voice, speed)
        .map_err(|e| e.to_string())?;

    if synthesis.audio.is_empty() {
        return Ok(None);
    }
    Ok(Some((synthesis.audio, synthesis.compute_seconds)))
}

/// The espeak paths, resolved from the Loom home.
fn espeak_paths() -> espeak::Paths {
    loom_core::paths::loom_home()
        .map(|home| espeak::Paths::from_home(&home))
        .unwrap_or_else(|_| espeak::Paths::from_home(std::path::Path::new(".")))
}

/// Base64-encodes a WAV, which is how audio crosses to the webview.
///
/// A data URL avoids writing a temporary file per sentence and avoids needing
/// the asset protocol to expose a directory the user never chose. The cost is
/// 33% overhead on a few hundred kilobytes per sentence.
fn base64_wav(audio: &Audio) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(audio.to_wav_bytes())
}

/// Emits an event, logging rather than failing if no window is listening.
fn emit(app: &AppHandle, event: &VoiceEvent) {
    if let Err(error) = app.emit(VOICE_EVENT, event.clone()) {
        eprintln!("[loom] could not emit a voice event: {error}");
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// What the Voice settings screen shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    /// Whether the user has voice mode switched on at all.
    pub enabled: bool,
    /// Whether every component is present.
    pub ready: bool,
    /// The first missing piece, in the order it has to be fixed.
    pub blocking: Option<String>,
    /// The four components and whether each is installed.
    pub components: Vec<ComponentStatus>,
    /// The configured voice, and the configured speed.
    pub default_voice: String,
    pub speed: f32,
    pub autoplay: bool,
    /// Whether a finished transcript sends itself. Reported so the settings
    /// screen shows the stored value rather than a local guess.
    pub auto_send: bool,
    /// Where the assets live, so the screen can show a path.
    pub home: String,
}

/// One component's state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub id: String,
    pub label: String,
    pub licence: String,
    pub installed: bool,
    pub bytes: u64,
}

/// One voice the model can speak with.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    pub id: String,
    /// The espeak-ng language name, or `None` when unmapped.
    pub language: Option<String>,
    /// A human label for the accent, when it is known.
    pub accent: Option<String>,
    /// The gender implied by Kokoro's prefix.
    pub gender: Option<String>,
    /// Whether this is the global default.
    pub is_default: bool,
}

#[tauri::command]
pub fn voice_status(state: State<'_, AppState>) -> Result<VoiceStatus, String> {
    let home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;
    let config = state.snapshot().voice;

    let paths = config.resolved_paths(&home);
    let espeak_paths = espeak::Paths::from_home(&home);
    let availability = Availability::probe(&paths, &espeak_paths);

    let components = install::status(&home)
        .into_iter()
        .map(|entry| ComponentStatus {
            id: entry.component.id().to_string(),
            label: entry.component.label().to_string(),
            licence: entry.component.licence().to_string(),
            installed: entry.step.is_done(),
            bytes: entry.component.expected_bytes(),
        })
        .collect();

    Ok(VoiceStatus {
        enabled: config.enabled,
        ready: availability.ready(),
        blocking: availability.blocking().map(str::to_string),
        components,
        default_voice: config.default_voice.clone(),
        speed: config.speed(),
        autoplay: config.autoplay,
        auto_send: config.auto_send,
        home: home.to_string_lossy().into_owned(),
    })
}

/// Every voice in the archive, with what is known about each.
///
/// Reads only the voices file, so this works before the runtime or the model
/// are installed — which matters, because the picker is one of the first things
/// a new user sees.
#[tauri::command]
pub fn voice_list_voices(state: State<'_, AppState>) -> Result<Vec<VoiceInfo>, String> {
    let home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;
    let config = state.snapshot().voice;
    let paths = config.resolved_paths(&home);

    if !paths.voices.exists() {
        return Ok(Vec::new());
    }

    let voices =
        loom_core::voice::voices::Voices::load(&paths.voices).map_err(|e| e.to_string())?;

    Ok(voices
        .names()
        .map(|id| VoiceInfo {
            id: id.to_string(),
            language: Kokoro::language_of(id).map(|lang| lang.espeak_name().to_string()),
            accent: accent_of(id),
            gender: gender_of(id),
            is_default: id == config.default_voice,
        })
        .collect())
}

/// The accent a voice's prefix implies.
///
/// Kokoro's ids start with a language letter and a gender letter, so the pair
/// is enough to label most of the roster. Unmapped languages get `None` rather
/// than a guess.
fn accent_of(id: &str) -> Option<String> {
    let label = match id.as_bytes().first().copied()? {
        b'a' => "American English",
        b'b' => "British English",
        b'e' => "Spanish",
        b'f' => "French",
        b'h' => "Hindi",
        b'i' => "Italian",
        b'j' => "Japanese",
        b'p' => "Portuguese",
        b'z' => "Mandarin",
        _ => return None,
    };
    Some(label.to_string())
}

/// The gender a voice's prefix implies.
fn gender_of(id: &str) -> Option<String> {
    let label = match id.as_bytes().get(1).copied()? {
        b'f' => "female",
        b'm' => "male",
        _ => return None,
    };
    Some(label.to_string())
}

/// Speaks `text`, returning the utterance id.
///
/// Returns immediately. Audio arrives as [`VoiceEvent::Chunk`] events, so the
/// caller streams it rather than waiting for the whole reply.
#[tauri::command]
pub fn voice_speak(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    voice: Option<String>,
    speed: Option<f32>,
) -> Result<u64, String> {
    let config = state.snapshot().voice;
    if !config.enabled {
        return Err("voice mode is switched off in Settings → Voice".to_string());
    }
    if text.trim().is_empty() {
        return Err("there is nothing to say".to_string());
    }

    let (utterance, _cancel) = state.voice.begin();
    let voice = voice.unwrap_or_else(|| config.default_voice.clone());

    state.voice.submit(
        &app,
        Job::Speak {
            utterance,
            text,
            voice,
            speed: speed.unwrap_or_else(|| config.speed()),
        },
    )?;

    Ok(utterance)
}

/// Speaks a fixed phrase in one voice, for the picker's preview button.
#[tauri::command]
pub fn voice_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    voice: String,
) -> Result<u64, String> {
    let (utterance, _cancel) = state.voice.begin();
    state
        .voice
        .submit(&app, Job::Preview { utterance, voice })?;
    Ok(utterance)
}

/// Stops the current utterance. Returns whether anything was speaking.
#[tauri::command]
pub fn voice_cancel(state: State<'_, AppState>) -> bool {
    state.voice.cancel()
}

/// Downloads and installs everything that is missing.
///
/// Long-running, so it runs on its own task and reports through
/// [`INSTALL_EVENT`]. Rejects if an install is already under way, because two
/// concurrent writers to the same directory is how a half-extracted DLL
/// happens.
#[tauri::command]
pub async fn voice_install(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state
        .installing
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return Err("an install is already running".to_string());
    }

    let installing = Arc::clone(&state.installing);
    let handle = app.clone();

    tauri::async_runtime::spawn(async move {
        let result = run_install(&handle).await;
        installing.store(false, std::sync::atomic::Ordering::SeqCst);

        if let Err(message) = result {
            let _ = handle.emit(
                INSTALL_EVENT,
                InstallEvent {
                    component: "all".to_string(),
                    label: "Voice mode".to_string(),
                    step: "failed".to_string(),
                    detail: message,
                    done: true,
                    percent: 100,
                },
            );
        }
    });

    Ok(())
}

/// The install, reporting progress by component and overall percentage.
async fn run_install(app: &AppHandle) -> Result<(), String> {
    let home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;
    let client = loom_core::voice::assets::client().map_err(|e| e.to_string())?;

    // Expected sizes, for weighting the overall bar: the model dominates, and
    // an even split would make the bar sit at 75% for most of the download.
    let total_expected: u64 = Component::ALL
        .iter()
        .map(|component| component.expected_bytes())
        .sum();
    let mut finished_bytes = 0u64;

    let mut progress = |component: Component, step: Step| {
        let detail = step.label();
        let done = step.is_done();

        // A finished component counts its whole expected size, so the bar only
        // depends on how many are done, not on how the network behaved.
        if done {
            finished_bytes = finished_bytes.saturating_add(component.expected_bytes());
        }

        let percent = ((finished_bytes.min(total_expected)) as f64 / total_expected as f64 * 100.0)
            .round() as u8;

        if let Err(error) = app.emit(
            INSTALL_EVENT,
            InstallEvent {
                component: component.id().to_string(),
                label: component.label().to_string(),
                step: format!("{:?}", step).to_lowercase(),
                detail,
                done,
                percent,
            },
        ) {
            eprintln!("[loom] could not emit install progress: {error}");
        }
    };

    let results = install::install_all(&home, &client, &mut progress).await;

    let failures: Vec<String> = results
        .into_iter()
        .filter_map(|(component, outcome)| {
            outcome
                .err()
                .map(|error| format!("{}: {error}", component.label()))
        })
        .collect();

    if failures.is_empty() {
        // Mark the whole thing complete, so the screen can stop polling.
        let _ = app.emit(
            INSTALL_EVENT,
            InstallEvent {
                component: "all".to_string(),
                label: "Voice mode".to_string(),
                step: "done".to_string(),
                detail: "installed".to_string(),
                done: true,
                percent: 100,
            },
        );
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// The config fields the Voice settings screen owns.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    pub enabled: Option<bool>,
    pub autoplay: Option<bool>,
    pub default_voice: Option<String>,
    pub speed: Option<f32>,
    pub speak_code: Option<bool>,
    /// Whether a finished transcript sends itself. See `VoiceConfig::auto_send`.
    pub auto_send: Option<bool>,
}

/// Saves the Voice settings.
#[tauri::command]
pub fn save_voice_settings(
    state: State<'_, AppState>,
    settings: VoiceSettings,
) -> Result<loom_core::config::AppConfig, String> {
    state.mutate(|config| {
        if let Some(enabled) = settings.enabled {
            config.voice.enabled = enabled;
        }
        if let Some(autoplay) = settings.autoplay {
            config.voice.autoplay = autoplay;
        }
        if let Some(speak_code) = settings.speak_code {
            config.voice.speak_code = speak_code;
        }
        if let Some(auto_send) = settings.auto_send {
            config.voice.auto_send = auto_send;
        }
        if let Some(voice) = &settings.default_voice {
            if VoiceConfig::is_plausible_voice(voice) {
                config.voice.default_voice = voice.clone();
            }
        }
        if let Some(speed) = settings.speed {
            // Clamped rather than rejected: a slider cannot exceed these, and a
            // hand-edited config should give working audio rather than silence.
            config.voice.speed = speed.clamp(
                loom_core::voice::config::MIN_SPEED,
                loom_core::voice::config::MAX_SPEED,
            );
        }
    })
}

/// Sets one persona's voice. An empty string clears it, falling back to the
/// global default.
#[tauri::command]
pub fn set_persona_voice(
    state: State<'_, AppState>,
    id: String,
    voice: Option<String>,
) -> Result<loom_core::config::AppConfig, String> {
    state.mutate(|config| {
        if let Some(persona) = config.personas.iter_mut().find(|item| item.id == id) {
            persona.voice = voice.as_ref().and_then(|value| {
                let trimmed = value.trim();
                // A persona with a nonsense voice id would fall back at speak
                // time anyway; storing `None` keeps the picker honest about
                // what has been chosen.
                (!trimmed.is_empty() && VoiceConfig::is_plausible_voice(trimmed))
                    .then(|| trimmed.to_string())
            });
        }
    })
}

// ---------------------------------------------------------------------------
// Listening
// ---------------------------------------------------------------------------

/// Where dictation events are emitted.
///
/// A separate channel from playback: the two run at once during barge-in, and
/// one payload shape covering both would be a union where half the fields are
/// always absent.
const LISTEN_EVENT: &str = "loom://voice-listen";

/// Something the listening service did.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ListenEvent {
    /// The models are being loaded. Only on the first block of a session.
    Loading,
    /// One block was scored. `level` is its RMS, for a meter.
    Level { level: f32 },
    /// The speaker started or stopped.
    ///
    /// `speaking: true` is the barge-in signal. It fires on the *first* block
    /// above the speech threshold rather than when a transcript arrives, which
    /// matters: waiting for a transcript would mean talking over the user for
    /// as long as it takes them to finish a sentence.
    Speech { speaking: bool },
    /// An utterance was recognised.
    Transcript {
        text: String,
        /// How much audio went in.
        seconds: f32,
        /// Compute divided by audio. Below 1.0 is faster than real time.
        real_time_factor: f32,
        /// The utterance hit the length cap and may end mid-word.
        truncated: bool,
    },
    /// The session ended.
    Stopped,
    /// Something went wrong, phrased for a person.
    Error { message: String },
}

/// Jobs the listening thread accepts.
enum ListenJob {
    /// One block of microphone audio, at whatever rate the device produced.
    Audio {
        samples: Vec<f32>,
        rate: u32,
        channels: usize,
    },
    /// End the session, transcribing whatever is still open.
    Stop,
}

/// Owns the dictation thread.
///
/// A thread of its own rather than a job on [`VoiceService`]'s, and that is not
/// tidiness. Recognition takes about a third of the audio's length, and the
/// speak thread holds a 325 MB model and a synchronous inference loop — so
/// putting audio on it would block recognition behind synthesis, which is
/// exactly backwards when the user has just interrupted to say something.
///
/// The thread is started once and lives for the session: `Dictation` holds both
/// Whisper graphs, and loading them per utterance would put a second of delay
/// between someone finishing a sentence and seeing it.
pub struct DictationService {
    /// `None` until the first session. Guarded because two commands could race.
    sender: Mutex<Option<Sender<ListenJob>>>,
    /// Whether a session is running.
    active: Arc<AtomicBool>,
}

impl Default for DictationService {
    fn default() -> Self {
        Self {
            sender: Mutex::new(None),
            active: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DictationService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether audio is being accepted.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Starts a session, if one is not already running.
    pub fn start(&self, app: &AppHandle) -> Result<(), String> {
        if self.is_active() {
            // Idempotent: the frontend may start a session it believes is over,
            // and a second model load would be a second 500 MB.
            return Ok(());
        }

        let mut slot = self.sender.lock().map_err(|_| "dictation poisoned")?;

        // A previous session's channel is closed, so a fresh thread is needed.
        // The old one exits when its `recv` fails, which it did as soon as the
        // sender was dropped.
        if slot.is_none() || slot.as_ref().is_some_and(|tx| tx.send(ListenJob::Stop).is_err()) {
            let (tx, rx) = mpsc::channel::<ListenJob>();
            let active = Arc::clone(&self.active);
            let handle = app.clone();

            std::thread::Builder::new()
                .name("loom-dictation".to_string())
                .spawn(move || listen_worker(rx, handle, active))
                .map_err(|e| format!("could not start the dictation thread: {e}"))?;

            *slot = Some(tx);
        }

        self.active.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Queues one block of microphone audio.
    pub fn audio(
        &self,
        samples: Vec<f32>,
        rate: u32,
        channels: usize,
    ) -> Result<(), String> {
        if !self.is_active() {
            return Err("not listening".to_string());
        }
        let slot = self.sender.lock().map_err(|_| "dictation poisoned")?;
        slot.as_ref()
            .ok_or_else(|| "the dictation thread has stopped".to_string())?
            .send(ListenJob::Audio {
                samples,
                rate,
                channels,
            })
            .map_err(|_| "the dictation thread has stopped".to_string())
    }

    /// Ends the session. Idempotent, because the frontend stops a session it
    /// may already have lost.
    pub fn stop(&self) -> Result<(), String> {
        if !self.is_active() {
            return Ok(());
        }
        self.active.store(false, Ordering::SeqCst);

        let slot = self.sender.lock().map_err(|_| "dictation poisoned")?;
        if let Some(sender) = slot.as_ref() {
            // Ignored: a closed channel means the thread has already finished,
            // which is the outcome this is asking for.
            let _ = sender.send(ListenJob::Stop);
        }
        Ok(())
    }
}

/// The dictation loop. Owns both models for the thread's lifetime.
fn listen_worker(rx: mpsc::Receiver<ListenJob>, app: AppHandle, active: Arc<AtomicBool>) {
    let mut dictation: Option<loom_core::voice::dictate::Dictation> = None;
    let mut announced = false;
    let mut was_speaking = false;

    while let Ok(job) = rx.recv() {
        match job {
            ListenJob::Audio {
                samples,
                rate,
                channels,
            } => {
                if dictation.is_none() {
                    if !announced {
                        emit_listen(&app, &ListenEvent::Loading);
                        announced = true;
                    }
                    match load_dictation() {
                        Ok(loaded) => dictation = Some(loaded),
                        Err(message) => {
                            emit_listen(&app, &ListenEvent::Error { message });
                            active.store(false, Ordering::SeqCst);
                            // Break rather than continue: every later block
                            // would fail the same way, and the frontend has
                            // already been told.
                            break;
                        }
                    }
                }

                let Some(engine) = dictation.as_mut() else {
                    break;
                };

                // The meter is fed every block regardless of recognition, so it
                // responds continuously rather than in bursts per utterance.
                emit_listen(
                    &app,
                    &ListenEvent::Level {
                        level: loom_core::voice::audio::rms(&samples),
                    },
                );

                match engine.push(&samples, rate, channels) {
                    Ok(Some(heard)) => {
                        // `real_time_factor` before `text`: `Heard` is not
                        // `Copy`, so taking the text by value ends the borrow.
                        let real_time_factor = heard.real_time_factor();
                        emit_listen(
                            &app,
                            &ListenEvent::Transcript {
                                text: heard.text,
                                seconds: heard.audio_seconds,
                                real_time_factor,
                                truncated: heard.truncated,
                            },
                        );
                    }
                    Ok(None) => {}
                    Err(error) => emit_listen(
                        &app,
                        &ListenEvent::Error {
                            message: error.to_string(),
                        },
                    ),
                }
                // Report transitions only. A barge-in signal per block would be
                // ten events a second for the same state, and the frontend only
                // acts on the edge anyway.
                let speaking = engine.speaking();
                if speaking != was_speaking {
                    was_speaking = speaking;
                    emit_listen(&app, &ListenEvent::Speech { speaking });
                }
            }

            ListenJob::Stop => {
                if let Some(engine) = dictation.as_mut() {
                    // Someone stopping the microphone mid-sentence gets that
                    // sentence. Losing it would be the worst behaviour — it is
                    // exactly what they just said.
                    match engine.finish() {
                        Ok(Some(heard)) => {
                            // Computed before the text is moved out, because
                            // `text` is the one field that has to be taken by
                            // value and `Heard` is not `Copy`.
                            let real_time_factor = heard.real_time_factor();
                            emit_listen(
                                &app,
                                &ListenEvent::Transcript {
                                    text: heard.text,
                                    seconds: heard.audio_seconds,
                                    real_time_factor,
                                    truncated: heard.truncated,
                                },
                            );
                        }
                        Ok(None) => {}
                        Err(error) => emit_listen(
                            &app,
                            &ListenEvent::Error {
                                message: error.to_string(),
                            },
                        ),
                    }
                }
                break;
            }
        }
    }

    // Whatever happened — a stop, an error, or the frontend going away — the
    // session is over from the Rust side's point of view.
    active.store(false, Ordering::SeqCst);
    emit_listen(&app, &ListenEvent::Stopped);
}

/// Loads the dictation models, with the message a missing component deserves.
fn load_dictation() -> Result<loom_core::voice::dictate::Dictation, String> {
    use loom_core::voice::dictate::Dictation;
    use loom_core::voice::tts;
    use loom_core::voice::whisper;

    let home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;

    // Checked before loading so a fresh install gets the fix rather than a
    // 500 MB load that fails halfway.
    if !whisper::Paths::from_home(&home).complete() {
        return Err(
            "the speech-to-text models are not installed. Install them from \
             Settings → Voice."
                .to_string(),
        );
    }
    if !tts::Paths::from_home(&home).runtime.exists() {
        return Err(
            "ONNX Runtime is not installed. Install it from Settings → Voice.".to_string(),
        );
    }

    Dictation::load_from(&home).map_err(|e| e.to_string())
}

/// Emits a listening event, logging rather than failing if nothing is listening.
fn emit_listen(app: &AppHandle, event: &ListenEvent) {
    if let Err(error) = app.emit(LISTEN_EVENT, event.clone()) {
        eprintln!("[loom] could not emit a listening event: {error}");
    }
}

// ---------------------------------------------------------------------------
// Listening commands
// ---------------------------------------------------------------------------

/// Starts a dictation session.
///
/// The microphone itself is captured by the webview — `getUserMedia` and an
/// audio worklet — because Rust has no portable access to a microphone that
/// would not also need its own permission flow. So this prepares the models and
/// the session, and audio arrives by [`voice_listen_audio`].
#[tauri::command]
pub fn voice_listen_start(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if !state.snapshot().voice.enabled {
        return Err("voice mode is switched off in Settings → Voice".to_string());
    }

    // Checked *before* the session starts, so a missing model is an immediate
    // error on the button press rather than one that arrives after the first
    // block of audio has been queued. The second version reads to a user as
    // "listening started, then silently did nothing", which is worse than a
    // refusal.
    let home = loom_core::paths::loom_home().map_err(|e| e.to_string())?;
    if !loom_core::voice::whisper::Paths::from_home(&home).complete() {
        return Err(
            "the speech-to-text models are not installed. Install them from \
             Settings → Voice."
                .to_string(),
        );
    }
    if !loom_core::voice::tts::Paths::from_home(&home).runtime.exists() {
        return Err("ONNX Runtime is not installed. Install it from Settings → Voice.".to_string());
    }

    state.dictation.start(&app)
}

/// Feeds one block of microphone audio.
///
/// Returns as soon as the block is queued: recognition happens on the dictation
/// thread and arrives as a [`ListenEvent::Transcript`]. Blocking this call on a
/// transcription would stall the webview's audio loop by a third of a second
/// per sentence.
#[tauri::command]
pub fn voice_listen_audio(
    state: State<'_, AppState>,
    samples: Vec<f32>,
    rate: u32,
    channels: usize,
) -> Result<(), String> {
    state.dictation.audio(samples, rate, channels)
}

/// Ends the session, transcribing anything still open.
#[tauri::command]
pub fn voice_listen_stop(state: State<'_, AppState>) -> Result<(), String> {
    state.dictation.stop()
}

/// Whether a session is running, so the UI can recover its state after a
/// reload rather than firing a second `start`.
#[tauri::command]
pub fn voice_listen_status(state: State<'_, AppState>) -> bool {
    state.dictation.is_active()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stopped_session_refuses_audio_rather_than_queueing_it() {
        let service = DictationService::new();
        // Not started: audio must be rejected, not silently buffered for a
        // session that will never read it.
        assert!(!service.is_active());
        assert!(service.audio(vec![0.0; 1600], 16_000, 1).is_err());

        // And stopping one that never started is not an error, because the
        // frontend stops sessions it may already have lost.
        assert!(service.stop().is_ok());
    }

    #[test]
    fn stopping_twice_is_harmless() {
        let service = DictationService::new();
        assert!(service.stop().is_ok());
        assert!(service.stop().is_ok());
        assert!(!service.is_active());
    }

    #[test]
    fn accents_and_genders_come_from_kokoro_prefixes() {
        assert_eq!(accent_of("af_heart").as_deref(), Some("American English"));
        assert_eq!(accent_of("bm_george").as_deref(), Some("British English"));
        assert_eq!(accent_of("zf_xiaobei").as_deref(), Some("Mandarin"));
        assert_eq!(gender_of("af_heart").as_deref(), Some("female"));
        assert_eq!(gender_of("bm_george").as_deref(), Some("male"));
    }

    #[test]
    fn an_unknown_prefix_yields_nothing_rather_than_a_guess() {
        assert_eq!(accent_of("qq_nobody"), None);
        assert_eq!(gender_of("ax_nobody"), None);
        assert_eq!(accent_of(""), None);
        assert_eq!(gender_of("a"), None);
    }

    #[test]
    fn utterance_ids_are_distinct_and_the_cancel_flag_is_cleared() {
        let service = VoiceService::new();
        let (first, flag) = service.begin();
        let (second, _) = service.begin();
        assert_ne!(first, second);

        // Cancelling marks the current utterance; beginning the next clears it,
        // so a late cancel does not silently kill the following reply.
        assert!(!flag.load(Ordering::SeqCst));
        assert!(!service.cancel(), "nothing was speaking yet");
        assert!(service.cancel(), "the second cancel sees the flag set");
        let (_, next) = service.begin();
        assert!(!next.load(Ordering::SeqCst), "begin must clear the flag");
    }

    #[test]
    fn cancel_reports_whether_it_was_already_set() {
        let service = VoiceService::new();
        assert!(!service.cancel());
        assert!(service.cancel());
    }

    #[test]
    fn a_wav_from_memory_matches_one_written_to_disk() {
        use loom_core::voice::tts::Audio;

        let audio = Audio {
            samples: vec![0.0, 0.5, -0.5],
            sample_rate: 24_000,
        };
        let dir = std::env::temp_dir().join("loom-voice-bytes-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("a.wav");
        audio.write_wav(&path).unwrap();

        // The UI plays from memory; the tooling writes a file. They must agree,
        // or the file a bug report contains will not match what was heard.
        assert_eq!(std::fs::read(&path).unwrap(), audio.to_wav_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_audio_is_a_wav_data_url_payload() {
        use loom_core::voice::tts::Audio;
        use base64::Engine;

        let audio = Audio {
            samples: vec![0.1, 0.2],
            sample_rate: 24_000,
        };
        let encoded = base64_wav(&audio);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .expect("valid base64");
        assert_eq!(&decoded[0..4], b"RIFF");
        assert_eq!(&decoded[8..12], b"WAVE");
    }
}
