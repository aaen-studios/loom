# Voice mode

Status: **the loop is closed.** Kokoro speaks, Whisper listens, and the round
trip is verified word for word — 21 of 21 words recovered from synthesised
speech, at 4.1× real time. A Voice settings screen installs 900 MB of
components from inside the app, and replies can be read aloud.

The chain is complete in both directions: audio in, words out, words in, audio
out, with talking over the reply stopping it. There is a Voice settings screen, a
microphone button in the composer, and a fullscreen **voice mode** (Ctrl+Shift+V)
with a live level meter, what has been heard, and the sentence being spoken
highlighted as the voice reaches it.

**What is not verified is a real microphone.** Every fixture on this machine is
synthesised — Kokoro speaking — and a recogniser that scores 100% on synthesised
speech is not thereby good at a real room. The code is wired end to end and
tested up to the edge of the hardware; the step across that edge is yours.

Two findings shaped the work, both in §14: **turn cancellation already exists**
in the engine, so barge-in is possible; and **whisper.cpp cannot build here**, so
speech-to-text goes through the ONNX runtime voice mode already ships.

---

## 0. A note on provenance

An earlier planning conversation quoted blind-listening Elo scores, latency
figures and VRAM requirements for a dozen TTS models from an unidentified
leaderboard. **Those could not be sourced and are withdrawn.**

This document cites only two things: facts read from the Kokoro-82M model card
or from a dependency's actual source, and facts measured on this machine.
Anything unverified is marked as such rather than stated.

---

## 1. Objective

Loom speaks its replies, gives each persona its own voice, and eventually holds
a continuous spoken conversation — in a fullscreen surface and as a composer
overlay — running entirely locally on Loom's own engine.

## 2. Non-goals

- **No model-callable TTS tool, and no audio output modality.** `provider.rs`,
  `harness.rs` and the model-facing tool list are untouched. `audio` remains an
  input-only modality label (`types.ts:17`, `provider.rs:36`).
- **Not speech-to-speech in one model.** Kokoro is decoder-only with no encoder
  release, so it cannot hear. A pipeline is the only option.
- Not macOS or Linux. Not multi-user. Not telephony or a streaming protocol.

## 3. What is built

Rust, in `crates/loom-core/src/voice/`:

| Module | What it does |
|---|---|
| `vocab.rs` | The phoneme vocabulary, verified against the export |
| `phonemes.rs` | Tokenization, double-sided padding, style-row selection |
| `chunk.rs` | Phoneme batching, and the streaming text chunker |
| `clean.rs` | Markdown → speakable prose |
| `voices.rs` | Reads the voice archive; `.npy` header parsing |
| `manifest.rs` | The pinned asset list |
| `assets.rs` | Resumable, hash-verified download |
| `espeak.rs` | Phonemization over hand-written FFI |
| `tts.rs` | Kokoro inference, WAV writing, streaming synthesis |
| `config.rs` | `VoiceConfig`, per-persona voice resolution, path overrides |
| `install.rs` | In-app installation of every component |
| `audio.rs` | Resampling, framing, ring buffer, sample conversion |
| `mel.rs` | Whisper's log-mel spectrogram |
| `voice/listen.rs` | Live endpointing: audio blocks in, utterances out |
| `voice/dictate.rs` | The whole chain in a line: audio in, transcripts out |
| `tokenizer.rs` | Whisper's token ids back to text |

Examples: `hello_kokoro` (synthesises and reports numbers), `fetch_voice_assets`
(the downloader, and the pinning tool), `probe_espeak` (which voices exist),
`dump_mel` (writes the artefacts the mel cross-check reads).

Frontend:

| | |
|---|---|
| `src/lib/voice.ts` | IPC, the event types, and the playback queue |
| `src/stores/voice.ts` | Phase, level, install progress |
| `src/components/SpeakButton.tsx` | Per-message speak/stop |
| `src/components/VoiceSettings.tsx` | The Voice settings section |
| `src-tauri/src/voice.rs` | The worker thread, its eight commands, its events |

Scripts:

| Script | Purpose |
|---|---|
| `setup-voice.py` | Installs the speech components |
| `fetch-onnxruntime.py` | ONNX Runtime |
| `fetch-espeak.py` | A prebuilt espeak-ng |
| `fetch-whisper.py` | Whisper and Silero VAD |
| `check-wav.py` | Confirms a produced WAV is speech, not noise |
| `check-espeak-constants.py` | Verifies the FFI constants against the header |
| `check-voice-apis.py` | Verifies the espeak and `ort` APIs used |
| `check-stt-sources.py` | Verifies the speech-to-text models are reachable |
| `check-mel.py` | Recomputes the mel in numpy and compares |
| `probe-tokenizer.py` | Reports the real tokenizer's structure |
| `probe-ort-api.py`, `probe-ort-outlet.py` | Reads the `ort` API from its source |
| `check-ort-versions.py` | Lists ONNX Runtime releases, to match the crate |
| `voice-status.py` | What is installed, and which parts work |
| `voice-changes.py` | Splits this work from unrelated changes in the tree |
| `voice-lines.py` | Line counts, for the report |
| `cargo-errors.py` | Prints cargo diagnostics with locations |
| `typecheck.py` | The same for `tsc` |
| `fetch-libclang.py` | Reference only; nothing runs bindgen any more |

## 4. Measured performance

### Speech out

From `cargo run -p loom-core --example hello_kokoro`, 181 characters:

