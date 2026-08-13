# A disc of the cover, turning once a minute

Branch: `feature/android-desktop-visualizer`, on top of the two fog commits
(`30bd495a0c`, `9434f82d46`).

## Why

The desktop's now-playing head carries three layers cut from the artwork: the
bloom (a blurred, enlarged copy lying behind the cover), the shimmer, and the
cover itself. The phone has the first and the third. This plan adds the second.

`crates/reprise-gnome/src/ui/now_playing/cover_shimmer.rs` is short and worth
reading whole before starting. What it does:

- Builds one 260 px square once per track: the 32 px blurred cover painted up
  across it, then a radial mask applied with `DestIn` — opaque out to 12 % of
  the radius, falling linearly to nothing at 68 %, nothing beyond. A soft disc
  with a defined core, not a wash.
- Per frame: clip to a band, translate to the band's centre, rotate by
  `TAU * (elapsed_s / 60 mod 1)` — **one turn a minute** — scale to a diameter
  of 3.1 × the cover, paint with
  `0.34 + 0.14 * pressure + 0.16 * swell`.
- Nothing per frame but a translate, a rotate and one paint. The raster is
  bought once per cover.

Its header carries a measurement worth keeping in mind: the mockup wanted a
conic gradient of the cover's three dominant colours, and against a real library
that failed — half the covers are greyscale or near-black and yield no palette,
so the sweep came out as one flat tone on a backdrop of the same tone. The
blurred artwork itself always has structure. Do not reintroduce a palette here.

## What the phone already has

`android/app/src/main/java/de/reprise/spike/CoverFogBitmap.kt` builds exactly
this kind of thing already: `prepareCoverFogBitmap` crops the cover to a bounded
square and prepares **two** textures (`wideImage`, `tightImage`) that
`NowPlayingFog.kt` draws at 620 dp and 470 dp over a 272 dp cover, rotating at
one turn per four minutes and one per ~6.7 minutes, counter-running.

So the phone's haze already turns — slowly, and with no defined core. The
shimmer is the fast, legible one: four times the speed and a mask that keeps a
bright centre instead of spreading evenly.

The files named below are a starting point, not a fence: adjoining files may be
changed minimally where the contract requires it, as long as the commit message
names them. Stop only if the *contract* here turns out to be wrong.

## What to build

### 1. A third texture beside the fog's two

Add a masked disc to `CoverFogBitmap`: same source crop, same one-shot
preparation, same cache (`CoverArtworkCache.fog`), so a track change pays for it
once and a frame never decodes anything. The mask is the desktop's: full alpha
to 12 % of the radius, linear to zero at 68 %, zero beyond.

The existing fog textures are 256 px with a 208 px content square. Match that
scale rather than inventing a third; the disc's softness comes from the mask and
the upscale, not from resolution.

### 2. Draw it between the fog and the cover

In `NowPlayingScene.kt`'s canvas the order is fog (previous, current) → cover →
visualizer. The disc goes after the fog and before the cover, rotating about the
same centre the fog uses.

Two numbers need deciding rather than copying, because the phone's geometry is
not the desktop's:

- **Diameter.** The desktop's 3.1 × cover would be 842 dp against a ~384 dp wide
  screen — the disc would not read as a disc at all. Start at the wide fog
  layer's own 620 dp (2.3 × the 272 dp cover) and say in the commit message what
  it looked like; the mask, not the diameter, is what has to carry the shape.
- **Speed.** Keep the desktop's one turn a minute. That is the whole point of
  the layer: it is four times the fog's speed, which is what makes it legible as
  movement rather than as more haze.

Opacity reads both signals the fog now reads, in the desktop's proportions:
`0.34 + 0.14 * bassPressure + 0.16 * swell`, normalised through the same windows
`NowPlayingFogSpec` uses (the raw values are not 0…1 — see the constants there).
Where the rest of the scene respects the power gate (`power.fogRotates`), so
does this: no rotation when animations are off.

### 3. Tests

- the mask is 1 at the centre, 0 at and beyond 68 %, and monotonically falling
  between 12 % and 68 %
- one turn per minute: the angle at t=0, t=30 s and t=60 s, and that a long
  session does not lose precision (the desktop wraps with `rem_euclid`; do the
  same)
- opacity rises with each of the two inputs and clamps at both ends
- the disc is prepared once per track, not per frame — assert against the cache,
  the way the fog's own texture test does

## Verification — and what NOT to do

Run the unit tests: `JAVA_HOME=/usr/lib/jvm/java-21-openjdk ./gradlew
testDebugUnitTest` in `android/`, then count the XMLs under
`android/app/build/test-results/testDebugUnitTest`. Gradle reports BUILD
SUCCESSFUL without running a single test, so the count is the evidence.

**Do not drive the emulator.** No `adb input`, no screen recording, no
cua-driver. A previous run spent an hour there and fought the session that owns
that device. The visual acceptance — does the sweep read as a turning highlight,
or does it just brighten everything — is done outside this task, on a device
this task does not touch. State in the summary what you could not verify.
