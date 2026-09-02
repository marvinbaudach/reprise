# Showreel — handover, 2026-08-29 evening

**Length is 51.0 s, not 55.8.** The agent/MCP shot was dropped after this file
was first written; every number below marked (51 s) is the current one.

Supersedes `showreel-66s.HANDOFF.md` on length, shot list, music and takes. That
file is still right about the tooling and about the traps at the end of it.

Worktree `~/Projects/reprise-showreel`, branch `showreel-recut-and-drivers`.
**Nothing is committed.**

The tree also carries changes this session never made and cannot account for:
`align-bed.py`, `bed.py`, `mcp-playlist.py` and
`showreel-60s.SESSION-2026-08-28.md` are modified, and
`showroom/public/media/showroom/android-cover-360.webp` and `-540.webp` are
**deleted**. Inherited state from an earlier session. Nobody should commit this
tree before deciding whether those two deletions are intentional — if they are
not, they take a showroom page's images with them.

## The film now

`~/Videos/reprise-showreel/preview-v.mp4` — **51.000 s**, scored, watchable,
and the phone half is finished: measured in sync, on the real take, against the
film's own bed. The desktop half still comes from the morning take with the info
panel open in every shot, which is what the reshoot is for. `preview-t` (47.0)
and `preview-u` (47.6) are the two mis-timed cuts, kept only as the evidence for
the table below; `preview-r.mp4` is the 55.8 s version with the agent shot.

    intro 3.0 | 6 desk shots 4.8 each -> 31.8 | handover 6.0 -> 37.8
    phone 9.6 -> 47.4 | end card 3.6 -> 51.0

The slide to the phone is at **36.6** and the bass lands at **36.5**.

Every boundary is a multiple of 0.6 s. Rebuild:

    bash scripts/showreel/cut-film.sh ~/Videos/reprise-showreel/reprise-showreel-51s.mp4
    SCRATCH=$HOME/.cache/reprise-showreel/score SHOWREEL_BPM=120 SHOWREEL_ARC=0.6 \
      SHOWREEL_DROP=0 SHOWREEL_ALIGN=0 SHOWREEL_WINDOW=0 \
      bash scripts/showreel/score.sh ~/.cache/reprise-showreel/musik/spliced-51s.wav \
      ~/Videos/reprise-showreel/reprise-showreel-51s.mp4 OUT.mp4

## What the user decided today

- **Shorter than a minute.** 66.6 -> 55.8 -> 51.0, because shots left, not
  because anything was tightened.
- **Sidebar order, top to bottom**: Music, Podcasts, Releases, Concerts, My
  Stats, Library Doctor. The old order jumped back up the sidebar twice.
- **The search shot is out** — it was the Music page a second time.
- **One phone shot, not four.** What the user cut was Android *navigation* —
  search and artist — leaving "die Coveransicht wo wir nen Song abspielen und
  dann die Visualisierung". That reads as two beats, and it was cut as two 4.8 s
  shots, but the app cannot deliver two pictures: `NowPlayingSheet`'s `onTap`
  swaps cover and spectrum inside the same square, and this take was recorded
  with the spectrum already chosen. Two shots would have been a cut where
  nothing changes. So it is one 9.6 s shot, and the caption band changes under
  it — `film2_callout_pair`, which also re-wipes the dash when the second
  caption lands. Both clauses hold in the one picture: the phone plays the
  library, and the bars are already moving to the beat.
  If the two pictures are ever wanted for real, the take has to be reshot with
  the cover chosen and the square tapped on camera — a new capture, and blind
  taps have gone wrong here before.
- **The sync page must be readable.** Its half of the handover holds 5.0 s
  instead of 1.3, and carries the announcement named below.
- **No drop filter.** `SHOWREEL_DROP=0`.
- **The Queue is not shown**, as a page or in the info panel.
- **The info panel must be closed** in the desktop take — no Recreant beside
  every shot.
