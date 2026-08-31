# The hand becomes visible — showreel re-record, 2026-08-28

Supersedes the open list in `showreel-66s.HANDOFF.md`. The tooling notes there
and the measurements in `showreel-60s.SESSION-2026-08-28.md` still hold.

Worktree `~/Projects/reprise-showreel`, branch `showreel-recut-and-drivers`.
Nothing is committed, including three untracked scripts (`arc-gain.py`,
`settle.py`, `score-candidates.py`).

## The verdict on the current film

Watched frame by frame, `cut-B-devicesync.mp4` fails for one reason, and it is
the same reason on both platforms: **nothing in the film has a hand.** Pages
turn by themselves.

Measured, not felt:

- **No cursor exists in any frame.** `screencast.py` disables `draw-cursor` by
  default, and its own comment says why: the takes are driven through AT-SPI, so
  the pointer never moves. A parked cursor sitting in frame for 189 s looked
  worse than none, so it was switched off. The cure was worse than the disease —
  without it, every page change is a jump cut with no cause.
- **The desktop shots are frozen.** First and last frame of a 4.8 s shot are
  near-identical: 12.70 ≈ 17.25, 22.30 ≈ 26.85, 31.90 ≈ 36.45. The typing that
  `type_into()` performs at 0.14 s per character is in the take and was cut past.
- **The phone's third shot is the dead one.** 53.50 ≈ 58.05; across 4.8 s only
  the 14 px mini player changes from play to pause.
- **The seek bar teleports at every cut.** Shots are lifted from a continuously
  playing 189 s take in the order 105.4, 36.3, 41.2, 51.1, 62.8, 94.2, 137.0 s.
  The playhead therefore jumps backwards and forwards across every cut in the
  film.

## What the film becomes

Sixteen shots, 66.6 s, 111 beats at 100 BPM — **the length does not change**, so
the chosen track, the beat grid and the cut points all survive:

    intro card                3.0
    nine desktop shots  9 x 4.8 = 43.2   (was eight; YouTube joins them)
    the handover              2.4
    three phone shots   3 x 4.8 = 14.4   (was four; the album detour goes)
    end card                  3.6
                            ------
                             66.6

Phone loses one shot, desktop gains one. That is a coincidence worth naming,
because it is what keeps the music untouched — and it is also a trap: the
bridge now starts at 46.2 instead of 41.4, so `arc_steps()` in `pick-window.py`
must be re-read even though the duration is identical.

## R1 — The pointer moves, and the film sees it — SOLVED, 2026-08-28

`scripts/showreel/pointer.py` is the mechanism, proven against recorded frames
rather than argued. What follows is the map of a minefield: every route below
was tried, and each failed silently rather than loudly.

**What works.** Mutter's `org.gnome.Mutter.RemoteDesktop`, driven **relatively**:

- `CreateSession`, then `Session.Start()`. No ScreenCast session, no PipeWire.
- `NotifyPointerMotionRelative(dx, dy)` moves the real pointer, 1:1, with no
  acceleration in the path.
- Absolute positioning comes from one fixed reference: eight large deltas into a
  corner, which the compositor clamps, then dead reckoning from there.
- `NotifyPointerButton(0x110, true/false)` clicks.
- **Measured:** asked for logical `(120,181)`, the recorded frame shows the
  cursor at `(125,189)` — the difference is the cursor hotspot.

**And the film sees it.** `SHOWREEL_DRAW_CURSOR=1` on `screencast.py` puts the
real cursor in the recording; a white arrow is visible in the proof frame. That
was the whole question behind the ghost hand, and the answer is yes.

### The dead ends, so nobody walks them twice

- **ydotool cannot position absolutely here.** Its virtual device reports
  `EV=7` (SYN, KEY, REL) with an empty `abs` capability bitmask — no absolute
  axes exist. `mousemove -a` therefore feeds the values in as relative deltas,
  which walked the pointer into the bottom-right corner and pinned it there.
  Evidence: `/proc/bus/input/devices` and
  `/sys/class/input/event16/device/capabilities/abs`. Its **keyboard** half
  works and is still used for raising the window.
