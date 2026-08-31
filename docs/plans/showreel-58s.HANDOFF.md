# Showreel — handover, 2026-08-30

**The film is 58.2 s and neither half is shot yet.** Supersedes
`showreel-56s.HANDOFF.md` on length, shot list, music and takes. That file is
still right about the phone-alignment method, about the traps at the end of it,
and about the tooling it names.

Worktree `~/Projects/reprise-showreel`, branch `showreel-recut-and-drivers`.
**Nothing is committed**, and the tree still carries the inherited deletions the
56 s handover warned about — around forty `showroom/public/media/showroom/*.webp`
files, `ProductGallery.tsx` and three showroom tests. Nobody should commit this
tree before deciding whether those are intentional.

`~/Videos/reprise-showreel/preview-v.mp4` is the last watchable film: 51.0 s,
scored, with the old desktop half (info panel open in every shot) and the old
single phone shot. It is untouched and it is what to fall back to.

## The film now

    intro 3.0 | 5 desk shots 5.4 each -> 30.0 | handover 7.8 -> 37.8
    phone gesture 9.6 -> 47.4 | phone visuals 7.2 -> 54.6 | end card 3.6 -> 58.2

| # | picture | length | caption |
|---|---|---|---|
| 00 | title card | 3.0 | |
| 01 | desktop · Music | 5.4 | One player. Everything you listen to. |
| 02 | desktop · Podcasts | 5.4 | Podcasts / shows, episodes, where you stopped |
| 03 | desktop · Releases | 5.4 | New releases / from the artists you keep |
| 04 | desktop · Concerts | 5.4 | Concerts nearby / for the same artists |
| 05 | desktop · My Stats | 5.4 | Your listening, counted |
| 09 | device sync, then the slide to the phone | 7.8 | A second frontend / the same core, now on Android |
| 12 | phone · titles → albums → artist → play | 9.6 | Reprise on Android / the same library, in your hand |
| 13 | phone · the spectrum | 7.2 | The same visuals / in time with the music |
| 14 | end card | 3.6 | |

Every boundary is a multiple of 0.6 s. The slide to the phone is at **36.6** and
the bass lands at **36.5** — that one landing is what the whole cut hangs on.

## What the user decided today

- **Library Doctor is out.** Five desk stations, not six.
- **The five that remain hold 5.4 s**, not 4.8. Dropping a station buys time and
  the shots are what should get it.
- **The 1.8 s freed by the missing station goes into the handover, not the title
  card.** The card's animation is choreographed to its own 3.0 s and would only
  sit still for longer; the sync page is the shot the user asked to be readable
  and the only one whose subject is time passing. So `bridge()`'s desk half is
  6.8 s (was 5.0) and the bridge is 7.8 s (was 6.0).
  This is also what keeps the arithmetic: the slide starts at `dhalf - xf` into
  the bridge, so with the desk run ending at 30.0 the slide is still at 36.6.
- **The phone shows more of the app**: titles → albums → artist → play, then the
  visualiser. Cut as **two** shots — one 9.6 s continuous gesture with no cut
  inside it, then 7.2 s of the spectrum. Not four cuts: the desktop half already
  makes the navigation case, and four phone cuts is the 19 s of Android
  navigation that was thrown out on 2026-08-29.
- **`Sync now` is clicked in the handover shot** — the user wants the progress
  bar visibly travelling. Run the take with `SHOWREEL_SYNC_CLICK=1`.
- The earlier "film it without the click" decision was **reversed**; the switch
  exists either way (see Tools).

## The music

`reprise-showreel-chosen.mp3` (run 19 b, female vocal hyperpop, 120 BPM) stays
the track. Measured structure, 100 ms resolution: quiet intro to **18.0**, full
section to **48.0**, breakdown to **66.0**, second full section from **66.0**.

The bed is already spliced and does **not** need re-deriving:

    bash scripts/showreel/splice-bed.sh \
      ~/.cache/reprise-showreel/musik/reprise-showreel-chosen.mp3 \
      ~/.cache/reprise-showreel/musik/spliced-58s.wav 13.5 50.0 66.0 58.2

    splice: atempo=0.999556  A=13.5060..50.0222 (36.5162s)  B=66.0293..+21.6838s

