# Handover — the 58.2 s showreel, session of 2026-09-02

**The desktop half is shot. The phone half is blocked on one thing, and it is
not the phone.** Read `showreel-58s.HANDOFF.md` for the film's shape, the music
and the cut arithmetic — none of that changed. This file is what this session
established, decided and broke.

Everything below was measured on this machine today, against the `origin/dev`
nightly `8a5c36227c` on the desktop and the release APK built from the same
revision on the phone.

## Start here: the four handovers were deleted from dev

PR #802 (`ad11b31b39`) removed `showreel-58s`, `-56s`, `-66s` and
`showreel-the-hand-becomes-visible.HANDOFF` from `origin/dev` as "completed plan
records", while the 58 s file's own "What is left" listed five unfinished steps.
Reading only `dev` hides the entire re-record lineage behind a 60 s film that
was mounted and withdrawn for unrelated reasons. They were restored to
`docs/plans/` and committed on branch `docs/showreel-58s-reshoot`
(worktree `.worktrees/showreel-58s`). **Do not delete them again before the film
is shot.**

## What is in the can

All three in `~/Videos/reprise-showreel/`, all `VERDICT PASS`, all with the
right-hand info panel **open**, player dressed and paused.

| file | length | holds |
|---|---|---|
| `roh-gnome-tour-2026-09-02-panelopen.mp4` | 150.6 s | seven sidebar stations, no retries |
| `roh-gnome-pickup-music-2026-09-02.mp4` | 29.9 s | the Music station again, this time with a real page turn |
| `roh-gnome-handover-sync-2026-09-02.mp4` | 90.2 s | the device page, `Sync now` clicked at 10.63, 78 s of running transfer |

**In-points**, from `find-page-turns.py`, rule `in-point = turn − 1.9`:

| shot | source | turn | in-point |
|---|---|---|---|
| 01 Music | pickup take | 18.54 | **16.64** |
| 02 Podcasts | tour take | 36.50 | 34.60 |
| 03 Releases | tour take | 97.15 | 95.25 |
| 04 Concerts | tour take | 118.34 | 116.44 |
| 05 My Stats | tour take | 139.96 | 138.06 |

Every turn's peak-to-background ratio is 185–3552 against a threshold of 4. The
lag from mark to turn is 3.6–4.7 s here, not the 1.9–2.6 s the 2026-08-29
control arm found; the rule is anchored on the turn, not the lag, so it still
holds — but a lag that different is worth noticing rather than explaining away.

**Why the pickup take exists.** The first tour started on Music, so clicking
Music changed nothing: frames at 10.43 s and 14.5 s are the same page and the
detected "turn" was the sidebar row's own highlight — peak 0.39, against 1.2–7.7
for the genuine ones. Re-shot with the app left on My Stats: peak 3.42, ratio
1268. The whole re-record exists because shots had no visible cause; that one
had a hand and no consequence.

## What the user decided today

- **The right-hand info panel stays open.** This reverses the premise of the
  whole re-record, which existed because all three 2026-08-29 takes had it open.
  Their call, taken knowingly.
- **The sync is shown sped up.** Filmed 70 s, compressed to the shot's 6.8 s.
- **The phone shows Lorna Shore, newest album** — *I Feel the Everblack
  Festering Within Me* (2025, 10 tracks). *Homesick* by A Day to Remember was
  tried first and rejected: no album cover on the phone, and the cover is the
  subject of shot 13.
- **The theme song plays in the last scene**, so the spectrum swings to the beat
  the viewer hears.
- **The menu navigation is part of the shot**, not just the destination.

## The blocker: the theme song has no analysis, so it has no spectrum

The phone carries **708 `.reprise-analysis` sidecars**, one per track, written by
the device sync itself (its phase is literally `writing_analysis`). A track
without one falls back to a plain seek bar and no spectrum — which is what the
user noticed on *Alive Again* by Fight The Fade, and it is exactly what would
happen to shot 13.

