# Showreel recut — handoff

State on 2026-08-26, evening. Supersedes `showreel-32s.HANDOFF.md`. The film is
being recut from 31.8 s to **60.0 s** and every take is being re-recorded.

## Done on 2026-08-28 — the film is locked

`reprise-showreel-60s.mp4`, **60.000000 s**, 1800 frames, −16.1 LUFS, −3.7 dBTP.
The showroom carries it (`dc48fcc631`), suite 85/85 before and after.

Everything below that says "blocked" or "to do" has happened, with three
exceptions named at the end. What is still worth reading here is the reasoning:
the shot list, the framing rule, the traps. What is stale is the status.

- **The re-record had already happened** when this handoff was written — the
  takes postdate it by twenty minutes. The only missing shot was **08b, MCP**,
  and `cut-film.sh:135` was skipping it *silently* and returning exit 0. The
  film measured 55.2 s and looked finished. 55.2 + 4.8 = 60.0 was the whole
  remaining job. Never accept this cut on an exit status; `ffprobe` is the
  acceptance test.
- **The agent shot is in**, driven by `take-mcp.sh` with `wait-active.py` in
  front of it. Three takes were still needed: the first filmed the terminal,
  the second caught the app on the YouTube page (`ui.session.v1` restores
  `browser_place`, which `ui.window_view_mode` does not cover).
- **In-points measured, not guessed.** The row arrives at 7.4 s of the take and
  is selected at 13.8 s, so `mcp 08b-agent 4.0 15.0`.
- **The seed contradicted the caption** and now does not: `mcp-playlist.py`
  seeded from Immortal Disfigurement while the overlay promised Lorna Shore,
  who was not even in the neighbour list.
- **Music: variant C, the original one**, windowed from 7.2 s. The regeneration
  was run and did not pay: ElevenLabs ignores timing markers in the prompt and
  all three new variants score worse than the old material. The gain came from
  fixing the meter instead — `pick-window.py` still carried the 31.2 s arc, and
  it had no notion of silence, so it happily chose a window that reached past
  the generated track's own ending and put two seconds of nothing at 52 s into
  the first scored cut.

**Still open:** the device-sync half of the agent shot (no handset was
connected, `sync-playlist.py` exits cleanly without one, so the take shows the
agent writing but not the playlist travelling); the lyrics shot still wants a
layout rather than a crop; the music is a pulse, not an arc, and no generator
tried so far will give the arc this film asks for.

## Checkout hazard — resolved

The work now lives in its own worktree at `/home/marvin/Projects/reprise-showreel`
on branch `showreel-recut-and-drivers`. The main checkout at
`/home/marvin/Projects/reprise` is back to carrying only the other session's
Rust/po work; `showroom/` there was restored and every showreel file removed.
Transfer was verified with `diff -rq` over `showroom/` and `scripts/showreel/`.

Do the showreel work in the worktree. Nothing needs moving again.

## Decisions taken

| Question | Decision |
|---|---|
| Music variant | **C** (arc match 0.678 against the 31.8 s cut) |
| Length | **60.0 s — exactly 100 beats at 100 BPM** |
| Music first or picture first | **Picture first**, music laid afterwards |
| The blocked sync step | **Fix the app, then shoot it** |
| Intro card | **Yes**, 3.0 s, carries the Rust-core claim |

## What changed in the cut

### Nothing runs under 3.0 s

The 32 s cut had 1.2 s bursts and a 1.8 s floor. That is enough to register
that a screen changed and not enough to read either the caption or the screen.
The bursts are gone; `Releases.` and `Concerts.` are full callouts now.

### Every shot holds

The old grammar pushed 2–4 % into **every** shot, alternating `in` and `out`.
At that amount the frame is not meaningfully closer — it is only never still,
which is what made the film restless. `film2_push` has a third direction now:

- **`hold`** — a constant scale about (fx, fy). The default for every shot.
- **`in` / `out`** — kept for the bridge, which is now the film's only camera
  move, and lands because nothing around it moves.

The app moving inside a locked frame is the motion: typing, scrolling,
playback and the visualiser supply it for free.

### What a framing may cut

Measured off the stage (1582 × 960, padded to 1920 × 1080 with a 120 px caption
rail):

| Region | Stage x |
|---|---|
| Sidebar | 0 – 14 % |
| Track list | 14 – 83 % |
| Right panel | 83 – 100 % |

The sidebar and the right panel are **bounded objects** — half of one reads as a
broken screenshot. A track list is not; it is meant to continue past the frame.
So: a shot either contains a bounded object or starts past it, and every region
shot sits at `fx=1.00`, where at these amounts the left edge lands in the track
list's own padding, clear of the sidebar, and the right edge is the window's.

