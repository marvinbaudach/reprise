# Handover — the 58.2 s showreel, afternoon of 2026-09-03

**The film is cut and scored:** `~/Videos/reprise-showreel/`
`reprise-showreel-58s-scored-v4.mp4`, 58.200000 s, −16.1 LUFS. Read
`showreel-58s.SESSION-2026-09-03.md` for the morning (the bed, the theme song,
the first version of shot 13) and `showreel-58s.HANDOFF.md` for the film's
shape. This file is the afternoon, which was one long answer to three things
the user saw in the finished morning cut.

## What the user found, and what it actually was

1. **"Concerts is in there twice."** It was. The Concerts shot held the good
   1000 km page; the My Stats shot that followed opened on its own 1.9 s lead,
   and that lead came from the tour take, whose Concerts station still stood at
   the old 500 km radius — three rows under "509 concerts hidden".
2. **"YouTube was skipped."** It was in the take and never in the cut. Measured
   on the 2026-09-02 tour: page turn at 56.99, ratio 557.
3. **Not noticed, and worse: the film opened on My Stats.** The Music pickup
   take had been shot from the stats page, so the first desk shot's lead was
   the same page that closed the tour.

The lead is not a defect — every desk shot opens on 1.9 s of the page it is
leaving, and Podcasts opening on Music reads as one continuous tour. It becomes
a defect exactly when the lead shows a page that is *not* the shot before it.
Mixing three desk takes is what produced both wrong leads.

**So the whole desktop half is one take again.** Stations walked once in cut
order, `SHOWREEL_STATIONS=library,podcasts,youtube,releases,concerts,stats`,
the app left on Radio before the take starts, and the device page taken as the
same walk's last station (no `--limit`, sync click off). Radio is the only page
in the film that appears once and is never returned to.

## Two more asks, both answered

- **"The visualisation longer, then the cover, then the end card."** Shot 13 was
  cover-then-spectrum; it is now spectrum-then-cover, and the two phone shots
  are 7.2 + 9.6 s where they were 9.6 + 7.2. The pair still ends at 54.6, so the
  end card did not move. In the cut the spectrum holds 7.8 s and the cover 1.8 s.
- **"The sync shot should have no active syncing."** It was filmed mid-transfer
  and run at 8× so the counter walked; the desk shots behind it carried the
  sync's own card in the sidebar. Both are gone. `SHOWREEL_BRIDGE_SPEED`
  defaults to 1 now — there is nothing left to compress.

## The takes that are in the film

| file | length | holds |
|---|---|---|
| `roh-gnome-tour-2026-09-03.mp4` | 102.4 s | six stations **and** the device page, one walk |
| `roh-android-gesture.mp4` | 21 s | shot 12, unchanged from 2026-09-02 |
| `roh-android-nowplaying3.mp4` | 69.9 s | shot 13: cover → spectrum → cover |

Page turns on the desk take, `find-page-turns.py`, in-point = turn − 1.9:

| station | turn | in-point | ratio |
|---|---|---|---|
| Music | 8.93 | 7.03 | 1180 |
| Podcasts | 20.12 | 18.22 | 3833 |
| YouTube | 31.23 | 29.33 | 630 |
| Releases | 41.96 | 40.06 | 2901 |
| Concerts | 52.83 | 50.93 | 1054 |
| My Stats | 64.30 | 62.40 | 2840 |
| device page | 73.78 | 71.88 | 2545 |

The lag from mark to turn is 2.20–2.60 s here, against 3.6–4.7 on 2026-09-02.

## The arithmetic, which is why nothing else moved

Six desk shots at 4.5 s where there were five at 5.4. **The desk block is 27.0 s
either way**, so the handover, both phone shots and the end card sit exactly
where they sat, and the whole cut is still 58.200000 s. The price is the grid
the file's header declares: 4.5 is not a multiple of 0.6. It is nine beats of
the 120 BPM bed, so the desk cuts land on the beat rather than beside it.

Boundaries now: 3.0 · 7.5 · 12.0 · 16.5 · 21.0 · 25.5 · 30.0 (handover) ·
37.8 (phone nav) · 45.0 (phone visualiser) · 54.6 (end card) · 58.2.

## Shot 13's timing, and how far the measurement can be trusted

The take: `play` at 10.907, sheet opened at 52.165 (position 41.3), tapped to
the spectrum at 54.299 (43.4), tapped back to the cover at 63.680 (52.8).
Film in-point started at 55.907 (position 45.0 at film 45.0) and was corrected
to **55.751** by the readout tick.

**The tick method's own noise is the thing to know.** Decoding the handset's
position readout at 30 fps and comparing digit changes to the whole second gave
the film a mean offset of −0.187 s before the correction and −0.044 s after.
But the readout itself jitters: consecutive ticks in the *source* take sit
1.03–1.13 s apart, and the same detector on the 2026-09-02 take — the one the
morning session measured at 8 ms in the finished cut — shows exactly the same
jitter. So the per-tick reading is worth ±0.2 s, the mean over five or six ticks
is worth roughly ±0.1, and a claim of single-digit milliseconds on this material
is a claim about one lucky window rather than about the shot.

