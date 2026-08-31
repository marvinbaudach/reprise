# Showreel — state at the end of 2026-08-28

Read `showreel-60s.HANDOFF.md` first for the reasoning. This file is only what
changed today and what is still open.

## The film is locked

`~/Videos/reprise-showreel/reprise-showreel-60s.mp4` — 60.000000 s, 1920x1080,
1800 frames, −16.1 LUFS, −3.7 dBTP. The showroom carries it. Suite 85/85.

**It is not reproducible from the tree any more.** It was cut with the
pre-fix `align-bed.py`, which stretched its cue by 0.9986 where the corrected
script says 1.0012 — about 0.16 s of drift across the film, plus a slightly
different head trim. Inaudible, almost certainly, but a re-render from
`reprise-showreel-cut.mp4` will not be bit-identical to the file it is meant to
reproduce. If that matters, re-cut rather than assume.

Commits on `showreel-recut-and-drivers`, **none pushed**:

| | |
|---|---|
| `ce5cc6fec2` | the agent shot, `wait-active.py`, the seed/caption fix, `pick-window.py` recalibrated |
| `dc48fcc631` | showroom carries the 60 s film, cue sheet rewritten |
| `3bec417ae1` | handoff status |
| `41bfc49a9e` | framing fixes: lyrics anchor, bridge target, the playlist mark |

## Acceptance, every time

`ffprobe` must say **60.000000**. `cut-film.sh` skips a missing take silently
and exits 0 — an exit status proves nothing here. Second check: the per-2 s
loudness profile must have no block below about −30 dB except the tail.

## Open

**The agent shot has no phone half.** `take-mcp.sh` is ready, the Pixel is
connected, the app with the device-sync fix runs from
`/home/marvin/Projects/reprise-external-changes-reach-device-sync/target/debug/reprise`,
and `Open Pixel 10 Pro XL` is in the sidebar. It needs one thing: somebody
clicking the Reprise window forward inside the preroll. Two attempts aborted
cleanly — `wait-active.py` refuses to record the wrong window, and
`org.freedesktop.Application.Activate` is accepted but does not raise the
window, so Mutter's focus-stealing prevention is confirmed for that route too.

**And having the footage is not having it in the film.** `mcp()` builds the
shot from two 2.5 s halves. A third beat for the sync costs 2.4 s that must
come off another shot, or the film stops being 100 beats.

**The music in the film is a pulse, not an arc** — variant C (the original,
31 s run), windowed from 7.2 s, match 0.293. Two attempts to improve it are in
flight; neither has been heard yet.

*What was learned the expensive way:* a prompt to ElevenLabs may not carry
**timestamps**. The 2026-08-28 regeneration wrote "riser from 37 seconds, full
stop at 39 seconds" and every variant came back worse than the material it was
meant to replace. **Character words are the lever that works** — genre,
instrumentation, "wide dynamic range", "real breakdowns", "it must not be one
loud block".

1. **Execution 16** of n8n workflow `Nv8IFwnuNSmBegAv` was started at the end
   of the session, prompts rewritten on character alone: **A** instrumental
   metalcore, **B** instrumental hip hop, **C** djent/electronic hybrid. The
   user chose metalcore, and B is the deliberate neutral counter-sample,
   because the showroom's reader is a hiring reader and metalcore polarises.
   The films's own library is metalcore, which is the argument for it.
   Files land at `/opt/n8n-stack/files/musik/reprise-showreel-{a,b,c}.mp3` on
   `hetzner-media` and **overwrite** the previous run — local copies of the
   originals are kept as `*-31s.mp3` in `~/.cache/reprise-showreel/musik/`.
2. **A composed bed**, `bed.py` extended from 34.8 s to 60.0 s, acceptance
   correlation > 0.80 against `target_arc`. Lands at
   `~/.cache/reprise-showreel/musik/bed-60s.wav`.

**How to finish the music.** Score every candidate with
`pick-window.py TRACK 60.0 100.0` — it now reports `match` *and* `quietest`,
and refuses windows with holes. Then
`score.sh TRACK reprise-showreel-cut.mp4 reprise-showreel-60s.mp4`, then
`encode-web.sh`, then `npm test` in `showroom/`. Judge by ear as well: the
match number says the shape fits, not that it sounds good.

**The lyrics shot** is legible but the lyrics are still a narrow column at the
edge. No crop fixes that — it wants a recording with the pane given the width.