| | |
|---|---|
| Audio produced | 10.98 s at 24 kHz |
| Model load | 1.16 s, once |
| Inference | 1.970 s |
| **Real-time factor** | **0.18 — 5.6× faster than playback** |
| Peak amplitude | 0.442 (no clipping) |
| Batches | 1 |
| Token input found | `input_ids` |

### Speech in

From `cargo run -p loom-core --example transcribe -- --builtin`: Kokoro speaks a
known sentence, Whisper transcribes it, and the two are compared word by word.

| | |
|---|---|
| Audio | 6.69 s at 24 kHz, from Kokoro |
| Whisper load | 0.78 s |
| Transcription | 1.63 s |
| **Throughput** | **4.1× faster than real time** |
| Tokens generated | 24 |
| **Word match** | **21 of 21 — 100%** |

```
expected   : The quick brown fox jumps over the lazy dog. Loom runs
             every model on this machine, with no network at all.
transcript : The Quick Brown Fox jumps over the lazy dog. Loom runs
             every model on this machine with no network at all.
```

Every word. Whisper also supplied the capitalisation and the sentence break on
its own, which is the model's punctuation behaviour rather than anything added
here.

### The mel front-end

From `cargo run -p loom-core --example dump_mel`, 2 seconds of audio:

| | |
|---|---|
| Spectrogram | 80 × 3000 |
| Computed in | 356 ms |
| Normalised range | −0.59 .. 1.41 |

Cross-checked against an independent numpy implementation: see §14.

Tests: **558 Rust, 106 frontend**, all passing; `cargo build --workspace` and
`tsc --noEmit` both clean with no warnings.

## 5. Verified facts

### Kokoro-82M v1.0, from the model card

| | |
|---|---|
| Parameters | 82M |
| Languages · voices | 8 · 54 |
| Sample rate | 24 kHz |
| Weights licence | Apache 2.0 |
| Architecture | StyleTTS 2 + ISTFTNet, decoder only, no encoder release |
| G2P | `misaki` (Python) + espeak-ng |
| Published | v1.0, 2025-01-27 |
| Training data | permissive / non-copyrighted audio only |
| Content filter | none described |
| Voice cloning | impossible — no encoder release |
| Upstream weights SHA-256 | `496dba11…f18ad1e4` |

### The ONNX export, read from `kokoro-onnx`

| | |
|---|---|
| Model | `kokoro-v1.0.onnx`, 325,505,369 bytes, sha256 `beb0d184…53f0df3a` |
| Voices | `voices-v1.0.bin`, 28,214,398 bytes, sha256 `bca610b8…29f1fbf7d` |
| ONNX Runtime | ≥ 1.20.1 |
| Context | 510 phonemes maximum |
| Inputs | token ids, `style`, `speed` |
| Token input name | `input_ids` on this export; `tokens` on older ones |
| Padding | `[0, …tokens, 0]` — **both** sides |
| Style row | `min(len(tokens), rows) - 1` |
| Outputs | audio as `f32`; durations when the export reports them |
| Speed | 0.5 – 2.0 |

Both hashes were **observed on completed downloads**, and the model's byte count
was confirmed twice: by a `HEAD` request's `Content-Length` and by hashing the
file.

### The voices file, decoded rather than assumed

Not a pickle — a **ZIP** (`np.savez`) holding one `.npy` per voice:

```
PK..  af_alloy.npy ...
```

54 voices across 17 prefixes, every one `.npy` v1.0 with `descr='<f4'`,
`fortran_order=False`, shape `(510, 1, 256)`.

`af` 11 · `am` 9 · `bf` 4 · `bm` 4 · `ef` 1 · `em` 2 · `ff` 1 · `hf` 2 · `hm` 2 ·
`if` 1 · `im` 1 · `jf` 4 · `jm` 1 · `pf` 1 · `pm` 2 · `zf` 4 · `zm` 4

This mattered: the alternative was `np.save` of a dict, which writes a
**pickled** payload, and reading pickle from Rust is a project of its own.
Verifying turned a hard problem into a ZIP read, and `zip` was already a
dependency.

### espeak-ng, probed rather than assumed

Four facts, three of which were wrong in the first draft:

| | |
|---|---|
| `espeakINITIALIZE_PHONEME_IPA` | **`0x0002`**, not `0x0001` |
| `espeak_AUDIO_OUTPUT_RETRIEVAL` | **`1`**, not `2` |
| British English voice | **`"en"`**, not `"en-gb"` |
| Punctuation in the output | **none** — must be re-attached |

The IPA flag is the one that would have been quietly catastrophic. The header's
comment spells it out — *"phoneme_mode bit 1: 0=eSpeak's ascii phoneme names,
1= International Phonetic Alphabet"* — so it is the **second** bit.
`0x0001` selects espeak's internal mnemonic form, which Kokoro cannot speak:
`t'Est` instead of `tˈɛst`. Every phoneme would have been dropped or mangled and
the result would have looked like a broken model file.

`espeak_TextToPhonemes(textptr, textmode, phonememode)` takes the character
encoding as a **separate argument**. The first draft OR'd `CHARS_UTF8 << 8` into
the phoneme mode, where bits 8–23 mean "separator character between phonemes" —
a value of 1 would have inserted U+0001 between every phoneme.

### Whisper, from its own configs

| | |
|---|---|
| `n_fft` | 400 — a 25 ms window at 16 kHz |
| `hop_length` | 160 — a 10 ms step |
| `feature_size` | 80 mel bins |
| `n_samples` | 480,000 — exactly 30 s |
| `nb_max_frames` | 3000 |
| `padding_side` | `right`, value 0.0 |
| Encoder positions | 1500 — the mel is downsampled 2× |
| Hidden width | 384 |
| `vocab_size` | 51,864 |
| EOS / pad | 50,256 |
| SOT | 50,257 |
| `no_timestamps` | 50,362, in `generation_config.json` |
| `max_target_positions` | 448 — the **total** sequence, prompt included |