- **`NotifyPointerMotionAbsolute` is a trap.** It takes coordinates in a screen
  cast stream's space, and a stream with no PipeWire consumer is accepted and
  then silently ignored: the call returns cleanly, the pointer does not move.
  This produced a full round of "the click fired and the page did not turn".
  Linking is also fiddly — the ScreenCast session must be created with
  `remote-desktop-session-id`, and then only the RemoteDesktop session may be
  started ("Must be started from remote desktop session"). None of it is needed
  for the relative path.
- **The pointer cannot be read back.** Under Wayland the X root pointer only
  tracks the cursor over XWayland surfaces, so `xdotool getmouselocation` and
  cua-driver's `get_cursor_position` both report a stale position over native
  windows. There is no closed loop; the homed corner is the only reference.
- **Home to the bottom-right, never the top-left.** Hot corners are enabled, so
  homing into `(0,0)` opens the Activities overview and takes focus off the app.
  The run then aborts one step before the click and appears to blame the move.
- **AT-SPI screen coordinates are all zeroes.** Every element reports `x=0,y=0`
  with only width and height filled in — a Wayland client does not know where it
  sits. **Window-relative coordinates are correct**, so a target is window
  extents plus the window origin, and for a maximized window the origin follows
  from Mutter's logical monitor size: here `(0, 32)`, the 32 being the top bar
  (1080 logical minus the 1048-tall frame).
- **The sidebar is `list item`, not `button`.** Asking for a *button* named
  "Podcasts" returns a different widget with an all-zero rect, which is how the
  pointer reached the hot corner the first time. Measured rows, all `227x36`:
  `Music 6,93`, `Podcasts 6,131`, `YouTube 6,169`, `Radio 6,207`,
  `Queue 6,245`, `Recently played 6,359`, `Top rated 6,397`,
  `Recently added 6,435`, `Releases 6,473`, `Concerts 6,511`.
- **Alt+Tab does not reach Reprise.** It cycles the pair of windows GNOME last
  used, here the terminal and the browser. What works is the overview search:
  Escape, Super, type "reprise", Enter. **Escape first is required** — Super
  toggles, so an overview left open by an earlier run is closed by it and the
  name is typed into nothing.
- **Every Bash call raises the terminal**, exactly as `take-desk.sh` says. A
  probe run in the foreground steals its own focus and aborts. Detach it
  (`setsid nohup ... &`).
- **GNOME appends its own extension.** `screencast.py` asked for `probe.mp4`
  and GNOME wrote `probe.mp4.mp4`; the run then looked for a file that never
  existed. Read the real path from the `RECORDING` line it prints.

### The guard stays

`do_action` reached its element whatever the window stacking was. A real click
lands wherever the compositor has the pointer, so **every click is preceded by a
check that Reprise is the active frame**, and the run aborts otherwise. It fired
five times during this work and never let a click into a foreign window. The
check must walk *all* frames of the app: Reprise puts progress windows on the
bus and child 0 is not reliably the focused one.

### Verify by the app own state

The first probe checked for an "Add podcast" button this build does not have,
and reported FAIL for a click that had not happened yet — a criterion invented
rather than looked up. Use the sidebar row `SELECTED` state.

## R1 closed for real — the two halves joined, 2026-08-29

R1 was previously "solved" in two halves that had never run together: a click
that drove the app, and a still that showed the cursor could be recorded. The
joint proof now exists — `probe-cursor-take.py` records and drives at once, and
the film was read frame by frame: the arrow is visible, it moves, and at each
of the three stations it sits on exactly the row that gets selected.

### There is no pointer drift — that was a measuring error

The first joint run clicked three rows it never aimed at, and two frames read
by eye suggested the pointer was drifting. It is not. `calibrate-pointer.py`
parks the cursor at ten stations — five reached by a single jump, four by an
ease — films them, and finds the cursor by shape. **The pointer is accurate to
about four pixels, over the ease exactly as over the jump**, the four being the
hotspot offset between the arrow tip and the click point.

The apparent drift was in the measuring tool: the film's zero is when the
recording starts, the driver's zero was set 2.5 s later, and with 2.35 s
between stations every reading came back as the *previous* station's target. A
constant offset reads exactly like a drift. Two lessons worth keeping:

- **Diffing only says something changed.** The first detector took the bounding
  box of every changed pixel and was wrong on nine stations out of ten, because
  moving the pointer over the app lights a hover highlight under it and the
  660x60 bar dominates the diff. The cursor has to be found by *shape* — small,
  compact, containing near-white pixels — not by change.
