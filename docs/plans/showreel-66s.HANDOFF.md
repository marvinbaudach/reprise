# Showreel — handover, 2026-08-28

Short on purpose. The reasoning and every measurement are in
`showreel-60s.SESSION-2026-08-28.md`; the older `showreel-60s.HANDOFF.md` is
still right about the tooling but wrong about the film's length and shot list.

Worktree `~/Projects/reprise-showreel`, branch `showreel-recut-and-drivers`.
**Nothing is committed.** Changed: `cut-film.sh`, `pick-window.py`, `score.sh`,
`align-bed.py`, `bed.py`. New: `arc-gain.py`, `settle.py`, `score-candidates.py`.

## What the film is now

`~/Videos/reprise-showreel/cut-B-devicesync.mp4` — **66.600000 s**, 111 beats at
100 BPM, silent. Rebuild with `scripts/showreel/cut-film.sh OUT.mp4`.

Sixteen shots: intro card 3.0, then eight desktop features at **4.8 s each**
(hook, search, releases, concerts, podcasts, doctor, stats, agent), the handover
2.4, four phone shots at 4.8 (search, artist, play, visuals), end card 3.6.

**The framing rule:** a shot holds the whole application at 1:1, or it is the one
camera move. Nothing in between — a 20 % push on a full-bleed window cuts the
Sort button in half, which is what shipped. The phone goes in whole (1080x2400
into a 373x830 frame); it used to lose its status bar to `crop=…:0:80`.

**The handover is the device sync.** It pushes into the device page and lands on
"Pixel 10 Pro XL · MTP connected · 743 Reprise tracks on device", then that phone
fills the screen. `SHOWREEL_BRIDGE=visualizer` switches to the other one, which
pushes into the visualiser panel instead.

## The music, decided

`~/.cache/reprise-showreel/musik/reprise-showreel-chosen.mp3` — run 19 b, female
vocal hyperpop, 120 BPM. Pinned under that name so a new run cannot overwrite it.
Runs 15 to 20 are kept as `*-runNN.mp3`.

    SHOWREEL_BPM=120 SHOWREEL_ARC=0.6 SHOWREEL_DROP=1 \
      scripts/showreel/score.sh ~/.cache/reprise-showreel/musik/reprise-showreel-chosen.mp3 \
      ~/Videos/reprise-showreel/cut-B-devicesync.mp4 OUT.mp4

`preview-vocal-b-drop.mp4` is that. Also on disk: `-drop-deep` (arc 0.8),
`-nodrop`, and `preview-run20-a-drop.mp4`.

## Open

1. **A new desktop take.** The recorded library has four podcasts (apolut, AUF1)
   and six YouTube channels, and there is no YouTube shot at all. Subscribe a set
   of unobjectionable shows and channels first, then re-record. This is the only
   reason those two features are weak. The film may run to 1:10, so a YouTube
   shot fits without taking time off anything.
2. **The agent shot has no phone half.** `take-mcp.sh` is ready; it needs a human
   clicking the Reprise window forward inside the preroll. `Activate` over D-Bus
   does not raise it — Mutter's focus-stealing prevention.
3. **A sung lyric against a caption.** Every desktop shot carries one. Nobody has
   judged whether they fight.
4. **The showroom still carries the old 60 s film.** After the film is final:
   `encode-web.sh`, `FILM_SECONDS` in `tests/showreel-film.test.mjs`, the cue
   times in `showreel.vtt`, the duration wording in `ShowreelFilm.tsx`, then
   `npm test`. Baseline before any of this work: 85/85, exit 0. `showroom/` also
   carries an unrelated uncommitted change from 26.08 (the product gallery
   removed) — that is the baseline, not breakage.

## Traps that cost time here

- **`cut-film.sh` exits 0 on a missing take** and `score.sh` used to pad the film
  with silence. `ffprobe` the duration; an exit status proves nothing. `score.sh`
  now refuses a track that is more than a second short.
- **`arc_steps()` in `pick-window.py` must be re-read whenever the shot list
  changes.** A window scored against the wrong shape measures nothing, and the
  drop's release time is read from it too.
- **`pick-window.py` cannot score exact-length material** — with a 66.6 s window
  and a 66.6 s file there is no candidate window and it prints the sentinels
  `match -2.000`, `quietest -99 dB`. Use `score-candidates.py SECONDS TRACK…`.
- **ElevenLabs ignores timestamps in a prompt** but honours `music_length_ms` and
  a named BPM. Ask for character and structure, never for "at 37 seconds".
  Asking it for the drop was the wrong lever; the drop belongs in the mix.
- **Two `cut-film.sh` runs share one scratch directory** and will corrupt each
  other's segments. One at a time.
- **Verify a shot by reading the finished film**, first and last frame, not by
  seeking the source take. Sampling a shot's middle hid that four shots opened on
  the previous page; sampling the source at the in-point hid the opposite.