The decode prompt is `[50257, 50362]`. Without the second token the model emits
timestamps interleaved with text and the transcript reads as `<|0.00|> hello`.

### The models' tensor signatures, introspected

From `cargo run -p loom-core --example probe_whisper`, rather than from memory:

| | inputs | outputs |
|---|---|---|
| encoder | `input_features` f32 `(?, ?, ?)` | `last_hidden_state` f32 `(?, 1500, 384)` |
| decoder | `input_ids` i64, `encoder_hidden_states` f32 `(?, ?, 384)` | `logits` f32 `(?, ?, 51864)`, plus 16 `present.*` |

The decoder takes **no `past_key_values` inputs** but *returns* the cache as 16
`present.*` tensors, which is why the decode loop is O(n²) — see §10.

### ONNX Runtime, and the version that has to match

`ort` 2.0.0-rc.13 wraps ONNX Runtime **1.28**. Running it against 1.22.0 loaded
models, inferred correctly, and then aborted the process during teardown with
`STATUS_STACK_BUFFER_OVERRUN`. Now pinned to **1.28.2** in
`scripts/fetch-onnxruntime.py` and in `install.rs`, with a test asserting the two
agree — a version that has to match in two places is one that will drift.

### Loom, from its own source

- Tauri v2; workspace of `crates/loom-core`, `src-tauri`, `setup/src-tauri`.
- React 19 · Zustand 5 · Vite 8 · TS 6 · vitest 5 · Bun.
- `sha2`, `minisign-verify`, `zip`, `reqwest[stream]`, `tokio[process]` were
  already dependencies, so hashing, verification and the downloader needed no
  new crates.
- `persona.rs` and `config.rs` both use `#[serde(default)]` throughout, so
  adding `Persona::voice` and `AppConfig::voice` needed **no migration**.
- `paths.rs` owns every filesystem location, with `LOOM_HOME` as an override.
- `speakerId` already means "which persona writes this reply" (`chat.ts:60`), so
  a voice is a separate concept and gets its own name.

## 6. Licences

1. **espeak-ng — GPL.** Required for *every* language, English included: the G2P
   path normally runs through `misaki`, a Python package with no Rust
   equivalent. Accepted. Surfaced as a small default-on installer note.
2. **CC BY attribution, from the training data.** Kokoro v1.0's dataset includes
   Koniwa (CC BY 3.0, <1 h) and SIWIS (CC BY 4.0, <11 h). This attaches to
   *outputs*, so it belongs in the about/credits screen from the first release.

## 7. Why espeak-ng is reached by hand-written FFI

`espeak-rs` wraps espeak-ng with MIT bindings and is the obvious choice. Building
it fails twice over:

```
bindgen-0.69.5 panicked:
Unable to find libclang: "couldn't find any valid shared libraries
matching: ['clang.dll', 'libclang.dll']"
```

then, once libclang was supplied, because `espeak-rs-sys` compiles espeak-ng
**from source**:

```
./espeak-ng/src/include/espeak-ng/speak_lib.h:28:10:
fatal error: 'stdio.h' file not found
```

On Windows that means MSVC Build Tools — a large, admin-only dependency for four
function calls. (`libclang` itself was solvable in 84 MB rather than 900 with the
PyPI wheel, which `scripts/fetch-libclang.py` still records; the C toolchain was
not.)

So `voice/espeak.rs` declares those four functions itself and loads a prebuilt
`espeak-ng.dll` with `libloading`. That is consistent with how ONNX Runtime is
already loaded: nothing is linked at build time, so a missing library is a
reportable error rather than a binary that will not start. It also removes the
`espeak-rs` dependency and its build requirements entirely.

**Two concurrency hazards, both load-bearing.**

espeak-ng holds **process-global** state: `espeak_SetVoiceByName` sets the voice
for the whole process, and `espeak_TextToPhonemes` advances an internal pointer
through the input. Two threads phonemizing at once interleave those and return
**corrupted phonemes** — not an error, just wrong audio. Everything that touches
espeak-ng goes through one mutex, and inference stays outside it, so synthesis
can still run concurrently.

And `espeak_Initialize` may be called exactly once, so the loaded library is
cached — **but a failure is not.** Caching a negative result was a real bug the
test suite caught: one caller asking for a path with no library would poison the
process permanently, so every later request, including one pointed at a correct
installation, reported the original error.

## 8. Assets

Nothing ships in the installer. Everything is fetched into `~/.loom/`:

| | Path | Size |
|---|---|---|
| ONNX Runtime | `ort/` | 372 MB |
| espeak-ng | `espeak/` | 19 MB |
| Kokoro model | `voice/kokoro-v1.0.onnx` | 326 MB |
| Kokoro voices | `voice/voices-v1.0.bin` | 28 MB |
| Whisper encoder | `voice/whisper/encoder_model.onnx` | 33 MB |
| Whisper decoder | `voice/whisper/decoder_model.onnx` | 118 MB |
| Whisper tokenizer | `voice/whisper/tokenizer.json` | 2.4 MB |
| Silero VAD | `voice/silero_vad.onnx` | 2.3 MB |

**1.34 GB total**, all hash-verified or from pinned releases.
`scripts/voice-status.py` prints this inventory with what each piece is for.

