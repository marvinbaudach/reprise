# Handover — the 58.2 s showreel, sessions of 2026-09-02

**The film is cut, scored and exactly 58.200 s. One claim in it is false:
the phone's visualiser is not in time with the music, and the take cannot be
made to be — it needs re-shooting with the handset connected.** Read `showreel-58s.HANDOFF.md` for the film's
shape, the music and the cut arithmetic — none of that changed. This file is
what the two sessions of 2026-09-02 established, decided and broke.

## Where it stands, right now

| shot | state |
|---|---|
| 01 Music, 02 Podcasts, 03 Releases, 05 My Stats | shot, `VERDICT PASS` |
| 04 Concerts | **shot at 1000 km**, `roh-gnome-concerts-2026-09-02.mp4` |
| 09 handover / sync | shot, 78 s of live transfer, cut at 8x |
| 12 phone navigation | shot, `roh-android-gesture.mp4` |
| 13 phone visualiser | shot, but **out of sync with the bed by 43.4 s** |
| the cut | `~/Videos/reprise-showreel/reprise-showreel-58s-scored.mp4`, 58.200 s |

Nothing is running and nothing is held: the `showreel-await` unit is stopped,
the device lock is free. Reprise is running under the `reprise-showreel` unit
with the info panel open, which is the state a take wants. The phone is **not
attached** — `adb devices` is empty — which is why the one open item is open.

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
| 04 Concerts | concerts take | 43.23 | **41.33** |
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

## The theme song's analysis: what it needed, and how it was made

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

**The route that was taken instead — no library change, no sync.** The sync
route above is not the only one, and reading the code showed why. The sidecar is
produced from data the desktop already holds, not from anything the transfer
creates: `AnalysisSidecar::for_track` in
`crates/reprise-core/src/device_sync/analysis_sidecar.rs` reads
`track_spectrograms.data` and `tracks.waveform_peaks` out of the database and
encodes them. The bytes come from decoding the source file, and that decode is
`GstreamerWaveformBackend` in `crates/reprise-platform-linux/src/waveform.rs` —
reachable without a library, a playlist or a device.

The phone side does not object to a hand-made sidecar. Its scanner pairs the two
files by device-relative path with the extension replaced
(`crates/reprise-core/src/library/scanner_mobile_sync.rs`), and the import in
`crates/reprise-core/src/device_sync/mobile_import.rs` uses the sidecar's own
fingerprint only as an idempotency token: the render data is stored against the
fingerprint of the file **on the phone**, read from the phone's database. There
is no hash and no duration check. All 708 existing sidecars were computed from a
desktop source and lie beside a transcoded phone file, so a sidecar computed
from the very file that is on the phone is the more faithful case, not a
smuggled one.

So there is now a tool: `cargo run -p reprise-platform-linux --example
analysis_sidecar -- "<audio file>"` writes `<audio file>.reprise-analysis`
beside its input, decoding with the same backend the backfill uses, encoding
with the same Core serializer, and refusing to write anything it cannot decode
back. Nothing else in the repo could produce a sidecar standalone; `reprise-cli`
has no such subcommand.

**Done for the theme song.**
`~/.cache/reprise-showreel/theme/Reprise Theme.reprise-analysis`, 29 012 bytes:
1165 spectrogram frames of 24 bands, 1000 waveform peaks, 52 bytes of header.
The arithmetic checks out from the other side — 58.2 s at the format's 20 Hz is
1164 frames — and the content is real rather than silence: band values run
0–236, mean 138, and the per-second means track the music, including the drop to
31 in the final second where the theme fades.

**Confirmed on the phone at 20:41.** The sidecar was pushed beside the track,
the library rescanned from the Library-actions menu (774 titles), and the theme
played: the square shows the spectrum swinging, and the seek bar carries a real
waveform instead of the flat line. Shot 13 has its subject.

That the bars move is not on its own proof that they are *this* track's bars —
the import is lazy and the square was already animating on arrival. The seek
bar is what settles it. Its envelope is loud through the middle and tapers at
both ends, which is the shape of the per-second means measured off the file
before it ever reached the phone (118 at the start, 160 in the middle, 31 in
the closing second as the theme fades). Data from another track would not draw
that curve.