`measure-vis-sync.py` remains the cross-check that cannot resolve this material
(r = 0.23 and 0.43, its two bands a quarter second apart), not the witness.

## Traps found this afternoon

- **A fresh app start syncs to the phone by itself**, and the first tour take of
  the day carried the resulting "Syncing · 5 … 27 %" card in all six shots. Wait
  for `phase: idle` *and* for the sidebar card to be gone — the AT-SPI check
  must match names starting with `Syncing`, because "Sync automatically when
  this phone connects" and "Last synced …" live on the device page and are
  always in the tree.
- **`desk.bring_to_front()` via the overview search resumed playback.** The
  second take ran with the music playing: the position advanced through every
  shot and the track changed mid-tour. Pause through MCP *after* the last raise,
  and check `music_get_playback_state` right before the take.
- **The auto-sync switch cannot be clicked where it is.** Its AT-SPI extents put
  it at y = 1323 in a 1048 px window — it is below the fold, and the pointer has
  no scroll. The way to a calm take is to let the sync finish, not to disable it.
- **`cp` is aliased to `cp -i` in this shell**; a copy that prompts looks like a
  copy that happened. Use `/usr/bin/cp -f` in scripts run from here.
- **The bottom bar is Titles / Artists / Queue and the tabs are not where the
  gesture step list says.** `539 2292` is Artists; Titles is `172 2292`. Typing
  a title into the Artists tab yields "No matching artists" and the take walks
  on regardless.

## State the machine and the phone were left in

- **Desktop Reprise is stopped** (unit `reprise-showreel`), deliberately: a
  running app syncs and a sync deletes the hand-pushed theme.
- **Phone**: `Reprise Theme.mp3` and its `.reprise-analysis` are back in
  `/sdcard/Music/Reprise/Reprise/Reprise/` and indexed (786 titles). The next
  desktop sync deletes them again.
- The Now Playing square is left on the **cover**, which is what a repeat of
  `take nowplaying` needs.
- Device lock released. Wake lock `showreel-recut` is still held — release it.
- Nightly `6802670776` is built and installed as `~/.local/bin/reprise`.

## What is left

1. Watch `reprise-showreel-58s-scored-v4.mp4` end to end and rule on the cover's
   1.8 s before the end card — it is the one length that was decided by
   arithmetic rather than by watching.
2. If the film is accepted: promote it to the canonical names
   (`reprise-showreel-58s.mp4` / `-scored.mp4`), then mount it on the showcase
   page — `showreel-film-on-the-showcase-page.HANDOFF.md` and
   `the-film-waits-outside-the-deploy.md`.
3. The handset is still a narrow strip in a 16:9 frame. The emulator route is
   costed in `showreel-58s.SESSION-2026-09-03.md`; the user chose to leave it.

## Correction, 14:20 — v4 was cut with the old script

