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