`Reprise Theme.mp3` was put on the phone with `adb push` into
`/sdcard/Music/Reprise/Reprise/Reprise/`. It is indexed and playable — the
Titles search finds it, "1 title, Reprise • Reprise" — but it has **no sidecar**,
so as things stand shot 13 would show an empty square. There is no CLI that
writes one: `reprise-cli` is not installed and there is no `reprise-worker` in
`~/.local/bin`.

**The route that must be taken:** put the file through the desktop library and
let device sync produce the sidecar the way it does for every other track.
Sketch, none of it done yet:

1. copy `~/.cache/reprise-showreel/theme/Reprise Theme.mp3` under
   `library_root` = `/home/marvin/Music`;
2. rescan the desktop library so it is a real track;
3. put it in a playlist and select that playlist as a sync source;
4. sync, which writes both the transcoded file and its `.reprise-analysis`;
5. delete the hand-pushed copy at
   `/sdcard/Music/Reprise/Reprise/Reprise/Reprise Theme.mp3` so there is no
   duplicate.

This adds a track and a playlist to the user's own library. It is reversible and
it is the only route that produces the feature the film is meant to show.

## The theme song itself

`~/.cache/reprise-showreel/theme/Reprise Theme.mp3` — `spliced-58s.wav` at
256 kbit/s, **58.200000 s**, exactly the film's length. Tagged *Reprise Theme /
Reprise / Reprise*, with a 1000×1000 cover built from
`data/brand/lockup-horizontal-outlined.svg` on the cards' ground `#0D1014`.

Use the **outlined** lockup. The vertical one is `<text>` in Fraunces and the
wordmark overflows its viewBox under font substitution — measured, it renders
clipped on both sides. The wordmark is `fill="currentColor"`, black and
invisible on this ground; replace it with the cards' ink `#EAF2F1`, exactly as
`cardkit.py` does.

## Tools written or changed today

All uncommitted except where noted; `scripts/check-shell.sh` passes.

- **`scripts/showreel/ui-find.py`** (new, committed) — resolves a tappable
  element from a live `uiautomator dump` by label and prints its centre; exits 2
  by name when the label is absent. Validated against the running app.
- **`scripts/showreel/take-android3.sh`** (new, committed then extended) — two
  modes, `gesture` and `nowplaying`, each walked twice: `probe` resolves every
  step with no recording and caches the centres, `take` replays the cache with
  no dumps at all. A dump costs about a second and shot 12 is 9.6 s of one
  continuous gesture; six dumps would be six seconds of the shot standing still.
  Step targets may also be gestures: `swipe:x1,y1,x2,y2,ms`, `tap:x,y`,
  `text:lorna`, `key:BACK`.
- **`scripts/showreel/await-take-gnome4.sh`** (new) — waits for a human to click
  Reprise, settles six seconds, then execs `take-gnome4.py`.
- **`scripts/showreel/take-gnome4.py`** — `SHOWREEL_STATIONS` (comma-separated
  labels; empty means the handover alone) and `SHOWREEL_SYNC_DWELL`.
- **`scripts/showreel/cut-film.sh`** — `SHOWREEL_BRIDGE_SPEED`, the time-lapse
  that the code comments had always promised and never implemented. Verified:
  both speeds give exactly 6.800 s desk half and 7.800 s bridge, and with the
  variable unset the rendered bridge is **md5-identical** to the pre-change
  output. First-to-last-frame difference 0.30 normal against 2.72 at 8×; the
  normal-speed shot sits on "Syncing · 0 of 74 files, 0%" the whole way while
  the sped-up one walks 0 → 4 of 74. The `accel` argument already in the file is
  an easing curve for the push, not a speed control.

## Measured navigation, so nobody guesses it again

Bottom bar is **Titles / Artists / Queue**. There is no Albums tab; an artist's
albums live on the artist page, which is where the film's "albums" moment
happens.

The search is **per tab** — in the Artists tab it searches artists, and
searching a track title there returns "No matching artists". `BACK` dismisses
the keyboard and keeps the results.

Scrolling to Lorna Shore is not steerable: the list holds 83 artists, one 400 ms
swipe moves seven rows, and one 90 ms fling overshoots from B all the way to U.
The search is the only affordance that lands precisely inside a 9.6 s shot.

