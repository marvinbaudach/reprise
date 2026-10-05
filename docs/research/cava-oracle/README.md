# The cavacore oracle

The golden test `ac_28_cava_bars_match_the_cavacore_reference_after_calibration`
(`crates/reprise-core/src/playback/cava_tests.rs`) pins the Rust CAVA port to
numbers produced by the original C `cavacore`. This directory holds everything
needed to regenerate them. `cavacore` itself is MIT-licensed and is **not**
vendored; fetch it at the pinned commit.

## Regenerate

`cavacore` needs FFTW (`libfftw3-dev` or the distribution's equivalent).

```bash
work=$(mktemp -d ~/.cache/reprise-scratch/cava-oracle.XXXXXX)
cd "$work"
base=https://raw.githubusercontent.com/karlstav/cava/4b12c2b043723f42567ddbfd5a516566bdf52316
curl -fLO $base/cavacore.c
curl -fLO $base/cavacore.h
sha256sum cavacore.c cavacore.h
#   416700906719a8e08cd8437e3fc64a90e5ea19f98dfff69c873ae0d40e821044  cavacore.c
#   ef427db332cab781e563be2336f4b29a9467b2872efb1b310f2d8d843dcea834  cavacore.h

gcc -O2 -o oracle <repo>/docs/research/cava-oracle/oracle.c cavacore.c -I. -lfftw3 -lm
python3 <repo>/docs/research/cava-oracle/gen.py synth.f32          # 360 frames x 735 samples
./oracle synth.f32 bars.csv cutoffs.txt                            # one CSV row per frame
sed -n '173p;241p;256p;331p' bars.csv                              # frames 172, 240, 255, 330
```

`oracle.c` mirrors the port's configuration: 64 bars, 44.1 kHz mono, autosens
on, noise reduction 0.77, 50-10 000 Hz, input scaled by 65535
(`CAVA_FIXED_POINT_SCALE`), one 735-sample hop per `cava_execute` call.
`cavacore` estimates its framerate from the sample counts it is given, exactly
as the port does, so no framerate is configured on either side. Frame `k` is the
output after chunk `k`, 0-based, so it is row `k + 1` of the CSV.

## What the test pins

- Frames 172, 240, 255 and 330, all 64 bars each, at **2e-3 absolute**.
- Frames 172 and 240: cavacore clips bar 26 (the constant 440 Hz tone) to 1.0
  and leaves every other band untouched. At frames 255 and 330 bar 26 is just
  under the clip (0.952 and 0.984), so a different overshoot history would show.
- Bar 26 sits in the autosensitivity limit cycle: the gain steps down 2 % on an
  overshoot and creeps back up 0.1 % per frame. The pinned frames therefore
  depend on the port making the same overshoot decisions as cavacore frame by
  frame.

Measured margin (2026-10-05): the port differs from the reference by at most
1.1e-6 at the pinned frames, which is the rounding of the six-digit reference.
The 2e-3 tolerance is about 2000 times that.

## Robustness probe

The port computes in `f32`, cavacore in `double`. To check that the test does
not hinge on one razor-thin overshoot decision, the test input was perturbed
(each sample, in `f64` before the cast to `f32`) and the unchanged reference was
asserted:

| Perturbation | Result |
|---|---|
| every sample x 1.000001 | green |
| every sample x 0.999999 | green |
| every sample x 1.00001 | green |
| every sample x 0.99999 | green |
| every sample x 1.0001 | green |
| every sample + 1e-7 | green |
| every sample + 1e-6 | green |
| every sample + 1e-5 | green |

The design stays as it is: the limit cycle is stable well beyond the noise an
`f32` port can introduce.

## Discrimination

Each of the two removed behaviours, reinstated in a scratch edit of
`playback/cava/smoothing.rs`, turns the test red at frame 172:

- noise-floor gate (`*bar <= 0.04 -> 0`, before gravity and integral): bar 6
  expected 0.077458, got about 0
- frame-wide duck on every overshoot (`output_scale` armed by `|| overshoot`):
  bar 0 expected 0.315462, got 0.267880