**The A segment is identical to the 51 s bed's** (36.5162 s either way), so
every landmark the 56 s handover verified still holds: −20 dB to 4.0, lift at
4.5, breakdown at 34.5, bass at **36.5**. Only the B segment is longer. The
track is 90.0 s and the 58.2 s cut needs it to 87.7, so there is room — but not
much, and a film longer than about 60.5 s runs off the end of the track.

`arc_steps()` in `pick-window.py` has been rewritten for this shape:

    (0.0, 0.35) (3.0, 0.72) (8.4, 1.0) (24.6, 0.85)
    (30.0, 0.32) (36.6, 1.0) (54.6, 0.95) (58.2, 0.22)

The breakpoint after the dip is 36.6, and `score.sh` reads that as the drop's
release, so it is also where the filter opens again. It must stay on the slide.

## Rebuild

    SHOWREEL_SYNC_CLICK=1 python3 scripts/showreel/take-gnome4.py
    python3 scripts/showreel/find-page-turns.py TAKE.mp4 TIMELINE.tsv
    # in-point = page turn − 1.9 s, per station, into SHOWREEL_IN_*

    SHOWREEL_IN_HOOK=… SHOWREEL_IN_PODCASTS=… SHOWREEL_IN_RELEASES=… \
    SHOWREEL_IN_CONCERTS=… SHOWREEL_IN_STATS=… SHOWREEL_BRIDGE_IN=… \
      bash scripts/showreel/cut-film.sh ~/Videos/reprise-showreel/reprise-showreel-58s.mp4

    SCRATCH=$HOME/.cache/reprise-showreel/score SHOWREEL_BPM=120 SHOWREEL_ARC=0.6 \
      SHOWREEL_DROP=0 SHOWREEL_ALIGN=0 SHOWREEL_WINDOW=0 \
      bash scripts/showreel/score.sh ~/.cache/reprise-showreel/musik/spliced-58s.wav \
      ~/Videos/reprise-showreel/reprise-showreel-58s.mp4 OUT.mp4

## The info panel is the blocker, and it must be closed by hand

Three takes were shot on the evening of 2026-08-29 and **all three have the info
panel standing open beside every shot**, which is the one thing the reshoot
exists to fix. Every remote route was tried and measured:

- **`ui.info_panel_visible = 0` in the DB before launch does nothing.** The app
  comes up with the panel open anyway and writes the key back to `1` on exit.
  (`crates/reprise-core/src/library/settings.rs:241`, read at
  `crates/reprise-gnome/src/ui/now_playing/now_playing.rs:368`.)
- **AT-SPI `do_action('click')` returns True and moves nothing.** Same reason
  the tour was rebuilt around a real pointer in the first place.
- **A real pointer click on the toggle's own centre does nothing either.** There
  is exactly one `Toggle info panel` in the tree, a toggle button at window
  `(1679, 0, 36×42)`, so screen `(1697, 53)` — which is where the click landed,
  on two takes, one starting with the panel open and one with it closed by hand.
  Neither moved. `PANEL_OFF` is therefore **off by default** in `take-gnome4.py`:
  a click that did work would re-open a panel someone had just closed.

So: **the user closes it by hand, and `panel-state.py` confirms it before a take
is spent.** Do not read the toggle's `CHECKED` state — it is never set, and
reading it as "closed" already cost one take.

## Two new tools, both with control arms

- **`scripts/showreel/find-page-turns.py TAKE TIMELINE`** — where each page
  actually turns. The take writes its mark *before* the pointer moves, so a mark
  is not an in-point. Frame-differences a window around each mark; a page turn
  repaints the content area and stands far above the pointer's own few pixels.
  Control arm, the 2026-08-29 11:11 take: every station's turn lands 1.90–2.60 s
  after its mark, background exactly 0.00, peak-to-background 150–1750×. Against
  that take's own six known-good in-points the lead is **1.55–1.98 s, median
  1.90** — so **in-point = page turn − 1.9 s**. Do not carry the old `+0.3` s
  over; it was a mark plus a constant from a take script that wrote its mark at
  a different moment. A profile whose peak is not 4× its background prints WEAK
  rather than an answer.
- **`scripts/showreel/panel-state.py`** — OPEN or CLOSED, from the widgets in
  the window's right-hand column (x ≥ 1450), not from the lying toggle. Two
  seconds, no screencast, no flash. Open reads 29 widgets; the threshold is 5.
  Reprise must be on the accessibility bus, which it only is while running.

