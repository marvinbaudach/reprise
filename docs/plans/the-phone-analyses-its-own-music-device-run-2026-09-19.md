# The phone analyses its own music — device run, 2026-09-19

Build under test: `91b2c576c5` (dev), release x86_64 APK built in the worktree
`/home/marvin/Projects/reprise-device-run`, installed on emulator
`pixel10xl_api37` (API 37, x86_64, started `-no-snapshot`). This is
cross-check 3 of `docs/plans/the-phone-analyses-its-own-music.md`'s "Verification
— the device run" section (analysis checks A1–A5, covers checks C1–C4, and the
Combined post-merge check).

The library was **synthetic ffmpeg-generated fixtures** (`gen-fixtures.sh`),
seeded under `/sdcard/Music/Device Run/…` on the emulator, not the physical
phone. The user decided against the physical device for this run because its
real library (761 tracks, 762 sidecars, no FLAC file at all) can only be
seeded one-way — pushing a throwaway synthetic fixture there is not reversible
the way it is on an emulator, and FLAC coverage requires a file the phone
does not otherwise have.

**This run is partial.** The worker session driving it was cleared at 11:23
CEST mid-task; the emulator process was gone by 11:28:45. Started `-no-snapshot`,
so the seeded library and app state could not be recovered afterward. Of the
ten checks, two reached a defensible verdict (A3, C1), two ran but do not
support the claim they were run for (A1, A4), and six never started (A2, A5,
C2, C3, C4, Combined).

All evidence cited below is under `~/.local/share/reprise-device-run-20260919/`
(stated once; every filename below is relative to it). Timestamps in
`run-logcat.log` are local device time (CEST, UTC+2); the worker transcript's
own timestamps are UTC.

## Summary

| Check | Verdict | One-line reason |
|---|---|---|
| A1 — MP3/Opus/FLAC compute + spectrum | INCONCLUSIVE | MP3 slower than spec and confounded with backfill; Opus's compute predates its play by ~4 min; FLAC never played |
| A2 — visualizer stays up / no cover fade | NOT RUN | no DEVRUN marker, no text, no screenshot anywhere in the transcript |
| A3 — backfill progress / pause / battery saver | PASS (with caveats) | a clean pause-then-resume instance exists; an earlier attempt looks like it did not stop; saver on → no backfill lines, saver off → `1/5` within 2 s |
| A4 — screen off + `am kill` + analysis lands | INCONCLUSIVE | `am kill` was a no-op (PID unchanged); the one `Computed` line after backgrounding lands 1.3 s after a tap made in the same foreground command, not demonstrably after the activity was gone |
| A5 — desktop-synced control arm | NOT RUN | no desktop-synced file, no sidecar, no comparison exists anywhere in the transcript |
| C1 — offline: placeholder, no network | PASS | UID has no row in netstats' byte table, `grep -ci musicbrainz` = 0, placeholder screenshot confirmed |
| C2 — online: fetch within timeout | NOT RUN | worker never confirmed the Online sources toggle switched on; last screenshot before session death shows the wrong settings page (Audio, not Online sources) |
| C3 — embedded art never triggers a request | NOT RUN | strand not reached |
| C4 — airplane mode retry | NOT RUN | strand not reached |
| Combined — backfill running + fresh play | NOT RUN | strand not reached |

## A1 — MP3, Opus, FLAC compute and seek-bar spectrum

The plan asks for three formats each producing a `Computed` outcome within
~5 s of play, with the seek bar showing the spectrum. The entire run logged
exactly five `Computed analysis for track N` lines total
(`run-logcat.log`); the DB pull (`reprise.db`, pulled and queried mid-run)
maps track IDs to files:

| Track | File | `Computed` logged | Play tap logged | Verdict |
|---|---|---|---|---|
| 35 | MP3 Tone | 10:27:49.263 | `A1 mp3 play start` 10:27:37.361 | logged 11.9 s after the tap, not ~5 s, while `Track analysis backfill: 1/35…5/35` was running concurrently against the whole freshly-scanned library — the compute cannot be attributed to the play action alone |
| 34 | Opus Tone | 10:28:37.180 | `A1 opus play start` 10:32:25.613 | the `Computed` line is **~3.8 minutes earlier** than the play tap; the file's spectrogram existed via the backfill sweep well before the deliberate on-demand test happened. No `Computed` line appears anywhere near 10:32:25 |
| 33 | FLAC Tone | none | none | never tapped, never opened in now-playing; no DEVRUN marker, no screenshot. Its `track_spectrograms` row (confirmed present, `36` rows total for `36` tracks) came from the same backfill sweep, not an on-demand play |

The worker also attempted a fourth, "never-backfilled" MP3 (`Fresh Tone`) to
get a play-triggered compute clear of the backfill confound
(`A1 fresh-tone play start`, 10:34:16.416). The resulting screenshot,
`shot-19-fresh-nowplaying.png`, shows **"Embedded Tone"** playing, not "Fresh
Tone" — the tap landed on the wrong list row. No new `Computed` line appears
after that tap either.

