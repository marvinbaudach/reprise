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

A re-run of the missing checks followed the same afternoon; the summary table
carries both.

All evidence cited below is under `~/.local/share/reprise-device-run-20260919/`
(stated once; every filename below is relative to it). Timestamps in
`run-logcat.log` are local device time (CEST, UTC+2); the worker transcript's
own timestamps are UTC.

## Summary

| Check | Morning | Re-run | One-line reason |
|---|---|---|---|
| A1 — MP3/Opus/FLAC compute + spectrum | INCONCLUSIVE | MP3/Opus FAIL, FLAC PASS | isolated from backfill this time: MP3 24.955 s and Opus 21.124 s miss the ~5 s target, FLAC 4.832 s meets it; render confirmed for all three; an x86_64 software-codec finding, not yet measured on hardware |
| A2 — visualizer stays up / no cover fade | NOT RUN | PASS | transition pinned via `dumpsys`; four screenshots from -2.3 s to +4.7 s show the visualizer never drops to a placeholder; the log tag the plan assumed does not exist in this build |
| A3 — backfill progress / pause / battery saver | PASS (with caveats) | not re-run | a clean pause-then-resume instance exists; saver on → no backfill lines, saver off → `1/5` within 2 s |
| A4 — screen off + `am kill` + analysis lands | INCONCLUSIVE | PASS / INCONCLUSIVE | screen-off part PASS (sleep proven, 8 pending tracks computed 3.4–5.65 s after the sleep marker); activity-removal part INCONCLUSIVE — no working way on this image to remove the task without killing the process |
| A5 — desktop-synced control arm | NOT RUN | NOT RUN | still needs a desktop-synced fixture; unchanged |
| C1 — offline: placeholder, no network | PASS | not re-run | UID has no row in netstats' byte table, `grep -ci musicbrainz` = 0, placeholder screenshot confirmed |
| C2 — online: fetch within timeout | NOT RUN | download PASS, display FAIL | real cover downloaded and cached correctly, but neither now-playing nor the album header ever shows it; logcat root cause: the downloaded-cover path is read through the SAF-backed content provider, which has none for that path |
| C3 — embedded art never triggers a request | NOT RUN | PASS | cache listing and netstats byte-for-byte unchanged; control arm = C2 moved both |
| C4 — airplane mode retry | NOT RUN | PASS / INCONCLUSIVE | steps 1–2 PASS (placeholder stays, no crash, no bytes, genuine `TransientFailure`); step 3 INCONCLUSIVE (portrait retried on relaunch, album cover produced no new file in the observation window) |
| Combined — backfill running + fresh play | NOT RUN | PASS | 3.657 s from tap to `Computed`, with both the artwork and the track-analysis backfills active concurrently |

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

## Re-run of the missing checks (afternoon)

Same build (`91b2c576c5`), same release x86_64 APK — no rebuild — reinstalled
onto the same AVD `pixel10xl_api37`, rebooted `-no-snapshot` and `pm clear`ed
first. The 91 synthetic fixtures from the morning were re-seeded, plus a
purpose-built fixture per check: synthetic 40 s tones for A1/A2/A4, and, for
the covers checks (C2–C4, Combined), tags naming real albums — OK Computer,
Nevermind, Abbey Road, and others — so MusicBrainz and the Cover Art Archive
could actually resolve them, unlike the morning's all-synthetic library. Two
Sonnet workers drove the emulator 12:00–16:10 CEST; the orchestrating session
held the device/wake locks throughout. The physical phone was never touched.
`tcpdump` was not installed on this host, so pcap counts were unavailable all
afternoon; `dumpsys netstats` served as the network channel instead (see
Observations). All evidence below is under `rerun/`, relative to the
evidence directory already named above.

### A1 (re-run) — MP3 and Opus miss the target, FLAC meets it, isolated from backfill

The deliberate fix over the morning attempt: the backfill sweep over all 91
seeded tracks was drained to completion (confirmed via DB, `track_spectrograms
= 91` for `91` tracks) *before* any timed test, so each fresh fixture pushed
afterward is the only pending track when tapped — no concurrent sweep to
confound the reading.

