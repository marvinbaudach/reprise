# S1 — the spectrogram, computed and stored

The first of the spectrogram chain. It builds **only the producer and its
storage** in `reprise-core` and the Linux backend. No UI, no transport, no phone.
Those are S2 (MTP and Wi-Fi sync) and S3 (the phone draws it), and both are
worthless until this exists.

The measurement behind every number here is
`docs/research/spectrogram-pipeline.md`. Read it before the plan; it refutes one
of the assumptions the request was written with.

## What this dataset is, and is not

One record per track, from which **everything** is derived and nothing is stored
twice:

- **seek bar amplitude** — the sum across bands
- **seek bar colour** — the band-index-weighted mean
- **spectrum bars** — the column at the playing position, interpolated
- **preview band** — the spectrogram itself, scaled to the display width

It is a **rendering dataset**. Song analysis — Similar Mix, Related Artists — was
deliberately removed from this project (`db_drop_audio_analysis_mix.rs`), and
this is not a way back in. No tempo, no brightness, no similarity scalars, no
per-track summary numbers that invite a recommendation feature later. If a field
would be useful to a recommender and useless to a renderer, it does not belong
here.

## Format

**24 logarithmic bands, 20 Hz to 16 kHz, 20 frames per second, one `u8` per
cell.** A 4-minute track is ~115 KiB.

**Stored uncompressed.** The measurement is the reason: the data carries 7.1–7.35
bits of entropy per byte, and zstd returns **1.09–1.14×** — nine to fourteen
percent, for a dependency, a failure mode and a decompression step on a phone.
The best variant tried (transpose + delta + xz) reached 1.45× and is not worth
the format complexity either.

That makes the real cost **~90–121 KB per track, ~170–195 MB** for the 1846-track
library, against the request's estimate of 20–30 KB. **Accept it.** The library
those files describe is **44 GB**; the spectrogram is **0.4 %** of it and travels
once per track. What was wrong was the estimate, not the design.

If size ever does become the constraint, the cheapest lever is already visible in
the same measurement: at 7.2 bits of real entropy, the low bits of a `u8` are
noise. Five bits per cell would save a third and be invisible. **Do not do this
now** — it is a note for whoever finds the constraint, not a requirement.

## Where it is stored

**Its own table, not a column on `tracks`.** `waveform_peaks` is a column, and it
is the wrong precedent to follow at this size: every windowed library query
projects `TRACK_COLUMNS` out of `tracks` (see
`crates/reprise-core/src/queries/clauses.rs`), and a 100 KB blob per row pushes
those rows onto overflow pages — the scan gets slower even though the blob is
never selected. `track_audio_analysis` (v18, since removed) was its own table for
this reason.

The migration is yours to write, including what happens to it when a track is
deleted.

**Decide and state: what invalidates a stored spectrogram.** A re-encoded or
replaced file behind the same row must not keep the old picture. Look at how
`waveform_peaks` handles — or fails to handle — the same question, and say which
you are following and why.

## How it is computed

**No new engine.** `realfft`/`rustfft` is already a dependency of `reprise-core`
and already drives the live visualiser (`crates/reprise-core/src/playback/cava.rs`,
a CAVA port). GStreamer's `spectrum` element is installed and unused, and stays
unused.

**One decode pass for peaks and bands.** `waveform.rs` already decodes every file
to produce `waveform_peaks`; decoding a second time for bands would be the
obvious waste. Its pipeline runs at **8 kHz**, which cannot carry a 16 kHz band —
raise the target rate to at least 32 kHz and feed both consumers from the one
pass.

**Then answer the question that follows, before you write the code:** does
raising the decode rate change the peaks that are already stored for 1846 tracks?
If it does, say by how much, and decide whether that is a re-computation, an
acceptable drift in a display artefact, or a reason to keep the peak accumulator
on its own downsampled feed. Do not discover this after the fact.

**Share the band mapping and the dB scale with the live visualiser, not the
smoothing.** The bands and the loudness scale must be one decision in one place —
this codebase has paid twice for the same decision living in two. Smoothing is
different: it is a property of *rendering*, and the stored data must be raw so a
consumer can choose its own attack and decay. Storing smoothed data would bake
one visualiser's taste into the file.

**Absolute loudness, no per-track normalisation.** This is the decision that
matters most to how it looks, and it contradicts what `waveform_peaks` does
today. Normalised per track, a quiet song and a loud one draw identically — the
bar stops saying anything about dynamics, and two tracks' colours stop being
comparable. Follow the principle already written down in this repo as *honest
loudness* (`bass_pressure.rs`, and the spec at
`docs/superpowers/specs/2026-07-22-visualizer-honest-loudness-design.md`): an
absolute dB window, no AGC. State the window you chose in dB, and why its floor
is where it is — everything below it becomes black, and a floor set carelessly
turns every quiet passage into a hole.

**The lowest band needs two FFT sizes.** The first of 24 log bands from 20 Hz is
**6.4 Hz wide**, while one bin at N=4096 over 32 kHz is **7.8 Hz** — the band is
narrower than the measurement grid, so it cannot be filled honestly from that
transform. CAVA solves this with two FFT sizes and that solution is already in
this codebase. Use it or better it; do not quietly let band 0 be a copy of
band 1.

## When it is computed

Lazily on play is enough for a desktop and **not** enough for a phone: the point
of the whole chain is that the data is already there before the track is first
heard. So there must be a **background backfill** over the library.

`pending_waveform_tracks` is a dead hook that exists for exactly this. Use it or
say why not.

It must be **resumable and interruptible**, and it must **not start unasked**.
The measured first run over 1846 tracks is **~46 minutes** of CPU at 1.1–1.7 s
per track. A machine that spends three quarters of an hour at full tilt after an
update, without being asked, is a bug regardless of how good the pictures are.
Decide what asks, and what happens when the user closes the app halfway through.

## A track without a spectrogram

Plays **immediately**. Flat bar, visualisers greyed. Find where that distinction
belongs in the existing code — the generation/async pattern in
`now_playing_wiring.rs` and `player_controller.rs` already carries this shape —
and make "not computed yet" distinguishable from "computed and empty". Those are
different states and only one of them is worth retrying.

## Proof

Every test mutation-proven: break the mechanism, show the real red output,
restore it, show green.

Beyond the ordinary unit tests, two that are specific to this:

- **A known signal.** Synthesise a tone at a known frequency and amplitude, run
  it through, and assert it lands in the band it belongs to at the level it
  should. A spectrogram pipeline that is subtly wrong still looks plausible;
  a sine at 1 kHz does not.
- **Two real tracks of different loudness** produce visibly different levels.
  This is the assertion that fails if per-track normalisation creeps back in.

## Verification

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p reprise-core --lib
cargo test -p reprise-android-ffi
```

Plus whatever desktop gates the touched crates carry. Starting counts: **2059**
in `reprise-core` (1 ignored), **47** in `reprise-android-ffi`.

Known and not yours: `podcasts::ytdlp::download_tests::failed_download_logs_…` is
occasionally red in the pack and green alone.

`crates/reprise-gnome` may change **zero lines** — this package builds no UI.

## Two hard limits

**The owner's real library and database are read-only.** Measure against files in
`/home/marvin/Music` by reading them; open
`/home/marvin/.local/share/reprise/reprise.db` only through a read-only URI, and
never write to it. Work on copies.

**The machine reached 96 °C tonight.** Measure single tracks, not the library.
No parallel `--workspace` runs. Nothing may open a window, a dialog or an app in
the owner's foreground.
