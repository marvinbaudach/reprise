# Spectrogram pipeline: measurement and producer decision

Measured on 2026-08-05 in `/home/marvin/Projects/reprise-mobile` at
`feature/mobile-m8`, read-only. The three sample files under
`/home/marvin/Music` were read but never modified. The library database was
opened through SQLite's read-only URI mode. Referenced source files were
confirmed identical in the main repository.

## 1. Cheapest honest producer

The existing waveform producer is the correct decode seam:

- `crates/reprise-core/src/waveform.rs` defines the platform contract,
  `STORED_PEAK_COUNT = 1000`, and a streaming bucketed-RMS accumulator. Its
  stored waveform is normalized to the loudest bucket of each track.
- `crates/reprise-platform-linux/src/waveform.rs` previously decoded through
  `uridecodebin ! audioconvert ! audioresample ! F32LE mono 8000 Hz ! appsink`.
  Eight kilohertz was enough for an amplitude envelope but cannot represent a
  16 kHz spectrogram band.
- GNOME generated a missing waveform lazily on play. The Core function
  `pending_waveform_tracks` existed only in tests and had no worker.

The installed GStreamer `spectrum` element is not used by Reprise. The live
visualizer taps PCM through an appsink and runs the pure-Rust CAVA port in
`reprise-core`, backed by the existing `realfft`/`rustfft` dependency. Adding
GStreamer's spectrum analyzer would create a second, Linux-only FFT engine and
would break Core's cross-platform boundary.

The producer should therefore decode once at 32 kHz and feed both the waveform
and spectrogram consumers from that PCM stream.

### Does 32 kHz change existing peaks?

Yes, slightly. The RMS envelope is time-bucketed, but the 32 kHz stream retains
energy above the old 4 kHz Nyquist limit. A single dense real track was decoded
at both rates and reduced with the existing 1,000-bucket algorithm:

| Measurement | 8 kHz versus 32 kHz |
|---|---:|
| Identical buckets | 328 / 1,000 |
| Mean absolute byte delta | 0.948 / 255 |
| 95th-percentile delta | 2 / 255 |
| Maximum delta | 10 / 255 |

S1 treats this as a one-time recomputation of a display artifact. The mean
change is below one byte, and keeping a separate 8 kHz feed would add another
resampler and another source of drift to avoid an imperceptible difference.

## 2. Measured cost

The machine had reached 96 °C earlier that night, so measurements were serial
and limited to three individual tracks. `ffmpeg` represented the decode cost;
single-threaded NumPy represented a deliberately pessimistic FFT/banding cost.

| Track | Character | Duration | Decode to 32 kHz mono f32 | 24-band STFT |
|---|---|---:|---:|---:|
| As I Lay Dying — An Ocean Between Us (MP3) | dense | 253.1 s | 1.297 s | 2.392 s |
| A Day to Remember — Homesick (Acoustic) (MP3) | sparse/quiet | 247.7 s | 1.107 s | 0.578 s |
| Asking Alexandria — Alone Again (FLAC) | dense | 229.0 s | 1.725 s | 1.298 s |

Commands used:

```sh
ffmpeg -y -nostdin -i "<file>" -ac 1 -ar 32000 -f f32le track.pcm -loglevel error
OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 MKL_NUM_THREADS=1 nice -n 15 python3 band_bench.py
```

The three-track mean for decode plus the pessimistic Python analyzer was about
2.80 seconds per track, or about 86 minutes for 1,846 tracks on one core. The
native `realfft` implementation already runs more work in real time; decode is
expected to dominate, yielding about 46 minutes for a serial initial backfill.

### Compression result

Raw frame-major data is 24 bytes × 20 frames/second: 109,848–121,440 bytes for
the measured tracks. The request's raw-size estimate was correct; its expected
4–5× compression was not.

| Track | Raw | zstd -19 | Ratio | Band-major zstd | Ratio | Delta + xz -9e | Ratio |
|---|---:|---:|---:|---:|---:|---:|---:|
| AILD | 121,440 B | 107,085 B | 1.13× | 95,659 B | 1.26× | 89,588 B | 1.35× |
| ADTR acoustic | 118,848 B | 103,798 B | 1.14× | 94,636 B | 1.25× | 86,672 B | 1.37× |
| AA | 109,848 B | 100,473 B | 1.09× | 90,399 B | 1.21× | 79,288 B | 1.38× |

Measured Shannon entropy was 7.1–7.35 bits per byte. Even three-frame smoothing
only moved the best ratio to roughly 1.31–1.45×. A general-purpose compressor
would add a dependency, decompression on the phone, and another failure mode
for a 9–30% saving. S1 stores the bytes uncompressed.

For 1,846 tracks the real total is about 170–195 MB, around 0.4% of the 44 GB
music library. If storage becomes the constraint later, five bits per cell is
the cheapest visible lever and saves roughly one third. It is deliberately not
part of S1.

## 3. Storage placement