- **The agent/MCP shot is dropped** ("das mit MCP wirkt nicht — lass es weg,
  einfach nur die sync seite zeigen"). Film is 51.0 s. `mcp()` stays in
  `cut-film.sh`; putting the shot back is one line.
- **The handover announces a second frontend**: `A second frontend / the same
  core, now on Android`.
- **The phone arrives with the beats.** Read as the phone landing on the bass,
  which the slide already does — the slide was kept because it was the one thing
  the user had called out as working. If "einblenden" meant a literal fade
  instead of the slideleft, that is a one-word change in `bridge()`.

## The music, and why it is spliced

`reprise-showreel-chosen.mp3` (run 19 b, female vocal hyperpop, 120 BPM) stays
the track. Measured structure, 100 ms resolution: quiet intro to **18.0**, full
section to **48.0**, breakdown to **66.0**, second full section from **66.0**.

The film wants a quiet opening *and* the bass on the slide to the phone. The two
lifts are 48 s apart in the track and 34 s apart in the film, so no window gives
both — that is why "starts mid-track" and "the handover fits the music" kept
trading places. So the breakdown is shortened:

    scripts/showreel/splice-bed.sh CHOSEN.mp3 spliced-51s.wav 13.5 50.0 66.0 51.0

Window 13.5, out at 50.0, in at 66.0 — 16.0 s removed, bar line to bar line.
Verified on the spliced file: −20 dB to 4.0, lift at **4.5**, breakdown at
**34.5**, bass at **36.5**. The slide to the phone is at **36.6**.

At 51.0 s the window is no longer free: the breakdown has to start before the
bass in film time, which forces the window past 11.5 and shortens the quiet
opening from 7.5 s to 4.5 s. That is the price of the shorter film, not a
setting anyone chose.

`arc_steps()` in `pick-window.py` is the 51.0 s shape and must be re-read
whenever the shot list changes. Its breakpoint after the dip is also the drop's
release time, which is why the handover cannot be moved casually.

## Takes

| file | state |
|---|---|
| `roh-gnome-tour.mp4` | 2026-08-29 11:11, **in use**, info panel open in every shot |
| `roh-gnome-tour-2026-08-29b-truncated.mp4` | evening reshoot: panel open **and** 60 s short |
| `roh-android-bed.mp4` | 2026-08-29 evening, **good**, not wired in yet |
| `roh-gnome-mcp.mp4` | the agent take, unchanged |

The phone take is the one piece of new picture that is finished: 68.9 s, the
handset playing the film's own bed, the visualiser moving to the mix that is
actually under the film, and a position readout that shows the right second of
the Showreel Theme at every point. Offset is ~5.0 s by eye; measure it with
`align-take-by-clock.py` before cutting.

## Two new tools, both with control arms

- `scripts/showreel/align-take.py` — waveform alignment. **Useless for the phone
  take**: scrcpy records this app as digital silence (−91.0 dB with playback
  running and media volume at 14). Keep it for material that has sound.
- `scripts/showreel/align-take-by-clock.py` — reads the on-screen position
  readout instead. Control arm: known offset 4.233 s -> `offset 4.233 slope
  1.0000 ticks 40`. Negative control (static readout) refuses.
- `scripts/showreel/splice-bed.sh` — the breakdown cut described above.

## What is left

1. **Reshoot the desktop tour.** It is the only thing standing between here and
   a finished film, and it is blocked on the desk being free for ~5 minutes with
   no mouse or keyboard (the pointer is dead-reckoned under Wayland).
   Preconditions, all three:
   - info panel closed, **proven by a frame**, not by the toggle's state;
   - something left to sync, or there is no `Sync now` button and no moving
     progress bar — the user wants the click in the shot;
   - nothing else in front of Reprise.
2. **Re-derive every desk in-point.** The `+0.3 s` in `cut-film.sh` belongs to an
   older take script. `take-gnome4.py` writes its mark *before* the pointer moves;
   the page is up about 3 s later. Measure it, do not carry the 0.3 over.
   The bridge in-point is about `sync-start − 0.8`.
3. ~~Wire in the phone take.~~ Done. `roh-android-bed.mp4` is now the default
   take in `cut-film.sh`, at the in-points derived below.

   **Bed time 0 sits at 4.2 s in the take.** `align-take-by-clock.py` refuses on
   this take, and correctly: the on-screen readout updates on a ~500 ms poll, so
   the tick times form a sawtooth in [4.200, 4.850] instead of landing on a
   line, and no crop can fix that. The minimum of that sawtooth is the one value
   free of display lag, so 4.2 is the answer the tool's own data gives even
   though the tool will not print it. Independent check: the readout's last
   second is 0:55 and the app navigates away at take time 60.0 — 4.2 + 55.8.

   While looking for it, the crop box turned out to be catching the **seek-bar
   playhead**, not the visualiser as assumed: the playhead's tick reaches y=1820,
   ten pixels inside the box's top edge. `ROH_ANDROID_BED_CROP = (40, 1825, 220,
   40)` is in the script for this take; `DEFAULT_CROP` is untouched.

   **The take plays the 55.8 s bed**, shot at 20:12 against `spliced-51s.wav` at
   21:05. It survives that anyway, because both beds are the same audio at the
   same tempo (`align-bed.py` gives 0.999556 for either length) and both rejoin
   the original track at 66.0 — only the A segment differs, 41.518 against
   36.516. So film time `t` in the 51 s cut is found in the take's bed at
   `t + 3.001` below 36.516 and `t + 5.002` above it. The whole phone half sits
   above it, so: bridge phone half at `4.2 + 36.6 + 5.002 = 45.8`, phone shot at
   `4.2 + 37.8 + 5.002 = 47.0`. Both are the defaults in `cut-film.sh` now.

   **The readout was wrong and the sign was wrong too.** Checking the
   arithmetic against the on-screen clock only proves the clock arithmetic, so
   the finished cut was measured instead: the spectrum's own bar heights against
   the film's own audio, band for band — the cyan third against 40-160 Hz, the
   magenta third against 1-3.5 kHz, RMS per frame on both sides. Two
   independently filtered channels landing on the same lag is what makes it
   believable; a single broadband correlation was too weak and edge-confounded
   to trust, and the beat period (~0.6 s) puts side-lobes right where the answer
   is, so the two-band agreement is doing real work.

   | shot in-point | lead of bars over sound, cyan / magenta |
   |---|---|
   | 47.0 | +0.57 / +0.67 s (r 0.478 / 0.440) |
   | 47.6 | +1.17 / +1.27 s (r 0.491 / 0.476) |
   | **46.4** | **-0.03 / +0.10 s (r 0.471 / 0.394)** — in sync |

   Three points, slope 1.00 and 0.98 between successive pairs, so the third is
   not a lucky landing but the line's own zero. The peak's half-width is about
   ±0.2 s, which is this method's floor: a residual smaller than that cannot be
   seen, and nothing that large is left.

   A **later** in-point makes the lead worse, not better: at every film second it
   shows a further-advanced moment of the handset's playback. Slope is +1.0
   across the two renders, so zero sits at 46.4, and the bridge's phone half at
   45.2. That is what `cut-film.sh` carries now.

   Method, if it has to be repeated: crop to the spectrum square only — real
   pixels x 848-1073, y 270-497 in the 1920x1080 frame — and drop the bottom
   rows, because the panel draws a mirrored reflection under the bars that
   muddies the signal. Exclude the first ~1.5 s of the shot; the slide's own
   decaying brightness swamps the statistic. Scripts are under
   `$SCRATCH/sync/`.

4. ~~Put the phone's media volume back to 0.~~ Done — set and read back as
   `volume is 0 in range [0..25]`. It never mattered anyway: scrcpy does not
   capture this app's output at any volume.

## Traps found today, on top of the ones in the 66 s handover

- **The on-screen clock and the picture disagree by 0.6 s, and the picture
  wins.** Aligning a phone take by its position readout put the visualiser 0.6 s
  ahead of the music, and the readout gave no hint of it. Only correlating the
  bars against the finished film's own audio found it. Any take aligned by the
  clock is provisional until the cut has been measured.
- **A later in-point makes a leading picture lead more.** Obvious in hindsight,
  and it still cost a render: at every film second a later in-point shows a
  further-advanced moment of the recorded playback. Measure twice and take the
  slope rather than reasoning about the direction.
- **A fixed work directory plus two runs is a reordered film, not a crash.**
  `cut-film.sh` wipes `$O/list.txt` at start and appends to it as it goes, so a
  second run started while the first is still going leaves a list with the
  earlier run's last shot at the top. The concat then succeeds, at the right
  duration, with the end card first — `ffprobe` cannot see it. One render came
  out that way. `cut-film.sh` now takes an exclusive `flock` on the work
  directory; before believing a concat, `cat list.txt` and count the lines.

- **`VERDICT PASS` does not mean the film is there.** The marks come from the
  clock and the picture comes from ffmpeg. A tour that retried two stations ran
  300 s against a 240 s screencast budget: complete timeline, PASS, and no
  handover in the file. Budget is now 480. `ffprobe` the take against the last
  mark before believing either.
- **Verify the panel by looking at it.** `checked: False` on `Toggle info panel`
  was read as "closed" and the take came back with the panel open in every shot.
- **A station retry is visible in the timeline** as two marks with the same
  label, or as stations 25-30 s apart instead of 13. It means focus was being
  stolen — usually somebody typing while the take runs.
- **scrcpy cannot record this app's audio.** Confirmed at media volume 14 with
  the visualiser moving. Align phone takes by the on-screen clock.
- **`pkill -f <script name>` kills the calling shell**, because the pattern
  matches the wrapper's own command line. Cost one confused abort (exit 144).
- **`busctl --user list | grep mpris | head -1` is the browser**, not Reprise.
  Pausing "the player" that way paused the user's video. Match
  `org.mpris.MediaPlayer2.reprise` exactly.
- **Showtime opens no window** for these files on this machine, while
  `ffmpeg -f null` and `gst-discoverer` both read them cleanly. Brave plays them.