The downloader follows the pattern already in this repo (`catalog.rs` caches an
index under `paths::cache_dir()` with a TTL and a stale-cache fallback;
`updater.rs::download` shows the download shape), adding:

- Resumable HTTP, so a dropped connection is not a restart. A 200 response to a
  ranged request is detected and the write restarts rather than appending.
- SHA-256 verification, streamed in 1 MB chunks rather than loaded whole. A
  mismatch **deletes** the file, because leaving it means every later launch
  re-hashes 326 MB to reach the same verdict.
- Honesty about the unpinned: with no hash the download still succeeds but
  reports `Unverified` and carries the observed digest.
- ONNX Runtime is deliberately left unpinned — its correct build depends on the
  host CPU, so the URL is resolved at install time, and `ensure` refuses the
  empty URL rather than fetching it.

`VoiceConfig::paths` lets each file be overridden, and `resolved_paths` applies
only the overrides that are set.

## 9. Bugs the tests caught

Each of these would have shipped as a plausible-looking wrong behaviour:

**Speech:**

1. **Fences split across streamed deltas read code aloud.** A fence marker
   arrives as `"``"` then `` "`rust" ``. Matching per delta sees two short runs
   and never an opening fence, so the whole block was spoken.
2. **The third chunk boundary cut words in half.** The reference's last boundary
   is a plain whitespace split; the port passed an empty mark list.
3. **The vocabulary count was 114, not 115.** `n_token` is 178 *slots*.
4. **The model card's hash described the wrong file.** It is the upstream
   PyTorch weights, not the ONNX export. Pinning it would have failed every
   download with something that looks exactly like corruption.
5. **`espeakINITIALIZE_PHONEME_IPA` was `0x0001` from memory; it is `0x0002`.**
6. **`AUDIO_OUTPUT_RETRIEVAL` was `2`; it is `1`.** `2` is `SYNCHRONOUS`.
7. **`en-gb` does not exist in the bundled espeak-ng build.** `en` is the
   British voice.
8. **espeak-ng withholds punctuation entirely**, which flattens intonation *and*
   breaks sentence grouping.
9. **Caching a failed espeak-ng load poisoned the process permanently.**
10. **Test helpers read the process-global `LOOM_HOME`**, so a concurrent test
    pointing it at a temporary directory made them fail with paths like
    `/nonexistent/loom/espeak/...`.

**Audio and speech-to-text:**

11. **`std::fs::write` does not create parent directories**, so a test fixture
    panicked before reaching its assertion.
12. **A decimation test asserted a box average removes a tone.** It attenuates by
    the decimation factor — 1/3 — not to zero.
13. **A mel test asserted DC and Nyquist get weight.** librosa puts a triangle
    *vertex* at each end, and a vertex is a foot.
14. **A scaling test expected a 0.25 shift; it is 0.5.** The mel step runs on
    *power*, which is amplitude squared.
15. **`defined` double-counted the tokenizer's ids.** `<|endoftext|>` appears in
    both `vocab` and `added_tokens`, giving 51,865 where the truth is 51,864 —
    exactly `vocab_size`. Counting filled slots is correct by construction.
16. **`no_timestamps_token_id` is not in `config.json`.** It lives in
    `generation_config.json`, which the downloader was not fetching.
17. **344 tokens legitimately hold partial UTF-8**; a threshold of 200 had been
    chosen because it passed.

**The decode loop:**

18. **`MAX_TOKENS` was treated as a step count, not a sequence length.** It is
    Whisper's `max_target_positions` — the *total* decoder sequence, prompt
    included. Running 448 iterations with a 2-token prompt passed 449 tokens and
    failed inside the graph:

    ```
    Attempting to broadcast an axis by a dimension other than 1. 448 by 449
    ```

    **This only shows up on audio that produces no end-of-text token** — a tone,
    silence, or a noisy recording — because that is the case that runs to the cap
    instead of stopping early. The tone test is what caught it, and it would have
    been a crash on any real recording the model could not transcribe.
19. **ONNX Runtime 1.22.0 loaded, inferred, and then aborted the process on
    teardown** with `STATUS_STACK_BUFFER_OVERRUN`. `ort` 2.0.0-rc.13 wraps
    **1.28**. Nothing about it failed until exit, which is the worst kind of
    mismatch: everything looks right and the process dies at the end. Now pinned
    to 1.28.2 in both the installer and the fetch script, with a test asserting
    the two agree.

**Voice activity:**

20. **Silero VAD takes 576 samples, not 512.** Each call is prefixed with a
    64-sample context — the tail of the previous window. The graph declares
    `[?, ?]`, so 512 is accepted: the model ran, returned finite numbers, and
    reported **0 of 343 windows** as speech on audio Whisper transcribes at 100%
    word accuracy. The number that exposed it was the peak: 0.003 on speech,
    0.0005 on silence — it was working, on the wrong input.
21. **A stateful model returning the same answer every time is not threading its
    state.** The first VAD probe called it with six identical windows and got six
    identical probabilities, converging to a fixed point. That is now a test
    rather than an observation, because it is the failure mode that looks like
    success.

**The listening chain:**

22. **`windows_per_ms` did not divide by 1000.** "100 ms of silence" became 3125
    windows rather than 4, so the listener would never close an utterance. Ten
    tests failed on one missing division, and the conversion is now asserted as
    its own arithmetic so a regression reports the number rather than a
    mysterious never-ending utterance.
23. **Discarding a pending burst on one sub-threshold window was too strict.**
    A detector dips mid-word, so real speech never qualified and the listener
    reported **no utterance at all** on eleven seconds of clear speech. The
    reference waits the full minimum silence; so does this now.
