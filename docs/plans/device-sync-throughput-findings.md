---
slug: device-sync-throughput
phase: findings
created: 2026-09-02
---
# Device sync is not bandwidth-bound, it is round-trip-bound

The UI reported `2.1 MiB/s` on a USB3 link. The link is not the limit: the same
phone, the same gvfs MTP backend, takes **6.54 s for 200 MB = 30.6 MB/s**.
Everything below that is per-object cost, and per object this device charges
about half a second no matter how many bytes are in it.

## Measurements

All against the connected Pixel 10 Pro XL over native `mtp://` URIs — not the
`/run/user/1000/gvfs` FUSE path, which is ~2.5x slower per object and would have
overstated every number below. Timings come from a Python `Gio` script so both
arms ran warm in one process; single-shot `gio` CLI calls pay ~0.7 s of
mount-connect each and are not used here.

| Arm | Result |
|---|---|
| 200 MB, one file | 6.54 s → **30.6 MB/s** |
| 30 × 20 KB, direct `copy`, fresh folder | **0.443 s/file** |
| 30 × 20 KB, `copy → .part` + `query_info` + rename | **0.930 s/file** |
| Recursive enumeration of `/Music/Reprise` (1979 files, 320 dirs) | **8.0 s** (8.17 / 7.98 on two runs) |

The two copy arms were interleaved per file into two fresh folders, B before A,
so neither arm carries the other's folder growth. Delta: **+0.487 s per file for
the publish dance — it costs more than the copy it protects.**

Per-object cost does grow with how many objects the target folder already holds
(0.39 s/file into an empty folder, 0.79 s/file into one with ~250 objects), but
Reprise writes into per-album folders, so this is not a live concern.

## Where a sync actually spends its time

`replace_managed` (`crates/reprise-platform-linux/src/device_sync.rs:472`) does,
per file: `ensure_managed_directories` (one `make_directory` per level, ~6 ms
each) → `copy_async` into `<name>.part` (`:500`) → `target_size` → `publish`
(`:713`: `delete_if_present(target)` → `move_future` → `verify_published`).

That is ~0.93 s of ceremony plus payload. For an average 6 MB track the payload
is 0.20 s. **Roughly 20 % of the time on a fresh audio copy moves bytes, and on a
sidecar it is under 1 %.**

And the same path runs up to three times per track, because both sidecars go
through `replace_track` → `replace_managed`:

- **analysis sidecar** — planned in `mirror.rs:373-408`, and it *is* change-gated:
  `existing_size_bytes == Some(size_bytes) → continue` (`mirror.rs:394-399`).
- **lyrics sidecar** — `device_sync_effects.rs:190-198` → `:543`. **No gate at
  all.** Every successful audio copy re-copies the whole `.lrc`, whether or not
  it changed. There is no analog of the analysis size check.

This is why the screenshot sat on `06 Worldeater.reprise-analysis` at `67 of 72
files`: the tail of a run is a parade of few-KB objects that each cost ~0.93 s.
Note also that the rate figure is known to freeze in that phase
(`the-sync-bar-counts-work-not-bytes.md`), so `2.1 MiB/s` is a stale reading on
top of a genuinely slow phase — the true instantaneous rate there is far lower.

## Levers, ranked

1. **Gate the lyrics sidecar like the analysis sidecar.** Saves ~0.93 s per
   copied track that has a `.lrc`, at no risk — the pattern already exists three
   files away. Smallest change, no trade-off.
2. **Drop `.part` + rename.** Measured −0.487 s/file, −52 % of per-file time,
   and it applies to audio *and* both sidecars. What it costs: today an
   interrupted copy leaves `X.part`, which `Effect::CleanPartials`
   (`machine.rs:311`, `device_sync.rs:402`) sweeps at the start of the next run.
   Re-copy recovery does **not** depend on that sweep — `inventory_matches`
   (`mirror.rs:663`) compares Reprise's own local `DeviceFileRecord`, which is
   only written after a verified publish, so a failed copy is re-planned either
   way and `OVERWRITE` repairs the file. The real regression is the window
   between an abort and the next sync: a truncated file sits at the *final* name
   and Android's media scanner will index it, so the phone's player shows a
   broken track until the next run. That is a product call, not a technical one.
3. **Overlap local work with device work.** The loop is strictly sequential
   (`device_sync_planned.rs:170`): transcode → copy → verify → sidecar → next.
   MTP is one session so device writes cannot be parallelised, but transcoding
   (`device_transfer.rs:15`, `opusenc`, one thread per track) and sidecar
   encoding/staging can run one track ahead for free. Worth most when the
   library holds lossless sources; worth nothing when everything is already lossy.
4. **Walk the managed root once per run, not twice.** `Effect::CleanPartials`
   and the live `managed_files` scan are separate recursive enumerations,
   measured at 8.0 s each on this tree. One pass can serve both. Fixed ~8 s per
   run, independent of how much is copied.

Not recommended now: replacing gvfs with direct libmtp bindings. It would cut
the D-Bus layer, but it is a rewrite of the platform layer and levers 1–4 are
available for a fraction of the effort.

## Housekeeping

Benchmark objects were written to `/Internal shared storage/reprise-bench`,
`reprise-bench2`, `bA`, `bB` and all removed afterwards (290 objects, 4 folders).
The tree walk was read-only.