- **MP3.** Marker `A1 mp3 tap play row` at **12:15:35.811**;
  `Track analysis backfill: 0/1` at 12:15:36.396 (0.585 s later, the fresh
  file entering the pending pool alone); `Computed analysis for track 92` at
  **12:16:00.766**. **Delta: 24.955 s** — FAILS the ~5 s target, with no
  other pending track to blame it on.
- **Opus.** Marker `A1 opus tap play row` at **12:25:27.767**;
  `Track analysis backfill: 0/1` at 12:25:28.623 (0.856 s later);
  `Computed analysis for track 93` at **12:25:48.891**. **Delta: 21.124 s** —
  also FAILS, same pattern as MP3. No `DecodeFailed` anywhere in the log:
  Opus decodes fine, just slowly.
- **FLAC.** Marker `A1 flac tap play row` at **12:39:50.374**;
  `Track analysis backfill: 0/1` at 12:39:50.990 (0.616 s later);
  `Computed analysis for track 94` at **12:39:55.206**. **Delta: 4.832 s** —
  PASSES.

All three `0/1` lines follow their tap marker by only 0.6–0.9 s, so the
deltas above are compute time, not marker/tap slack. Rendering was confirmed
for all three: `shot-20-tap-448-2246.png` (MP3-sibling track), `shot-29-a1-opus-final.png`
(Opus, full coloured seek-bar spectrum), `shot-39-a1-flac-spectrum2.png`
(FLAC, both the visualizer bars and the seek-bar spectrum rendering together
— the clearest of the three). No screenshot lands inside MP3's own 40 s play
window (`adb` round-trip latency of 10–30 s per call made a timed capture
structurally impossible); the 24.955 s figure itself comes from log
timestamps, not subject to that overhead, and the DB confirms its
spectrogram row instead.

**Caveat that belongs here, not in a footnote:** this is an x86_64 emulator
using software codecs, not the physical phone. Nobody measured MP3/Opus
compute time on real hardware today. The 21–25 s numbers are a genuine,
reproducible **emulator** finding, not yet a confirmed device defect — that
needs a hardware re-measurement before anyone treats it as a phone-side
regression.

### A2 (re-run) — visualizer never drops across the auto-advance boundary

Precondition: a purpose-built two-track album ("A2 Fresh Album", track ids
95/96, both 15 s MP3 tones) pre-analysed by playing each once, so the
auto-advance boundary itself — not first-time compute — is what gets timed.

The transition instant, read from `dumpsys`/`media_session` (the active item
id flips 0→1, position resets to 0), landed at **13:07:49.054**. Four
screenshots were captured in a single tight burst spanning that instant:
`shot-a2c-t6.png` (-2.3 s, track one still playing, visualizer animating),
`shot-a2c-t7.png` (+0.18 s, title label one frame stale but the visualizer
panel still rendering, no placeholder), `shot-a2c-t8.png` (+2.5 s, two tall
bars animating, seek bar mid-reset but no cover-art placeholder), and
`shot-a2c-t9.png` (+4.7 s, fully recovered on track two). At no captured
instant does the panel drop to a placeholder or a blank frame.

The plan's assumed `NowPlayingScene`/`storedFrameCount` log tag does not
exist in this build: `grep -c "NowPlayingScene\|storedFrameCount"
run-logcat-full.log` returns exactly 1, and that hit is `adbd`'s own startup
echo of the requested log-tag list, not an app log line. The tag that does
fire and does cover the transition is `RepriseVisualScene:
dropped_audio_frames=N`, logged roughly every 2 s — including one line at
**13:07:49.117**, 63 ms after the transition, reading `dropped_audio_frames=0`.
The visual scene kept rendering straight through the boundary.

**Verdict: PASS.** The transition instant itself is a `dumpsys`/`media_session`
reading, not a logcat one — noted so the evidence trail states its source
honestly. `shot-a2c-t7`'s stale title text is attributed to screenshot
capture latency rather than an app-side recomposition lag, per the worker's
own correction; it does not change the verdict, since the visualizer panel
is animating in all four captured frames.

### A4 (re-run) — screen-off proven clean; activity removal still not achieved