**PR #728** (external changes reach device sync) is open against `dev`. Its
green check means nothing: it finished in 6 s with every real suite skipping.

## Execution 16 came back, and it does not carry the film

Downloaded and scored. Run 15's copies are kept as `*-run15.mp3`, the same way
the 31 s run is kept as `*-31s.mp3`.

**`pick-window.py` cannot score exact-length material.** With a 60.0 s window
and a 60.024 s file there is one candidate window; with a 60.000 s file there
is none, and it prints `match -2.000` and `quietest -99 dB` — sentinels, not
measurements. That is why `bed-60s.wav` looked like the worst candidate. Score
every candidate over the same stretch instead, and run the hole check from
3.0 s to 58.8 s, where `score.sh` has not already faded:
`score-candidates.py TRACK...` does that, and printed the table below.

| candidate | arc corr | quietest 3–58.8 s | verdict |
|---|---|---|---|
| `bed-60s.wav` (composed) | **0.812** | −6.7 dB | the only one with an arc and no hole |
| `a.mp3` run 16 — metalcore | 0.196 | −27.1 dB | 24 s fade-in; body too short (see below) |
| `b.mp3` run 16 — hip hop | −0.102 | −4.1 dB | flat: the pulse problem, unsolved |
| `c.mp3` run 16 — djent | 0.311 | −29.4 dB | full stops at 40 s and from 54 s |
| `c-31s.mp3` (in the film now) | 0.293 | −7.7 dB | the incumbent, and flat +0 throughout |
| run 15 a/b/c | 0.181 / 0.064 / 0.059 | −11 / −51 / −37 dB | worse than the material they replaced |

**The genre choice is not the problem; the material is.** Variant A is 133 BPM,
so at the film's 100 BPM grid it runs 79.9 s — but its first 24 s are a fade-in
from −37 dB, and the body that follows is about 56 s. Four seconds short of the
picture, whatever window is chosen. No cut fixes that.

So run 16 produced no usable cue, and the choice is between the incumbent pulse
and the composed bed. **Judge by ear** — the previews are cut and mastered:

- `~/Videos/reprise-showreel/preview-bed-60s.mp4` — 60.000000 s, −16.0 LUFS, −4.3 dBTP
- `preview-reprise-showreel-a.mp4`, `preview-reprise-showreel-c.mp4` — for comparison
- `reprise-showreel-60s.mp4` is **untouched**: still the incumbent, still 60.000000 s

If the bed wins: `score.sh` it onto `reprise-showreel-cut.mp4` with
`SHOWREEL_ALIGN=0`, then `encode-web.sh`, then `npm test` in `showroom/`.

## Two bugs that were destroying candidates

Both fixed in the working tree, **not committed**, alongside the uncommitted
`bed.py`.

- **`align-bed.py` had the tempo ratio inverted.** `factor = bpm / target_bpm`
  speeds a 101 BPM track up to reach 100, when its own comment says it has to
  be slowed. Everything measured so far sat within 0.2 % of 100 BPM, so the
  error was inaudible and stood. Variant A at 133 BPM was sped to 177, ran out
  after 45 s, and `apad` finished the film with **fifteen seconds of silence**.
- **`score.sh` padded without saying so.** It now refuses a track that gives
  more than a second less music than the picture needs. Control arm:
  `SHOWREEL_BPM=200 score.sh a.mp3 …` starves the track to 39.8 s, prints the
  refusal and writes **no file** — checked by the file's absence, not by an
  exit status read through a pipe.
- `score.sh` also takes `SHOWREEL_ALIGN=0` now. A composed bed is already on
  the film's grid; measuring its tempo and correcting the 0.14 % estimation
  error trimmed 0.53 s off its head and slid every section away from the cut it
  was written for.

**Still open in the tooling:** `pick-window.py` scores the *unstretched* track,
so a track that only becomes long enough after the stretch is never offered the
windows it has. That is what would be needed to give variant A a fair hearing.
It is unlikely to rescue it: A's body runs about 56 s against a 60 s picture.
That figure is derived — the 24 s fade-in was read off a 2 s-resolution profile
— so treat it as "probably short", not as proof.

## Worktree, before anyone measures anything here

`showroom/` carries a long-standing uncommitted change from 2026-08-26: the
product gallery removed — component, data, css, tests and 39 `.webp` assets.
It is the baseline, not fresh breakage. `npm test` on it: **85/85, exit 0**,
re-measured 2026-08-28 13:44 before any music work. The later `score.sh` and
`align-bed.py` edits do not touch it: nothing under `showroom/` names either
script.

