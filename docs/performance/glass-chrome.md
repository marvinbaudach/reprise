# Glass chrome and AlbumView glow performance

The reproducible runner is `scripts/glass-render-cost.sh OUTPUT_DIR`. It builds
release binaries, seeds 10,000 generated metadata-only tracks per mode, uses
private XDG data/cache roots, a private D-Bus session, Xvfb, fake audio, and
two fresh app processes on the same host.

## Metrics and budgets

- `baseline` retains the identical overlay shell and neutral tint but suppresses
  only the backdrop snapshot/blur node.
- `glass` renders the normal clipped backdrop blur and tint.
- Both modes discard 10 warm-up frames and retain 120 CPU wall-time samples
  from GDK `before-paint` to `after-paint`.
- Glass must remain at or below 20 ms p95, 50 ms maximum, and 3 ms p95
  overhead over its paired baseline. Missing samples fail closed.
- These are CPU submission timings. A compositor/GPU trace is still required
  for definitive GPU completion and native-Wayland frame-pacing claims.

## Cost model

There are at most two chrome backdrop nodes: the shared header/search zone and
the player zone. Each snapshots the already-composed content, clips to its own
allocated rectangle, applies a 24 px blur only for a classified GL/NGL/Vulkan
renderer, then draws a neutral tint. Cairo, unknown renderers, disabled
animations, and High Contrast draw the near-opaque tint without a blur node.

The Albums glow is not part of that live blur cost. On a track change, the
normal cover resolver creates one 32 x 32 thumbnail on a worker, that reduced
file is blurred once on the worker, and the result is cached. Steady-state
drawing is one 32 x 32 texture (4 KiB decoded RGBA) scaled behind the grid at
22 percent opacity. Stop, no-cover, stale generations, and High Contrast do
not draw it. The runner reports the cold downscale, cold blur, and 1,000 cached
lookups separately in `album-glow.json`.

## 2026-07-19 evidence

The release microbenchmark on the implementation host used a generated
1200 x 1200 gradient cover and measured 3.631 ms for the single downscale,
0.114 ms for the single blur, and 3.745 ms total. The p95 of 1,000 cache hits
was 0.571 microseconds. The resulting decoded texture is 4 KiB.

The fully isolated host run used `GskGLRenderer`, 10 warm-up frames and 120
retained frames per mode. Baseline measured 0.246 ms p95. Glass measured
0.231 ms p95 and 0.435 ms maximum, with the fail-closed calculation reporting
0 ms p95 overhead. All three budgets passed by wide margins: 20 ms p95, 50 ms
maximum and 3 ms p95 overhead. The slightly lower Glass p95 is run noise, not
evidence that blur improves performance; the overhead calculation deliberately
saturates at zero.

The paired run's AlbumView glow measured 3.829 ms for the cold downscale,
0.110 ms for the cold blur, 3.939 ms total and 0.601 microseconds p95 over
1,000 cached lookups. The decoded texture remained 4 KiB.

These measurements prove the CPU paint-submission budget for the isolated X11
GL path. They do not measure GPU completion, native-Wayland frame pacing or
visual blur quality; those remain compositor/manual checks. The retained raw
artifacts are `baseline.json`, `glass.json`, `album-glow.json` and
`summary.json` from the same paired run.