The measured database used 4,096-byte pages and occupied about 6.5 MB. Its
`tracks` table occupied 2,347,008 bytes over 573 pages. Existing 1,000-byte
waveform blobs fit within the practical row/page threshold and are omitted by
windowed `TRACK_COLUMNS` projections.

A roughly 100 KB spectrogram crosses SQLite's inline threshold and uses overflow
pages. Putting it on `tracks` would expand the main table by two orders of
magnitude and couple ordinary row maintenance to large derived blobs. The
removed v18 `track_audio_analysis` table already established the appropriate
shape for optional per-track derivations:

```sql
track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE
```

S1 therefore uses a separate `track_spectrograms` table. Fixed format facts
(32 kHz, 24 bands, 20 fps) are represented once by a format version and Core
constants, not repeated on every row. Frame count is derived from blob length,
so it is not stored twice.

### Invalidation decision

The old waveform cache did not invalidate: scanner upserts refreshed
`file_mtime` and `file_size` while preserving `waveform_peaks`. S1 does not
follow that bug.

A spectrogram is bound to the scanner's source fingerprint: modification time,
size, device, and inode. Changing any component deletes the spectrogram and
clears the old waveform cache in the same database update. Reads also compare
the stored fingerprint with the current track row, and writes refuse a result
when the source changed during decode. A rename that preserves the complete
fingerprint retains the valid rendering data; an in-place re-encode changes
mtime/size, and an atomic replacement changes inode.

## 4. Bands and absolute scale

The 24 requested logarithmic edges are:

```text
20.0 · 26.4 · 34.9 · 46.1 · 60.9 · 80.5 · 106.4 · 140.5 · 185.7 · 245.3 ·
324.1 · 428.2 · 565.7 · 747.4 · 987.4 · 1304.6 · 1723.5 · 2277.1 · 3008.5 ·
3974.7 · 5251.4 · 6938.0 · 9166.3 · 12110.4 · 16000.0 Hz
```

Band 0 is only 6.4 Hz wide. At 32 kHz, a 4,096-point FFT has 7.8 Hz bins and
cannot resolve it. S1 shares CAVA's two-grid band planner but uses a stronger
offline pair: 4,096 samples for the main grid and 16,384 for frequencies below
100 Hz. The bass grid has about 1.95 Hz bins, so band 0 contains independent
measurements instead of copying band 1.

Cells use RMS dBFS in a fixed **−70…−6 dBFS** window:

- −70 dBFS is below the measured quiet passages (bass-pressure calibration put
  quiet intros around −40 dBFS), leaving 30 dB for reverb tails and sparse
  high-frequency energy instead of punching black holes through them.
- −6 dBFS leaves headroom for a strong individual band while allowing genuinely
  loud material to saturate.
- No per-track normalization or AGC is applied to stored data. A 20 dB source
  difference remains about 80 byte levels across the 64 dB window.

The logarithmic band planner, Hann/FFT calibration, and absolute dB window live
in one shared Core module. CAVA measures its live bands on that same scale
before applying renderer-specific smoothing; PCM below the shared −70 dBFS
floor also no longer ages live autosensitivity. Stored frames are unsmoothed so
every consumer can choose its own attack and decay.

## 5. Boundary from removed song analysis

Schema v27 removed `track_audio_analysis` and the mix-draft tables. Those rows
contained per-track summary features such as loudness, dynamic range, spectral
centroid, rolloff, flux, onset rate, tempo, and projected comparison dimensions.

The spectrogram is different: it is a raw time series for rendering. S1 stores
no tempo, centroid, brightness, similarity fingerprint, confidence, or other
track summary. Seek color may later compute a band-index-weighted mean per frame
at render time; persisting that again as a track scalar would cross the removed
analysis boundary and is explicitly out of scope.

## 6. Missing data and the backfill

Playback does not wait for rendering data. The existing generation-based
asynchronous waveform wiring already separates starting audio from loading a
picture. S1's Core facade returns:

- `None` for absent or stale data: retryable and rendered as a flat/disabled
  placeholder by a future consumer;
- `Some(TrackSpectrogram::empty())` for a successfully decoded empty stream:
  complete, flat, and not retryable.

The old `pending_waveform_tracks` query could not be used unchanged: 89.3% of
the measured library already had peaks and would therefore be skipped despite
having no spectrogram. S1 generalizes the hook to pending **render data**: a row
is pending when either the legacy peaks or the source-current spectrogram is
missing.

Backfill is serial, resumable, interruptible, and opt-in:

- nothing calls it at application startup;
- an explicit caller creates the Linux backfill handle, which opens its own
  ready database connection on a worker thread;
- peaks and spectrogram are decoded together and committed atomically after
  each track;
- cancellation is checked between tracks and during GStreamer's bounded sample
  pulls; dropping the handle requests cancellation;
- completed rows remain committed, failed or source-changed rows remain pending,
  and the next explicit run resumes only those rows.

This is the producer/storage boundary only. MTP and Wi-Fi transport belong to
S2; phone rendering belongs to S3.