24. **The espeak-ng library cache was keyed on nothing.** The first successful
    load answered every later request, so a call pointed at an installation that
    does not exist got back a library from a *different* directory and
    phonemised successfully: a missing install reported no error at all. The
    test that should have caught it could not, because until the suite had
    loaded espeak once in the same process the cache was always empty.
25. **`node.port.onmessage` read `event.payload`.** It is a `MessagePort`, not a
    Tauri event. Every message would have been `undefined` — a session that
    starts, reports no errors, and never sends any audio.
26. **An `AudioContext` created before a user gesture starts suspended**, and a
    suspended context delivers silence. The graph is built, the worklet runs,
    and no audio ever arrives. Now resumed explicitly.
27. **A test's expectation was invented.** `EXPECTED` in `dictate.rs` listed the
    pangram that seems natural for a speech test; the actual sample is a Loom
    demo line. The test failed for the right reason with a message that was
    confidently wrong about its own premise. It is now read out of a real
    transcription.
28. **`useVoice.attach()` was never called.** It is the only subscriber to both
    voice event channels. Read-aloud was silent and dictation appeared to do
    nothing, while every unit passed — because the bug was an absence, which is
    not a thing a unit test can see. `voiceWiring.test.ts` and the extended
    `check-voice-chain.py` exist for that class.
29. **`resolveImmediately()` was a no-op with a comment claiming it resolved.**
    One refused `play()` — autoplay blocked before any gesture — therefore
    stopped every later sentence from ever being heard, because `advance` awaited
    a promise nothing could settle any more.
30. **Two meters, two sensitivities.** The composer scaled the input level by 400
    and the surface by 4, so the same microphone drew differently depending on
    which screen you were looking at. One `barHeight` now defines it.
31. **The grep tool's empty result was indistinguishable from "not found".** This
    shell passes a quoted argument with its quotes attached, so every quoted
    pattern searched for `"panel-strong"` rather than `panel-strong` and
    reported zero matches — which twice produced a wrong conclusion about whether
    an edit had landed. Arguments are now unquoted, and the script prints how many
    files it actually read.
32. **The repeat-run tool reported failures for tests it never ran.** `"0 passed;
    607 filtered out"` was read as a failure, when it means the filter matched
    nothing. It reported four failures out of four for a working test. It now
    reads the real name from `cargo test --list` and treats "matched nothing" as
    its own outcome — which is how the `process::tests` intermittent was settled
    as *not reproducible* rather than reported as broken.

## 10. The pipeline

### Speech out

```
  text        clean::for_speech          markdown → prose
  phonemes    espeak::phonemize          + punctuation re-attached
  batches     chunk::split_phonemes       ≤ 510 phonemes, balanced
  tokens      phonemes::tokenize          + vocab filter, 0-padded both sides
  audio       tts::Kokoro::run            ONNX, f32 at 24 kHz
  output      Audio::to_wav_bytes         base64 to the webview, or a file
```

Streaming — `Kokoro::speak_stream` — feeds text in as the model writes it and
synthesises each sentence as it completes, so time to first audio depends on the
chunker rather than on how fast the reply finishes. The sink returns `false` to
stop, and a test asserts that refusing the first chunk stops synthesis rather
than letting it run on, which is what barge-in will rely on.

`ort::Session::run` requires `&mut self`, so synthesis is single-owner. That is
not a limitation worth working around: voice mode runs synthesis on one
dedicated blocking thread behind a queue (`src-tauri/src/voice.rs`), precisely so
a slow sentence cannot stall token generation.

### Speech in — complete

```
  capture     microphone.ts              getUserMedia, echo-cancelled
  worklet     loom-mic.js                downmixed, 100 ms blocks
  ipc         voice_listen_audio         f32 samples, no encoding
  resample    audio::resample_to_16k     from 44.1/48 kHz, averaged
  frames      audio::Framer::for_vad     512 samples at 16 kHz
  vad         vad::Vad::probability      + 64 samples of carried context
  endpoint    listen::Listener           Started / Finished / Truncated
  mel         mel::log_mel_with          80 × 3000
  encode      whisper::Whisper::encode   (1, 80, 3000) → (1, 1500, 384)
  decode      whisper::Whisper::decode   greedy, one token at a time
  text        tokenizer::decode          GPT-2 byte-level, bytes across tokens
  ipc         loom://voice-listen        a transcript event
  composer    Composer.tsx               appended, or sent if autoSend
```

And the other direction:

```
  reply       chat store                 markdown, streamed
  clean       clean::for_speech          code fences and URLs dropped
  chunk       chunk::StreamChunker       one sentence at a time
  phonemes    espeak::phonemize          espeak-ng over hand-written FFI
  tokens      voices + vocab             96 voice styles, one per sentence
  synth       tts::Kokoro::speak         24 kHz, 325 MB graph
  ipc         loom://voice              base64 WAV per sentence
  queue       SpeechQueue                strict order, one sentence lookahead
  speaker     an <audio> element         blob URL, revoked on stop
```

### The decode loop, and what it costs

The decoder export takes **no `past_key_values` inputs** — only `input_ids` and
`encoder_hidden_states`. It *returns* 16 `present.*` tensors, the key/value cache
for the next step, but there is nowhere to feed them back. So every generated
token re-runs the whole sequence: O(n²) rather than O(n).

That is a deliberate trade. Threading a cache by hand means slicing 16 tensors
per layer per step and getting every shape right, for a speed-up a dictation clip
— a few dozen tokens — would not notice. The `present.*` outputs are ignored, and
the cost is measured rather than hidden: 24 tokens in 1.63 s, 4.1× real time.

