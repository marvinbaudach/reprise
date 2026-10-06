# The cavacore oracle

The golden test `ac_29_cava_bars_match_the_cavacore_reference_after_calibration`
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
./oracle synth.f32 bars.csv cutoffs.txt sens.txt                   # one CSV row per frame, gain per frame
sed -n '173p;241p;256p;331p' bars.csv                              # frames 172, 240, 255, 330
sed -n '101p;201p;301p' sens.txt                                   # the gain after frames 100, 200, 300
```

`oracle.c` mirrors the port's configuration: 64 bars, 44.1 kHz mono, autosens
on, noise reduction 0.77, 50-10 000 Hz, input scaled by 65535
(`CAVA_FIXED_POINT_SCALE`), one 735-sample hop per `cava_execute` call.
`cavacore` estimates its framerate from the sample counts it is given, exactly
as the port does, so no framerate is configured on either side. Frame `k` is the
output after chunk `k`, 0-based, so it is row `k + 1` of the CSV.

## The gain the test starts from

The port does not climb from a cold start the way `cavacore` does: at a stream
boundary it measures the gain from the new audio (AC-29), so its gain history
before the pinned frames differs from `cavacore`'s by design. The test therefore
feeds the port the first 101 frames, then sets its gain to `cavacore`'s own gain
after frame 100 (`0.745497722`, the fourth output of `oracle.c`, row 101 of
`sens.txt`) and compares from frame 172 on. From that frame both run the same
creep, which is what the reference pins. Every comparison after it is
unchanged, and the overshoot limit cycle at frames 255 and 330 still depends on
the port making `cavacore`'s decisions frame by frame.

The test also pins `cavacore`'s gain after frames 200 (`0.774628729`) and 300
(`0.806024941`), rows 201 and 301 of `sens.txt`, at 1e-4 relative. A creep that
drifted would show there before it moved a bar past the 2e-3 tolerance.
Regenerate all three values with the `sed` line above; `sens.txt` holds one
gain per frame, so row `k + 1` is the gain after frame `k`.

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
The 2e-3 tolerance is about 2000 times that. With the gain injected as
described above the margin is unchanged (2026-10-06: the test still passes at
a tolerance of 1.2e-6).

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