The step list now in the script, all labels read off the running app:

    artists   Artists            1.2
    search    Search library     0.8   --contains
    type      text:lorna         1.0
    keyboard  key:BACK           0.6
    artist    Lorna Shore        1.6   --contains
    album     I Feel the Everblack  1.6   --contains
    play      "Play "            2.5   --contains

`Play ` with the trailing space matters: it matches `Play I Feel the Everblack…`
and not the mini player's bare `Play`.

## Traps found today

- **The info panel's only reliable lever is the DB key written while the app is
  stopped**, and it works in both directions: `1` came up OPEN (28 widgets right
  of x=1400), `0` came up CLOSED (0). The header toggle is **one-way** — a real
  pointer click closes the panel and the same click does not re-open it, because
  the header bar re-lays-out when the panel goes and the cached coordinate then
  lands on nothing. Both earlier handovers are wrong about this in opposite
  directions.
- **`desk.raise_by_search()` failed twice** and cost an aborted run. Use
  `await-take-gnome4.sh`.
- **Never `systemctl stop` the await unit while it is running** — it kills the
  take it exec'd. One take died at station three that way, and that one is on
  me, not on the tooling.
- **`IFS=$'\t' read` collapses runs of tabs.** An empty field between two tabs
  vanishes and every field after it shifts left, so a dwell arrives where the
  options were expected. Empty option fields are written `-`.
- **Two `BACK`s leave the app**, not just the current page.
- **The APK's `versionName` stays `0.1.94`** because the Android module's
  version is hardcoded in `build.gradle` and does not track the commit. Prove an
  install with `lastUpdateTime` from `dumpsys package`, never with the version.

## State the user's machine and phone were left in

- **Desktop**: `~/.local/bin/reprise` is the `origin/dev` nightly `8a5c36227c`.
  `ui.info_panel_visible` is `1`. The app runs under the systemd user unit
  `reprise-showreel`.
- **Sync configuration changed**: *Recently played* (50 tracks, 341 MB) was
  added as a sync source so the handover shot had a progress bar to show. It
  transferred and is still selected. **Deselecting it queues those 50 tracks for
  deletion on the next sync** — which is why it was left alone rather than
  quietly reverted. One click either way, the user's call.
- **Phone**: the release APK from `8a5c36227c` is installed (`lastUpdateTime`
  2026-09-02 19:54:44), the library re-indexed, and
  `/sdcard/Music/Reprise/Reprise/Reprise/Reprise Theme.mp3` is the hand-pushed
  theme song that still needs replacing by a synced copy.

## What is left

1. Get the theme song a `.reprise-analysis` by the route above. Until then
   shot 13 has no spectrum, and the spectrum is the shot.
2. `probe gesture`, then `take gesture` — shot 12.
3. `probe nowplaying`, then `take nowplaying` — shot 13, with the theme playing.
4. Measure both phone in-points on the finished cut by the two-band method in
   `showreel-56s.HANDOFF.md`. The on-screen clock is not good enough: it once
   put the visualiser 0.6 s ahead of the music and gave no hint of it.
5. Cut with `SHOWREEL_BRIDGE_SPEED` set, score, and check the duration is
   58.200.
6. Decide the Concerts shot. It currently shows three rows under a
   `Zurich · 500 km` filter with "509 concerts hidden" beneath them, which reads
   sparse. Nobody has ruled on whether that is what the shot should say.

## One process failure, recorded because it could have cost a take

The device lock was acquired as `showreel-sync` at 19:35 with the default
1800 s TTL, and the phone work ran past it. By the time the session ended the
lease had expired and **another session (`swipe-cover`) had taken the phone** —
so part of the navigation probing above ran with no valid lease. Nothing
collided this time, but that is luck, not method: the 2026-09-02 rule exists
because two sessions once installed different builds in the same second and one
of them filmed the other's. A phone take is a lease over the whole measurement;
re-acquire it before every stretch, and check `device-lock status` rather than
assuming the lease from an hour ago is still yours.
