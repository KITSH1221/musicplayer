# musicplayer

A minimal terminal music player written in Rust — playlist, metadata, live waveform visualiser.

![status](https://img.shields.io/badge/status-learning%20project-blue)

```
  musicplayer                                            48000 Hz · 2 ch
  ♪  Jigsaw Falling Into Place  ·  Radiohead

  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡠⠔⠒⠊⠉⠑⠒⠤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
  ⠔⠒⠉⠉⠉⠉⠑⠒⠤⠤⠤⠤⠤⠔⠊⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠒⠤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠑⠒⠒⠤⠤⢄⣀⣀⣀⣀⣀⣀⣀⣀⣀⡠⠤⠤⠒⠒⠒⠉⠉⠉⠉⠉⠉
  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣀⡠⠤⠤⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠤⠤⠤⣀⣀⣀⣀⠀⣀⣀⣀
  ⠤⢄⣀⣀⣀⣀⣀⠤⠤⠔⠒⠒⠢⠤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡠⠔⠊⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠀⠀⠀
  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠒⠢⠤⣀⣀⡠⠤⠒⠉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀

  ▸ ♪ Jigsaw Falling Into Place                Radiohead  04:08
      When You Sleep                 My Bloody Valentine  04:11

  01:23  ──────────────────────────────────────────  04:08

  enter play   space pause   ←/→ ±5s   ↑/↓ select   q quit
```

## Features

- **Playback** — decodes MP3 / FLAC / WAV / OGG / M4A via `rodio` (which wraps `symphonia` + `cpal`)
- **Recursive folder scanning** — point it at a directory, it walks subdirectories
- **Lazy tag loading** — scanning only collects paths (fast), tags are read by a background
  thread and stream into the list, so a large library never blocks the UI
- **Live waveform visualiser** — a lock-free audio tap feeds an amplitude envelope, drawn as a
  smooth braided line using braille dot patterns
- **Graceful degradation** — files with no tags, corrupted files, and unreadable directories are
  skipped or shown with fallbacks instead of crashing the app
- **Minimal UI** — no borders, two colours, whitespace-based grouping

## Requirements

- **Rust 1.85+** (uses edition 2024)
- **ALSA development headers** (Linux)

```bash
sudo apt install libasound2-dev pkg-config
```

### WSL2 / WSLg note

WSL2 has no real sound card, so ALSA cannot find `card 0`. Audio has to be routed to WSLg's
PulseAudio server through the ALSA→Pulse bridge:

```bash
sudo apt install libasound2-plugins pulseaudio-utils

cat > ~/.asoundrc <<'EOF'
pcm.!default { type pulse }
ctl.!default { type pulse }
EOF
```

Verify with `paplay somefile.wav` before blaming the code — if `paplay` is silent, the problem is
the environment, not the player.

## Install

```bash
git clone https://github.com/KITSH1221/musicplayer.git
cd musicplayer
cargo build --release
```

Optionally install it into `PATH`:

```bash
cargo install --path .
musicplayer ~/Music
```

## Usage

```bash
# play a single file
musicplayer song.mp3

# play a whole folder (recursively)
musicplayer ~/Music

# with cargo — note the `--` separator
cargo run --release -- ~/Music
```

> `--release` is a **cargo** flag and must come *before* `--`. Writing
> `cargo run -- ~/Music --release` silently runs an unoptimised build — the flag gets passed to
> the program as an extra argument and ignored.

### Keybindings

| Key | Action |
| --- | --- |
| `Enter` | play the selected track |
| `Space` | pause / resume |
| `←` / `→` | seek −5s / +5s |
| `↓` / `j` | move selection down |
| `↑` / `k` | move selection up |
| `Home` / `End` | jump to first / last track |
| `q` / `Esc` | quit |
| `Ctrl+C` / `Ctrl+Q` | quit |

Tracks advance automatically when one finishes.

## Architecture

```
src/
├── main.rs       entry point + event loop
├── app.rs        application state, key handling, per-frame update
├── ui.rs         rendering (minimal style, no borders)
├── audio.rs      playback backend — wraps rodio
├── library.rs    Track model, directory scanning, tag reading
├── waveform.rs   lock-free audio tap + amplitude envelope
└── util.rs       small shared helpers
```

Dependencies point one way only:

```
main ──► app ──► audio ──► rodio
 │       │
 │       └────► library ──► lofty
 ├────► ui ──► app, util
 └────► util
```

`library.rs` and `waveform.rs` contain no I/O to the terminal or the audio device, so they are
unit-testable in isolation.

### Audio pipeline

```
decoder ──► AudioTap ──► Player ──► Mixer ──► cpal callback ──► speakers
               │
               │ push (lock-free)
               ▼
         rtrb ring buffer
               │ pop (once per frame)
               ▼
         Waveform: peak envelope → temporal smoothing → braille line
```

The tap runs on the **audio callback thread**, where blocking is not allowed. `rtrb` is a lock-free
single-producer/single-consumer queue: when it is full, samples are dropped rather than waited for.
Visualisation data is disposable; audio must never stutter.

### Async tag loading

```
scan (3 ms / 2000 files) ──► Vec<Track> with meta = None
                                    │
                    background thread ──► mpsc channel ──► apply every frame
```

`meta: None` means *not read yet*; `meta: Some(..)` means *read* (possibly with fallback values).
Keeping those two states distinct is what prevents untagged files from being stuck showing
"loading" forever.

## Tech stack

| Crate | Role |
| --- | --- |
| [`rodio`](https://crates.io/crates/rodio) | playback: decoding + device output + player controls |
| [`lofty`](https://crates.io/crates/lofty) | audio metadata (ID3v2, Vorbis comments, …) |
| [`ratatui`](https://crates.io/crates/ratatui) | terminal UI |
| [`rtrb`](https://crates.io/crates/rtrb) | lock-free ring buffer for the audio tap |

## Tests

```bash
cargo test
```

- `library` — extension matching, recursive scan, and that scanning stays lazy
- `waveform` — the tap forwards audio unchanged while filling the ring buffer; the envelope
  follows amplitude, stays within range, and decays to a flat line when the music stops
- `ui` — the braille dot bit layout matches the Unicode standard, silence draws a flat centre
  line, and full amplitude reaches the top and bottom rows

## Performance

Measured on this machine, decoding a 4:08 MP3 (≈22 M samples):

| | debug | release |
| --- | --- | --- |
| binary size | 81 MB | 5.7 MB |
| full decode | 10.8 s | 0.29 s |

Use `--release` for actual listening.

## Known limitations

- Tracks are played one at a time, so there is a small gap between them (no gapless playback)
- The waveform mixes both channels to mono, so it cannot show stereo information
- Seeking through an `AudioTap` relies on `Source::try_seek` being forwarded
- The waveform uses braille dot patterns (`U+2800`–`U+28FF`), which requires a terminal font that
  covers them (Cascadia Mono, JetBrains Mono, Fira Code and DejaVu Sans Mono all do)
- Very large libraries still parse every file's tags on startup (in the background, but the list
  fills in progressively)
- Layout assumes a dark terminal background and roughly 20+ rows

## License

MIT — see [LICENSE](LICENSE).
