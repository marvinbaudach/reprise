---
slug: the-phone-analyses-its-own-music
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-18
strands: analysis,covers
merge_order: covers,analysis
---
# The phone analyses its own music

The Android app must show the seek-bar spectrum, keep the visualizer panels
instead of holding the cover on every track change, and download album covers
**without ever having synced with the desktop**. Today all three come from the
desktop sync alone, and there is **no flag to remove**: nothing in the app is
gated on "synced" — the phone simply never produces the data.

Read against `origin/dev` @ `3403a6e2cb`. Every line number below comes from
that state; whoever cannot find one has a different base.

This file carries the shared context, the settled decisions, the rules and
the cut. The tasks live in two strand files that run **concurrently**:

| Strand | File | Content | Tasks |
| --- | --- | --- | --- |
| `analysis` | `docs/plans/the-phone-analyses-its-own-music-analysis.md` | Rust core session + FFI sink/decoder callback + backfill worker + Kotlin decoder and service binding | A1–A6 |
| `covers` | `docs/plans/the-phone-analyses-its-own-music-covers.md` | `fetch_and_cache_in` in core + FFI fetch + cover pass in the artwork backfill + Kotlin rungs and wording | B1–B6 |

## What the phone does today (measured 2026-09-15, re-read 2026-09-18)

| Feature | Without sync | Why |
| --- | --- | --- |
| Seeking | works — the slider is gated on `durationMs > 0` only (`NowPlayingSheet.kt:594`) | — |
| Seek-bar **spectrum** | flat line | `track_render_bars` (`crates/reprise-android-ffi/src/track_analysis.rs:61-111`) returns `None` unless both `tracks.waveform_peaks` and a `track_spectrograms` row exist; they only arrive through the `.reprise-analysis` sidecar the desktop sync drops next to the file (`mobile_sync.rs:5-37` → `reprise_core::device_sync::mobile_import::import_analysis_for_track`, `mobile_import.rs:63-75`) |
| Visualizer | the live engine works (Media3 `TeeAudioProcessor` tap, `ReprisePlaybackService.kt:83`) | `panelHasVisualData` (`NowPlayingScene.kt:238-242`) keeps the **cover** up while `storedFrameCount == 0` until the live engine has captured a frame — every automatic track change shows the cover for a sidecar-less track; `NowPlayingPanelsTest` pins that as intended |
| Cover download | none | `reprise_core::cover_download` (MusicBrainz + Cover Art Archive) has a desktop caller only (`crates/reprise-gnome/src/ui/cover/cover_download_worker.rs:236-260`); the Android resolver already reads `covers/downloaded/` first (`cover.rs:83-93`, stage 1) but nothing on the phone ever writes there |

The `RenderDataBackend` trait (`crates/reprise-core/src/waveform.rs:61-88`) has
exactly one implementation, `GstreamerWaveformBackend`
(`crates/reprise-platform-linux/src/waveform.rs`). **The maths is backend
independent already:** GStreamer only decodes to 32 kHz mono F32
(`waveform.rs:25-27`) and feeds `WaveformAccumulator` (`waveform.rs:96-160`)
and `SpectrogramAccumulator` (`spectrogram.rs:215-305`, 24 bands, 20 frames/s,
1600 samples per frame, dual `realfft` 4096/16384). Storage is
`set_track_render_data(db, track_id, source_fingerprint, data)`
(`db_spectrogram.rs:74`) with the `TrackSourceFingerprint`
(`spectrogram.rs:207-212`) the scanner recorded; pending work is
`pending_render_data_tracks(db)` (`db_spectrogram.rs:307`); the desktop's
serial backfill loop is `run_render_data_backfill` (`spectrogram_backfill.rs:31`).

Artist photos are the finished template for anything that needs the network:
`artist_portrait.rs:106-130` (`artist_portrait_fetch`, gated by
`online_sources::network_allowed_or_off(&reader, &modules::ARTWORK_MODULE)`),
`artist_portrait.rs:178-220` (a core worker thread started from
`database_path`, reporting through a UniFFI callback interface), and on the
Kotlin side `ArtistPhotoBackfillConnection.kt`, `ArtistPhotoOffer.kt`,
`settings/OnlineSourcesSettingsPage.kt`.