The worker's own conclusion at this point ("DB confirms all 36 tracks …
have computed spectrogram rows") is a row-count check, not per-format
causal evidence — and the mother plan's A1 asks specifically about the
play-then-compute path. `shot-40-c1-nowplaying2.png`, taken later during the
C1 pass, does show the MP3 Tone placeholder tile with a rendered seek-bar
spectrum at 0:00 — real corroboration that the rendering path works, but it
is a re-visit of an already-analysed file, not a fresh 5-second compute.

**Verdict: INCONCLUSIVE.** MP3 rendering works but took over twice the
target window under concurrent backfill load; Opus's evidence predates the
deliberate test and cannot be attributed to it; FLAC was never exercised at
all. A resume needs a library where the file under test is *not* already
covered by an in-flight backfill sweep, and a verified tap (list-position or
`content-desc` check) before trusting a screenshot's filename.

## A2 — visualizer panels stay up, no cover fade on auto-advance

No evidence exists anywhere in the transcript or in `run-logcat.log`: no
`DEVRUN` marker (the worker used a distinct marker for every other check —
`A1 …`, `A3 …`, `A4 …`, `C1 …` — and none reads `A2`), no assistant text
discussing auto-advance, cover fade, or the visualizer, and no screenshot
filename referencing it. The strand jumped from A1 straight to A3.

**Verdict: NOT RUN.**

## A3 — backfill progress, pause, battery saver

**Progress and pause.** The first pause attempt is genuinely ambiguous. After
`A3 pause definitive mid-flight` (10:43:18.922) and one trailing `3/8`
(10:43:19.126, the in-flight item completing), a **new** batch starts and
runs to completion regardless: `1/5` at 10:43:24.578 through `5/5` at
10:43:46.867. That instance does not demonstrate a stop.

A later, cleaner attempt does: `A3 huge-batch start` / `A3 huge-batch
pause-midflight` land within 0.8 s of each other (10:47:27.862 /
10:47:28.655), `Track analysis backfill: 0/20` is logged at 10:47:28.775 and
then **holds for ~55 seconds** — confirmed paused via a UI dump in that
window (`ui4.xml`, `content-desc="Play"`, meaning the transport was showing
"resume", i.e. actually paused). The worker then taps play at 10:48:22.734,
and progress resumes immediately: `1/20` at 10:48:24.146 through `20/20` at
10:48:29.670. This instance supports "pause stops the backfill, resume
continues it" cleanly.