**But it will not survive the next sync, and that is not a small footnote.**
Between 20:02 and 20:28 another session ran a device sync, and it emptied
`/sdcard/Music/Reprise/Reprise/Reprise/` — the hand-pushed theme song from
earlier today was gone, and the sidecar count fell from 708 to 638. Both files
were pushed again afterwards. So: the theme song and its sidecar are unmanaged
files that a sync prunes. **Shoot shot 13 without a sync in between, and check
both files are still there immediately before the take** — one `adb shell test
-f` each. If a sync has eaten them, push both again and rescan; nothing else is
needed, the sidecar is already made.

The old sync route — copy into `library_root`, rescan, playlist, sync, delete
the hand-pushed copy — is recorded here only as the fallback if the pairing
turns out not to register. It changes the user's library and it is not needed
unless that happens.

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

- **`crates/reprise-platform-linux/examples/analysis_sidecar.rs`** (new) — the
  only way in this repo to write a `.reprise-analysis` without a device sync.
  Decodes with `GstreamerWaveformBackend`, encodes with Core's own serializer,
  and decodes its own output before writing, because a sidecar that will not
  parse is not an error on the phone — it is a silent plain seek bar.
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

## Traps found while shooting the phone (2026-09-02, evening)

- **`take-android3.sh` walked exactly one step and reported success.** The step
  loop read its list on stdin and `adb shell` swallowed the rest of it. Both
  loops now read on file descriptor 3. This is the failure the probe was meant
  to make impossible and it looked identical to a clean run.
- **A German locale aborted the probe after step one.** `bc` prints `3.6` and
  `printf %.1f` under `de_DE` calls that an invalid number. The script exports
  `LC_ALL=C`. Note that these are two independent bugs with the same symptom:
  the locale one fired first and hid the stdin one behind it. A run that stops
  after one step is not evidence of either in particular.
- **`kill -INT` does not stop scrcpy 4.1 here.** The recorder kept running and
  `wait` never returned: a 19 s shot produced a 119 s file, and the take only
  ended when something outside killed it. `stop_scrcpy` sends SIGTERM and waits
  with a bound. SIGKILL is deliberately not sent — it would leave the mp4
  without its moov atom, which is a lost take rather than a long one.
- **`scripts/check-shell.sh` did not pass**, contrary to what the earlier note
  in this file said: a `# shellcheck disable` in front of a single `case` branch
  made the whole file unparseable. It passes now.
- **Do not match the result row by its title.** The title is also the text
  standing in the search field one row above it, so `--contains "Reprise Theme"`
  resolved to the field: the take tapped the search box, the keyboard came back,
  and every step after that typed into it. The row is matched by
  `Reprise • Reprise` instead.
- **The Now Playing square must be left on the cover before a take**, and every
  take flips it. The choice is stored on the phone and does survive a restart —
  two takes in a row showed the spectrum first for exactly this reason. There is
  no read-out; one screenshot of the open sheet is the check.
- **Let the artist photos finish downloading first.** A "Downloading artist
  photos 3/5" banner appeared during the first probe, pushed the page down
  between the dump and the tap, and the Play button was missed. It is also not
  something the shot should show.
- **A device sync deletes the theme song and its sidecar**, and one ran three
  times during this session (20:02, 21:11, and once more mid-probe). Check both
  files immediately before every take.

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

1. ~~The theme song's `.reprise-analysis`.~~ **Done and confirmed on the
   phone.** The one standing caveat: a device sync deletes both the track and
   its sidecar, so re-push and rescan before any re-take of shot 13.
2. ~~`probe gesture`, then `take gesture` — shot 12.~~ **Shot 12 is in the can:**
   `roh-android-gesture.mp4`, 17.97 s, marks in
   `~/.cache/reprise-showreel/timeline-android-gesture.tsv` (begin 4.01, artists
   5.52, search 6.98, type 8.06, keyboard 9.60, artist 10.60, album 12.48, play
   14.26, end 17.11). Artists tab, search "lorna", the artist page with four
   albums, the album's track list, and the tap that starts it.
3. ~~`probe nowplaying`, then `take nowplaying` — shot 13.~~ **Shot 13 is in the
   can:** `roh-android-nowplaying.mp4`, ~30 s, marks in
   `timeline-android-nowplaying.tsv` (open 14.02, spectrum 17.65, end 29.81).
   The sheet opens on the Reprise cover and the tap at 17.65 brings the spectrum
   in, swinging to the theme. That was settled, not assumed — see the traps
   below.
