# Sounds

The sounds loom plays for its events, on the desktop and the iPhone ([PLAN.md, "Sounds"](../../PLAN.md#sounds)): a soft plucked string, B1 · Even, with notes from D major pentatonic.

They're loom's own. [`sounds.calx`](sounds.calx) makes every one of them in Calx, from noise and arithmetic (Karplus–Strong), with no samples or recordings, so they're under loom's MIT licence like the rest of the repo.

Each is a WAV, 22050 Hz, mono, 16-bit:

| File | Cue | Notes |
|---|---|---|
| `loom-reply.wav` | Reply arrived, and Another agent answered in a group chat | A4, then D5 0.09 s later |
| `loom-waiting.wav` | Waiting on you | A5, F#5 at 0.11 s, A5 at 0.22 s |
| `loom-routine.wav` | Routine posted | E5 |
| `loom-failed.wav` | Routine failed | B4, then F#4 at 0.13 s, ringing shorter |
| `loom-sent.wav` | Message sent | D6, short, at −12 dB |
| `loom-dictation-on.wav` | Dictation on | A4, then E5 at 0.07 s |
| `loom-dictation-off.wav` | Dictation off | E5, then A4 at 0.07 s |

The string: noise smoothed 6 times, a second string 5 cents sharp starting 4 ms later at 70%, damping 0.995, a 10 ms swell, and two passes of a one-pole low-pass at 0.22. Peaks are −6 dB but for Message sent's.

To make them again, from the repo's root:

```sh
calx run --release design/sounds/sounds.calx -- design/sounds
```

The desktop app builds them in, the Mac app's bundle and the iPhone app carry them as resources, where a notification names one, and loom.rip's demo plays these same files.
