---
slug: source-artwork-is-decoded-in-full-on-every-view
worktree: /home/marvin/Projects/reprise-source-artwork-is-decoded-in-full-on-every-view
branch: feature/source-artwork-is-decoded-in-full-on-every-view
phase: planned
codex_session:
created: 2026-08-25
---
# Source artwork is decoded in full on every view

Podcast covers arrive one after another when the Podcasts view opens,
visibly, over several seconds. Reported from use, then measured.

## What was measured

The obvious explanation is wrong and worth writing down so nobody spends
the afternoon on it again. The persistent remote-image cache sits exactly
at its cap and every entry is same-day fresh:

```
~/.cache/reprise/covers/remote-images-persistent
  1000 / 1000 files, 28 MB
  oldest 0.6 d, median 0.3 d
```

That looks like the covers are being evicted between visits. It is the
opposite. `cache::cached_path_in` touches every hit and eviction orders by
mtime, so everything looked at today carries today's date. **Control arm:**
fetch the six subscription image URLs and compare the bytes against the
cache directory — all six were present, mtimes from the two most recent
visits. Nothing is being evicted.

What costs the time is the size of what is cached. The cache stores the
**original bytes**, and subscription artwork is large:

| source | dimensions | file | decode + scale to 40x40 |
| --- | --- | --- | --- |
| RSS show A | 3000x3000 | 402 KB | **221 ms** |
| RSS show B | 3000x3000 | 407 KB | **238 ms** |
| RSS show C | 1500x1500 | 117 KB | 56 ms |
| RSS show D | 1024x1024 | 93 KB | 50 ms |
| YouTube avatars | 900x900 | ~130 KB | 27–29 ms |

`source_image_texture::decode_pixels` calls
`Pixbuf::from_file_at_scale(path, w*2, h*2, true)` on that original file, so
each row pays the full decode of a 9-megapixel JPEG to fill a 40 px slot.
Two rows alone account for the better part of half a second, and the eye
reads six staggered arrivals rather than one list.

Nothing else is the bottleneck, and each was ruled out rather than assumed:
`ARTWORK_WORKERS = 8`, so six rows do not queue behind each other; a memory
hit via `cached_texture` sets the texture synchronously on the GTK thread.

The in-memory `TEXTURE_CACHE` (128 entries, thread-local) does absorb the
second visit within one run. It cannot absorb the first — and the first is
what a user sees on every launch.

## The fix already exists in this repo

The album-cover path solved this. `cover_loader.rs` does not decode the
original: it calls
`reprise_core::cover::thumbnail(&CoverSource::FolderImage(source), size)`,
which is content-addressed — hash the bytes, return
`<XDG cache>/reprise/covers/<key>-<px>.png` if it exists, otherwise decode,
resize with aspect preserved, and write the PNG atomically.

Source artwork simply never got that stage. The change is to route the
source-artwork worker through the same one, so the expensive decode happens
**once per image, ever**, instead of once per view construction.

**Pick the thumbnail size from the request, not from the row.**
`decode_pixels` asks for `w*2`/`h*2` because it is filling a HiDPI slot, so
the stage must be selected against that doubled figure, not against the
40 px the caller passed. `ThumbnailSize::List` is 48 px and would be visibly
soft on a 2x display; `Bar` at 96 px covers the 40 px row. If no existing
variant is large enough for a caller's dimensions, add one rather than
rounding down — a blurry avatar is a worse defect than the one being fixed.

`thumbnail()` re-reads and re-hashes the original file on every hit
(`source_bytes` runs before the existence check). At ~400 KB that is
single-digit milliseconds against the 221 ms it replaces, so take it: it
reuses code that is already proven and already tested. Only if measurement
shows that read mattering is a key derived from the cache filename — the
remote-image cache already stores as `<hash(url)>.<ext>`, so a sibling
`<hash(url)>-<px>.png` needs no read at all — worth the extra surface.

## Control arm

The claim is a time, so the evidence is a time, measured the same way in
both arms. The harness exists: `REPRISE_MEASURE_SOURCE_ARTWORK=1` prints

```
source-artwork-measure phase=gtk_return request=… row=… visible=… jobs_ahead=… wait_us=…
```

`wait_us` at `phase=gtk_return` is queue-entry to texture-on-screen for one
row — exactly the span the user perceives.

Three runs, and all three are required:

1. **Control** — unmodified `dev`, cold: clear both remote-image cache
   directories, launch, open Podcasts, record every `gtk_return` line.
2. **Fix, cold** — same procedure. This arm pays the resize once and must
   not be reported as the win.
3. **Fix, warm** — relaunch without clearing anything and open Podcasts
   again. This is the arm that carries the claim, and it is the state a
   user is in on every launch after the first.

Report the per-row figures, not an average: the defect is that two rows are
an order of magnitude slower than the rest, and a mean hides exactly that.

## The regression test

A timing test would be flaky and would not say why it regressed. Assert the
mechanism instead: after one load, the thumbnail file for that size exists,
and a second load produces the texture **without opening the original
file**. Fixture images must include one large enough that the two paths
cannot be confused.

Mutation probe when the test is written: point the worker back at the
original file, confirm the test goes red, paste that output into the plan
file as acceptance evidence, then discard the reversion. No `cfg(test)`
switch in the production path.

## Out of scope, deliberately

- **`StartupTiming::AfterQuiet` stays.** It delays the first attempt until
  startup is quiet, which also contributes to the staggering — but it is a
  deliberate startup-cost decision, and changing it in the same commit
  would make the measurement above unattributable. If the warm arm still
  shows a visible stagger, that is the next investigation, with its own
  evidence.
- **`CacheScope::entry_limit` stays at 1000/200.** The measurement above
  says eviction is not happening. Raising a limit because a directory looks
  full is how the wrong fix ships.
- **The Android side.** `reprise_core::cover::thumbnail` already carries
  `MobileList`/`MobilePortrait`/`MobileFull`, so the same stage is available
  there — but no measurement was taken on the device, and this plan does
  not claim one.

## Verification

The local gate list comes from `merge-readiness`, never hand-assembled.

Closing check, because the reported symptom is visible rather than
measurable: launch, open Podcasts, and watch the covers appear as a list
rather than as arrivals.