4. ~~Rewire `cut-film.sh` to the takes that exist.~~ **Done.** It cuts six
   sources now, and two of them are new take ids rather than new filenames —
   rewiring only the `IN*` assignments would have kept cutting Concerts out of
   the old 500 km tour take and both phone shots out of one file:

       IN1  T1  roh-gnome-tour-2026-09-02-panelopen.mp4     02, 03, 05
       IN2  T2  roh-gnome-pickup-music-2026-09-02.mp4       01
       INC  TC  roh-gnome-concerts-2026-09-02.mp4           04
       INB  TB  roh-gnome-handover-sync-2026-09-02.mp4      the bridge's desk half
       INA  PA  roh-android-gesture.mp4                     12, and the bridge's phone half
       INV  PV  roh-android-nowplaying.mp4                  13

   `phone()` grew a take id the way `desk()` already had one. The bridge's phone
   half is the navigation take 1.2 s before shot 12's own in-point, so the slide
   hands over to a picture that then simply continues — that relationship is now
   written down in the call rather than in two numbers that happened to agree.
5. ~~Cut with `SHOWREEL_BRIDGE_SPEED` set, score, check 58.200.~~ **Done.**
   `SHOWREEL_BRIDGE_SPEED` now defaults to `8` rather than to no time-lapse at
   all: at normal speed the shot sits on "Syncing · 0 of 74 files, 0%" for its
   whole 6.8 s and the one thing it is there to show does not happen. The cut is
   `reprise-showreel-58s.mp4` and the scored film
   `reprise-showreel-58s-scored.mp4`, both **58.200000 s**, −16.1 LUFS.
6. **The phone's visualiser is not in time with the music.** This is the one
   thing left and it needs the handset — see the section below.
7. ~~Decide the Concerts shot.~~ **Decided at 1000 km and shot** — see below.

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


## The Concerts shot: decided, set, and now in the can

The shot used to show three rows under `Zurich · 500 km` with "509 concerts
hidden" under them, which reads sparse. The choice was put as 2000 km against
1000 km. **1000 km wins on two measurements, and neither is a matter of taste.**

Counting `concert_events` by great-circle distance from the stored location
(47.3744, 8.5410):

| radius | concerts in frame |
|---|---|
| 500 km | 4 |
| **1000 km** | **45** |
| 1500 km | 79 |
| 2000 km | 79 |

2000 km shows nothing that 1500 does not — beyond that there is nothing until
North America. And the app's own presets are `[100, 250, 500, 1000]`
(`crates/reprise-core/src/location.rs:23`): 2000 is a value the filter bar
cannot offer, so a film showing it would be showing something the viewer cannot
reach. 45 rows overflow the visible list either way.

`concerts.filter.radius_km` is set to `1000` in `~/.local/share/reprise/`
`reprise.db`. It was already 1000 when this session looked — the 500 km in the
existing take predates whatever changed it. The header will read
`Zürich · 1000 km` (`strings_concerts.rs:95`). The value is a string holding an
f64 in `settings(key, value)`, and it is read when the Concerts page opens
rather than at startup, so writing it with the app running is enough.

**How to shoot it:**

    systemd-run --user --unit=showreel-await --same-dir \
      --setenv=WAYLAND_DISPLAY="$WAYLAND_DISPLAY" --setenv=DISPLAY="$DISPLAY" \
      --setenv=XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" --setenv=XDG_CURRENT_DESKTOP=GNOME \
      --setenv=SHOWREEL_STATIONS=concerts --setenv=SHOWREEL_SYNC_DWELL=3 \
      scripts/showreel/await-take-gnome4.sh

then click the Reprise window and leave it in front. `SHOWREEL_STATIONS=concerts`
shoots that station alone; the handover station still runs afterwards because it
is not part of the station list, which is why the sync dwell is turned down to
3 s. Output lands in `~/.cache/reprise-showreel/roh-gnome-tour.mp4.mp4` — the
work directory, not `~/Videos`, so the good tour take is not at risk.

**The trap this cost a take to find.** `active-window.py` reports every window
AT-SPI marks active, and a browser behind the app counts. The first attempt took
the first hit, settled its six seconds, and filmed 45 seconds of a browser
playing a video; only the take's own focus guard called it (`VERDICT FAIL`,
`FAIL concerts: lost focus`). The wrapper now asks again on the far side of the
settle and keeps waiting instead of shooting when Reprise is no longer in front.
Whatever was in front at the click is not necessarily in front six seconds
later.


## The Concerts shot, as it was actually taken (third session, 22:18–22:36)

It took three runs and the first two are the lesson.