## Recut: what a framing may cut, and the answer to it

The film shipped with the Sort button sliced in half. The rule in `cut-film.sh`
claimed a 0.20 push at fx=1.0 lands "in the list's own padding, clear of the
sidebar" — it does not. The window fills the recording from column 10 to 2879,
so 0.20 takes 480 px off the left and that lands on the button. Verified by
rendering the film's own frame at 11 s beside a re-render of the shot: identical,
and both sliced.

**The film now has two kinds of shot and nothing in between.** A page shot holds
the whole application at 1:1. A `focus` goes all the way onto one bounded thing
so that thing fills the frame. The half measure is what produced the defect.

`focus()` fits its region **by height and centres it**, over a blurred darkened
enlargement of itself — the treatment the phone shots already had. Cropping the
region to the stage's 16:9 would drag a strip of the track list in beside it,
which is the same defect one step smaller. The panel was measured with a pixel
ruler, not guessed: **x 2380, width 480**; the lyrics band at y 770 h 560, the
visualiser at y 690 h 545.

What changed, shot by shot:

- Every page shot is at zoom 0.00 — search, releases, concerts, podcasts,
  doctor, stats, and both halves of the agent shot. The agent shot's marker box
  moved with it, to x=171 y=298 w=214 h=40.
- **`03-lyrics` was captioned over the queue.** It was a 0.17 push on take 2 at
  50.5 s, and take 2 has the queue open in that panel — the film has been saying
  "Lyrics, in time" over a list of upcoming tracks. The lyrics are in take 1 at
  106 s and now get the frame: seven lines, the playing one lit.
- **The handover is the visualiser on both sides.** It used to be a 3x dive into
  the bottom of the desktop frame, which landed on the seek bar's waveform — a
  teal squiggle in the player chrome, not the visualiser. Take 1 has the real
  visualiser panel open from 110 s to 121 s; the desktop half now holds it at
  117 s, tab bar, spectrum and all six readings, and the phone slides in showing
  its own.
- **Four shots opened on the page before them.** The hook opened on Library
  Doctor, releases on the library, concerts on releases, the doctor on the
  device page — one to one and a half seconds each, with the caption already
  naming a page that had not arrived. `settle.py` (new) finds the moment the
  page lands by comparing the window title against the strip from the end of the
  shot; the in-points moved to 105.4, 41.2, 51.1 and 94.2. Verified by reading
  the first and last frame of every page shot out of the finished film: all
  eight now hold their own page end to end.

  The title has to be compared at full size. Downscaled, "Music" and "Releases"
  differ by less than the noise floor, and the first version of the check
  reported every page as already settled.
- **The phone is no longer cut.** `crop=1080:2240:0:80` sliced the status bar off
  a 1080x2400 take. The frame is built at 373x830 instead of 400x830 — the same
  830 px of screen height, the width following the recording — and the take goes
  in whole.

`reprise-showreel-cut-v2.mp4`, 60.000000 s. The locked film and the showroom are
untouched.

## The music is metalcore now, and the arc is a mix decision

Run 17: three metalcore variants, 90 s each so there is material to window from,
prompted against run 16's two measured defects — 133 BPM with an 18 s fade-in.
All three came back at **75 BPM**, which needs no stretching at all: three music
beats are four film beats, so the grids meet every 2.4 s, one bar. Score them
with `SHOWREEL_BPM=75`.

| | corr | hole | LRA |
|---|---|---|---|
| **b — melodic metalcore** | **0.473** | −7.2 dB | 6.0 |
| a — metalcore | 0.118 | −1.1 dB | 2.6 |
| c — djent hybrid | 0.040 | −0.5 dB | 0.9 |

**No holes anywhere — and no arc either.** "Never drops to silence" was an
over-correction: the model returned one consistently loud block. At two-second
resolution all three are flat, which is the same complaint the recut was meant
to answer, arrived at from the other side.

So the shape is no longer asked of the generator. `arc-gain.py` turns
`pick-window.py`'s own `target_arc` into an ffmpeg volume automation and
`score.sh SHOWREEL_ARC=<depth>` applies it — the music ducks under the title
card, opens at the hook, pulls back at the handover and falls away under the end
card. The window is chosen against the same curve it is then given. Off by
default: a track that already breathes does not want a second hand on the fader.

To hear it — all 60.000000 s, all −16.1 LUFS, none clipping, all against the
recut picture:

- `preview-metalcore-b-arc06.mp4` — B with the arc at 0.6, LRA 9.6
- `preview-metalcore-b-arc10.mp4` — B with the arc full, LRA 7.3
- `preview-metalcore-b-flat.mp4` — B untouched, LRA 6.0, for the comparison
- `preview-metalcore-a-arc06.mp4`, `preview-metalcore-c-arc06.mp4` — the others
- run 16 is kept as `*-run16.mp3`, run 15 as `*-run15.mp3`

## Second pass: the lyrics go, the handover becomes the claim

**The lyrics shot is out.** The film is **56.4 s**, 94 beats, every boundary
still a multiple of 0.6 s. `focus()` went with it — nothing used it any more.
The measurements survive here if it is ever wanted back: the right-hand panel is
x 2380 wide 480, the lyrics band y 770 h 560, the visualiser y 690 h 545, and
the trick was to fit such a region by height and centre it over a blurred
enlargement of itself rather than crop it to the stage's aspect.

**`target_arc` was re-read** — the file warns that a window scored
against the wrong shape measures nothing, and dropping a 4.2 s shot moved every
boundary after it. The breakpoints now live in `arc_steps()` and `arc-gain.py`
reads them from there instead of carrying its own copy, so the shape a window is
chosen for and the shape it is given on the fader cannot drift apart again.
`score-candidates.py` takes the film's length as its first argument.

**The handover is switchable, and it defaults to the device sync.**
`SHOWREEL_BRIDGE=visualizer` restores the other one.

- `cut-A-visualizer.mp4` — pushes into the visualiser panel. A rhyme: two
  spectra that look alike. The landing frame drags a strip of the track list in
  beside the panel, because the panel is upright and the frame is not.
- `cut-B-devicesync.mp4` — pushes into the device page and lands on **"Pixel 10
  Pro XL · MTP connected · Last synced 26.08.2026 · Verified · 743 Reprise
  tracks on device"**, then the phone fills the screen. It says the thing the
  handover exists to say instead of rhyming a picture, and it reads at a glance.

## Music: metal out, pop in, then a female voice

Metalcore did not carry the film. Two more runs, same structure in the prompt —
the run-17 lesson holds: "never drops to silence" alone buys a dead flat block,
so the sections have to be named (intro already musical, build, drop, a quieter
stripped section, final drop).

Both runs hit **120 BPM exactly**; against the 100 BPM grid they meet every
3.0 s. Score with `SHOWREEL_BPM=120`.

| | corr vs 55.8 s arc | quietest |
|---|---|---|
| run 18 b — instrumental pop | **0.509** | −9.2 dB |
| run 19 b — female vocal, hyperpop | 0.434 | −8.5 dB |
| run 19 c — female vocal, future bass | 0.400 | −6.7 dB |
| run 17 b — melodic metalcore | 0.473 (vs the 60 s arc) | −7.2 dB |

Runs are kept as `*-run15/16/17/18/19.mp3`.

**To hear it** — all 55.800000 s, −16.1/−16.2 LUFS, none clipping, all with the
arc at depth 0.6:

- `preview-vocal-b.mp4`, `preview-vocal-c.mp4` — female voice, device-sync handover
- `preview-instrumental-b.mp4` — the higher-scoring instrumental, for comparison
- `preview-vocal-b-visualizer.mp4` — the same music over the other handover

*Unresolved:* a sung lyric competes with a caption. Every desktop shot carries
one. Nobody has watched a vocal cut against the captions yet.

## The phone search shows the search now, not its result

`10-search` opened on the finished state: the field already filled, the list
already narrowed. That shows what the feature produced, not that it works. The
take has the whole flow — the full artist list to 7.0, the field opening, "lorna"
landing at 8.0, the four albums and the artist under it — so the shot starts at
6.6 and runs 4.8 s, which is what all four beats cost; at 4.2 the result was cut
off as it arrived. `11-artist` moved to 17.4, just past where the artist view
lands, so it holds one thing.

That is the +0.6 s: the film is **56.4 s**, 94 beats, and `arc_steps` moved its
end-card breakpoint from 52.2 to 52.8 with it.

| against the 56.4 s arc | corr | quietest |
|---|---|---|
| run 18 b — instrumental pop | **0.520** | −9.2 dB |
| run 19 b — female vocal, hyperpop | 0.422 | −8.1 dB |
| run 19 c — female vocal, future bass | 0.400 | −6.7 dB |