`scripts/showreel/panel-off.py` also exists — it drives the toggle and can film
a proof — but the toggle does not respond, so it is evidence, not a fix.

## take-gnome4.py switches

| env | default | meaning |
|---|---|---|
| `SHOWREEL_SYNC_CLICK` | `0` | click `Sync now` on the device page. The user wants **1**. |
| `SHOWREEL_PANEL_OFF` | `0` | click the info-panel toggle first. Leave at 0 — it does not work. |

`STATIONS` still holds eight rows (Music, Podcasts, YouTube, Radio, Releases,
Concerts, My Stats, Library Doctor). The film uses five of them; the spare
material costs about 26 s of a 480 s screencast budget and is worth keeping.

## Takes

| file | state |
|---|---|
| `~/.cache/reprise-showreel/roh-gnome-tour-panelopen.mp4` | 22:26, PASS, panel open, `Sync now` clicked unintentionally |
| `…/roh-gnome-tour-take2-panelopen.mp4` | 22:41, PASS, panel open, no sync click |
| `…/roh-gnome-tour.mp4.mp4` | 23:0x, **FAIL** — Podcasts did not select after two tries, YouTube needed two; panel open |
| `~/Videos/reprise-showreel/roh-gnome-tour-2026-08-29-1111.mp4` | the take `preview-v` is cut from; panel open, in use |
| `~/Videos/reprise-showreel/roh-android-bed.mp4` | the finished phone take — **superseded**, it cannot show navigation |

The 68.9 s phone take was measured into sync at enormous cost (two-band spectrum
correlation, three renders, in-point 46.4, bridge half 45.2) and that work is now
**spent**: the new phone half needs navigation, and the app draws cover and
spectrum into the same square and swaps them on a tap, so a take recorded with
the spectrum already chosen cannot deliver titles → albums → artist. The method
survives; the numbers do not.

## What is left

1. **Shoot the desktop tour.** Preconditions, all four, and the first two are
   the user's hands:
   - info panel closed — confirm with `panel-state.py`, never with the toggle;
   - a sync source selected that has not been transferred, so `Sync now` exists
     and the bar travels through the handover's 6.8 s;
   - Reprise in front, nothing over it, playback loaded and **paused**;
   - nobody touching mouse or keyboard for ~2.5 minutes.
2. **Measure the desk in-points** with `find-page-turns.py`, turn − 1.9 s, and
   the bridge in-point from the `sync` mark the same way.
3. **Reshoot the phone**, driven: titles → albums → artist → play in one
   continuous gesture, then the square tapped over to the spectrum on camera.
   It must be recorded **while the handset plays `spliced-58s.wav`**, or the
   bars will not sit on the beat.
4. **Measure both phone in-points on the finished cut**, by the two-band method
   in the 56 s handover — the on-screen clock is not good enough, it once put
   the visualiser 0.6 s ahead and gave no hint of it.
5. Cut, score, check the duration is 58.200.

## Traps, on top of the ones in the 56 s and 66 s handovers

- **A dead DB key and a lying toggle.** Written above; the short form is that
  the panel has no remote control and its own `CHECKED` state is never set.
- **`can_start: false` from the MCP device-sync state is not a promise that the
  button is absent.** One take found `Sync now`, clicked it, and started a real
  transfer to the user's phone after the state had reported there was nothing to
  start. If a click must not happen, take the click out of the script.
- **The GNOME Screenshot D-Bus interface refuses here** —
  `org.freedesktop.DBus.Error.AccessDenied: Screenshot is not allowed`. A
  screencast works, which is why proofs used to cost a 4 s recording and a
  flash. Prefer an accessibility-tree oracle like `panel-state.py` when the
  question is small enough to answer that way.
- **The scratchpad under `/tmp/claude-*` can vanish mid-session.** Two scripts
  written there were gone an hour later. Tools worth a second run belong in
  `scripts/showreel/`.
- **`pkill -f <script name>` kills the calling shell** — cost exit 144 again,
  exactly as the 56 s handover said it would.
- **Asking someone to click the window right before a take costs stations.** The
  third take retried Podcasts and YouTube, and the run ended FAIL; both are the
  documented signature of focus being stolen. If a human has to click Reprise
  into front, leave several seconds between the click and the take's first move.