**Run one aborted before a frame.** `SHOWREEL_STATIONS=concerts` alone still
appends the device handover, because `resolve()` adds it whenever no `--limit`
is given — and with no phone attached `Open Pixel 10 Pro XL` is not in the
sidebar, so the pre-flight refused the take. Correct behaviour, and cheap: it
costs a minute, not a take. `--limit 1` is what shoots one station and no
handover.

**Run two shot 21 s of nothing happening.** The app was left on Concerts by the
previous session, so clicking Concerts turned no page: the frames at 7.7 s and
13.0 s are the same screen, `find-page-turns.py` reported a turn at 9.59 with a
peak of **0.99** — between the 0.39 of the Music false positive and the 1.2–7.7
of the genuine ones — and the ratio of 624 said nothing, because a sidebar row
lighting up is a real difference in a small region. **The ratio does not
separate a page turn from a row highlight; the peak does, and neither settles
it as well as two frames side by side.** Look at the frames.

**Run three drove Releases first, then Concerts.** `SHOWREEL_STATIONS` filters
`STATIONS` in place, so the order is the sidebar's, not the string's: `stats`
sorts after `concerts` and would have been shot second. `releases,concerts`
gives a real page to leave. The take retried Concerts once (marks at 29.02 and
40.23) — the first click landed while a playlist page was up — and it is the
**second** turn, at 43.23, that is the shot. In-point 41.33, peak 2.88, ratio
310.

What the shot now holds: `Zurich · 1000 km`, **45 of 524 concerts**, the list
full to the bottom of the window, and a status line reading "Up to date —
checked 22:20". The 500 km take it replaces had three rows, "509 concerts
hidden by the 500 km radius around Zurich", and a status line reading "Updating
concerts …". Run two, taken 20 minutes earlier, read "Update failed — showing
saved concerts from 22:20" — the same page, the same radius, a different
sentence. **A status line is part of the shot**; check it before keeping a take.

One continuity break, knowingly kept: shot 04 comes from its own take, so the
player bar under it carries a different track (*Perish feat. Christian Roche*,
playing) than the four shots around it (*Welcome to the Family*, paused), and
the info panel a different cover. Shot 01 already had this — it is the pickup
take — and at 5.4 s a shot nobody has told to look at the player bar.

## The phone's visualiser is 43.4 s out of step with the bed

Shot 13 says "The same visuals · in time with the music". **It is not**, and
this is the one open item.

The bed is the theme song and the theme song is the bed: `spliced-58s.wav` is
58.200000 s, `score.sh` lays it down from 0 with `SHOWREEL_ALIGN=0
SHOWREEL_WINDOW=0`, so film time *t* is track position *t*. Shot 13 sits at
47.4–54.6 of the film, so the handset must be at 47.4–54.6 of the same track
for its bars to be drawing what the viewer hears.

It is at 0:04, 0:08 and 0:11. Read straight off the finished frames at film
47.6, 51.0 and 54.4 — the now-playing readout, cropped and enlarged. **The
offset is 43.4 s.** The take was recorded from near the start of the track;
its whole 30.7 s only ever covers positions −10.6 to 20.1, so no in-point in it
can reach 47.4. This is not a lag to be trimmed away, it is the wrong seven
seconds of the song.

The correlation says the same thing more quietly. `measure-vis-sync.py` (new,
this session) correlates the square's lit-pixel count against the bed's own
band power — the low third against 40–160 Hz, the high third against 1–3.5 kHz
— and over ±2 s it finds *r* = 0.29 at +1.27 s in the low band and *r* = 0.42 at
+0.50 s in the high one. Two bands that disagree by three quarters of a second
at correlations that weak are two bands finding the 120 BPM grid, not each
other. **Where the readout and the correlator disagree, the readout is not the
weaker witness — it is the direct one.** The 2026-08-29 note that the clock "is
not good enough" was about a 0.6 s error; it does not license ignoring a 43 s
one.

**The fix needs the handset, and nothing else.** Re-shoot `nowplaying` with the
theme started and left to run: open the sheet and tap the square over to the
spectrum before position 45, then hold past 56. The shot wants the picture from
47.4 onward, so a take that reaches 0:56 has the whole of it with room at both
ends. Then `SHOWREEL_PHONE_VIS_IN` is the take time at which the readout shows
0:47, and `measure-vis-sync.py` on the finished cut is the proof — it should
come back inside ±0.1 s in **both** bands, agreeing with each other, not one
band at a time.

Until then the honest options are: re-shoot (best), or change the caption so
the film does not claim a sync it does not have. Shipping it as it stands
claims something a viewer with the track in their ear can catch.