`preview-vocal-b.mp4`, `preview-vocal-c.mp4`, `preview-instrumental-b.mp4` —
all 56.400000 s, −16.1 LUFS, on the device-sync handover.

## The cut is 66.6 s, and the drop is in the mix

**Chosen: run 19 b** — female vocal hyperpop. Pinned as
`~/.cache/reprise-showreel/musik/reprise-showreel-chosen.mp3` so the next
generation run cannot overwrite it.

**Every feature now runs 4.8 s.** 3.0 to 4.2 was still too quick: a shot has to
carry a caption, a page, and whatever the page is doing, and the ones at 3.0
went past before the third of those registered. One length for all of them, so
no feature reads as the minor one. 66.6 s, 111 beats.

Verified rather than assumed — first and last frame of every shot read out of
the finished film: all six desktop shots hold their page end to end, and the
phone shots hold theirs. One did not: **`12-play` ran into the now-playing view
at 33.0 and ended on the very picture `13-visuals` opens with**, so the cut
between them read as no cut at all. Moved to 27.4, where it holds the album list
and contains the tap that starts it — the mini player turns from play to pause
inside the shot.

**`arc_steps` had the phone section fading.** It interpolated straight from the
handover's slam down to the end card's 0.30 — a fifteen-second fade across the
whole phone half, every phone shot quieter than the one before it, for nothing
in the picture. A breakpoint at 63.0 makes it the hold-then-fall the docstring
always claimed it was.

**`SHOWREEL_DROP=1` is the build and the release.** Over the four beats before
the release the music crossfades into a heavily lowpassed copy of itself, so the
top end closes; on the release frame it is dry again. Level and filter move
together, which is what a build is. The release is not typed in — it is read off
the breakpoint where the arc returns to full after the handover's dip, so if the
edit moves, the drop moves with it.

Measured on the finished film, as the ratio of 2–9 kHz to 40–320 Hz energy:

| | groove 38–40 s | build 43–43.8 s | release 43.8–45 s |
|---|---|---|---|
| with the drop | 6.7 dB | **0.1 dB** | **14.7 dB** |
| without | 6.7 dB | 13.8 dB | 14.7 dB |

A 14.6 dB swing across the cut to the phone, against nothing at all before.

**Run 20 asked the generator for the bass drop instead, and it was the wrong
lever**: 0.148 / 0.131 / 0.124 against the 66.6 s arc, where the chosen track
scores 0.458. Kept as `*-run20.mp3`.

The canonical invocation:

    SHOWREEL_BPM=120 SHOWREEL_ARC=0.6 SHOWREEL_DROP=1 \
      scripts/showreel/score.sh ~/.cache/reprise-showreel/musik/reprise-showreel-chosen.mp3 \
      ~/Videos/reprise-showreel/cut-B-devicesync.mp4 OUT.mp4

- `preview-vocal-b-drop.mp4` — the one to watch
- `preview-vocal-b-drop-deep.mp4` — arc at 0.8
- `preview-vocal-b-nodrop.mp4` — the same, without the build
- `preview-run20-a-drop.mp4` — the generated bass drop, for the comparison

## Still open after this session

- **The new desktop take.** Agreed: subscribe a set of unobjectionable shows and
  channels first, then re-record. The recorded library has four podcasts (apolut,
  AUF1, "ungeskriptet") and six YouTube channels, and there is no YouTube shot in
  the film at all. Four thin rows read as an empty app.
- **The film may run past 60 s** to make room for YouTube — agreed, not yet cut.
  60.0 s is 100 beats; 63.0 s is 105. Whatever it becomes, `target_arc` in
  `pick-window.py` has to be re-read against the new shot list, and the music
  re-windowed: run 17 has 90 s of material, so there is room.
- The phone half of the agent shot still needs somebody clicking the Reprise
  window forward inside the preroll.
- **A page shot is now smaller than it was.** At 1:1 the 2880-wide window sits
  in a 1582 stage — 55 % — where the old framing pushed 20 % into it. Nothing is
  cut any more, and that is the price. Two levers exist if it reads too small: a
  shot can hold longer now that the film may run past 60 s, and `focus()` will
  give any bounded thing the whole frame. Neither is applied yet.

## Two traps this session paid for

- **`ui.session.v1` restores `browser_place`.** `ui.window_view_mode=library`
  does not stop the app opening on the YouTube page. One take died to this.
- **`sqlite3` writes bypass `change_log`,** so the running app does not see
  them. Restart it after editing the library out from under it.