The user rejected `…-scored-v4.mp4` as outdated, and it was. Everything above
describes the cut correctly; **v4 is not that cut.** Its segments
(`~/.cache/reprise-showreel/cut35`, written 13:56–13:58) are
`s-03-releases`, five desk shots at 5.4 s, no YouTube, phone 9.6 + 7.2 — the
shot list of `/home/marvin/Projects/reprise/scripts/showreel/cut-film.sh`
(mtime 2026-09-02 19:47, uncommitted on another branch's checkout), not of this
worktree's HEAD. `$SHOWREEL_WORK/cut35` is a fixed path shared by every copy of
the script, so the wrong checkout wrote the right cache.

**Why it passed unnoticed: 5 × 5.4 = 27.0 and 6 × 4.5 = 27.0, and the phone
pair is 16.8 s in either order.** Both plans total exactly 58.200000 s, so
`ffprobe duration` verifies nothing here. The discriminating check is
`cut35/list.txt` — 11 entries, `s-03-youtube` among them — and the per-segment
durations 3.0 / 4.5 × 6 / 7.8 / 7.2 / 9.6 / 3.6.

Re-cut from this worktree with no environment at all:

    bash scripts/showreel/cut-film.sh ~/Videos/reprise-showreel/reprise-showreel-58s-v5.mp4
    SCRATCH=$HOME/.cache/reprise-showreel/score-v5 SHOWREEL_BPM=120 \
      SHOWREEL_ARC=0 SHOWREEL_DROP=0 SHOWREEL_ALIGN=0 SHOWREEL_WINDOW=0 \
      bash scripts/showreel/score.sh \
        ~/.cache/reprise-showreel/musik/bed-final-58s.wav \
        ~/Videos/reprise-showreel/reprise-showreel-58s-v5.mp4 \
        ~/Videos/reprise-showreel/reprise-showreel-58s-scored-v5.mp4

→ `reprise-showreel-58s-scored-v5.mp4`, 58.200000 s, −16.1 LUFS, −4.5 dBTP,
LRA 4.4. Frame checks: 14.5 s is the YouTube page under "YouTube channels",
47.0 s is the spectrum under "The same visuals".

The in-points were re-measured against the take on disk before the re-cut, in
case the prose had drifted there too. It had not — `find-page-turns.py` on
`roh-gnome-tour-2026-09-03.mp4` gives turns 8.93 / 20.12 / 31.23 / 41.96 /
52.83 / 64.30 / 73.78, ratios 630–3833, lag 2.20–2.60 s. Minus 1.9 those are
exactly HEAD's defaults, bridge included.

**One thing the desk take carries that the text above does not mention:** the
sidebar device card reads "Pixel 10 Pro XL · Playlists updating" and the status
line "Refreshing podcasts…" in the desk shots. That is not the `Syncing · N %`
card the take was waiting out — the AT-SPI gate only matches names starting
with `Syncing`, and this phase is named differently. It is in all six shots and
cannot be removed without a reshoot.

**v1–v4 are all stale, the canonical names too.** `reprise-showreel-58s.mp4`
and `-scored.mp4` (09:59) are the morning cut. Nothing was promoted — that
waits on the ruling.

## The film is on the showcase page — 2026-09-03, 14:50

v5 was accepted and mounted. What that took, and what it moved:

- **`ChapterThree.tsx` closes on `<ShowreelFilm />` where it closed on
  `<ProductGallery />`.** The mosaic walked the same surfaces in stills that the
  film now walks in motion, so the page said it twice. The component, its CSS
  and `GALLERY_MOSAIC_*` stay in the tree — unmounted, not deleted; putting it
  back is one import and one element. `product-gallery.test.mjs` now guards
  exactly that state.
- **The encodes moved from `showroom/media/showreel/` to
  `showroom/public/media/showreel/`.** That directory pair *is* the deploy
  switch: Vite copies `public/` into `dist/`, and `pages.yml` uploads `dist`.
  `showreel-film.test.mjs` already owned the coupling and needed no help.
  `POSTER_AT=9.5` puts the poster on the Podcasts shot — the frame where the
  whole window is lit and the callout is legible.
- **The copy and the caption track were written for the old 60 s cut** and named
  search, lyrics, the Library Doctor and the MCP shot, none of which are in v5.
  Both rewritten from the boundaries the cut actually has: 3.0 · 7.5 · 12.0 ·
  16.5 · 21.0 · 25.5 · 30.0 · 37.8 · 45.0 · 54.6 · 58.2, eleven cues.
- `FILM_SECONDS` is 58.2, and a new test caps each encode and keeps the 720
  step below the 1080 step — mounting the film put 13.5 MB into a deploy that
  has no asset-size gate at all.

### The mobile pass, and what it actually found

Measured, not reasoned: headless Chromium over CDP, both faces loaded, CPU
throttled 4×, every element's rect against the viewport, at 280 · 300 · 320 ·
360 · 390 · 414 · 600 · 736 · 760 · 800 · 900 · 1024 · 1120 · 1280 · 1440 px.

`html, body { overflow-x: clip }` (global.css) means overflow never shows up as
a scrollbar or a document `scrollWidth` — it is swallowed silently. So
`scrollWidth === clientWidth` proves nothing here, and the offenders have to be
found by walking rects. Three did:

1. **`.site-header__hire` was outside the window from 737px to about 1105px.**
   The nav collapsed at 46rem, but the full row — mark, wordmark, Alpha, five
   chapter links, Source, hire pill — needs ~1105px of client width, because
   `--frame-pad` grows with the viewport and eats the rest. A phone in landscape
   and a 1024px tablet had the call to action entirely off screen. Breakpoint
   moved to 70rem.
2. **`.hero-product__phone { right: -5% }`** drew the tile 2–3px outside the
   window between 736 and 800px, where 5% of the frame exceeds `--frame-pad`.
   Now `right: max(-5%, calc(-1 * var(--frame-pad)))` — the lean survives
   wherever it fits.
3. **`.incident-panel`** ran 20px past the page below 300px: `minmax(17.5rem,
   1fr)` keeps its track minimum even when the container is narrower. Now
   `minmax(min(17.5rem, 100%), 1fr)`, the pattern `.hero__grid` already uses.

At 280px the hire pill was still 7px out with everything else hidden, so the
header gap drops to 12px under 26.5rem.

**Scroll smoothness measured clean, before and after** — median 16.6–16.8 ms,
p99 ≤ 25 ms, no frame over 32 ms, no long tasks, idle and with the film playing,
portrait and landscape. The one change made on judgement rather than on a
reading: the fixed header carried `backdrop-filter: blur(14px)` for the whole
page below the hero, which is a compositing pass per scrolled frame on a phone —
the exact trade `lightbox.css` already refuses under 720px. The header now
refuses it there too and closes its ground instead. A headless harness cannot
reproduce a phone's compositor, so this is precedent, not measurement.

### Left over

- **1.9 MB of screenshots ship without being referenced.** The nine mosaic
  captures and their ladders are still in `public/media/showroom`; the page uses
  eight files of the 46 there. `capture-ladder.test.mjs` still asserts every
  ladder step exists on disk, so deleting them is a decision about the data, not
  a cleanup — it was not made here.
- The film is on the page but nothing is promoted to the canonical
  `reprise-showreel-58s.mp4` / `-scored.mp4` names: those still hold the
  09:59 morning cut.
