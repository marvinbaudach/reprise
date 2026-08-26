# Handover — The Reprise showreel: the shot list and how it was shot

Recorded 25.08.2026 in one evening session, recut to length on 26.08.2026.
Both takes are in the can and the film is cut. The deliverables live under
`~/Videos/reprise-showreel/`; the drivers and cut scripts that made them are in
[`scripts/showreel/`](../../scripts/showreel/). This file is the script, so the
film can be re-shot without reconstructing the decisions from the footage.

## What exists

`~/Videos/reprise-showreel/`

| File | Length | What it is |
|---|---|---|
| `reprise-showreel-60s.mp4` | **59.9 s** | the cut film — GNOME, then the phone |
| `reprise-gnome.mp4` | 39.8 s | the desktop half alone |
| `reprise-android.mp4` | 20.1 s | the phone half alone |
| `roh-gnome-take1.mp4` | 105.2 s | raw take 1, the sidebar tour |
| `roh-gnome-take2.mp4` | 109.4 s | raw take 2, the three pickups |
| `roh-android-take.mp4` | 64.1 s | raw phone take |
| `welcome-plate.png` | still | the welcome screen, shot headless |
| `previous-63s/` | — | the first cut, 63.4 s, kept as the record |

All video is 1920×1080 at 30 fps, mute. The takes are the only irreplaceable
files here: everything else is one `scripts/showreel/cut-*.sh` away.

## The script

The order was given by the user, not derived: *settings (layout with clicks,
plugins); library search; releases, concerts, sync, library doctor,
visualisation, lyrics; my stats; YouTube and podcasts; the search suggestions;
the welcome screen as the opening; and subscribing to a well-known podcast.*

### Part one — GNOME (39.8 s, 15 shots)

Source and second are exact; `cut-gnome.sh` is the shot list in executable
form. `T1`/`T2` name which raw take the second belongs to.

| # | Shot | Source | In | Length |
|---|---|---|---|---|
| 00 | welcome screen (still) | plate | — | 2.5 s |
| 01 | library | T1 | 6.5 | 2.5 s |
| 02 | search with suggestions | T2 | 76.5 | 3.0 s |
| 03 | releases | T1 | 14.0 | 2.0 s |
| 04 | concerts | T1 | 21.0 | 2.0 s |
| 05 | podcasts | T1 | 27.5 | 2.0 s |
| 06 | subscribing to *Darknet Diaries* | T2 | 50.5 | 3.0 s |
| 07 | YouTube | T1 | 34.5 | 2.0 s |
| 08 | device sync (Pixel 10 Pro XL) | T1 | 41.5 | 2.5 s |
| 09 | library doctor | T1 | 49.5 | 2.5 s |
| 10 | visualiser | T1 | 57.5 | 3.0 s |
| 11 | lyrics | T2 | 95.5 | 3.0 s |
| 12 | my stats | T1 | 76.0 | 2.5 s |
| 13 | settings → layout, player bar to the top | T1 | 87.5 | 4.5 s |
| 14 | settings → plugins | T1 | 96.5 | 2.5 s |

Take 2 was shot with a `SCROLL-LOG` debug badge pinned in the header bar. Its
three segments patch that corner over with a slice of empty header bar from
further right — same gradient, same row, so the seam is invisible and the badge
never blinks on mid-film. That is the `DEBADGE` filter in `cut-gnome.sh`; drop
it only if take 2 is re-shot without the badge.

### Part two — Android (20.1 s, 7 shots)

| # | Shot | In | Length |
|---|---|---|---|
| 1 | library | 6.0 | 2.5 s |
| 2 | search "lorna" | 13.6 | 4.0 s |
| 3 | play | 25.0 | 2.5 s |
| 4 | visualiser | 29.6 | 3.5 s |
| 5 | cover artwork | 36.0 | 2.5 s |
| 6 | seek | 46.5 | 2.5 s |
| 7 | queue | 53.0 | 2.5 s |

Portrait footage sits centred on the 1920×1080 canvas with its own blurred
enlargement behind it, so the sides are not dead black.

## How the length was found

The first cut ran 63.4 s against a name that says 60. Rather than pick shots to
shorten by feel, every shot was measured: the share of the frame that changes
from one tenth of a second to the next, over the raw take, sampled at 10 Hz
(`tblend=difference` thresholded at 16/255, then the mean — so the number is
"percent of the picture that moved"). Four shots turned out to be sitting on a
frozen frame, and only those four were touched:

- **search** acted for 1.0 s and then held a still frame for 3.0 s → 3.0 s.
- **podcast-add** acted for 1.5 s and held for 2.4 s → 3.0 s.
- **lyrics** never moved at all in its 4.0 s → 3.0 s.
- **layout** waited 2.9 s for the click that is its entire point. Its *in*-point
  moved from 86.3 to 87.5 instead of its length being cut, so the switch now
  lands 1.9 s in and the result gets 2.6 s to read.

That is −3.5 s of hold and no content, landing the film at 59.87 s. The method
is worth keeping: a shot that reads as slow and a shot that is genuinely frozen
look identical in a shot list and completely different in this measurement.

## How it was shot

