---
phase: reviewed
---

# Android live CAVA visualizer

## Comparable paused-motion measurement

Use this exact image-difference procedure for every device or emulator
measurement so later results stay comparable:

1. Capture at 1344 x 2992 pixels and crop the Now Playing visualizer rectangle
   from `(270, 1240)` through `(1075, 1440)`.
2. Count pixels whose largest absolute difference between any corresponding
   colour channel in the two crops is greater than 8.
3. Start playback and capture two frames exactly 1.0 seconds apart. Their pixel
   count is the playback reference.
4. Pause playback. Do not use the immediate post-pause frame: the one-time drop
   from live height to resting height would dominate the motion measurement.
   The blend itself takes about 0.42 seconds, but a peak cap falling from 0.9
   takes about 50 ticks, or 0.83 seconds, and remains visible while it falls.
   Therefore wait at least 1.0 second for the slowest visible transition; this
   protocol uses about 1.7 seconds as a deliberately conservative capture-grid
   value. Use that first settled frame as the paused baseline.
5. Capture a paused series spanning at least two complete wave periods and
   compare every frame with the settled paused baseline.
6. Report both the peak paused difference as a ratio of the playback reference
   and the elapsed time between successive minima as the measured period.