The hook is the exception at `hold 0.00` — the establishing shot holds the whole
application, which is what earns every later shot the right to be a region.

Verified against the old takes: **search, Library Doctor and My Stats came out
clean and readable.** See "Open craft problem" for the one that did not.

### Shot list — 60.0 s, 100 beats

| # | Shot | Take | In | Dur | Camera |
|---|---|---|---|---|---|
| 00 | Intro card | generated | — | 3.0 | — |
| 01 | Hook, whole app | T1 | 57.5 | 4.2 | hold 0.00 |
| 02 | Instant search | T2 | 76.5 | 4.8 | hold 0.20 / 1.00 / 0.00 |
| 03 | Lyrics | T2 | 95.5 | 4.2 | hold 0.20 / 1.00 / 0.50 |
| 04 | New releases | T1 | 14.2 | 3.0 | hold 0.20 / 1.00 / 0.00 |
| 05 | Concerts nearby | T1 | 21.2 | 3.6 | hold 0.20 / 1.00 / 0.00 |
| 06 | Podcasts | T1 | 27.7 | 4.2 | hold 0.20 / 1.00 / 0.00 |
| 07 | Library Doctor | T1 | 49.5 | 4.2 | hold 0.20 / 1.00 / 0.00 |
| 08 | Stats | T1 | 76.2 | 3.0 | hold 0.15 / 1.00 / 0.00 |
| 08b | MCP | TM | — | 4.8 | hold — **not yet shot** |
| 09 | Handover bridge | T1 + A2 | 59.2 / 46.0 | 2.4 | the one move |
| 10 | Android search | A2 | 11.0 | 4.2 | hold 0.08 |
| 11 | Artist view | A2 | 18.6 | 3.6 | hold 0.06 |
| 12 | Newest album | A2 | 30.0 | 3.6 | hold 0.06 |
| 13 | Phone visualiser | A2 | 48.6 | 3.6 | hold 0.06 |
| 14 | End card | generated | — | 3.6 | — |

**All in-points above are against the old takes and are expected to be wrong
after the re-record.** The durations and the camera column are the part that
carries over.

### The intro card

`introcard.py`, 3.0 s, five beats. The mark lands with exactly the moves it
lands with on the end card, so the film is bracketed by one identity.

Content: the lockup, a hairline, `One Rust core. Four native frontends.`, then
four columns landing left to right as a count —

| GNOME | Android | Terminal | Agents |
|---|---|---|---|
| GTK4 · libadwaita | Material 3 | a real CLI | an MCP server |

The four are real and checkable: `reprise-gnome`, `reprise-android-ffi`,
`reprise-cli`, `reprise-mcp`, all over `reprise-core`. The card also sets up the
MCP shot later in the film. The platform line that used to hang under the hook's
statement is gone — it competed with the visualiser for the same seconds.

The card does not fade to black; it hard-cuts into the hook on the beat.

### cardkit.py

The intro and end cards were 180 lines of the same compositing code. The ground,
the mark's landing, the light streak, the rising line and the hairline now live
in `cardkit.py`, and each card is only the score that calls them.

**The extraction is verified byte-exact**: the end card renders 108/108
identical frames before and after, checked with `ffmpeg -f framemd5`. Keep that
check if cardkit is touched again — it is one command and it is the only thing
standing between a refactor and a silently changed deliverable.

### Renames

`cut-30s.sh` → `cut-film.sh` (it is no longer 30 s and should not be renamed
again for the next length). The comment references in `bed.py` and
`pick-window.py` were updated. `encode-web.sh` no longer points at the deleted
`reprise-showreel-31s-music.mp4`.

## What is blocked

### The re-record

The user has built app improvements since the takes were shot, so **every take
is being re-recorded**. Do not spend effort tuning in-points or framings against
`roh-gnome-take1/2.mp4` or `roh-android-take2.mp4` — they are about to be
replaced.

Take this into the recording session:

- **The lyrics shot needs a layout, not a crop.** See below.
- **The MCP shot needs the sync fix landed first** so the whole flow can be shot
  in one session. See below.
- Every shot needs to hold still for its full duration now — up to 4.8 s. The
  old takes were shot for 1.8 s shots, so several in-points have only a second
  or two of usable footage after them.

### The device-sync gap — diagnosed, not fixed

A playlist created over MCP appears in the sidebar but **not** in the
device-sync page's playlist list, so the sync half of the MCP shot cannot be
filmed. Located precisely:

- `external_changes/mod.rs:94` `start()` coalesces `change_log` into a
  `RefreshPlan` and hands it to **one** subscriber.