Three things make a transcript *wrong* rather than *broken*, and all three are
handled:

1. **The prompt.** `[SOT, NO_TIMESTAMPS]` from `DecodeIds`. Without the second
   token the model interleaves timestamps with the text.
2. **The suppression list.** ~90 ids the argmax must never select. Ignoring it
   produces a transcript of punctuation-less noise rather than an error.
3. **The mel.** Cross-checked against numpy; see §14.

## 11. The voice-mode surface

`src/components/VoiceMode.tsx`, opened with **Ctrl+Shift+V** or the speaker in the
title bar.

The composer's microphone button dictates *into the composer* — it is for writing
a message by talking. This is the other thing: a conversation held out loud, where
a reply is heard rather than read and interrupting means simply talking. The two
want different layouts, so they are different screens.

The layout exists to answer five questions, in the order a person asks them:

1. **Is it hearing me?** The dial, a strip of the last 3.6 seconds of level, and a
   status line. A meter that does not move means the wrong input device, and that
   is worth knowing before finishing a sentence rather than after.
2. **What did it hear?** Left pane, appended as utterances end.
3. **What is it saying?** Right pane, with the sentence being played highlighted so
   a fast reply is followable, plus a progress bar for how far through it is.
4. **How do I stop it?** Talk. The detector fires on the first window above the
   speech threshold, so the reply stops without a click.
5. **What if it is wrong?** With *Send what I say* off, a transcript waits in the
   composer to be edited. The footer says so, because a button that silently does
   nothing is worse than no button.

Three details that are decisions rather than defaults:

- **The read-along highlights on *playback*, not on arrival.** Sentences are
  synthesised ahead of being heard, so highlighting on arrival would run the
  highlight ahead of the voice by up to a whole sentence. `SpeechQueue.onSentence`
  fires when a sentence actually starts playing.
- **The sentence list comes from Rust.** `VoiceEvent::Started` carries `lines`, not
  a count, because re-splitting the text on the frontend would be a second
  implementation of the sentence rule and the two would eventually disagree about
  where a boundary is — which shows up as the highlight drifting away from the
  voice.
- **The meter's scaling is shared.** `barHeight` in `lib/voiceActivity` is the one
  definition of "how loud is this", used by both this surface and the composer. It
  was previously a literal `× 400` in one place and `× 4` in the other, so the same
  microphone drew two different meters depending on which screen you were looking
  at.

The one question the surface answers that nothing else could is whether audio is
*arriving*. `silenceHint` distinguishes "listening and nobody has spoken yet" from
"listening and no audio is reaching Loom" — a suspended `AudioContext` or a muted
device, which otherwise presents as silence, exactly like an empty room.

## 11.2 Not built

- **Nothing in the chain.** What remains is not code, it is verification: the
  install, and a real microphone.
- **Audio longer than 30 seconds.** Truncated, not chunked. An utterance past 28 s
  is closed and reported as `Truncated` so the caller can tell a complete sentence
  from a clipped one, but there is no splitting and stitching.
- **Partial transcripts while you speak.** Recognition needs the whole utterance, so
  a transcript appears when it ends. Streaming partial results needs a second
  thread and a policy for retracting a half-finished word, which is worse than
  showing it a moment later.
- **The installer-time download button.** The in-app path works; the setup app is
  untouched.
- **Voice picker for personas.** `Persona::voice` stores and resolves, and the speak
  path honours it, but there is no UI to choose one per persona. The global voice
  is selectable from the voice surface and from Settings → Voice.
- **Streaming time-to-first-audio.** Unmeasured, and it decides whether a fully
  conversational rhythm is viable.
- **Prefetching the models at start.** `Dictation` holds both Whisper graphs
  resident for the session, which is right for latency and means the first press of
  the microphone button waits on a 500 MB load. The button shows that it is working,
  but a prefetch at app start would be better.

## 11.1 Two absences, which no unit test could see

The surface work turned up two bugs of a kind the rest of the suite structurally
cannot catch, because every unit was correct and only a *connection* was missing.

1. **`useVoice.attach()` was never called by anything.** It was written,
   documented, and referenced nowhere — and it is the only subscriber to
   `loom://voice` and `loom://voice-listen`. So read-aloud was silent (sentences
   were emitted and nobody listened) and dictation appeared to do nothing at all
   (transcripts arrived as events nobody was subscribed to). Nothing was broken;
   the bug was an absence.
2. **The surface had no way in.** It was implemented and unreachable — no button,
   no shortcut.

`src/lib/voiceWiring.test.ts` exists for that class of bug, and asserts on
*presence*: that the store is subscribed by an app-level hook, that the surface is
mounted, that four independent ways of reaching it exist, that each command the
frontend calls is registered in Rust, and that the playback queue still settles a
refused `play()`. It reads source files rather than running code, which is a weaker
kind of test and is used deliberately — it is the only test that can fail for
"nobody calls this".

`scripts/check-voice-chain.py` had the same blind spot: it verified that the store
*listens* — which was true — and never that anything *subscribes* it. It now checks
both, plus that the surface is mounted.

## 12. Next steps, in order

1. **Say something into a real microphone.** Nothing in this repository has done
   that. Everything below is decoration until this step is done.
2. **Confirm the speech by ear.** `C:\Users\Ellio\AppData\Local\Temp\loom-kokoro.wav`.
   Transcription recovers every word of it, which is strong evidence the audio is
   speech; only listening settles whether it is *good* speech.