**Screen-off, playback continuing (PASS).** Two earlier attempts on fresh
pools finished their whole backfill batch before the tap-to-sleep sequence
even completed and are recorded as null results, superseded by a third,
clean run on 8 fresh tracks (ids 125–132, "A4 Sleep3"). Marker `A4 sleep3
tap play track1` at **14:58:18.947**, `A4 sleep3 home` 0.11 s later, `A4
sleep3 keycode sleep` at **14:58:19.111** (the tightest tap-to-sleep sequence
achieved this session). `dumpsys power` confirmed `mWakefulness=Asleep`
within ~2 s and stayed asleep for the rest of the window. `pidof
io.github.marvinbaudach.reprise` = **9343** before, during, and after —
unchanged throughout, so the process was never killed, only backgrounded
with the screen off. `Computed analysis for track 132` (the tapped track)
landed at 14:58:22.490, **3.4 s** after the sleep marker; the remaining 7
pending tracks (125–131) finished their backfill by 14:58:24.766, **5.65 s**
after the sleep marker (`run-logcat.log` lines 521–529, same PID throughout).
All 8 tracks confirmed analysed in the DB pull taken while the screen still
reported asleep.

**Activity removal (still INCONCLUSIVE).** Five mechanisms were tried to
remove only the activity, not the process, from a task holding the
foreground playback service: `am task remove` (no such subcommand on this
image), `cmd activity remove-task` (unknown command), `am stack remove`
(accepted, but `dumpsys activity activities | grep -c
reprise/.MainActivity` unchanged before/after — a no-op), and two
recents-overview swipe-to-dismiss gestures at different speeds (neither
registered). None of the adb-shell primitives exist on this image, and the
UI gesture did not register either — recorded as INCONCLUSIVE for this
image, not as evidence the feature is broken. A sixth, accidental event
during the swipes brought a second installed package, `org.reprise`
(`de.reprise.spike.MainActivity`), to the foreground; shortly after, the
target app showed zero activity/service records while its PID was
unchanged — two unresolved, competing explanations (the delayed `am stack
remove` finally landing, or `org.reprise` taking the audio focus/media-session
slot) are on record, neither isolated. Separately, backfill work for other
pending tracks continued to completion while the app sat backgrounded
(never resumed) in the recents overview — five backfill completions land
after the `KEYCODE_APP_SWITCH` marker with the app never brought forward
again.

**Verdict: screen-off PASS (clean); activity-removal INCONCLUSIVE.**

### C2 (re-run) — cover downloaded and cached correctly; never shown

Setup: a pre-existing fixture, `Real Album/01 - Airbag.mp3`, tagged
Radiohead / OK Computer — a real album so MusicBrainz/CAA can resolve it,
confirmed via `ffprobe` after pulling the file and pre-validated against the
canonical release from the host with `curl` before touching the device.

Toggling Settings → Online sources → "Download artwork" (confirmed on via UI
dump, `checked="false"` before, `checked="true"` after) started the
artist-photo/cover backfill sweep, which fetched the OK Computer cover as
part of its pass over the whole library rather than from a subsequent
on-demand tap — a genuine mechanism confirmed working, just not the exact
on-demand path the plan describes; noted, not a defect.

**Download and cache: PASS.** `covers/downloaded/eb5ff6672225f3df.jpg`
appeared, was pulled, and is the real OK Computer cover (`cache-radiohead-cover.jpg`).

**Display: FAIL, at both checked rungs.** Now-playing, paused, after playing
Airbag: the square shows the generated placeholder tile, not the downloaded
cover — repeated after a full `am force-stop` + relaunch with the same
result, so it is not a same-process transient. Album detail header (Artists
→ Radiohead → OK Computer): same placeholder, across three separate visits.
This is despite the download itself being correct and despite a **freshly
generated 640px/168px thumbnail existing on disk with a new hash**
(`covers/36a368f0830fcc3c-{640,168}.png`), proving the Rust-side thumbnail
render did produce the right image at least once. Only the 1092px
(now-playing) size thumbnail is stale — `covers/b7acba88eef20757-1092.png`,
mtime hours before the switch was ever turned on today, never regenerated.

