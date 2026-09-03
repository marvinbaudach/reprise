# Handover — the hand becomes visible, 2026-08-29

Continues `showreel-the-hand-becomes-visible.md`. The measurements live in
`showreel-the-hand-becomes-visible.EVIDENCE.md` (phone) and in the plan itself
(desktop). Read those two first; this file is only the state.

Worktree `~/Projects/reprise-showreel`, branch `showreel-recut-and-drivers`.
**Nothing is committed.** The same worktree still carries 39 deleted
`showroom/public/media/showroom/*.webp` belonging to somebody else — commit by
pathspec (`scripts/showreel/`, `docs/plans/`) or not at all.

## Where this stands

The desktop mechanism is finished and proven. The phone is fully measured but
not re-recorded. The tap-indicator compositor does not exist.

**A desktop tour take was rolling when this handover was written** — nine
stations plus the sync handover, ~2.5 minutes. Check it before anything else:

    grep -E '^(\[|TIMELINE|FILM|FAIL|VERDICT)' <scratch>/take.log
    ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 \
      ~/.cache/reprise-showreel/roh-gnome-tour.mp4.mp4

`cut-film.sh` exits 0 on a missing take, so `ffprobe` the duration rather than
trusting an exit code. Verify a shot by reading the finished film, first and
last frame.

## Proven, with counts

**R1 is closed.** `probe-cursor-take.py` records and drives at once; the film
was read frame by frame and the arrow is visible, moves, and sits on exactly
the row that gets selected.

**There is no pointer drift.** `calibrate-pointer.py` parks the cursor at ten
stations — five reached by a single jump, four by an ease — films them and
finds the cursor by shape. Accurate to about four pixels, over the ease exactly
as over the jump; the four are the hotspot offset. An earlier "drift" was the
measuring tool sampling one station early, because the film's zero is when the
recording starts and the driver's zero was set 2.5 s later.

**The real defect was reading coordinates too early**, and the fix is counted:

| | runs against a freshly started app | clicks right |
|---|---|---|
| before | 0 of 2 | 0 of 6 |
| after  | 4 of 4 | 12 of 12 |

Every one on the first attempt; the retry the driver carries never fired.

## What is built

New, untracked, in `scripts/showreel/`:

- `desk.py` — reaching the window and knowing it is really in front:
  `logical_screen`, `active_frame`, `selected_row`, `raise_by_search`,
  `bring_to_front`, `window_origin`, `centre_of`, plus `row_map`,
  `wait_quiescent`, `stable_rect`, `settled_centre`.
- `take-gnome4.py` — the desktop tour. Nine sidebar stations then the device
  page; `--dry` resolves every target and prints them without shooting.
- `calibrate-pointer.py` — the drift ruler described above.
- `probe-cursor-take.py` — the short joint proof (three rows, ~20 s).
- `pointer.py`, `probe-click.py`, `probe-*.py` — from 2026-08-28, unchanged.
  `probe-click.py` is left exactly as it was: it is the record of the R1
  measurement, not a library.

## Still open

1. **The tap-indicator compositor does not exist**, and its interface is
   undecided. `tap()` in `take-android2.sh` shells straight to
   `adb shell input tap` and logs nothing; only six phase marks reach the
   timeline. Two decisions come before code: a tap is an instant but an
   indicator is a window, so a duration lives either in the log or in the
   compositor; and the log's zero must be reconciled with the recording's,
   which starts later. Get either wrong and every indicator is offset by a
   constant nobody sees until the frames are read.
2. **The phone is measured but not re-shot.** Coordinates and gestures are in
   the EVIDENCE file. `take-android2.sh` still walks the old four-shot album
   detour and must be rewritten to the three-shot flow.
3. **The visualiser has lost its home.** The sync took the handover (R9), and
   the handover was the one shot with playback running that carried the
   visualiser. Decide where it goes.
4. **Recut, re-score, showroom** — plan steps 4 to 6, untouched. `arc_steps()`
   in `pick-window.py` must be re-read: the shot list changed even though the
   66.6 s length did not.

## Decisions taken this session

- **The sync is the handover**, not a tenth shot, so 66.6 s and the beat grid
  survive intact.
- **The desktop track is loaded and paused** — `Recreant`, Will Ramos, id 1911.
  Visual only; R3 stands and the playhead does not move across cuts.
- **The sync progress is filmed long and compressed in the cut.** The take
  holds 45 s on it (`SYNC_DWELL`).
- Two playlists were created through MCP, which is what the sync shot carries
  and what the agent shot (R7) claims: **`Like Lorna Shore`** (300, id 1) and
  **`Like As I Lay Dying`** (200, id 2 — the 47 real As I Lay Dying tracks plus
  153 metalcore, because the library holds only 47 by that band).

## Traps found this session

- **`org.gnome.Shell.Screenshot` is refused** on this desktop ("Screenshot is
  not allowed"). The recording is the only thing that can see the real cursor,
  so calibrating means filming.
- **A diff only says something changed.** The first cursor detector took the
  bounding box of every changed pixel and was wrong on nine stations out of
  ten: the pointer lights a hover highlight under itself and the 660x60 bar
  dominates. Find the cursor by shape — small, compact, near-white.
- **Roles are `button`, not `push button`.** The device row is a *button* named
  `Open Pixel 10 Pro XL`, and the start button is `Sync now`. Asking for the
  wrong role returns nothing and looks exactly like a missing widget. The
  `--dry` pass exists because it caught this before it cost a take.
- **Waiting for quiescence must happen before the recording starts.** It costs
  about 2 s per station; done on film it is a hole in the take.
- **The MCP server returns `internal server error` while the app is busy**
  computing a sync diff. Both times, waiting 20-30 s and repeating the same
  call worked. It is transient, and it left no partial state behind.
- **`Sync automatically when this phone connects` is on.** Reconfiguring the
  sources was enough to start a real sync with no explicit `start` — it ran the
  approved 42 copies and 237 removals on its own. Expect a configure to be
  destructive here, and check `music_get_device_sync_state` afterwards.

## Machine state left behind

- Nightly built and installed: `origin/dev` `06ae442415`, 2026-08-29 09:03.
  Reprise runs from `~/.local/bin/reprise` on that build.
- `Recreant` is loaded and paused in the app.
- Device sync mirrors three sources (Top rated, both new playlists) at
  `opus_160`. **969.6 MiB / 280 files were still pending** when the tour take
  started them; nothing destructive is left in that delta (`files_to_remove: 0`).
- The phone is connected. If `adb` cannot see it, `adb kill-server`,
  `adb start-server`, then wait a few seconds — `lsusb` showing `18d1:4ee7` is
  not enough on its own.
- A wake lock named `showreel` is held. Release it when the work stops.
