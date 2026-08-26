# The showreel drivers

The film itself is not in this repository — it is a deliverable, not source, and
1920×1080 footage does not belong in a git history. What lives here is the way
it was made: the drivers that shot it and the cut scripts that assemble it, so
the reel can be re-shot without reconstructing the decisions from the footage.

The shot list, the reasoning behind each cut and the open questions are in
[`docs/plans/reprise-showreel.HANDOFF.md`](../../docs/plans/reprise-showreel.HANDOFF.md).

## Where things live

Two directories, both settable by environment variable:

| Variable | Default | Holds |
|---|---|---|
| `SHOWREEL_DIR` | `~/Videos/reprise-showreel` | the raw takes, the plate, the finished films |
| `SHOWREEL_WORK` | `${XDG_CACHE_HOME:-~/.cache}/reprise-showreel` | per-shot intermediates, take logs, timelines |

Nothing in `SHOWREEL_WORK` is worth keeping; every script recreates what it
needs. Run everything from the repository root.

## Shooting

| Script | What it does |
|---|---|
| `rp.py` | AT-SPI helpers against the running app — the app keeps focus, no synthetic pointer, no cursor in frame |
| `active-window.py` | prints which app AT-SPI reports as active; the focus gate `await-run.sh` polls |
| `screencast.py` | holds one D-Bus connection for the whole `org.gnome.Shell.Screencast` session |
| `take-gnome.py` | take 1 — the sidebar tour, writes `timeline.json` |
| `take-gnome2.py` | take 2 — the three pickups (podcast subscribe, search, lyrics), writes `timeline2.json` |
| `await-run.sh` | waits for the one human click, then runs take 2, stops the cast and shoots the plates |
| `plates-gnome.py` | the eight GNOME showroom plates, native pixels rather than frames pulled from a take |
| `welcome-shot.sh` | the first-run screen under Xvfb, which the real session can never show again |
| `take-android.sh` | drives the phone over `adb` and writes `timeline-android.tsv` |

`screencast.py` no longer draws the cursor. The shipped takes were shot with it
on, and because AT-SPI driving never moves the pointer, it sits parked in frame
for the whole take — visible in the library shot at 7.7 s. Set
`SHOWREEL_DRAW_CURSOR=1` to reproduce the original footage.

Two things that are easy to get wrong. The screencast session belongs to the
D-Bus connection that started it, so a second `gdbus call` cannot stop it —
that is why `screencast.py` sits there holding the connection until a stop-flag
file appears. And the app's AT-SPI name is `reprise`, lowercase; sidebar entries
have role `button`, not `push button`; preferences pages expose no action at all
and have to be selected through the parent's `SelectionIface.select_child()`.

The phone coordinates are pinned to a Pixel 10 Pro XL in portrait. They will not
survive a different device or a layout change.

## Cutting

```sh
scripts/showreel/cards.sh          # the title and end card, from data/brand/
scripts/showreel/cut-gnome.sh      # title card + 15 shots -> reprise-gnome.mp4
scripts/showreel/cut-android.sh    #  7 shots -> reprise-android.mp4
scripts/showreel/cut-showreel.sh   # the halves, the dip to black, the end card
```

`film.sh` holds what makes it a film rather than a slideshow, and both cut
scripts source it:

- **The push.** Two percent over each shot, alternating direction so a run of
  shots does not read as one mechanical drift. Half the shots in this reel are
  literally frozen — the lyrics shot changes zero pixels across four seconds —
  and without motion they read as screenshots.
- **The rail.** The frame is a 988-row stage carrying the app and a 92-row band
  carrying the comment, divided by a hairline. Captions never sit on the
  interface: the first attempt put "Library Doctor" straight on top of the
  app's own Library Doctor entry.
- **The captions themselves** are Adwaita Sans, GNOME's own UI font, with the
  brand teal on the leading dash. A shot with nothing honest to say gets an
  empty caption rather than a claim — which is why the phone's seek shot is
  silent.

Each prints the duration it produced. The cut scripts are the shot list in
executable form: the in-points and lengths in `cut-gnome.sh` are the table in
the handover, and changing one means changing the other.

Both halves land on the same 1920×1080 canvas so the film never changes
resolution mid-play. The desktop is cropped `2880:1747:0:53` — fractional
scaling at 1.6667 puts the top bar in those first 53 rows. The portrait phone
frame sits centred on its own blurred enlargement, so the sides are not dead
black.

## Four things ffmpeg will get wrong here

Each of these cost a render before it was found, and none of them fails loudly.

1. **The takes are variable-rate.** `ffprobe` reports `r_frame_rate=10000/1`
   for the screencasts; the real average is about 22.6. `zoompan` counts input
   frames, so fed VFR it emits the wrong number of them and the shot runs long.
   `fps=30` must come first in every chain.
2. **`#` starts a comment in a filtergraph.** `color=#0d1014` silently swallows
   the rest of the chain. Every colour here is written `0xRRGGBB`.
3. **`drawtext` needs its text quoted** on ffmpeg 9 — `text=Hallo` is rejected
   with "Either text, a valid file, a timecode or text source must be
   provided", while `text='Hallo'` works.
4. **`lockup-horizontal.svg` renders as "Repris"** anywhere Fraunces is not
   installed: it keeps the wordmark as a live `<text>` element, so rsvg falls
   back to a wider serif and the last glyph leaves the viewBox. The cards use
   `lockup-horizontal-outlined.svg`, which carries the wordmark as paths.