- **MPRIS has no `Raise` on this app.** `org.mpris.MediaPlayer2.Raise` →
  `Unknown method`. The only routes to the front are GNOME's overview search
  (`desk.raise_by_search()`) and a person.
- The playlist that disappeared on 2026-08-29 at 22:28 — `Like Bring Me The
  Horizon · 10` — **was the user's own doing**, confirmed. Not a take artefact.

## Builds under the reshoot

- **Desktop**: `~/.local/bin/reprise`, 30 Aug 21:14. Carries the fix to the
  device-sync error message the user made tonight.
- **Phone**: `io.github.marvinbaudach.reprise` **0.1.71 release**, built from
  `/home/marvin/Projects/reprise/android` (`./gradlew assembleRelease`) and
  installed 30 Aug. Release rather than debug because the film shows the shimmer
  and the visualiser and the debug build's frame times are visible in them.
  The phone had the **debug** build (0.1.70) until then, so the swap needed an
  `adb uninstall` first — release and debug are signed with different keys and
  `adb install -r` fails on the signature. That wiped
  `/data/user/0/io.github.marvinbaudach.reprise`; the synced music under
  `/Music/Reprise` is shared storage and survived, but the app re-indexes it.
  **Check the library is back and a track plays before spending a phone take.**

## Amendment, 2026-08-31 — two claims above are wrong, and both cost takes

**The info panel does have a remote control: the DB key, written while the app
is not running.** The handover says setting `ui.info_panel_visible = 0` "does
nothing". It does — `get_info_panel_visible_in` defaults to `false` and the
value is read into `ToggleButton::active(visible)` at construction
(`crates/reprise-core/src/library/settings.rs:442`,
`crates/reprise-gnome/src/ui/now_playing/now_playing.rs:368`). What defeats it
is the order: the app writes the key back from its own state on exit, so a write
made while the app runs is overwritten seconds later. The sequence that works:

    quit the app  ->  sqlite3 … "update settings set value='0'
                        where key='ui.info_panel_visible'"  ->  launch

Verified both directions on 2026-08-31: after the launch the panel was closed,
and the app's own clean exit then saved the key as `0` again.

**`panel-state.py` returned a false OPEN and has been rewritten.** Counting
every widget right of x=1450 only has a control arm for the open case. With the
panel closed the track list widens to the window's right edge and its columns
fall into the count: the old rule read 103 widgets on a demonstrably closed
panel. The reading is now the panel's own cover — a widget at least 100x100 at
x >= 1400. Arms: open, several such widgets at x=1474 including the 170x170
cover; closed, exactly zero.

**Anything the take needs must run in its own systemd user unit.** A long
process started from an agent's shell — even `setsid nohup … &` — is reaped
about 60-90 s after the spawning call returns. Measured three times on
2026-08-31: two `reprise` launches died at 90 s and 60 s with a clean session
save and no one touching them, and the take of 2026-08-30 22:12 died after 11 s
and produced nothing. This is the same reason `wake-lock` lives in a unit.

    systemd-run --user --unit=reprise-showreel --same-dir \
      --setenv=WAYLAND_DISPLAY="$WAYLAND_DISPLAY" --setenv=DISPLAY="$DISPLAY" \
      --setenv=XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
      --setenv=XDG_CURRENT_DESKTOP=GNOME ~/.local/bin/reprise

The same applies to `take-gnome4.py`: a 2.5 minute take started the ordinary way
does not survive its own first minute. **A take log must not live under
`/tmp/claude-*`** either — that directory vanished with the failed take and took
the only evidence of why it failed with it.

**The phone's USB mode is `setScreenUnlockedFunctions`, not `setFunctions`.**
`svc usb setFunctions mtp,adb` is accepted (`setCurrentFunctions opId:1`) and
changes nothing; the descriptor stays `18d1:4ee7 (charging + debug)`.
`svc usb setScreenUnlockedFunctions mtp` re-enumerates the device as
`18d1:4ee2 (MTP + debug)`, gvfs mounts it, adb survives, and Reprise goes to
`connected: true` with a real diff. The screen must be unlocked. Note the device
dropped off the bus entirely a few minutes later — check `lsusb` before
believing a sync precondition.