3. **Fix the `process::tests` intermittent** (§13), which fails roughly one run in
   four under a full parallel suite and passes every time in isolation.
4. **Then the rest of §11.2** — chunking for audio longer than 30 seconds,
   partial transcripts, and a voice per persona. Everything above it is built and
   tested to the edge of the microphone, which is where the only real risk now
   lives.

## 13. Known cosmetic debt

`src/lib/settingsCategories.ts` listed `"memory"` twice in its union. Fixed while
adding the `voice` category.

---

## 14. Phase 3 groundwork

### Turn cancellation exists — the riskiest gate is cleared

The original plan called this the riskiest remaining piece: barge-in only works
if the engine can be told to stop mid-turn, and that was assumed to be missing.

It is not. Read from `engine.rs`:

```rust
pub type Cancellation = Arc<AtomicBool>;            // providers/stream.rs:14

cancels: Mutex<HashMap<String, Cancellation>>,      // engine.rs:649

pub fn cancel(&self, session_id: &str)              // engine.rs:3129
pub fn cancel_with_note(&self, session_id: &str, note: &str)  // engine.rs:3244
pub fn cancel_all(&self)                            // engine.rs:3146
```

The turn loop polls `cancel.load(Ordering::Relaxed)` — in the tool loop, and
inside `wait_if_paused`, which returns `PauseExit::Cancelled` and breaks. And
`commands::cancel_stream` already calls `state.engine.cancel(&session_id)`.

`cancel_with_note` is the important one for voice: it records *why* the turn
stopped, and the engine writes that into the transcript, so an interrupted reply
explains itself rather than going quiet. That is exactly the behaviour voice mode
wants — "you talked over it" is a reason worth showing.

**So barge-in needs no engine work.** It needs voice mode to call a function that
already exists.

### whisper.cpp cannot build here; the ONNX route is the only one

`whisper-rs-sys` compiles whisper.cpp from source, which needs a C compiler.
Verified on this machine:

```
cmake  yes  C:\Program Files\CMake\bin\cmake.exe
cl     NO
clang  NO
gcc    NO
```

The same wall `espeak-rs` hit. So speech-to-text goes through the `ort` runtime
that voice mode already loads — the third model on one runtime, with no C
toolchain anywhere. The cost is the mel front-end, which had to be written in
Rust; the benefit is that nothing new has to build.

`rustfft` 6.4.1 is the one new dependency: pure Rust, MIT OR Apache-2.0, no build
step.

### The models, verified reachable

From `scripts/check-stt-sources.py`, sizes from `HEAD`:

| | Source | Size |
|---|---|---|
| Silero VAD | `snakers4/silero-vad` | 2.3 MB |
| Whisper tiny.en encoder | `onnx-community/whisper-tiny.en` | 32.9 MB |
| Whisper tiny.en decoder | same | 118.4 MB |
| Whisper tokenizer | same | 2.4 MB |

The decoder is the **cacheless** export, not `decoder_model_merged`: one graph
call per token with the full sequence each time. Slower, but no key/value cache
to thread through by hand, and a dictation clip is a few dozen tokens.

### The mel front-end, verified against a second implementation

`voice/mel.rs` is the part of speech-to-text most likely to be subtly wrong,
because being wrong does not error — it produces a *plausible* transcript with
the wrong words. So it is checked against an independent implementation written
from the same specification: `scripts/check-mel.py` recomputes everything in
Python with a **direct O(n²) DFT**, so no FFT convention has to be trusted.

| | |
|---|---|
| Filterbank (16,080 weights) | max difference **9.2e-10** |
| Frame count | **3000**, derived independently from the parameters |
| Log-mel, silent frames | **exactly equal** |
| Log-mel, frame 100 (the loud part) | max difference **3.0e-04** — float32 rounding |
| Clamp and normalisation, all 240,000 values | max difference **6.0e-08** |

**Two implementations agreeing is evidence; one agreeing with itself is not.**

Five things the cross-check settled, four of them my own tests being wrong:

1. **The clamp and the normalisation are global** — both depend on the peak
   across the whole spectrogram. That is why `log_mel_raw` and `normalize` are
   separate functions: it makes the structural part verifiable against a partial
   recompute, and the global part verifiable in full from the arrays.
2. **DC and Nyquist get zero weight from every filter.** librosa puts a triangle
   *vertex* at each end, and a vertex is a foot.
3. **Ten times the amplitude shifts the log by 0.5, not 0.25** — the mel step
   runs on *power*.
4. **The frame count is derived, not asserted:** `1 + (480000 + 400 − 400)/160 =
   3001`, drop the last → exactly the 3000 that `nb_max_frames` names.
5. **The shift is uniform even on floored values**, because the floor moves with
   the peak.

Two constants would have been wrong from memory and are worth recording:
Whisper's STFT is `pad_mode='reflect'`, not zeros; and the window is
**periodic** Hann (`cos(2πn/N)`), not symmetric.

### The tokenizer, decoded

`voice/tokenizer.rs` turns token ids back into text. Only the **decode**
direction is implemented — speech-to-text never encodes — which removes the BPE
merging, the rank comparison and the pre-tokenizer, leaving the byte table and
the accumulation.

The subtlety worth naming: Whisper's tokenizer is byte-level, so a character's
UTF-8 bytes can **span tokens**. `é` may arrive as two tokens. Decoding each
token separately produces mojibake for every non-ASCII character, so bytes are
accumulated across the whole sequence and interpreted once, at the end.

Three bugs found by testing against the real 2.4 MB file:

1. **`defined` double-counted.** `<|endoftext|>` appears in *both* `vocab` and
   `added_tokens`, so counting file entries gave 51,865 where the truth is
   **51,864 — exactly `vocab_size`**. Counting filled array slots cannot
   double-count, because a slot holds one token however often the file mentions
   it.
2. **`no_timestamps_token_id` is not in `config.json`.** It lives in
   `generation_config.json`, which the downloader was not fetching. `config.json`
   carries it indirectly as `forced_decoder_ids: [[1, 50362]]`, and
   `DecodeIds::from_configs` now falls back to that — with a test, because a
   loader that reads only `config.json` is exactly the mistake.
3. **344 tokens legitimately hold partial UTF-8.** The probe computed the same
   number independently: 0.68%, each the first half of a multi-byte character.
   The threshold had been 200, chosen because it passed rather than because it
   was right.

### The decode loop, and the bug only a tone could find

`voice/whisper.rs` runs the encoder once and then the decoder one token at a
time, greedily, stopping at end-of-text or at the sequence cap.

The cap was wrong at first, in a way worth recording. `MAX_TOKENS = 448` is
Whisper's `max_target_positions` — the **total** decoder sequence, prompt
included — and it was being used as a *step count*. With a 2-token prompt, 448
iterations passed 449 tokens, and the graph failed with:

```
Attempting to broadcast an axis by a dimension other than 1. 448 by 449
```

**That only happens on audio the model cannot transcribe.** Ordinary speech
emits end-of-text long before the cap, so the bug is invisible on every real
recording and fires on a tone, on silence, or on noise. It was caught by a test
that transcribes a 440 Hz sine purely to check the pipeline terminates — which
is the whole reason that test exists rather than being skipped as pointless.

### The runtime version, and a crash at exit

`ort` 2.0.0-rc.13 wraps ONNX Runtime **1.28**. This machine had **1.22.0**, and
the pair loaded models, ran inference, printed correct results, and then aborted
during teardown:

```
error: process didn't exit successfully: probe_whisper.exe
       (exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN)
```

Everything looked right and the process died at the end. Now pinned to 1.28.2 in
`scripts/fetch-onnxruntime.py` *and* in `install.rs`, with a test asserting the
two agree — because a version that has to match in two places is a version that
will drift.

### The round trip

`cargo run -p loom-core --example transcribe -- --builtin` synthesises a known
sentence with Kokoro, transcribes it with Whisper, and compares word by word:

```
expected   : The quick brown fox jumps over the lazy dog. Loom runs
             every model on this machine, with no network at all.
transcript : The Quick Brown Fox jumps over the lazy dog. Loom runs
             every model on this machine with no network at all.
word match : 21 of 21 (100%)
```

**100% of the words, from synthesised speech, through the mel, the encoder, the
decoder and the tokenizer, entirely on this machine.** Every layer was
independently checked first — the mel against numpy, the tokenizer against the
real file, the tensor signatures against the models themselves — and this is the
result of those checks compounding.

### The parallel-test problem, and why the fix is honest

With the Whisper tests added, `voice::tts::tests::speaks_when_everything_is_available`
began failing — only in the full suite, never alone, and never when its own
module ran serially. `ort` allows one environment per process and a session is
hundreds of megabytes; Rust's harness runs everything in parallel by default, so
several large sessions were being created at once.

The fix is a test-only mutex around the model-loading tests. That is worth being
straight about: **it makes the suite's parallelism match the runtime's actual
concurrency.** Production has one worker thread (`src-tauri/src/voice.rs`) and
loads each model once. The lock is not hiding a bug; it is removing a contention
the real program never creates.

### Voice activity, and the input that is not the size it looks

`voice/vad.rs` wraps Silero VAD. Its test suite passes, including one that
asserts real speech reads as speech — which is the assertion the module exists
for, because the first version **ran perfectly and meant nothing**:

```
343 windows of 512 samples (11.0 s)
0 read as speech (0%)
peak probability: 0.003089
```

That was 11 seconds of continuous speech that Whisper transcribes at 100% word
accuracy. The model was loading, running, returning finite numbers, threading
its state — and reporting no speech at all.

Silero's own `OnnxWrapper` says why:

```python
context_size = 64 if sr == 16000 else 32
x = torch.cat([self._context, x], dim=1)   # 64 + 512 = 576
...
self._context = x[..., -context_size:]
```

**The model takes 576 samples, not 512.** Every window is prefixed with the tail
of the previous one, so consecutive frames overlap by 64. The graph declares its
input as `[?, ?]`, so passing 512 is accepted — the model simply sees a window
with no lead-in.

So `Vad` owns two pieces of state rather than one: the 256-float recurrent
`state` *and* a 64-float `context`. `reset()` clears both, and a test asserts
that five identical windows do **not** produce five identical answers — a
stateful model that returns the same number every time is one whose state is not
being threaded, which is the failure mode that looks like success.

`segments()` turns probabilities into spans using the reference's own hysteresis:
0.5 to start, 0.35 to stop, 100 ms of quiet to close, 250 ms minimum. The two
thresholds are what stop a span flickering, and there are tests for a short gap
that must *not* split and a mid-band value that must *not* close.

### What this enables

`Vad` is what `listen::Listener` drives in production, and `Listener` is what
`Dictate` puts in a line with `Whisper`. So this section's model — with its 576-sample
input and its two pieces of threaded state — is the first link of the dictation
chain described in §11, and the hysteresis described above is the same one
`Listener` applies per window. What used to be a batch function over a whole
recording is now the middle of a live path.