- **`org.gnome.Shell.Screenshot` is refused on this desktop** ("Screenshot is
  not allowed"), so the recording is the only thing that can see the real
  cursor. Calibrating means filming.

### The real defect was reading coordinates too early

A freshly started Reprise keeps laying out for seconds; the playlist, device
and issue sections fill in and shove the sidebar rows around. A run that read
the rects once, up front, aimed at where the rows had been.

The first fix — wait until one row's rect agrees with itself twice — **did not
work, and the failing run is the evidence**: it still missed, and this time in
both directions. The list settles in steps, so a quiet moment is not the end of
the settling. `desk.wait_quiescent()` now holds until the *whole* row map is
unchanged across several consecutive reads, and each coordinate is re-read
immediately before it is used rather than once at the start.

Counted, not felt: **before the fix 0 of 2 runs against a freshly started app
passed (6 of 6 clicks wrong); after it 4 of 4 passed, 12 of 12 clicks, every
one on the first attempt** — the retry the driver now carries never fired.

**Waiting for quiescence must happen before the recording starts.** It cost
about 2 s per station in the probe and pushed the marks from 1.0/4.4/7.7 to
3.0/8.2/13.2. In a take that would film the waiting.

## R2 — YouTube joins the desktop

No re-shoot is needed for the content: the tour take marks `youtube` at 63.4 s.
But the mark is written before the click takes effect — the current podcasts
shot runs 62.8 to 67.6 and shows the Podcasts page throughout. **Sample the take
for where the page actually turns; do not trust the mark.**

## R3 — Nothing plays during the page shots

The seek jump is not a cut problem, it is a recording problem. The take is shot
with a track loaded and paused, so the player bar is identical in every shot and
the cuts stop announcing themselves.

The exception is the handover: the visualiser only moves while audio plays, and
the handover is the one shot built on it. Playback starts for that section only.

## R4 — A shot contains its own change

In-points move so that each shot holds its own transition — the pointer
arriving, the click landing, the page turning, the text being typed — instead of
the settled page afterwards. This is what 4.8 s is for.

## R5 / R6 — The phone, re-recorded

`adb` sees the device (`59100DLCQ006SB`), a Pixel 10 Pro XL, 1080x2404,
**Android 17 (SDK 37)**.

**`show_touches` is a dead end, and this was measured, not assumed.** Developer
options are on, `settings put system show_touches 1` takes (`settings get`
returns 1), and a 6 s `screenrecord` across an injected swipe and tap shows no
indicator in any frame. The reason is that `adb shell input` injects above the
driver, and the pointer overlay draws only for real touchscreen events. Going a
layer down is closed too: `/dev/input/event1` is the touchscreen
(`ABS_MT_POSITION_X/Y`, max 13439 x 29919) and `shell` is in group `input`, but
`sendevent` returns `Permission denied` — SELinux, not file mode. Root would be
needed. The setting has been put back to 0.

So the tap indicator is composited in post — and that is not guessing, because
**the take script issues every tap itself**: it already knows the exact
coordinate and the exact moment. It logs them, and the compositor draws them.

This is also the better picture. A system ripple appears in the same frame as
the reaction, which barely helps the ghost-hand problem. A drawn indicator can
arrive, hover, and press — the same easing the desktop pointer gets, so both
halves of the film move the same way.

The flow shrinks to what was asked for, three shots:

1. **filter by an artist** — the Artists tab, the search icon, "lorna" typed
2. **the artist view, and play from it** — one shot, so the tap that starts the
   music is inside the picture that shows the artist
3. **the visualisation**

`take-android2.sh` currently taps through to the newest *album* and plays from
there (`ALBUM_NEWEST` then `ALBUM_PLAY`). Playing from the artist view is a
different affordance — **confirm it exists before the shot list is fixed**. All
coordinates are pinned to this device at 1080x2404 and must be re-measured, not
adapted.

## R7 — The agent shot says what it does

The current card reads "Build me a playlist like Lorna Shore." over
"asked of an agent, over Reprise MCP". Most people who see this will not know
what MCP is, and the shot spends its only line on the acronym.

The claim is the plain one: **you can drive your music player by asking — or
stop needing the interface at all.** Wording to be picked; jargon is out.

## R8 — The music breathes out

The arc the film wants: build through the desktop, **drop at the handover** —
the one camera move in the film, at 46.2 s — then the phone rides the drop, and
the **release begins at the visualisation shot (58.2 s)** and carries the end
card out.

`SHOWREEL_ARC` and `SHOWREEL_DROP` already exist in `score.sh`, but a release
tail is a third shape, not a setting of those two — check `arc-gain.py` (created
in the same minute as the chosen track) for whether it can express one before
promising it. The release time is read from `arc_steps()`, and the shot list has
changed, so `arc_steps()` must be re-read regardless.

## R9 — The sync is the handover, 2026-08-29

Asked for: show device sync. Decided: it does not become a tenth shot, it
**becomes the handover** — the 2.4 s at 46.2 s that is already the one camera
move in the film. Thematically it is the same thing said twice; a sync *is* the
crossing from desktop to phone, and putting it there keeps the length at 66.6 s,
so the track, the beat grid and every cut point survive untouched. That was the
constraint the whole shot list was built around and it stays.

The cost is named rather than hidden: the handover currently rides on the
visualiser, and it is the one shot with playback running (R3). Moving the
visualiser out of it has to be settled before the recut — either the phone's
third shot carries it alone, or a desktop shot takes it.

**The playlist is made through MCP, in the film.** That is what makes the sync
shot worth its place: the agent shot (R7) creates a playlist by being asked,
and the sync shot carries that same playlist onto the phone. One claim, told
twice, instead of two unrelated features.

Measured preconditions, 2026-08-29, device connected:

- The sync is **blocked**: `missing_playlist:playlist:2`, a selected manual
  playlist that no longer exists. Nothing can start until it is deselected.
- With the blocker gone there would still be **nothing to copy**:
  `has_work: false`, 741 managed tracks already on the device. A shot of a
  progress bar at zero is not a shot.

So the new playlist is the work: **~50 tracks, ~200 MB at `opus_160`**, chosen
so the progress bar and the transfer rate visibly move without needing a time
lapse. This writes real files to a real phone — it is not a dry run.

## Order of work

1. Prove a real pointer drives the app: move, click, confirm the page turned.
   **Nothing else starts before this works** — it is the only unproven mechanism
   and it gates the entire desktop re-record.
2. Rewrite the desktop driver around it; shoot the tour paused, in shot order.
3. Re-record the phone with `show_touches` and the three-shot flow.
4. Recut: nine desktop, three phone, new in-points, YouTube in.
5. Re-score with the new arc; re-read `arc_steps()` first.
6. Then, and only then, the showroom: `encode-web.sh`, `FILM_SECONDS` in
   `tests/showreel-film.test.mjs`, the cue times in `showreel.vtt`, the wording
   in `ShowreelFilm.tsx`, `npm test`. Baseline is 85/85, exit 0.

## Traps carried over

- `cut-film.sh` exits 0 on a missing take, and one scratch directory is shared —
  never two runs at once, and `ffprobe` the duration instead of trusting exit 0.
- Verify a shot by reading the finished film, first and last frame. Sampling the
  source at the in-point hides the opposite errors from sampling the middle.
- `pick-window.py` cannot score exact-length material; use `score-candidates.py`.

## Where this stands, 2026-08-28

**No new footage exists yet.** The film on disk is still the old 66.6 s cut.
What was built this session is the mechanism the re-record depends on.

New and untracked: `scripts/showreel/pointer.py` (the mechanism), plus the
probes that established it — `probe-click.py`, `probe-pointer.py`,
`probe-calibrate.py`, `probe-coords.py` — and this plan. Untracked from before:
`arc-gain.py`, `settle.py`, `score-candidates.py`. **Nothing is committed**, and
the same worktree carries 39 deleted `showroom/public/media/showroom/*.webp`
belonging to somebody else. Commit by pathspec (`scripts/showreel/`,
`docs/plans/`) or not at all.

Machine state left behind:

- A **fresh nightly was built** (exit 0); the film is to be shot against it.
- Reprise playback was **paused over MPRIS** so the visualiser would hold still
  for a measurement. R3 wants it paused for the take anyway, but it is a change.
- The phone `show_touches` was set and **put back to 0**.
- The Activities overview may be open; Escape closes it.

### Next step

Re-run `probe-click.py` detached. It should raise Reprise, ease the pointer to
the Podcasts row, click, and report the selected row changing from `Music` to
`Podcasts`. That closes R1 empirically. Everything after it follows the order
above — the driver rewrite first, because both takes depend on it.
