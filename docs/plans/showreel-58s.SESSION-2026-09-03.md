# Handover — the 58.2 s showreel, session of 2026-09-03

**The film is finished and correct.** `~/Videos/reprise-showreel/`
`reprise-showreel-58s-scored.mp4`, 58.200000 s, −16.1 LUFS. Every shot is in
it, the music is the one the user picked, and the phone's visualiser is in time
with that music to **8 milliseconds**.

Read `showreel-58s.SESSION-2026-09-02.md` for how the takes were shot and
`showreel-58s.HANDOFF.md` for the film's shape. This file is only what changed
on 2026-09-03, and the two things that are still open.

## What the user decided today

- **The music is variant `e`** of five generated for this session. Chosen by
  watching all five under the finished cut, not by listening to them alone.
- **The film must start with a real beginning**, not partway into a track. This
  is what started the whole music rebuild: the old bed began at second 13.5 of
  its source and the user heard it as an entry mid-song.
- **Female voice, tension building, variety.** The generation prompt is in
  `~/.cache/reprise-showreel/musik/v2/` beside each take's log.
- **Music and cut should harmonise.** Partly delivered — see the last section.

## The music

Five variants, `sonilo_music`, 75 s each, in
`~/.cache/reprise-showreel/musik/v2/v-{a..e}.m4a`. Each was trimmed to the
film's 58.2 s from **its own second zero** and laid under the existing silent
cut, so the choice was made against the picture:
`~/Videos/reprise-showreel/musik-{a..e}.mp4`.

The chosen bed is `~/.cache/reprise-showreel/musik/bed-final-58s.wav` — `v-e`
from 0 to 58.2 with a 0.5 s fade at the tail. **Scored with `SHOWREEL_ARC=0`**,
not the 0.6 the old bed used: the arc was an envelope shaped for a bed with no
dynamics of its own, and these tracks have their own build. Leaving it on
ducked the opening to 35 % and flattened exactly what was chosen.

    SCRATCH=$HOME/.cache/reprise-showreel/score-final SHOWREEL_BPM=120 \
      SHOWREEL_ARC=0 SHOWREEL_DROP=0 SHOWREEL_ALIGN=0 SHOWREEL_WINDOW=0 \
      bash scripts/showreel/score.sh \
        ~/.cache/reprise-showreel/musik/bed-final-58s.wav \
        ~/Videos/reprise-showreel/reprise-showreel-58s.mp4 OUT.mp4

**`v-d` is unusable and it is worth knowing why.** Its breakdown falls on
46–59 s, which is precisely the visualiser shot: the one shot whose whole
subject is the bars moving would have sat in near-silence. A track is not
judged on how it sounds, it is judged on what it is doing during each shot.

## The theme song and the sidecar, rebuilt for the new bed

The handset plays the film's own bed — that, and nothing else, is what lets the
bars be claimed to move with the music. So the bed change forces all of this:

1. `Reprise Theme.mp3` re-encoded from `bed-final-58s.wav`, 256 kbit/s,
   58.200000 s, tagged *Reprise Theme / Reprise / Reprise*, cover from
   `~/.cache/reprise-showreel/theme/cover.png` (unchanged, still good).
2. The sidecar rebuilt: `cargo run -p reprise-platform-linux --example
   analysis_sidecar -- "<file>"` → 29 012 bytes, 1165 spectrogram frames of 24
   bands. That example is the only route; there is no CLI subcommand for it.
3. Both pushed to `/sdcard/Music/Reprise/Reprise/Reprise/` and the library
   rescanned from the Library-actions menu (785 → 786 titles).

**The trap that cost forty minutes: the desktop auto-syncs the moment the phone
is plugged in, and a sync deletes every file it did not put there.** The push
succeeded, the rescan then found nothing, and the directory was empty when
looked at again — the hand-pushed theme and its sidecar had been deleted
between the two. `ls` right after a push proves nothing; look again after the
rescan. **Stop the desktop app before touching the phone's files:**

    pkill -f '\.local/bin/reprise$'; systemctl --user stop reprise-showreel

Two dead ends recorded so they are not tried again: `MEDIA_SCANNER_SCAN_FILE`
returns `result=0` with no receiver, and `content call --method scan_file`
returns `STREAM=null`. Neither matters — the app does **not** read MediaStore.
It walks the SAF tree itself (`crates/reprise-core/src/library/scanner.rs:328`)
and accepts `mp3` among seven extensions (`scanner.rs:22`). A hand-pushed mp3 is
indexed like any other file; the only thing that had removed it was the sync.

## Shot 13, now actually in time

The old take had the handset at 0:04–0:11 of the track under film seconds
47.4–54.6 — 43.4 s out. Re-shot with a step list that lets the song run before
the sheet is opened:

    search    163 191                 0.8
    type      text:Reprise%sTheme     1.2
    keyboard  key:BACK                0.6
    play      347 572                46.0     <- the song runs, nothing is filmed
    open      tap:400,2020            3.5
    spectrum  tap:540,925             7.0