## Decisions settled in the grill (2026-09-18)

1. **Decoder: the platform's `MediaExtractor` + `MediaCodec`, in Kotlin.** Not
   symphonia: it lives only in `reprise-stems` (not a dependency of
   `reprise-android-ffi`), has no Opus decoder (the sync itself writes Opus
   160), and would be a second decoder stack next to the platform's.
   `MediaCodec` decodes exactly what the phone can play and adds no
   dependency. Its cost — the decode loop is not Robolectric-testable — is
   paid by one small interface and the device run.
2. **All maths stays in Rust; Kotlin only pumps PCM.** A UniFFI callback
   interface `TrackPcmDecoder` (Kotlin implements, Rust calls) mirrors the
   live visualizer's shape (`visualizer.rs:205-245`, `ingest_pcm_i16(bytes,
   byte_count, sample_rate_hz, channel_count) -> bool`): the decoder pushes
   16-bit interleaved PCM at native rate and channel count into a Rust
   `AnalysisPcmSink`; Rust downmixes, resamples to 32 kHz and feeds the same
   accumulators the desktop uses. Host tests drive the whole path with a fake
   decoder.
3. **Resampling in Rust, linear interpolation, no anti-alias filter.** The
   consumer is a 24-band, 20 fps visual ending at 16 kHz
   (`SPECTROGRAM_HIGH_HZ`, `spectrogram.rs:18`); the only aliasing candidates
   are the 16–22 kHz remnants of 44.1/48 kHz sources.
4. **Waveform peaks are re-bucketed from per-frame RMS at the end**, not
   mapped through an `expected_samples` upper bound. `WaveformAccumulator::new`
   (`waveform.rs:106`) needs the count in advance and errors past it
   (`waveform.rs:122-133`); the desktop pays with a VBR headroom hack
   (`platform-linux/waveform.rs:145-160`). The session keeps one RMS per
   1600-sample frame and distributes the frames over the 1000 buckets when the
   stream ends. `WaveformAccumulator` is untouched; equivalence is a test.
5. **The running track first, then what comes next, then the rest — and the
   rest only while playing.** `import_track_analysis(track_id)`
   (`mobile_sync.rs:5-37`) becomes import-or-compute: after a sidecar import
   that ends in `Missing`/`Invalid`, it computes. It is called by the activity
   (`TrackAnalysisLoader.prepare` for the current track,
   `TrackAnalysisLoader.kt:111-125`; `prefetch` for the upcoming queue entries,
   `MobileSurfaceViewModel.kt:296-300`) **and by the service** on every track
   change on a background thread, so the current track is analysed even when
   the activity is gone; Rust deduplicates per track (in-flight set). The rest
   of the library is a backfill worker that `ReprisePlaybackService` starts
   while playback is `Playing` and cancels otherwise, skipped while
   `PowerManager.isPowerSaveMode`, one track at a time at
   `THREAD_PRIORITY_BACKGROUND`, preempted by any foreground request (the sink
   returns `false`, the decoder stops, the track stays pending). **No settings
   row** — no permission, no consent, and the standing decision of 2026-08-20
   ("kein 'Ich habe die Desktop-App'-Schalter") rules out a switch that only
   exists because the desktop could have done it.
6. **Writer discipline: decode holds nothing; the store is one short
   transaction on the shared `writer`.** `MusicLibrary` serialises writers
   through one mutex (`library_types.rs:25-29`); `scan()` holds it across the
   SAF walk (`lib.rs:172-179`) and `persist_queue` takes it on the main thread
   — the documented ANR pair. Rules: the decoder callback is never invoked
   while `reader` or `writer` is held; URI and fingerprint are read under
   `reader` and the guard released before the decode; the store is
   `writer()` → `set_track_render_data` → release, on a background thread,
   never on main (`LibraryWriteThreadGuard.kt` stays in force). A background
   wait on `writer()` during a scan is harmless; no main-thread wait is added.
   `set_track_render_data` compares the fingerprint itself, so a file replaced
   mid-decode is rejected like a stale sidecar.
7. **Sidecar first, compute second, never both.** A computed analysis is
   stored under the same fingerprint, so a sidecar arriving later reads as
   `AlreadyImported`. Decode failures are remembered per process and skipped
   by the backfill; the next process start retries them — they fail fast.
8. **Covers reuse the artwork consent.** One module (`ARTWORK_MODULE`), one
   switch: the "Download artist photos" row and the offer banner are reworded
   to artwork (artist photos and album covers); an existing yes covers both.
   Covers land in `<cache_root>/reprise/covers/downloaded/<key>.<ext>`, where
   `resolve_source_with_source` stage 1 already looks — the resolver needs no
   change. `album_dirs` is empty on the phone: nothing is written into the
   music folders.
9. **Cover fetch policy: only albums whose local resolution finds nothing.**
   The desktop downloads a canonical cover for every album; on the phone that
   is mobile data spent on albums that already show art. The phone fetches on
   demand at the now-playing rung and on the album detail page, and in a
   cover pass that runs inside the existing artwork backfill after the
   portraits (same handle, same progress). Lists never fetch
   (`ArtworkRequest.allowFetch`, `ArtworkRequestGate.kt:16-24`).
10. **The visualizer gets no code.** With a stored analysis present at track
    start, `storedFrameCount > 0` and `panelHasVisualData` is true from the
    first frame; the cover hold disappears as a consequence of decision 5. It
    is an acceptance criterion of the device run.
11. **No new dependencies, no renames.** No WorkManager, no symphonia, no
    libc; thread priority is set from Kotlin on the thread the callback runs
    on. A rename sweeps files the strand does not own.
12. **Two strands, concurrently; `covers` lands first.** See Parallelität.
13. **Device run per strand before landing, plus a short combined run after
    the merge.** By the pipeline session, never by Codex (Verification).

## Rules for the implementer — read first

**Per task: the test first, then the code.** Every task names its tests. A
test that is green on its first run has measured nothing — it must be red
before the code exists, and the red belongs in the log.

**Touch only the files your strand owns.** The ownership list is a claim
about where the state lives; if the contract cannot be met inside it, stop
and say so rather than reaching across — the other strand is editing the
files you would reach into.

**No device, no `adb`, no emulator** in either strand.

**Hard environment facts** (measured, not negotiable):

- The Android suite needs **JDK 21**; the system default kills Robolectric.
  `JAVA_HOME=/usr/lib/jvm/java-21-openjdk` before every Gradle call.
- The FFI tests depend on `readdir` order; the suite is green with `TMPDIR=/tmp`.
- `BUILD SUCCESSFUL` proves nothing: Gradle reports `:app:testDebugUnitTest`
  as up-to-date and runs nothing. The verdict is in
  `android/app/build/test-results/testDebugUnitTest/*.xml`;
  `scripts/check-android-suite.sh` checks their freshness itself.
- Run the Android suite **only** through `scripts/check-android-suite.sh` —
  it builds the host `.so`, regenerates the UniFFI bindings from it and sets
  `LD_LIBRARY_PATH` itself. `scripts/android-build.sh` builds for the device
  and makes 28 Robolectric tests falsely red afterwards. A fresh worktree has
  no `android/local.properties`; copy it from the main checkout.
- The UniFFI bindings under `android/app/src/main/java/uniffi/` are generated
  and gitignored. After every change under `crates/reprise-android-ffi/**`
  they must be regenerated (the suite script does it) or Kotlin does not know
  the new methods.
- `scripts/check-android-theme.sh` is a text scan: no `Color.` / `Color(` in
  Kotlin outside `ui/theme/`, KDoc included.
- Long runs go to a file, never to the console; the verdict is read with
  `grep -c '^test result: FAILED'` on the log, not from the last line.
- Never build under `/tmp`; the worktree's own `target/` (AGENTS.md).

**The Rust gates apply, scoped.** This plan touches Rust, so `cargo fmt
--check`, clippy and tests run — but `-p reprise-core -p reprise-android-ffi`,
**never `--workspace`, never `cargo audit`, never a release build**: two
strands share the machine. AGENTS.md's workspace-wide "Gates — ALL must pass
before every commit" section is overridden by name for the duration of the
strand; the workspace gate is a post-merge cross-check. The gate list per
strand:

```
cargo fmt --check
cargo clippy -p reprise-core -p reprise-android-ffi --all-targets -- -D warnings
cargo test -p reprise-core -p reprise-android-ffi
scripts/check-android-suite.sh
npm --prefix android run lint
scripts/check-android-theme.sh
```

## Verification — the device run (by the pipeline session, before landing)

Holder of `device-lock`; release build; a library with **no sidecars** (a
folder copied by hand, never synced — the run copy is seeded, a seeded file
cannot be unseen). One run per strand on that strand's build before it lands
(decision 13), then the combined run on the merged `dev`.

Strand `analysis`:
1. Play a sidecar-less MP3, an Opus and a FLAC file. Within ~5 s of the start
   the seek bar shows the spectrum; the loader logs the `Computed` outcome.
2. Let the track advance automatically. The visualizer panels stay up; no
   cover fade (`storedFrameCount > 0` at track start — logcat from
   `NowPlayingScene`'s existing availability log, or the screencap channel
   signature from `android-state-without-an-accessibility-node`).
3. With playback running, the backfill progress lines advance; pause → they
   stop within one track; battery saver on → the backfill does not start.
4. Screen off, activity killed (`am kill`), playback continuing: the next
   track's analysis still lands (service-driven request, decision 5).
5. Control arm: the same three files synced from the desktop show the same
   spectrum shape (visual; equality is not the claim, the decoders differ).

Strand `covers`:
1. Switch off: an album without embedded art shows the generated placeholder
   at the now-playing rung; logcat shows no MusicBrainz request.
2. Switch on: the same album gets its cover at the now-playing rung within
   the request timeout; `covers/downloaded/<key>.<ext>` exists under the
   app's cache dir; the album list row shows it after scrolling away and back.
3. An album with embedded art never triggers a request.
4. Airplane mode with the switch on: the placeholder stays, no crash, the
   attempt is retried after connectivity returns (`TransientFailure`).

Combined (post-merge): with the artwork backfill running, start playback of a
sidecar-less track — the seek bar still fills within ~5 s.

## Risks

- **`MediaExtractor` container coverage.** minSdk 26
  (`android/app/build.gradle.kts:56`); Ogg-Opus in the platform extractor is
  Android 10+. On older devices an Opus file is a `DecodeFailed`, skipped,
  retried next start — flat seek bar for it, nothing else breaks.
- **CPU and battery of the backfill.** One track ≈ 1–3 s of a background
  thread. A 3 000-track library ≈ an hour of low-priority CPU spread over
  listening sessions, bounded by decision 5. If the device run shows thermal
  throttling, the policy gains a thermal-status check before landing.
- **Writer wait during a scan.** The store blocks on `writer()` on a
  background thread while `scan()` walks the tree; harmless by construction,
  but the current track's seek bar waits as long as the scan does.
- **Cover fetch memo.** `LibrarySession.artworkFor` memoises resolved paths;
  if the memo is not dropped after a fetch the placeholder survives until
  restart. Named in B4 with a test.
- **Consent widening.** Users who enabled artist photos now also download
  covers. Same module, same sources, same promise — settled in the grill
  without a notice.

## Parallelität

Two strands, **run concurrently**: `strands: analysis,covers`,
`merge_order: covers,analysis`.

- **Strand `analysis`** (A1–A6). Owns
  `crates/reprise-core/src/lib.rs`, `crates/reprise-core/src/render_data_session.rs`,
  `crates/reprise-core/src/pcm_resample.rs`,
  `crates/reprise-android-ffi/src/lib.rs`, `crates/reprise-android-ffi/src/library_types.rs`,
  `crates/reprise-android-ffi/src/track_analysis.rs`, `crates/reprise-android-ffi/src/track_analysis/**`,
  `crates/reprise-android-ffi/src/mobile_sync.rs`,
  `android/app/src/main/java/io/github/marvinbaudach/reprise/{MediaCodecTrackDecoder.kt,SharedMusicLibrary.kt,ReprisePlaybackService.kt,TrackAnalysisLoader.kt,TrackAnalysisBackfillPolicy.kt}`,
  `android/app/src/test/java/io/github/marvinbaudach/reprise/{TrackAnalysisLoaderTest.kt,TrackAnalysisBackfillPolicyTest.kt,ReprisePlaybackService*Test.kt}`,
  `docs/plans/the-phone-analyses-its-own-music-analysis.md`.
  It is the only strand that edits the FFI `lib.rs` and `library_types.rs`;
  its new FFI modules are declared inside `track_analysis.rs`. In core it
  adds two modules and two `pub mod` lines; `waveform.rs` and
  `spectrogram.rs` stay untouched.
- **Strand `covers`** (B1–B6). Owns
  `crates/reprise-core/src/cover_download.rs`, `crates/reprise-core/src/cover_download_retry_tests.rs`,
  `crates/reprise-core/src/artist_portrait/{mod.rs (mod lines only),cover_backfill.rs,cover_backfill_tests.rs}`
  (the portrait engine `backfill.rs` is read, not edited),
  `crates/reprise-android-ffi/src/artist_portrait.rs`, `crates/reprise-android-ffi/src/artist_portrait/**`,
  `crates/reprise-android-ffi/src/artist_portrait_tests.rs`,
  `crates/reprise-android-ffi/src/online_sources.rs`,
  `android/app/src/main/java/io/github/marvinbaudach/reprise/{TrackCover.kt,LibrarySession.kt,AndroidLibrarySessionPort.kt,MainActivity.kt,ArtistPhotoBackfillConnection.kt,ArtistPhotoOffer.kt,ArtistPhotoOfferBanner.kt,BrowseTabs.kt,NowPlayingScene.kt,ArtistCover.kt}`
  (`NowPlayingScene.kt` for one argument at `:467`, the now-playing request
  site; `panelHasVisualData` and `NowPlayingPanelsTest` stay untouched),
  `android/app/src/main/java/io/github/marvinbaudach/reprise/settings/OnlineSourcesSettingsPage.kt`,
  `android/app/src/test/java/io/github/marvinbaudach/reprise/{TrackArtworkTest.kt,ArtistArtworkTest.kt,ArtistPhotoOfferTest.kt,OnlineSourcesSettingsPageTest.kt,AlbumCoverFetchTest.kt}`,
  `docs/plans/the-phone-analyses-its-own-music-covers.md`.
  Its new FFI module is declared inside `artist_portrait.rs`; it touches
  neither `lib.rs` nor `library_types.rs` (test injection at call level, B2).
  In core it edits `cover_download.rs` and the portrait backfill — no new
  top-level module, no `lib.rs` line. `MainActivity.kt` belongs to this strand
  alone (one line, B4).

Why the cut holds: the two features share no state and no file. The seam
that would have joined them — both wanting a field on `MusicLibrary` and a
`mod` line in `lib.rs` — is moved out of the way by giving `lib.rs` and the
struct to `analysis` and by hanging each strand's new modules under a file it
owns. The generated UniFFI bindings are gitignored, so each worktree generates
its own from its own `.so`; neither strand needs the other's Rust to compile.
Two concurrent cargo builds of the same two crates are inside the cap of three.

Merge order `covers,analysis`: the smaller branch lands first; `analysis`
rebases onto it — expected conflicts: none (disjoint files). The order
carries no dependency.

Post-merge cross-checks (comparisons that read files a strand does not own,
therefore in neither strand's acceptance):
1. `cargo clippy --all-targets --workspace -- -D warnings`, `cargo test --workspace`
   and `cargo audit` on the merged `dev` — the workspace gate neither strand runs.
2. `scripts/check-android-suite.sh` on the merged `dev` — both strands' Kotlin
   compiled against one set of bindings from one `.so`.
3. The combined device run above.
4. `git diff --name-only` of both branches intersected is empty (the code
   phase's disjointness check; if it is not, the cut was wrong — stop).
5. This file is frozen at the end of the plan phase; each strand writes only
   its own strand file.