Everything below is executable — see `scripts/showreel/README.md` for which
script does what, and `SHOWREEL_DIR` / `SHOWREEL_WORK` for where things land.

**The desktop, without stealing focus.** GNOME 49 on Wayland refuses
`org.gnome.Shell.Screenshot` and `Introspect`, refuses per-window capture
("Wayland capture cannot prove pixels belong to window …"), and refuses
`bring_to_front`. What works:

- **Driving**: AT-SPI (`gi.repository.Atspi`) against the running app —
  `rp.py` holds the helpers, `take-gnome.py` the take. The app keeps focus,
  the compositor never sees a synthetic pointer, and no cursor is in frame.
  The app's AT-SPI name is `reprise`, lowercase, and `Atspi.init()` must run
  at import. Sidebar entries have role `button`, not `push button`.
  Preferences pages expose no action — select them through the parent's
  `SelectionIface.select_child(index)`.
- **Recording**: `org.gnome.Shell.Screencast` records the full output at
  2880×1800@60. The session belongs to the D-Bus connection that started it,
  so a second `gdbus call` cannot stop it — `screencast.py` holds one
  connection until a stop-flag file appears. Its options must be a plain dict
  of `GLib.Variant`s, never a pre-built `a{sv}` variant.
- **Stills**: `cua-driver get_desktop_state --screenshot-out-file` gives a
  native 2880×1800 PNG. `plates-gnome.py` derives the showroom plates from it.
- **The one human step**: the window has to be clicked once to take focus.
  `await-run.sh` waits for that in the background and fires the take itself;
  `active-window.py` is the focus gate it polls.
- **Cropping**: fractional scaling 1.6667 means `crop=2880:1747:0:53` strips
  the top bar and leaves the maximised window.

**The phone.** `take-android.sh` drives it with `adb shell input tap/swipe/text`
and writes `timeline-android.tsv` as it goes; `adb shell screenrecord
--size 1080x2400 --bit-rate 16M` records. The tap coordinates are pinned to a
Pixel 10 Pro XL and will not survive a different device or a layout change.

**The welcome screen** could not be shot from the real session — it only
appears on first run. `welcome-shot.sh` renders it under Xvfb at 3456×2160
with `GDK_SCALE=2` (logical 1728×1080, matching the real session),
`GSK_RENDERER=cairo`, `LIBGL_ALWAYS_SOFTWARE=1`, `REPRISE_AUDIO_SINK=fakesink`
and a seeded `gtk-4.0/settings.ini` carrying
`gtk-decoration-layout=close,minimize:appmenu` — without that the window
buttons end up on the wrong side.

## What is decided

- **The film is a file.** It was explicitly not built into the showroom as a
  video component. The user's intent was that it *could* later replace the
  many screenshots there — that decision has not been taken.
- **The plates that came out of the same session are live.** The eleven
  gallery plates were reshot and shipped in #695, and the podcasts plate was
  reshot from an anonymised profile in #698 after the first one showed real
  subscriptions on a public page.
- **The length is settled.** 59.87 s, taken entirely out of frozen holds; the
  63.4 s original is kept under `previous-63s/`.
- **The scripts are in the repository.** `scripts/showreel/`, with the
  scratchpad paths replaced by `SHOWREEL_DIR` and `SHOWREEL_WORK`. They pass
  `scripts/check-shell.sh` and the Python lint gate.

## What is open

1. **The Android seek shot shows no seek.** Measured on the raw take: across
   46.5–52.0 s the playhead only advances at playback speed (0:21 → 0:23 →
   0:24 → 0:26), with no jump. The `adb` swipe at `200,1763 → 760,1763` never
   landed on the scrubber, so shot 6 is the same still Now Playing as shots 3
   and 5. Either re-shoot the phone take with a working swipe, or drop the
   shot — dropping it costs 2.5 s and no information.
2. **No audio.** Every take is mute. A film about a music player that cannot
   be heard is a choice worth making deliberately.
3. **Whether the film belongs on the showroom** — and if so, whether it
   replaces plates or joins them. What the ground looks like today:
   `showroom/` is React 19 + Vite, deployed by `.github/workflows/pages.yml`
   to GitHub Pages on pushes to `main` touching `showroom/**`. The gallery is
   eleven `.webp` plates declared in `showroom/src/data/showcase.ts`, served
   from `showroom/public/media/showroom/` — **2.4 MB for the whole media
   payload**, against **6.0 MB for the film alone**. There is no `<video>`
   anywhere in the showroom today, no LFS and no asset-size gate, but
   `showroom/tests/product-gallery.test.mjs:39` pins the plate count at
   exactly eleven, so removing plates is a test change too. If it goes up, it
   should go up as a click-to-play `<video preload="none">` with an existing
   plate as its poster, *beside* the gallery — not as a replacement and not
   as anything that autoplays 6 MB at a first-time visitor.
4. **The welcome plate's crop is reconstructed, not recorded.** The original
   hand step between `welcome-raw.png` (3456×2160) and the shipped 2400×1456
   plate was never written down; `welcome-shot.sh` now does
   `crop 3456x2097+0+63`, derived from the shipped plate's own geometry — which
   is why its header bar is sliced. Compare against `welcome-plate.png` after a
   re-shoot before trusting it.
