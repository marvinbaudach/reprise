---
slug: source-artwork-is-decoded-in-full-on-every-view
worktree: /home/marvin/Projects/reprise-source-artwork-is-decoded-in-full-on-every-view
branch: feature/source-artwork-is-decoded-in-full-on-every-view
phase: refactored
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

## Acceptance evidence

The regression was mutation-tested by changing the worker's final decode
target from the content-addressed thumbnail back to the original source file,
then running:

```console
$ cargo test -p reprise-gnome source_artwork_uses_the_cached_thumbnail_for_texture_decode --no-fail-fast
   Compiling reprise-gnome v0.1.74 (/home/marvin/Projects/reprise-source-artwork-is-decoded-in-full-on-every-view/crates/reprise-gnome)
warning: unused variable: `thumbnail_path`
  --> crates/reprise-gnome/src/ui/podcasts/source_image_texture.rs:64:9
   |
64 |     let thumbnail_path = resolve_thumbnail(&CoverSource::FolderImage(path.to_path_buf()), size)
   |         ^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_thumbnail_path`
   |
   = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: `reprise-gnome` (bin "reprise" test) generated 1 warning (run `cargo fix --bin "reprise" -p reprise-gnome --tests` to apply 1 suggestion)
warning: `reprise-gnome` (bin "reprise") generated 1 warning (1 duplicate)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 48.04s
     Running unittests src/main.rs (target/debug/deps/reprise-cc2c8bfabda18feb)

running 1 test
test ui::podcasts::source_image::source_image_texture::tests::source_artwork_uses_the_cached_thumbnail_for_texture_decode ... FAILED

failures:

---- ui::podcasts::source_image::source_image_texture::tests::source_artwork_uses_the_cached_thumbnail_for_texture_decode stdout ----

thread 'ui::podcasts::source_image::source_image_texture::tests::source_artwork_uses_the_cached_thumbnail_for_texture_decode' (3668110) panicked at crates/reprise-gnome/src/ui/podcasts/source_image_texture.rs:208:28:
called `Result::unwrap()` on an `Err` value: Error { domain: g-file-error-quark, code: 4, message: "Failed to open file “/tmp/.tmpsWc6RM/original-must-not-be-opened.png”: No such file or directory" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    ui::podcasts::source_image::source_image_texture::tests::source_artwork_uses_the_cached_thumbnail_for_texture_decode

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2839 filtered out; finished in 0.08s

error: test failed, to rerun pass `-p reprise-gnome --bin reprise`
     Running tests/gnome_conformance.rs (target/debug/deps/gnome_conformance-59027ff405eb86cc)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.00s

error: 1 target failed:
    `-p reprise-gnome --bin reprise`
```

### Mutation probe: the YouTube boundary

The first probe above pins the mechanism (thumbnail, not original). A second
probe pins the *size selection*, because rounding down there is the failure the
plan calls worse than the defect being fixed. `ThumbnailSize::Portrait` was
removed from the ladder in `thumbnail_size_for_edge`, which is what a careless
future edit looks like, and the caller-driven boundary test went red on exactly
the YouTube case:

```console
$ cargo test -p reprise-gnome --bins desktop_thumbnail_ladder
test ui::podcasts::source_image::source_image_texture::tests::desktop_thumbnail_ladder_covers_every_source_artwork_edge ... FAILED

assertion `left == right` failed: 128px should select the 192px variant
  left: Grid
 right: Portrait

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2841 filtered out
```

128 px is `MediaShape::Wide` (64x36) doubled — the YouTube episode thumbnail,
the only source-artwork caller whose row exceeds `Bar`. The reversion was
discarded; the test is green again.

A third probe made the ladder round down (`.rev()` with `<=`) and went red on
the first case it reaches, 72 px selecting `List` instead of `Bar`, so the loop
never got as far as the YouTube case — which is why the isolated probe above is
the one recorded.

## Measured: the three arms

Release builds on both sides — a debug build would have decoded the resize
unoptimised on the fix side while gdk-pixbuf stayed full-speed C on the control
side, understating or inverting the result. Both arms ran against reflinked
copies of a real profile (6 RSS and 8 YouTube subscriptions, all with image
URLs), never the live one. `wait_us` at `phase=gtk_return`, in milliseconds:

| arm | n | p50 | p90 | p99 | max | rows > 200 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| control (dev) r1 | 519 | 1402 | 2517 | 3723 | 3812 | 99 % |
| control (dev) r2 | 519 | 874 | 2262 | 3414 | 3488 | 91 % |
| fix, cold r1 | 519 | 857 | 2316 | 3868 | 4374 | 89 % |
| fix, cold r2 | 519 | 895 | 2349 | 3485 | 3518 | 91 % |
| fix, warm r1 | 378 | 132 | 210 | 296 | 305 | 16 % |
| fix, warm r2 | 378 | 134 | 200 | 221 | 225 | 10 % |

The cold arm is indistinguishable from the control, exactly as this plan said
it must be: it pays the resize once and is not the win. The warm arm is, and it
is the state a user is in on every launch after the first.

**What makes these numbers trustworthy, stated so a later reader can attack
them:**

- Each cold arm asserted `remote=0 thumbnails=0` before launching and aborted
  otherwise. The first attempt at this measurement counted the whole `covers/`
  tree, which also holds `downloaded/` and `resolved/` album covers no arm
  clears, and could therefore not tell a cold arm from a warm one.
- Every arm waited for at least four free slots in the shared core budget and a
  one-minute load below 3, and an arm whose load more than doubled while it ran
  marks itself SUSPECT. None did; load ran 2.47 down to 1.63 across all six.
  The first attempt checked once, before the first arm, and load drifted from
  3.75 to 13.77 — the last arm was measured under three times the load of the
  first.
- Two rounds, so drift and effect are separable. The warm arm reproduces to
  within 2 ms (132/134 p50); the control arm's own p50 varies by 60 % between
  rounds (1402/874), which is why the claim rests on the order of magnitude and
  not on any single figure.

**Caveat, deliberately not smoothed away:** the arms do not carry equal row
counts (519 vs 378). In the warm arm some artwork resolves synchronously from
the in-memory texture cache and never enters the queue, so 141 requests produce
no measurement line at all. That is part of the improvement rather than an
artefact, but it means this is not a strict row-for-row comparison.

**Still not done:** the closing visual check from the section above — launch,
open Podcasts, watch the covers appear as a list rather than as arrivals. These
numbers say the wait is gone; they do not say the eye agrees.
