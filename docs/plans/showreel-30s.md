# The 35-second cut — storyboard, EDL and the pipeline that renders it

A landing-page recut of the 59.9 s showreel. The long film stays as the record;
this is a second, shorter deliverable. Two of its shots come from footage that
does not exist yet and one from footage that is provably out of date, so this
is a partial re-shoot, not only a recut.

Read [`reprise-showreel.HANDOFF.md`](reprise-showreel.HANDOFF.md) first: it holds
how the takes were shot, the AT-SPI and screencast constraints, and why
`SHOWREEL_DIR` is shared state two sessions can silently overwrite.

## What has to be re-shot, and what does not

**The desktop take is still current.** The last GNOME commit is `7c3dfcc10a`
(2026-08-25 21:12), before take 1 and take 2 were recorded; everything since is
Android or the showreel itself. Shots 1–5, 10 and 11 stay as they are.

**Two flows were never filmed.** The 60 s film shows the podcast *view* and the
YouTube *view*, never an add.

- The Add Podcast dialog opens on Apple's country chart —
  `strings_podcasts.rs:461` heads it `PODCASTS · TOP IN {country}`,
  `:454` offers the chip `Popular in {country}`, and `strings_location.rs:28`
  calls it "Apple's country chart in Add Podcast". `take-gnome2.py:98` typed
  straight into the search field and skipped past it.
- YouTube has no chart. `add_dialog_chips.rs:63` gives that dialog a genre chip
  (`{genre} channels`) and `strings_podcasts.rs:99` a hint reading "Search or
  paste a channel URL" — search is the only route in, which is what the shot
  wants anyway. Take 1 only did `sidebar('YouTube', dwell=6.5)`.

**The phone take is older than the visualiser fix.**

```
a5ee0f9801  2026-08-26 00:01  Der Android-Visualizer bekommt seine Daten in Wiedergabezeit (#701)
roh-android-take.mp4          2026-08-25 22:05
```