**Battery saver.** `dumpsys power` confirms saver state directly:
`Battery Saver is currently: ON`, `Enabled=true full=true` (after `settings
put global low_power 1` — a first attempt with `low_power 0`→`1` in the same
command, then `am broadcast POWER_SAVE_MODE_CHANGED`, then re-verified).
After a force-stop/relaunch and a play tap confirmed via UI dump
(`content-desc="Pause"`, i.e. actually playing) with five pushed "Saver
Batch" tracks pending, no `backfill:` log line appears at all — the worker's
own text notes this explicitly ("No backfill lines despite tapping play with
battery saver enabled and 5 pending tracks").

The control arm for the saver step is in the log as well, although the
worker never pointed at it: saver was switched off again at 10:52:15
(`settings put global low_power 0` + `dumpsys battery reset`, transcript
08:52:15Z), and `Track analysis backfill: 1/5` follows at 10:52:17.469, running
to `5/5` by 10:52:18.910 (`run-logcat.log` lines 344–349, same app PID 6578,
no play tap in between). So the backfill was held back by saver alone, not by
a missing trigger, and the `/5` batch size independently confirms that
exactly the five pushed "Saver Batch" tracks were pending while saver was on.

**Verdict: PASS**, on the strength of the huge-batch pause/resume instance
and the saver on → no lines / saver off → `1/5` within 2 s pair, with the
earlier "3/8 → new batch continues" attempt flagged as a caveat rather than
folded into the pass silently.

## A4 — screen off, activity killed, playback continuing

The first attempt used `am kill` after `KEYCODE_SLEEP`:
```
PID_BEFORE=6578
PID_AFTER=6578
```
The process was never killed — expected Android behaviour for a process
holding an active foreground (playback) service, but it means "activity
killed" was never actually achieved in this run.

The worker then substituted HOME-backgrounding (`KEYCODE_HOME` +
`KEYCODE_SLEEP`, logged as `A4 activity backgrounded via HOME, screen off,
waiting for track advance`, 10:54:20.763) and waited for the track to
auto-advance. No `Computed` line appears anywhere near or after that marker.

A second attempt (`A4 tap fresh-target then background immediately`,
10:56:47.958) does show a `Computed analysis for track 89` at 10:56:49.287 —
1.3 s later. But the tap, `KEYCODE_HOME`, and `KEYCODE_SLEEP` were fired in
the same command block with no gap between the tap and backgrounding, so the
compute for track 89 (confirmed as "A4 Batch 2" in the DB) is plausibly
initiated while the activity was still in the foreground, not after it left.

**Verdict: INCONCLUSIVE.** Screen-off and backgrounding-without-kill were
exercised; the specific claim ("activity killed, next track's analysis still
lands") was never demonstrated — `am kill` was a no-op against the
foreground service, and the one `Computed` line that followed backgrounding
cannot be pinned to after-backgrounding rather than during-the-tap.

## A5 — desktop-synced control arm

The window between A4 finishing (DB confirmation at 08:57:06Z / 10:57:06
CEST) and the worker's own claim "Analysis strand (A1–A5) is now solidly
evidenced" (08:57:29Z, 23 seconds later) contains only a `ResumedActivity`
check and the DB read that returned `89|A4 Batch 2` — both A4 bookkeeping.
No desktop-synced file, no `.reprise-analysis` sidecar, no second spectrum
render, and no comparison of any kind appears anywhere in the transcript,
before or after that point. The claim is narration with no artifact behind
it.

By construction this cross-check could not have produced a desktop-synced
arm: the library on this emulator was ffmpeg-generated fixtures pushed
directly to `/sdcard` over `adb push`, never synced from the desktop
database. A5 needs a real device-sync pass (or a fixture carrying a
desktop-encoded `.reprise-analysis` sidecar, as the earlier
`now-playing-scene-verification.md` emulator run used) to exist at all.

**Verdict: NOT RUN.**

## C1 — offline: placeholder shown, no network request

`shot-40-c1-nowplaying2.png` shows the MP3 Tone now-playing screen with the
generated placeholder note tile (album without embedded art) and a rendered
seek-bar spectrum, with Online sources confirmed Off beforehand (worker text:
`"Online sources: Off" confirmed`, 08:58:45Z).

`grep -ci musicbrainz run-logcat.log` = **0** (verified independently).
`strings run.pcap | grep -ci -e musicbrainz -e coverartarchive` = **0**
(verified independently; the capture runs to 11:27, well past this check).

`netstats-c1.txt` is a single snapshot (captured 09:08Z), not a before/after
pair — there was no earlier snapshot to diff against. The app's UID was
identified via `pm list packages -U io.github.marvinbaudach.reprise` →
`uid:10237`. In the snapshot, `uid=10237` appears only in the `set=1` state
list; it has **no row at all** in `mAppUidStatsMap` (the byte-counter table),
meaning zero bytes were ever recorded for it — the evidence is an absence,
not a measured delta.

**Verdict: PASS**, with the netstats caveat stated above.

## What did not run and why

**A2, A5** never started (see their sections above).

**C2, C3, C4, and the Combined post-merge check never started.** After C1,
the worker began toggling Online sources on (shot-41 through shot-45) but
never confirmed the toggle actually switched: `shot-42-online-sources.png` —
despite the filename — shows the **Audio** settings page (Gapless playback,
Equalizer), not Online sources. The following screenshots (`shot-43-nav.png`,
`shot-44-settings-root.png`, `shot-45-settings-root2.png`) show the worker
lost in back-navigation, retrying `KEYCODE_BACK` and re-tapping menu entries.
The transcript ends at 09:23:09Z (11:23:09 CEST) with no further action —
the session was cleared mid-navigation, before C2's precondition (Online
sources actually on) was ever established. The emulator process was gone by
11:28:45 CEST; started `-no-snapshot`, so the seeded library, the app's
installed state, and the in-progress settings navigation are unrecoverable.

**To resume:** the release APK is still built at
`/home/marvin/Projects/reprise-device-run/android/app/build/outputs/apk/release/app-release.apk`
(from `91b2c576c5`); the fixture sources used to build the synthetic library
are under `lib/` in this evidence directory and can be re-pushed with
`gen-fixtures.sh` (paths inside the script point at the original worker's
scratch directory and need adjusting). A resume needs: reseed the library,
reinstall the APK, confirm the Online sources toggle actually switches (by
UI dump, not by screenshot filename) before starting C2, then run C2 through
C4 and the Combined check per the mother plan. A1, A4, and A5 are also worth
re-running cleanly given the findings above — particularly isolating the
file under test from a concurrent backfill sweep for A1, and building a
desktop-synced fixture (or reusing the sidecar approach from
`now-playing-scene-verification.md`) for A5.

## Observations

No crash: `grep -c "AndroidRuntime.*E " run-logcat.log` = 0 across the whole
run.

The log carries a recurring warning throughout, e.g.:
```
W/Reprise reprise_android_ffi::queue_snapshot_file: the Android queue
snapshot is running unlockederror=try_lock() not supported
```
This is environment-caused: the emulator's filesystem does not support
advisory locking, which the queue-snapshot and play-journal code both try
and fall back from. It says nothing about behaviour on real hardware and is
not evidence of an app defect.