Written straight into `~/.cache/reprise-showreel/steps-nowplaying.tsv` after a
`probe` with short dwells had resolved the coordinates — the probe caches
`label / xy / dwell`, so the dwells can be replaced without re-probing.

`roh-android-nowplaying2.mp4`, 68.0 s. `play` at 9.688, so track position 0 is
take second 9.688 and the spectrum tap at 59.751 is **position 50.06**. Shot 13
wants position 47.4 at its in-point, which is take second 57.09.

**Then it was measured, and the measurement is the point.** The correlator
(`measure-vis-sync.py`) reported −1.62 s with *r* = 0.23 and 0.43 and its two
bands disagreeing by a quarter second — it cannot resolve this material and
said so both times, once when the shot was 43 s out and once when it was right.
What settled it is the **readout tick**: decode the position readout at 30 fps,
find the frames where the digits change, and compare those to the whole second.

    ticks 49.83 50.80 51.83 53.83  ->  offset -0.175 s   (picture ahead)

In-point 57.09 − 0.175 = **56.915**, re-rendered, re-measured:

    ticks 50.00 50.97 52.00 54.00  ->  offset -0.008 s

Eight milliseconds. The tick method is in the shell history of this session and
worth extracting into a script the next time shot 13 moves; `measure-vis-sync.py`
should keep its place as a cross-check, not as the witness.

## Concerts, and the frame nobody looked at

Also re-shot today, and the story is in `SESSION-2026-09-02.md` under "The
Concerts shot, as it was actually taken". Short version: a take that passed
every rule — `VERDICT PASS`, a real page turn, peak 2.88, ratio 310 — was still
unusable, because the 1.9 s lead before the turn showed the *playlist* page a
stray click had opened. The lead is not neutral footage. **Look at the first
frame of every shot**, not a frame sampled after the cut. Final in-point 36.22
out of `roh-gnome-concerts-2026-09-02.mp4`.

## What is still open

### 1. Music and cut only harmonise at one point — the best one

Measured events in the bed (0.1 s resolution, ≥4 dB over one second):

| bed time | what |
|---|---|
| 1.0 | the intro settles out of its fade-in |
| 14.5 | first change |
| 41.6 | the breakdown thins |
| **50.0** | **the main drop, +6 dB** |
| 57.8 | the outro falls away |

Film cuts: 3.0, 8.4, 13.8, 19.2, 24.6, 30.0, 36.6 (the slide), 37.8, 47.4, 54.6.

**The one that lands is the one that matters**: the drop at 50.0 falls on the
tap at 50.06 that turns the cover into the spectrum. The film's biggest musical
event and its biggest visual event are the same moment, to six hundredths of a
second. Nothing else lands: 14.5 misses the cut at 13.8 by 0.7 s, and 41.6 sits
mid-shot.

**And the rest cannot be made to land by nudging.** The film's edit grid is
0.6 s — every boundary is a multiple of it — which is a 100 BPM grid. Variant e
is 120 BPM, a 0.5 s beat. The two grids only coincide every 3.0 s, so most cuts
fall between beats no matter where the bed starts. Three honest routes:

- **Leave it.** The climax lands and the rest is near enough that no one has
  complained about the same mismatch in the old bed, which was also 120 against
  0.6.
- **Generate at 100 BPM** in variant e's style and choose again. Full grid
  alignment, at the cost of the track the user picked.
- **Re-time the film to a 0.5 s grid.** Every shot duration and every boundary
  changes; the in-points all move. Largest change, best result.

Whatever is chosen, **a bed change forces the phone round again** — new theme
file, new sidecar, push, rescan, re-shoot shot 13, re-measure. Budget the phone
session with the music decision, not after it.

### 2. The phone is very tall in a 16:9 frame

The user's own observation and it is right: the Pixel 10 Pro XL is 1080×2400,
so in the 1920×1080 stage the handset is a narrow strip with dead ground either
side. A 1080×1920 device, or an unfolded foldable, would fill the frame.

An emulator is the one place where the screen shape can simply be chosen, and
for shot 13 it would be equivalent — the visualiser draws from the sidecar, not
from live audio, so it would look identical. For shot 12 it is weaker: that shot
is a continuous hand gesture and real scroll behaviour on real hardware.

The work, if it is wanted: build the library on the virtual device, install the
APK, scan, re-shoot both phone shots, and recompute the handset mock in
`device-frame.py` for the new geometry. Roughly an hour.

## State the machine and phone were left in

- **Desktop Reprise is stopped** — deliberately, so it cannot sync over the
  phone's theme file. Restart it under the unit before any desktop take.
- **Sync configuration unchanged**: *Recently played* (50 tracks) is still a
  selected source. Deselecting it queues those tracks for deletion.
- **Phone**: the theme and its sidecar are on it and indexed (786 titles). A
  sync will delete them again.
- Device lock released, wake lock `showreel-vis` still held — release it.
- `scripts/showreel/cut-film.sh` carries every in-point as a default, so
  `bash scripts/showreel/cut-film.sh OUT.mp4` reproduces the cut with no
  environment at all.