**Root cause, found in logcat, not hypothesised.** The exact failure recurs
25 times between 15:20:40.845 and 16:07:15.514 — 24 hits against the
Radiohead cover's key and one against a Beatles/Abbey Road cover downloaded
later in the Combined check, confirming this is not specific to one file.
The log line, verbatim (concatenated exactly as logcat emits it, no spaces
added):

```
W/Reprise: reprise_android_ffi: no artwork: cover cache unusableerror=cover cache I/O failed: not found: No content provider: /data/user/0/io.github.marvinbaudach.reprise/cache/reprise/covers/downloaded/eb5ff6672225f3df.jpgtrack="content://com.android.externalstorage.documents/tree/primary%3AMusic%2FDevice%20Run/document/primary%3AMusic%2FDevice%20Run%2FReal%20Album%2F01%20-%20Airbag.mp3"
```

The downloaded-cover path is a plain path inside the app's own private cache
directory, not a member of any SAF-granted tree — but every lookup for it is
routed through the same SAF/`content://`-backed library source used for
folder cover images, which has no content provider registered for a path
outside the granted tree, so the read fails every time and the pipeline
falls back to the placeholder. This is a defect in the covers strand (#985),
not in the run itself: **a fix needs to read app-private cache paths with
plain file I/O, not through the document provider** — no code is proposed
here, only the requirement the fix must satisfy.

**Network evidence.** `dumpsys netstats detail` for uid 10237, post-backfill:
`10237 1556228 1299 49765 839` (rx bytes/pkts, tx bytes/pkts) — no clean
pre-switch baseline exists (the backfill started faster than expected), so
this is a post-only reading, reused as the pre-C3 baseline below. The pcap
channel was unavailable, not zero: `tcpdump` is not installed on this host,
so an earlier "0 matches" reading in the raw notes came from that missing
binary silently failing, not from absent traffic — see Observations.

### C3 (re-run) — embedded art still never triggers a request

Setup: `Embedded Art/04 - Embedded.mp3`, a solid indigo 600×600 PNG as its
embedded picture (from the fixture generator). Navigated to its album header
immediately after C2, on the identical route.

`shot-74-c3-embedded-art-header.png`: the correct solid indigo square, the
real embedded picture decoded and shown correctly — the same
`AlbumDetailHeader` composable that just showed the wrong placeholder for
C2's downloaded cover displays this one correctly, isolating C2's failure to
the downloaded-cover source rather than a general display defect.
`covers/downloaded/` listing before and after is byte-for-byte identical
(`diff cache-listing-c2-post.txt cache-listing-c3-post.txt`, empty, exit 0)
and netstats for uid 10237 is unchanged, byte-for-byte, from the C2 baseline
(`10237 1556228 1299 49765 839` in both `netstats-c2-baseline-for-c3.txt` and
`netstats-c3-post.txt`) — zero bytes moved. Code (`album_cover.rs:116-119`,
read before the run) confirms offline resolution returns before any album
key is even computed for the network path, so this album is skipped before
the network gate, not merely deprioritised after a failed lookup.

**Verdict: PASS.** Control arm: C2's Radiohead album, checked on the
identical route immediately before, moved both the cache listing and (during
the earlier backfill) netstats bytes; this album moved neither.

### C4 (re-run) — airplane mode: genuine failure and no crash; retry partially confirmed

Code check before the run (`online_sources.rs:176-181`): the network-allowed
gate reads only the settings/module flags, with no connectivity check — so
airplane mode should produce a genuine attempted-and-failed request, not a
skipped one.

1. Airplane mode on, confirmed (`ping` → "Network is unreachable"). A fresh
   tags-only real-album fixture (Nirvana / Nevermind) was auto-picked up via
   SAF folder observation, no manual rescan needed.
2. Opening the album header under airplane mode
   (`shot-77-c4-nevermind-header-airplane.png`) shows the placeholder, **no
   crash** (`AndroidRuntime` count 0 before and after, PID unchanged), zero
   new files or `.notfound2` markers, and netstats for uid 10237 exactly
   unchanged byte-for-byte. Logcat confirms a real network-shaped failure was
   hit repeatedly: `reprise_android_ffi::artist_portrait: artist portrait
   request failederror=MusicBrainz transport failed` at 15:50:38.782 and
   15:54:02.653 — a genuine `TransientFailure`, matching the code-level
   expectation above, plus the portrait backfill's own "Waiting for a
   connection" paused state.
3. Airplane mode off, confirmed (`ping` succeeds). **Retry arm A** (revisit
   the album header, no relaunch): still placeholder, cache listing
   unchanged — the paused portrait run did not self-resume from a screen
   revisit alone. **Retry arm B** (`am force-stop` + relaunch, firing
   `onCreate`'s unconditional backfill start): the Nirvana **portrait**
   retried successfully — a real photo replaced the generated tile
   (`shot-79`) and netstats moved by +221,481 rx bytes, the right order of
   magnitude for one portrait image.

**Album cover retry: INCONCLUSIVE.** Despite the portrait retry succeeding —
proving connectivity, the retry trigger, and the network path all work
end-to-end — no new file or `.notfound2` marker appeared under
`covers/downloaded/` for the Nevermind album key within the observation
window after arm B (two listings 3 s apart, identical). Per the code's own
comment, the cover pass is chained to start once the portrait run reports
complete; this either did not fire for this single album inside the
observation window, or fired and produced an outcome the session did not
catch. Recorded as INCONCLUSIVE, not PASS or FAIL: the plan's retry claim is
confirmed for the portrait half and unestablished either way for the
album-cover half.

**Verdict: steps 1–2 PASS; step 3 (album-cover retry) INCONCLUSIVE.**

### Combined (re-run) — artwork backfill running, fresh FLAC fills its seek bar within ~5 s

Setup: 10 tags-only real albums (Beatles/Abbey Road, Pink Floyd/The Wall,
Fleetwood Mac/Rumours, and seven more) plus one fresh sidecar-less FLAC
("Combined Fresh FLAC", no embedded/folder art), pushed while playback was
stopped. Rescanning via the overflow menu brought the library to 144 titles
and itself started a fresh artist-photo/cover backfill sweep
(`covers/downloaded/` grew from 21 to 32 files during the run, including a
real Beatles Abbey Road cover, `c7f899ba35d590af.jpg` — the same file whose
unreadable-cache warning supplied C2's second occurrence above). FLAC was
chosen deliberately: A1's re-run measured MP3/Opus at 20–25 s and FLAC at
4.8 s alone on this emulator, so only FLAC has headroom left to survive a
concurrent backfill without the ~5 s claim being unwinnable by construction.

Marker `COMBINED tap play flac` at **16:08:11.636**;
`Computed analysis for track 144` at **16:08:15.293** — **3.657 s**, inside
the target. A second, independent backfill (the track-analysis one, distinct
from the artwork one) was concurrently reporting progress in the same
window — `Track analysis backfill: 1/12` at 16:08:29.282 through `7/11` at
16:08:53.797 — direct confirmation that a second background pass was
actively running alongside this track's own on-demand computation.

**Verdict: PASS.**

## What is still open

**A5 — desktop-synced control arm.** Never run, morning or afternoon. It
needs a real device-sync pass, or a fixture carrying a desktop-encoded
`.reprise-analysis` sidecar (as the earlier
`now-playing-scene-verification.md` emulator run used) — the ffmpeg/`adb
push` fixtures used everywhere else in this report cannot produce one by
construction.

**A4 — activity removal.** Every adb-shell primitive for removing only the
activity from a task holding the foreground playback service either does not
exist on this emulator image (`am task remove`, `cmd activity remove-task`)
or is a documented no-op (`am stack remove`), and the UI swipe-to-dismiss
gesture did not register at two different speeds. A future attempt needs
either a different emulator image/API level where these commands exist, or a
different mechanism entirely (e.g. `am force-stop` immediately followed by
relaunch, accepting that this kills the process rather than isolating the
activity — a materially different test from what the plan asks for).

**C4 — the album-cover retry step.** The portrait half of the retry is
confirmed; the album-cover half is not. A re-run needs a longer post-relaunch
observation window (several minutes) and a check of `covers/downloaded/` for
either a new file or a new `.notfound2` marker for the Nevermind album key
before concluding either way.

**C2 — the display defect.** This is not something a further run can fix.
The root cause is confirmed in logcat: the downloaded-cover path is read
through the SAF/`content://`-backed library source, which has no provider
for a path outside its granted tree. A fix needs to read app-private cache
paths with plain file I/O instead of routing them through the document
provider, then a re-check (not another full device run) to confirm both the
now-playing and album-header rungs pick up the downloaded cover.

**C2 re-check, same day (issue #995).** Fixed by `CoverSource::CacheImage`:
a downloaded cover is read with plain file I/O regardless of the library
source. Re-checked on the same AVD state (the 144-title library, the SAF
grant and `covers/downloaded/eb5ff6672225f3df.jpg` all survived the
emulator restart) by installing the fixed release APK (0.1.152, same signing
key) over 0.1.148. Control arm first, old APK: Airbag playing, placeholder
tile at the now-playing rung, `W/Reprise: … No content provider:
…/eb5ff6672225f3df.jpg` at 17:02:15 and five more by 17:04:10. Fixed APK,
after the install marker at 17:13:13: the real OK Computer cover at the
now-playing rung (`shot-04`), in the album header and the track row
(`shot-06`), and again after `am force-stop` and a relaunch (`shot-08`; the
cover tile — crop `(270,720)–(1075,1520)` of the 1344×2992 screenshot,
downsampled to 16×16 and compared as the mean absolute per-channel
difference — measures 0.0 against `shot-04`, 441 against the placeholder in
`shot-03`). `shot-07`, taken after leaving to the artist page and coming
back, is byte-identical to `shot-06`; on a static screen that can be
genuine, but the screenshot then proves nothing about the navigation by
itself — that is evidenced only by the DEVRUN marker "C2-fix back to
artists, then reopen album" (17:15:24) and by `shot-05` (the artist page).
Zero `No content provider` / `cover cache unusable` lines after the install
marker. Fresh-fetch arm: with the cached cover and every thumbnail deleted
as root, playing Airbag re-downloaded the cover 11 s after the tap (17:17:44
tap, 17:17:55 file and 1092 px thumbnail mtime) and the next visit showed it
(`shot-12`; the same crop/downsample comparison measures 0.0 against
`shot-04`); whether the already-open now-playing view repaints within the
same visit could not be observed on a 15 s fixture. Evidence under
`~/.local/share/reprise-device-run-20260919/issue-995/`.

## Observations

No crash: `grep -c "AndroidRuntime.*E " run-logcat.log` = 0 across the whole
morning run, and the same command against the afternoon's `run-logcat.log`
also returns 0.

The log carries a recurring warning throughout, e.g.:
```
W/Reprise reprise_android_ffi::queue_snapshot_file: the Android queue
snapshot is running unlockederror=try_lock() not supported
```
This is environment-caused: the emulator's filesystem does not support
advisory locking, which the queue-snapshot and play-journal code both try
and fall back from. It says nothing about behaviour on real hardware and is
not evidence of an app defect.

**`tcpdump` was not installed on the afternoon's host.** Every pcap-based
count from the covers checks (C2–C4, Combined) is therefore recorded as
"channel unavailable," not as "zero traffic" — an earlier working note that
read a missing-binary silent failure as "0 matches" was withdrawn during the
run once the cause was found. `dumpsys netstats` carried the network-absence
evidence instead throughout the afternoon. This is a host-tooling gap, not a
finding about the app; the morning's C1 pcap reading (`strings run.pcap`,
genuinely captured, zero hits) is unaffected and stands as written above.

**A stale second package is on this AVD.** `org.reprise`
(`de.reprise.spike.MainActivity`), an old spike/prototype build, is
installed alongside the target `io.github.marvinbaudach.reprise` and was
triggered once by an imprecise recents-overview swipe during the A4 retest
(see A4's activity-removal section above). Anyone reusing this emulator
image should avoid horizontal swipes in the recents overview and use
`KEYCODE_HOME` or `am start` to return to the target app instead.