One hour fifty-six minutes older. The oil-film work (#690, #691, 19:17 and
19:37) is in the footage; the PCM-in-playback-time correction is not. The phone
half is re-shot on a build from HEAD, which also brings the queue drag (#702,
#704) and lets the new artist flow be filmed in the same pass.

## What else changes against the 60 s film

| Change | Reason |
|---|---|
| Title card and welcome plate dropped from the head | 3.5 s before anything moved, and the welcome plate is a PNG — the worst possible opening frame |
| Cold open on the visualiser | the only shot that moves on its own from frame 0, and it reads as "music player" in under a second |
| Wordmark becomes a persistent bug, bottom right | costs 0 s, brands every social crop |
| Layout-preferences and plugin shots dropped | settings do not sell |
| Library, my-stats, phone-search-only, phone-queue dropped | the desktop search shot already shows the library; the rest lost to the two add flows |
| Android seek shot dropped | it never seeked — open point 1 of the handover, solved by omission |
| Releases and concerts become 1.2 s burst words | breadth at a glance, −1.6 s |
| Podcasts and YouTube become real add flows, 3.6 s each | a view proves nothing; an add proves the feature |
| Caption band grows 92 → 120 px, type 34 → 48 px | the film autoplays muted on a landing page; the captions carry it alone |
| Centred overlays allowed, but only on shots with no competing text | the first attempt put "Library Doctor" on the app's own Library Doctor entry |
| Every cut on a 100 BPM grid (beat 0.6 s = 18 frames) | nothing lands between two beats once the bed is under it |

## Frame

Stage 1920x960, band 1920x120, 2 px hairline at y=960. Ground `0x0D1014`,
teal `0x4FDBD4`, ink `0xF2F6F6`, Adwaita Sans. 1920x1080 at 30 fps throughout.

## Caption classes

- **Statement** — centred over the stage on a `rgba(13,16,20,.55)` scrim with a
  6 px backdrop blur, 76 px, words staggered 3 frames apart. Permitted only on
  shots that carry no competing text: the visualiser hook, the platform bridge,
  the end card. Three uses, no more.
- **Callout** — on the band. Teal dash wipes in by `scaleX`, text rises 14 px
  and fades, both on a spring; out over the last 0.35 s. Optional 28 px subline,
  which may swap mid-flow when two shots share one claim.
- **Burst** — the 1.2 s shots. One word, hard in with no fade, `scale 1.06 -> 1.0`
  over 5 frames.

## Storyboard

`T1`/`T2` = the existing takes, `T3` = the new desktop pickup, `A2` = the new
phone take. In-points into `T1`/`T2` are seconds into the raw file.

| Time | Shot | Source | Overlay | Sound | Motion |
|---|---|---|---|---|---|
| 0.0–3.0 | visualiser | T1 @57.5 | STATEMENT @0.4 "One player. Everything you listen to." | cold open on the downbeat | push in 4 %, centre |
| 3.0–6.0 | search with suggestions | T2 @76.5 | CALLOUT "Instant search" / "every field, as you type" | keystroke ticks | push in 3 % onto the entry |
| 6.0–8.4 | lyrics | T2 @95.5 | CALLOUT "Lyrics, in time" | bed open | push out 3 % |
| 8.4–9.6 | releases | T1 @14.2 | BURST "Releases." | tick | push in 2 % |
| 9.6–10.8 | concerts | T1 @21.2 | BURST "Concerts." | tick | push in 2 % |
| 10.8–12.6 | Add Podcast opens on the chart, `PODCASTS · TOP IN DE` | **T3** | CALLOUT "Podcasts" / "the country chart, built in" | bed opens | push in 3 % onto the chart heading |
| 12.6–14.4 | a chart entry subscribed, the show lands in the list | **T3** | CALLOUT holds, sub swaps to "one click to subscribe" | click accent | push in 3 % onto the subscribe row |
| 14.4–16.2 | Add Channel, the channel name typed, results appear | **T3** | CALLOUT "YouTube" / "search a channel" | keystroke ticks | push in 3 % onto the entry |
| 16.2–18.0 | channel subscribed, uploads in the list | **T3** | sub swaps to "its uploads arrive as audio" | click accent | push out 3 % |
| 18.0–20.4 | library doctor | T1 @49.5 | CALLOUT "Library Doctor" / "finds what's broken" | one soft click | push in 3 % |
| 20.4–23.4 | device sync | T1 @41.5 | STATEMENT @21.6 "…and it's on your phone." | riser; duck −6 dB + LPF 400 Hz from 23.0 | push in 3 %; dip out over the last 0.4 |
| 23.4–25.8 | phone: search "lorna", results | **A2** | CALLOUT "Reprise on Android" | downbeat at 23.4, filter opens | dip in 0.4; push in 3 % |
| 25.8–28.2 | phone: the artist view, held — header, discography, albums all legible | **A2** | CALLOUT "Straight to the artist" | bed | push out 2 %, deliberately slow: this page is meant to be read |
| 28.2–30.0 | phone: the newest album starts, Now Playing | **A2** | CALLOUT "Play the newest album" | play accent | push in 3 % onto the cover |
| 30.0–32.4 | phone visualiser, build ≥ #701 | **A2** | CALLOUT "The same visuals" | bed peak | push in 3 % — rhymes with the hook |
| 32.4–34.8 | end card | `card-end.png` | STATEMENT wordmark + "Free and open source · Linux and Android" + URL | tail, last beat at 34.8 | slow push out 2 % from black |

**Total 34.8 s.** Every boundary is a multiple of 0.6 s and a whole frame. If it
has to come down, the concerts burst goes first: 33.6 s.

The two burst in-points are provisional — the old in-points plus 0.2 s to skip
the settle. Before the final render, find the busiest 1.2 s window with the
motion probe from the handover (`tblend=difference` thresholded at 16/255,
sampled at 10 Hz, take the mean). At 36 frames the difference between the right
window and the wrong one is the difference between legible and not.

## The two new takes

### `take-gnome3.py` — the desktop pickup, about 45 s of raw

```
Podcasts → 'Add podcast'          dwell 4.0   # let the chart render — do NOT type
  collect buttons named 'Subscribe to …', pick the nth chart row
  do(subscribe)                   dwell 3.5
  Close                           dwell 3.0   # the new show in the list
YouTube  → 'Add channel'          dwell 2.0
  type_into(entry, CHANNEL, 0.14)
  'Search'                        dwell 4.0
  'Subscribe to {CHANNEL}'        dwell 3.5
  Close                           dwell 3.0
```

The AT-SPI names are fixed: `Add podcast` / `Add channel`
(`strings_podcasts.rs:41,42`), dialog titles `Add Podcast` / `Add Channel`
(`:96,98`), and every subscribe button is named `Subscribe to {source}`
(`strings_sources.rs:13`) — which is what makes a chart pick possible without
knowing the show's name in advance. `rp.py` and the helpers in `take-gnome2.py`
carry the rest; sidebar entries have role `button`, not `push button`.

**The channel.** NPR Music (Tiny Desk) is the recommendation: known everywhere,
unmistakably a channel rather than an artist, and uncontroversial. Boiler Room
is the alternative that illustrates the app's own promise — "long mixes, sets,
instrumentals. Shorts stay hidden" (`strings_podcasts.rs:66`).

**The chart is live.** It changes daily. Do not take position one blindly: look
at the row before the take and choose deliberately, because whatever is on
screen that evening sits on the landing page for months.

### `take-android2.sh` — a full re-shoot on a HEAD build

```
build and install the APK from HEAD, confirm the installed versionCode
search 'lorna' → tap the artist hit → hold the artist view 6 s
  → tap the newest album → play → Now Playing → visualiser 10 s
```

Every coordinate is pinned to a Pixel 10 Pro XL and survives no layout change.
The header tap `892 219` and the visualiser tap `540 913` from
`take-android.sh` are the starting point; the artist result row is new and has
to be read off the device.

**Verify the fix, do not trust it.** Run the same motion probe over the
visualiser window of the new take. A stalled visualiser measures near zero, a
running one measurably above it. That makes the shot evidence before it reaches
the cut, instead of after someone watches the finished film.

## Sound

Every take is mute. Everything below is laid in.

- Instrumental bed at **100 BPM**, no vocals — vocals fight captions.
- Loudness `I=-16 LUFS`, `TP=-1.5 dBTP`, `LRA=11`, two-pass `loudnorm`.
- Duck the bed −6 dB with a 400 Hz low-pass across the platform dip (20.0–20.4),
  open it again on the downbeat at 20.4.
- The landing-page hero autoplays muted. The captions must carry the whole film
  with the sound off; the sound is an upgrade, never the story.

**The trap, now split across two takes.** The visualiser reacts to whatever was
playing while it was recorded. The desktop hook (shot 1) comes from take 1,
recorded 25.08.2026 against a track nobody wrote down — so its bars will dance
against any bed laid under them, in the film's **first three seconds**. The
phone visualiser is being re-shot anyway, so it can simply be filmed with the
chosen bed playing.

Pick the bed first, then either license the track that was playing during take 1
or re-shoot the desktop visualiser shot with the bed (`take-gnome.py`, roughly
fifteen minutes, and the desktop build is still current so nothing else in
take 1 needs touching).

## Pipeline

Four stages, one source of truth (`scripts/showreel/edl.json`).

```
scripts/showreel/proxies.sh      ffmpeg    VFR -> CFR 30, crop, debadge   once
tools/showreel/                  Remotion  cuts, push, typography         iterate
scripts/showreel/mix.sh          ffmpeg    bed, ducking, loudness
scripts/showreel/encode-web.sh   ffmpeg    the delivery ladder
```

### Stage 0 — proxies (mandatory, not an optimisation)

Probed 26.08.2026:

```
roh-gnome-take1   r_frame_rate 10000/1   avg 23.75   2499 frames / 105.2 s
roh-gnome-take2   r_frame_rate 10000/1   avg 22.60   2472 frames / 109.4 s
roh-android-take  r_frame_rate 4/1       avg 68.20   4373 frames /  64.1 s
```

All three are variable-rate with lying metadata. Remotion's `<OffthreadVideo
startFrom/endAt>` converts frames to timestamps; against these files every
in-point drifts. The same fact is why `cut-gnome.sh` puts `fps=30` first in
every filter chain.

`proxies.sh` writes `$SHOWREEL_WORK/proxy/p-gnome1.mp4`, `p-gnome2.mp4`,
`p-android2.mp4`:

- `-fps_mode cfr -r 30`
- desktop: `crop=2880:1747:0:53`, then `scale=1920:1164:flags=lanczos`
- take 2 additionally bakes in `DEBADGE` from `cut-gnome.sh`, so Remotion never
  has to know the badge existed
- phone: native 1080x2400, CFR only
- `-an -c:v libx264 -crf 14 -preset veryfast` — a visually lossless intermediate

After this, `startFrom = Math.round(seconds * 30)` is exact.

### Stage 1 — Remotion

`edl.json` is the whole edit:

```json
[
  { "id": "01-hook", "src": "p-gnome1", "in": 57.5, "dur": 3.0,
    "push": { "from": 1.0, "to": 1.04, "origin": [0.5, 0.5] },
    "caption": { "kind": "statement", "line": "One player. Everything you listen to.", "at": 0.4 },
    "dip": null },

  { "id": "02-search", "src": "p-gnome2", "in": 76.5, "dur": 3.0,
    "push": { "from": 1.0, "to": 1.03, "origin": [0.46, 0.10] },
    "caption": { "kind": "callout", "line": "Instant search", "sub": "every field, as you type" },
    "dip": null }
]
```

`origin` is the normalised point the push moves toward — this is the whole
reason for Remotion. `zoompan` crops around a computed centre and its rectangle
is integral; a CSS `transform-origin` is continuous and costs one line.

Components under `tools/showreel/src/`:

| File | Job |
|---|---|
| `Showreel.tsx` | `<Series>` over `edl`, total = `sum(dur) * 30` frames |
| `Shot.tsx` | stage + band; `<OffthreadVideo muted startFrom endAt>` in a wrapper carrying `scale()` and `transformOrigin` |
| `Portrait.tsx` | two layers — `filter: blur(28px) scale(1.15)` behind, sharp `height:100%` in front |
| `Callout.tsx` | dash `scaleX` on `spring({damping:200})`, text `translateY 14 -> 0` + fade, out from `dur-0.35` |
| `Statement.tsx` | scrim + `backdrop-filter: blur(6px)`, 76 px, 3-frame word stagger |
| `BurstWord.tsx` | hard in, `scale 1.06 -> 1.0` over 5 frames |
| `Dip.tsx` | black overlay, 12-frame ramp |
| `Bug.tsx` | wordmark bottom right, opacity 0.55, whole film |

Render: `npx remotion render Showreel out/picture.mov --codec=prores --prores-profile=hq`.

Three things that will bite:

1. `<OffthreadVideo>`, never `<Video>`. `<Video>` is a preview convenience and
   is not frame-accurate under render.
2. Load Adwaita Sans through `staticFile()` + `@font-face` and hold
   `delayRender()` until `document.fonts.ready`. Without it the opening frames
   render in the fallback face and nobody notices until the master is out.
3. Remotion is source-available under the Remotion License, not an OSI licence
   — free for individuals and companies up to three people. That is fine for
   this project, but it is a non-free build dependency in a FOSS repository, so
   it lives under `tools/` and ships with nothing. Keep the ffmpeg route
   viable: `film.sh` already renders this look, it just cannot aim a push.

### Stage 2 — sound and delivery

`mix.sh` — bed and stingers over `picture.mov`, `sidechaincompress` for the dip
duck, two-pass `loudnorm=I=-16:TP=-1.5:LRA=11`.

`encode-web.sh` — the ladder. Context: the 60 s film weighs 19.7 MB against
**2.4 MB for the showroom's entire media payload**.

| File | Purpose | Target |
|---|---|---|
| `hero-720.webm` VP9 CRF 34, `-b:v 0` | muted autoplay loop, first `<source>` | ≤ 2.5 MB |
| `hero-720.mp4` h264 CRF 26 | fallback | ≤ 3.5 MB |
| `showreel-1080.mp4` CRF 22 + Opus | click-to-play, with sound | ~9 MB |
| `poster.webp` @0.9 s | poster for `preload="none"` | ~80 KB |
| `social-1x1.mp4` | centre crop of the stage | — |
| `social-9x16.mp4` | the phone half alone — free, it is already portrait | — |

Showroom ground truth: React 19 + Vite 8, deployed by `.github/workflows/pages.yml`
on pushes to `main` touching `showroom/**`; eleven `.webp` plates from
`showroom/src/data/showcase.ts`; no `<video>` anywhere yet; no LFS and no
asset-size gate. `showroom/tests/product-gallery.test.mjs:39` pins the plate
count at exactly eleven — putting the film **beside** the gallery is test-free,
**replacing** plates is a test change.

## Open

1. **Bed track chosen**, and the desktop visualiser shot reconciled with it —
   either by licensing the original or by a fifteen-minute re-shoot. Until then
   the hook is out of sync with its own music.
2. **The chart row for the podcast add** — chosen by eye on the day of the take,
   not by rank.
3. **The channel** — NPR Music or Boiler Room.
4. **Burst in-points** confirmed by the motion probe; the new phone visualiser
   confirmed by the same probe as actually running.
5. Whether this cut replaces the 60 s film on the landing page or joins it. The
   60 s version stays the record either way.