- That subscriber is `window/window_external_changes_wiring.rs:16`, whose
  closure refreshes exactly two surfaces: `sidebar.refresh()` (`:29`) and
  `track_list.reload()` (`:37`).
- The device-sync module never subscribes at all — `rg external_changes
  crates/reprise-gnome/src/ui/device_sync/` returns nothing.
- The picker reads named playlists from the **cached** `device.page.playlists`
  (`device_sync_picker_runtime.rs:36` → `DeviceState::view()`), not from the DB.
  Only the synthetic "Everything" row is queried fresh.
- That cache is written **only** by `recompute_delta_silent()`
  (`device_sync_compact.rs:132`, writes at `:205`), and none of its call sites
  is triggered by an external DB write.

So the fix has two candidate legs, and probably wants both:

1. Wire the device-sync page into the existing `apply` closure so an external
   change calls `recompute_delta` for the current device.
2. Have `picker_snapshot()` read named playlists fresh at dialog-open time, the
   way it already does for "Everything".

No test covers the external-writer case: `device_sync_picker_tests.rs:4` calls
`recompute_delta("a")` by hand. A regression test should drive a `change_log`
write and assert the picker sees the new playlist without an explicit
recompute.

## Open craft problem

**The lyrics shot.** At `hold 0.35 / 1.00` the crop starts at stage x 410, which
is inside the track list, and the left edge lands in the middle of the artist
column (`rnja & Distant)`, `Alvarez)`). There is no x where a tighter crop does
not cut through a word, and at the safe amount the lyrics pane is legible but
small. It is currently set to the safe `hold 0.20 / 1.00 / 0.50`.

The fix belongs to the recording, not the cut: record that shot with the lyrics
given the width — a wider lyrics pane, or the track list collapsed.

## Music

Still to do, after the picture. Two constraints:

- **60 s needs an arc, not a pulse.** The user asked for more variation than the
  31.8 s cut had: a build, a change at the handover to the phone, a lift into
  the end card.
- Variant C's source is 88 s at
  `/opt/n8n-stack/files/musik/reprise-showreel-c.mp3` on `hetzner-media`. Local
  scratchpad copies do not survive a tmpfs sweep. `pick-window.py` can window a
  60 s stretch out of it; whether C has enough variation over 60 s is unproven.
- `music_length_ms` is ignored by `music_v2` — a 31.2 s request returned 78 and
  88 second pieces. That is why `align-bed.py` and `pick-window.py` exist.

Workflow `Nv8IFwnuNSmBegAv` on the user's n8n, credential `elevenlabs`
(`tcSNjysZl9KmQtx1`), ~1200 credits for the three variants.

## Showroom

Unchanged from the previous handoff and **still against the 31.2 s film**. After
the recut: re-run `encode-web.sh`, update `FILM_SECONDS` in
`tests/showreel-film.test.mjs`, the cue times in `showreel.vtt` and the duration
wording in `ShowreelFilm.tsx`, then rebuild and re-run the suite. Last green:
`npm run lint`, `tsc --noEmit`, `node --test` 85/85.

## Traps worth keeping

- **Every raw take is VFR.** `fps=30` must be first in every chain, and
  `-frames:v` is the authority on length — `-t` alone drifts the cut off the
  grid.
- **Tempo estimation quantises.** At 100 BPM a beat is 51.7 frames; rounding the
  autocorrelation lag to 52 reports 99.4 BPM. Parabolic interpolation against a
  synthesised 100.00 BPM control: 99.38 → 99.85, and onset energy on the
  estimated beats 0.61 → 2.47.
- **A superellipse is not a rounded rectangle.** Applying the exponent to the
  full half-axes rounds the whole silhouette; the phone came out as a bar of
  soap. The exponent must act on the corners alone.
- **Blur spills inward.** The device shell's shadow and glow, blurred from the
  body mask, washed teal across the screen. Both are multiplied by `1 - body`
  so only the halo outside survives.
- **AT-SPI hands rows over before they are painted.** The podcast chart returned
  rows 3 s in and still read "Searching…" on screen. Poll for rows, then wait a
  fixed dwell.
- **Percussion must be band-limited, not merely bright.** Differencing white
  noise put the synthesised bed's spectral centroid at 13 kHz — hiss on top of
  the mix rather than inside it.
- **Every Bash call raises the terminal.** GNOME 49 on Wayland will not let a
  script raise Reprise back, and this has cost three takes. `take-mcp.sh` waits
  `SHOWREEL_PREROLL` (16 s) before recording so the window can be clicked back
  by hand — and **nothing may run in the shell** until the take ends.
